use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Response;
use cab_core::CabError;
use cab_core::types::RequestLog;
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

use crate::fallback::{ProxyRequest, execute_with_fallback};
use crate::router::{pick_endpoints_for_protocol, resolve_route};
use crate::state::GatewayState;

/// POST /v1/messages
///
/// Anthropic Messages API proxy.
pub async fn handle_messages(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, CabError> {
    let start = std::time::Instant::now();
    let agent = crate::agent_id::extract_agent_id(&headers);

    // Parse body to extract model and stream
    let body_json: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| CabError::InvalidRequest(format!("Invalid JSON body: {e}")))?;

    let requested_model = body_json
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let stream = body_json
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Resolve route
    let resolved = resolve_route(
        &state.pool,
        &agent,
        requested_model.as_deref(),
        Some(&body_json),
    )
    .await?;

    let provider = cab_db::provider::get_by_id(&state.pool, &resolved.model.provider_id)
        .await
        .map_err(CabError::Database)?
        .ok_or_else(|| {
            CabError::NotFound(format!("Provider {} not found", resolved.model.provider_id))
        })?;

    let endpoint_candidates = pick_endpoints_for_protocol(&provider, "anthropic");

    let mut primary = resolved.as_primary_model();
    primary.endpoint_candidates = endpoint_candidates;
    primary.model_protocol = "anthropic".to_string();

    // Anthropic path is /v1/messages
    let proxy_req = ProxyRequest {
        body,
        headers: headers.clone(),
        stream,
        path_suffix: "v1/messages".to_string(),
    };

    let result = execute_with_fallback(
        &state.client,
        &state.pool,
        &primary,
        &resolved.fallback_models,
        &proxy_req,
    )
    .await;

    let latency_ms = start.elapsed().as_millis() as i64;

    match result {
        Ok((response, provider_name, model_name)) => {
            let log_id = Uuid::new_v4().to_string();
            let mut input_tokens = 0;
            let mut output_tokens = 0;
            let mut final_response = response;

            if stream {
                let (parts, body) = final_response.into_parts();
                let tracking_stream = crate::protocol::TokenTrackingStream::new(
                    body.into_data_stream(),
                    state.pool.clone(),
                    log_id.clone(),
                );
                final_response =
                    Response::from_parts(parts, axum::body::Body::from_stream(tracking_stream));
            } else {
                let (parts, body) = final_response.into_parts();
                let body_bytes = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::error!("Failed to read response body: {e}");
                        Bytes::new()
                    }
                };
                if let Ok(json_val) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                    if let Some(usage) = json_val.get("usage") {
                        input_tokens = usage
                            .get("input_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        output_tokens = usage
                            .get("output_tokens")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                    }
                }
                final_response = Response::from_parts(parts, axum::body::Body::from(body_bytes));
            }

            let log = RequestLog {
                id: log_id,
                timestamp: Utc::now().to_rfc3339(),
                agent: agent.clone(),
                provider: provider_name,
                model: model_name,
                input_tokens,
                output_tokens,
                total_tokens: input_tokens + output_tokens,
                latency_ms,
                status: 200,
                error: None,
                path: "/v1/messages".to_string(),
                stream,
            };
            let pool = state.pool.clone();
            tokio::spawn(async move {
                if let Err(e) = cab_db::log::insert(&pool, &log).await {
                    tracing::error!("Failed to log request: {e}");
                }
            });
            Ok(final_response)
        }
        Err(e) => {
            let status_code = match &e {
                CabError::ProviderError { status, .. } => *status as i32,
                CabError::Proxy(_) => 502,
                CabError::NotFound(_) => 404,
                _ => 500,
            };
            let log = RequestLog {
                id: Uuid::new_v4().to_string(),
                timestamp: Utc::now().to_rfc3339(),
                agent,
                provider: resolved.provider_name.clone(),
                model: resolved.model.name.clone(),
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                latency_ms,
                status: status_code,
                error: Some(e.to_string()),
                path: "/v1/messages".to_string(),
                stream,
            };
            let pool = state.pool.clone();
            tokio::spawn(async move {
                if let Err(e) = cab_db::log::insert(&pool, &log).await {
                    tracing::error!("Failed to log request: {e}");
                }
            });
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::post;
    use axum::{Json, Router};
    use cab_core::types::{ApiKeyConfig, Model, Provider, ProviderEndpoint};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    struct TestServer {
        base_url: String,
        shutdown: Option<oneshot::Sender<()>>,
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            if let Some(shutdown) = self.shutdown.take() {
                let _ = shutdown.send(());
            }
        }
    }

    async fn spawn_router(app: Router) -> TestServer {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app.into_make_service())
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await
                .unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        TestServer {
            base_url: format!("http://{addr}"),
            shutdown: Some(tx),
        }
    }

    async fn anthropic_success() -> impl axum::response::IntoResponse {
        Json(serde_json::json!({
            "id": "msg_1",
            "model": "native-model",
            "content": [{"type": "text", "text": "anthropic answer"}],
            "usage": {"input_tokens": 4, "output_tokens": 6}
        }))
    }

    async fn anthropic_stream() -> impl axum::response::IntoResponse {
        (
            [("content-type", "text/event-stream")],
            "data: {\"message\":{\"usage\":{\"input_tokens\":4}}}\n\
data: {\"usage\":{\"output_tokens\":6}}\n\
data: [DONE]\n",
        )
    }

    async fn anthropic_error() -> impl axum::response::IntoResponse {
        (StatusCode::BAD_REQUEST, "bad upstream")
    }

    fn provider(base_url: &str) -> Provider {
        Provider {
            id: "provider-1".into(),
            name: "Provider One".into(),
            endpoints: vec![ProviderEndpoint {
                id: "anthropic".into(),
                protocol: "anthropic".into(),
                url: base_url.into(),
                label: None,
                priority: 50,
                enabled: true,
            }],
            api_key: "key-1".into(),
            enabled: true,
            created_at: "now".into(),
            updated_at: "now".into(),
            privacy_policy_url: None,
            terms_of_service_url: None,
            status_page_url: None,
            headquarters: None,
            datacenters: None,
            api_keys: vec![ApiKeyConfig {
                key: "key-1".into(),
                enabled: true,
                subscribed: false,
                quota_reset_at: None,
            }],
            api: None,
            doc: None,
            env: None,
            npm: None,
            model_count: 0,
            catalog_models: vec![],
        }
    }

    fn model() -> Model {
        Model {
            id: "provider-model".into(),
            name: "provider/model".into(),
            display_name: "Provider Model".into(),
            provider_id: "provider-1".into(),
            protocol: "anthropic".into(),
            context_length: 128000,
            input_cost: Some(1.0),
            output_cost: Some(2.0),
            enabled: true,
            overall_intelligence: 50.0,
            coding_index: 50.0,
            agentic_index: 50.0,
            math_index: 50.0,
            created_at: "now".into(),
            updated_at: "now".into(),
            canonical_slug: None,
            hugging_face_id: None,
            created: None,
            description: None,
            architecture: None,
            pricing: None,
            top_provider: None,
            per_request_limits: None,
            supported_parameters: None,
            default_parameters: None,
            supported_voices: None,
            knowledge_cutoff: None,
            expiration_date: None,
            links: None,
        }
    }

    fn state(base_url: &str) -> Arc<GatewayState> {
        let pool = cab_db::InMemoryStore::new();
        {
            let mut data = pool.inner.write().unwrap();
            data.providers
                .insert("provider-1".into(), provider(base_url));
            data.models.insert("provider-model".into(), model());
        }
        Arc::new(GatewayState {
            pool,
            client: reqwest::Client::new(),
        })
    }

    async fn body_json(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn messages_success_returns_response_and_logs_usage() {
        let server =
            spawn_router(Router::new().route("/v1/messages", post(anthropic_success))).await;
        let state = state(&server.base_url);
        let response = handle_messages(
            State(state.clone()),
            HeaderMap::new(),
            Bytes::from_static(
                br#"{"model":"provider/model","messages":[{"role":"user","content":"hi"}]}"#,
            ),
        )
        .await
        .unwrap();
        let json = body_json(response).await;

        assert_eq!(json["content"][0]["text"], "anthropic answer");
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let data = state.pool.inner.read().unwrap();
        let log = data.request_logs.last().unwrap();
        assert_eq!(log.provider, "Provider One");
        assert_eq!(log.model, "provider/model");
        assert_eq!(log.path, "/v1/messages");
        assert_eq!(log.input_tokens, 4);
        assert_eq!(log.output_tokens, 6);
        assert_eq!(log.total_tokens, 10);
        assert_eq!(log.status, 200);
    }

    #[tokio::test]
    async fn messages_stream_tracks_usage_when_body_is_dropped() {
        let server =
            spawn_router(Router::new().route("/v1/messages", post(anthropic_stream))).await;
        let state = state(&server.base_url);
        let response = handle_messages(
            State(state.clone()),
            HeaderMap::new(),
            Bytes::from_static(
                br#"{"model":"provider/model","stream":true,"messages":[{"role":"user","content":"hi"}]}"#,
            ),
        )
        .await
        .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let bytes = to_bytes(response.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("[DONE]"));
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let data = state.pool.inner.read().unwrap();
        let log = data.request_logs.last().unwrap();
        assert_eq!(log.stream, true);
        assert_eq!(log.input_tokens, 4);
        assert_eq!(log.output_tokens, 6);
    }

    #[tokio::test]
    async fn messages_error_logs_provider_failure() {
        let server = spawn_router(Router::new().route("/v1/messages", post(anthropic_error))).await;
        let state = state(&server.base_url);
        let err = handle_messages(
            State(state.clone()),
            HeaderMap::new(),
            Bytes::from_static(
                br#"{"model":"provider/model","messages":[{"role":"user","content":"hi"}]}"#,
            ),
        )
        .await
        .unwrap_err();

        assert!(matches!(err, CabError::ProviderError { status: 400, .. }));
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let data = state.pool.inner.read().unwrap();
        let log = data.request_logs.last().unwrap();
        assert_eq!(log.status, 400);
        assert!(log.error.as_ref().unwrap().contains("bad upstream"));
    }

    #[tokio::test]
    async fn messages_invalid_json_returns_invalid_request_without_log() {
        let state = state("http://127.0.0.1:1");
        let err = handle_messages(
            State(state.clone()),
            HeaderMap::new(),
            Bytes::from_static(b"{"),
        )
        .await
        .unwrap_err();

        assert!(matches!(err, CabError::InvalidRequest(_)));
        assert!(state.pool.inner.read().unwrap().request_logs.is_empty());
    }
}
