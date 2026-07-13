use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Response;
use cab_core::CabError;
use std::sync::Arc;

use crate::adapters::{AnthropicAdapter, handle_proxied_request};
use crate::state::GatewayState;

static ANTHROPIC: AnthropicAdapter = AnthropicAdapter;

/// POST /v1/messages
///
/// Anthropic Messages API proxy.
pub async fn handle_messages(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, CabError> {
    handle_proxied_request(&ANTHROPIC, state, headers, body).await
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
            "type": "message",
            "role": "assistant",
            "content": [{"type": "text", "text": "anthropic answer"}],
            "model": "native-model",
            "usage": {"input_tokens": 5, "output_tokens": 9}
        }))
    }

    async fn anthropic_stream() -> impl axum::response::IntoResponse {
        (
            [("content-type", "text/event-stream")],
            "event: message_start\ndata: {\"message\":{\"usage\":{\"input_tokens\":2}}}\n\n\
event: message_delta\ndata: {\"usage\":{\"output_tokens\":4}}\n\n",
        )
    }

    async fn anthropic_error() -> impl axum::response::IntoResponse {
        (StatusCode::TOO_MANY_REQUESTS, "rate limited")
    }

    fn model(name: &str, provider_id: &str) -> Model {
        Model {
            id: name.into(),
            name: name.into(),
            display_name: name.into(),
            provider_id: provider_id.into(),
            protocol: "anthropic".into(),
            context_length: 128000,
            input_cost: Some(1.0),
            output_cost: Some(2.0),
            enabled: true,
            overall_intelligence: Some(80.0),
            coding_index: Some(80.0),
            agentic_index: Some(80.0),
            math_index: Some(80.0),
            output_speed_tps: None,
            time_to_first_token_secs: None,
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

    fn gateway_state(upstream_url: &str) -> Arc<GatewayState> {
        let pool = cab_db::InMemoryStore::new();
        {
            let mut data = pool.inner.write().unwrap();
            data.providers.insert(
                "provider-1".into(),
                Provider {
                    id: "provider-1".into(),
                    name: "Provider One".into(),
                    endpoints: vec![ProviderEndpoint {
                        id: "anthropic".into(),
                        protocol: "anthropic".into(),
                        url: upstream_url.into(),
                        label: None,
                        priority: 50,
                        enabled: true,
                    }],
                    api_key: "key".into(),
                    enabled: true,
                    created_at: "now".into(),
                    updated_at: "now".into(),
                    privacy_policy_url: None,
                    terms_of_service_url: None,
                    status_page_url: None,
                    headquarters: None,
                    datacenters: None,
                    api_keys: vec![ApiKeyConfig {
                        key: "key".into(),
                        enabled: true,
                        quota_reset_at: None,
                    }],
                    api: None,
                    doc: None,
                    env: None,
                    npm: None,
                    model_count: 1,
                    logo: None,
                    catalog_models: vec![],
                },
            );
            data.models
                .insert("model-1".into(), model("provider/model-1", "provider-1"));
        }
        Arc::new(GatewayState::new(pool))
    }

    #[tokio::test]
    async fn anthropic_messages_proxy_returns_upstream_body() {
        let upstream =
            spawn_router(Router::new().route("/v1/messages", post(anthropic_success))).await;
        let state = gateway_state(&format!("{}/v1", upstream.base_url));
        let body = Bytes::from(
            serde_json::json!({
                "model": "provider/model-1",
                "max_tokens": 1024,
                "messages": [{"role": "user", "content": "hello"}]
            })
            .to_string(),
        );
        let headers = HeaderMap::new();

        let response = handle_messages(axum::extract::State(state), headers, body)
            .await
            .unwrap();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["content"][0]["text"], "anthropic answer");
    }

    #[tokio::test]
    async fn anthropic_messages_stream_tracks_tokens() {
        let upstream =
            spawn_router(Router::new().route("/v1/messages", post(anthropic_stream))).await;
        let state = gateway_state(&format!("{}/v1", upstream.base_url));
        let body = Bytes::from(
            serde_json::json!({
                "model": "provider/model-1",
                "max_tokens": 1024,
                "stream": true,
                "messages": [{"role": "user", "content": "hello"}]
            })
            .to_string(),
        );

        let response = handle_messages(axum::extract::State(state.clone()), HeaderMap::new(), body)
            .await
            .unwrap();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(String::from_utf8_lossy(&bytes).contains("message_delta"));
    }

    #[tokio::test]
    async fn anthropic_messages_maps_upstream_errors() {
        let upstream =
            spawn_router(Router::new().route("/v1/messages", post(anthropic_error))).await;
        let state = gateway_state(&format!("{}/v1", upstream.base_url));
        let body = Bytes::from(
            serde_json::json!({
                "model": "provider/model-1",
                "max_tokens": 1024,
                "messages": [{"role": "user", "content": "hello"}]
            })
            .to_string(),
        );

        let err = handle_messages(axum::extract::State(state), HeaderMap::new(), body)
            .await
            .unwrap_err();
        assert!(matches!(err, CabError::ProviderError { status: 429, .. }));
    }
}
