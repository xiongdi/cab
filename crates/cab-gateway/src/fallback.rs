use axum::response::Response;
use cab_core::types::ApiKeyConfig;
use cab_core::{CabError, ordered_api_keys, resolve_quota_reset_at};
use reqwest::Client;
use std::collections::HashSet;

use crate::router::ResolvedModel;

/// Request info needed for fallback execution.
pub struct ProxyRequest {
    pub body: bytes::Bytes,
    pub headers: axum::http::HeaderMap,
    pub stream: bool,
    pub path_suffix: String,
}

fn is_messages_path(path_suffix: &str) -> bool {
    path_suffix == "v1/messages" || path_suffix == "messages"
}

fn is_responses_path(path_suffix: &str) -> bool {
    path_suffix == "responses"
}

async fn upstream_native_model_id(
    pool: &cab_db::InMemoryStore,
    resolved: &ResolvedModel,
    fallback: &str,
) -> String {
    if let Ok(Some(ep)) =
        cab_db::endpoint::find_for_model_provider(pool, &resolved.model.name, &resolved.provider_id)
            .await
        && !ep.native_model_id.trim().is_empty()
    {
        return ep.native_model_id.clone();
    }

    resolved
        .model
        .links
        .as_ref()
        .and_then(|links| links.get("native_model_id"))
        .and_then(|native| native.as_str())
        .filter(|native| !native.trim().is_empty())
        .map(|native| native.to_string())
        .unwrap_or_else(|| {
            if let Some(pos) = resolved.model.name.find('/') {
                resolved.model.name[pos + 1..].to_string()
            } else {
                fallback.to_string()
            }
        })
}

fn build_upstream_url(endpoint: &cab_core::types::ProviderEndpoint) -> String {
    let base = endpoint.url.trim_end_matches('/').to_string();
    match endpoint.protocol.as_str() {
        "anthropic" => {
            if base.ends_with("/v1/messages") || base.ends_with("/messages") {
                base
            } else if base.ends_with("/v1") {
                format!("{base}/messages")
            } else {
                format!("{base}/v1/messages")
            }
        }
        "openai-responses" => {
            if base.ends_with("/v1/responses") || base.ends_with("/responses") {
                base
            } else if base.ends_with("/v1") {
                format!("{base}/responses")
            } else {
                format!("{base}/v1/responses")
            }
        }
        _ => {
            if base.ends_with("/v1/chat/completions") || base.ends_with("/chat/completions") {
                base
            } else if base.ends_with("/v1") {
                format!("{base}/chat/completions")
            } else {
                format!("{base}/v1/chat/completions")
            }
        }
    }
}

fn keys_for_attempt(resolved: &ResolvedModel) -> Vec<String> {
    let ordered = ordered_api_keys(&resolved.api_keys);
    if ordered.is_empty() && !resolved.provider_api_key.trim().is_empty() {
        vec![resolved.provider_api_key.clone()]
    } else {
        ordered
    }
}

fn is_subscribed_key(api_keys: &[ApiKeyConfig], key: &str) -> bool {
    api_keys
        .iter()
        .any(|entry| entry.key == key && entry.subscribed)
}

async fn mark_subscribed_key_rate_limited(
    pool: &cab_db::InMemoryStore,
    provider_id: &str,
    api_keys: &[ApiKeyConfig],
    api_key: &str,
    retry_after: Option<chrono::DateTime<chrono::Utc>>,
    body: &str,
) {
    if !is_subscribed_key(api_keys, api_key) {
        return;
    }

    let reset_at = resolve_quota_reset_at(retry_after, body);
    if let Err(err) =
        cab_db::provider::mark_api_key_quota_reset(pool, provider_id, api_key, reset_at).await
    {
        tracing::warn!(
            "Failed to persist subscription quota reset for provider {provider_id}: {err}"
        );
    } else {
        tracing::warn!(
            "Subscription key for provider {provider_id} rate-limited until {}",
            reset_at.to_rfc3339()
        );
    }
}

async fn clear_recovered_quota_if_needed(
    pool: &cab_db::InMemoryStore,
    provider_id: &str,
    api_keys: &[ApiKeyConfig],
    api_key: &str,
) {
    let had_reset = api_keys
        .iter()
        .any(|entry| entry.key == api_key && entry.quota_reset_at.is_some());
    if !had_reset {
        return;
    }

    if let Err(err) = cab_db::provider::clear_api_key_quota_reset(pool, provider_id, api_key).await
    {
        tracing::warn!(
            "Failed to clear subscription quota reset for provider {provider_id}: {err}"
        );
    }
}

/// Try each model in order, cycling through keys and endpoint candidates.
pub async fn execute_with_fallback(
    client: &Client,
    pool: &cab_db::InMemoryStore,
    primary: &ResolvedModel,
    fallbacks: &[ResolvedModel],
    request: &ProxyRequest,
) -> Result<(Response, String, String), CabError> {
    let all_models = std::iter::once(primary).chain(fallbacks.iter());

    let mut last_error = CabError::Proxy("No models available".to_string());
    let mut exhausted_subscribed_providers: HashSet<String> = HashSet::new();

    for resolved in all_models {
        if exhausted_subscribed_providers.contains(&resolved.provider_id) {
            tracing::info!(
                "Skipping model {} — subscribed provider {} already exhausted",
                resolved.model.name,
                resolved.provider_id
            );
            continue;
        }
        if resolved.endpoint_candidates.is_empty() {
            tracing::warn!(
                "No endpoint matches model {} protocol {}",
                resolved.model.name,
                resolved.model_protocol
            );
            last_error = CabError::Proxy(format!(
                "no endpoint matches model protocol {} for model {}",
                resolved.model_protocol, resolved.model.name
            ));
            continue;
        }

        let keys = keys_for_attempt(resolved);
        if keys.is_empty() {
            tracing::warn!(
                "No usable API keys for provider {} model {}",
                resolved.provider_name,
                resolved.model.name
            );
            last_error = CabError::Proxy(format!(
                "no usable API keys for provider {}",
                resolved.provider_name
            ));
            continue;
        }

        let mut model_error = None;
        let upstream_model_id =
            upstream_native_model_id(pool, resolved, &resolved.model.name).await;

        'keys: for api_key in keys {
            let mut endpoint_error = None;

            for endpoint in &resolved.endpoint_candidates {
                let upstream_url = build_upstream_url(endpoint);

                tracing::info!(
                    "Trying model {} via {} [{}] at {}",
                    resolved.model.name,
                    resolved.provider_name,
                    endpoint.protocol,
                    upstream_url
                );

                let is_messages_path = is_messages_path(&request.path_suffix);
                let is_responses_path = is_responses_path(&request.path_suffix);
                let needs_responses_shim =
                    endpoint.protocol != "openai-responses" && is_responses_path;
                let needs_messages_to_responses_shim =
                    endpoint.protocol == "openai-responses" && is_messages_path;

                let client_protocol = if is_messages_path {
                    crate::protocol::PROTOCOL_ANTHROPIC
                } else if is_responses_path {
                    crate::protocol::PROTOCOL_OPENAI_RESPONSES
                } else {
                    crate::protocol::PROTOCOL_OPENAI_CHAT
                };

                let mut rewritten_body = request.body.clone();
                if let Ok(json_val) = serde_json::from_slice::<serde_json::Value>(&request.body) {
                    let mut converted_val = crate::protocol::convert_request(
                        client_protocol,
                        &endpoint.protocol,
                        &json_val,
                    );

                    if let Some(obj) = converted_val.as_object_mut() {
                        obj.insert(
                            "model".to_string(),
                            serde_json::Value::String(upstream_model_id.clone()),
                        );
                        if needs_responses_shim || needs_messages_to_responses_shim {
                            obj.insert("stream".to_string(), serde_json::Value::Bool(false));
                        }
                    }
                    if let Ok(new_body_bytes) = serde_json::to_vec(&converted_val) {
                        rewritten_body = bytes::Bytes::from(new_body_bytes);
                    }
                }

                let upstream_stream = if needs_responses_shim || needs_messages_to_responses_shim {
                    false
                } else {
                    request.stream
                };

                match crate::proxy::proxy_request(
                    client,
                    &upstream_url,
                    &api_key,
                    &endpoint.protocol,
                    rewritten_body,
                    &request.headers,
                    upstream_stream,
                )
                .await
                {
                    Ok(response) => {
                        clear_recovered_quota_if_needed(
                            pool,
                            &resolved.provider_id,
                            &resolved.api_keys,
                            &api_key,
                        )
                        .await;

                        let final_response =
                            convert_success_response(response, resolved, request, endpoint).await;
                        return Ok((
                            final_response,
                            resolved.provider_name.clone(),
                            resolved.model.name.clone(),
                        ));
                    }
                    Err(CabError::ProviderError {
                        status,
                        body,
                        retry_after: _,
                    }) if status == 400 => {
                        tracing::warn!(
                            "Provider {} endpoint {} returned 400 for model {}: {body}",
                            resolved.provider_name,
                            endpoint.url,
                            resolved.model.name
                        );
                        endpoint_error = Some(CabError::ProviderError {
                            status,
                            body: body.clone(),
                            retry_after: None,
                        });
                    }
                    Err(CabError::ProviderError {
                        status: 429,
                        body,
                        retry_after,
                    }) => {
                        mark_subscribed_key_rate_limited(
                            pool,
                            &resolved.provider_id,
                            &resolved.api_keys,
                            &api_key,
                            retry_after,
                            &body,
                        )
                        .await;
                        tracing::warn!(
                            "Provider {} subscription key returned 429 — skipping all models for this provider",
                            resolved.provider_name,
                        );
                        model_error = Some(CabError::ProviderError {
                            status: 429,
                            body,
                            retry_after,
                        });
                        exhausted_subscribed_providers.insert(resolved.provider_id.clone());
                        break 'keys;
                    }
                    Err(CabError::ProviderError {
                        status,
                        body,
                        retry_after: _,
                    }) if status >= 500 => {
                        tracing::warn!(
                            "Provider {} endpoint {} returned {status}, trying next endpoint: {body}",
                            resolved.provider_name,
                            endpoint.url
                        );
                        endpoint_error = Some(CabError::ProviderError {
                            status,
                            body,
                            retry_after: None,
                        });
                    }
                    Err(CabError::Proxy(msg)) => {
                        tracing::warn!(
                            "Provider {} endpoint {} proxy error, trying next endpoint: {msg}",
                            resolved.provider_name,
                            endpoint.url
                        );
                        endpoint_error = Some(CabError::Proxy(msg));
                    }
                    Err(e) => return Err(e),
                }
            }

            if let Some(e) = endpoint_error {
                model_error = Some(e);
            }
        }

        if let Some(e) = model_error {
            last_error = e;
        }
    }

    Err(last_error)
}

async fn convert_success_response(
    response: Response,
    resolved: &ResolvedModel,
    request: &ProxyRequest,
    endpoint: &cab_core::types::ProviderEndpoint,
) -> Response {
    let is_messages_path = is_messages_path(&request.path_suffix);
    let is_responses_path = is_responses_path(&request.path_suffix);
    let client_protocol = if is_messages_path {
        crate::protocol::PROTOCOL_ANTHROPIC
    } else if is_responses_path {
        crate::protocol::PROTOCOL_OPENAI_RESPONSES
    } else {
        crate::protocol::PROTOCOL_OPENAI_CHAT
    };

    if client_protocol == endpoint.protocol {
        return response;
    }

    let is_sse = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("text/event-stream"))
        .unwrap_or(false);

    if request.stream && is_sse {
        let (mut parts, body) = response.into_parts();
        let upstream = body.into_data_stream();
        let converted = crate::protocol::convert_sse_stream(
            &endpoint.protocol,
            client_protocol,
            upstream,
            resolved.model.name.clone(),
        );
        parts.headers.insert(
            axum::http::header::CONTENT_TYPE,
            "text/event-stream".parse().unwrap(),
        );
        return Response::from_parts(parts, axum::body::Body::from_stream(converted));
    }

    let (parts, body) = response.into_parts();
    match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(body_bytes) => {
            if let Ok(upstream_json) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                if request.stream {
                    let sse = crate::protocol::synthesize_sse_from_response(
                        &endpoint.protocol,
                        client_protocol,
                        &upstream_json,
                        resolved.model.name.clone(),
                    );
                    let mut out = Response::from_parts(parts, axum::body::Body::from(sse));
                    out.headers_mut().insert(
                        axum::http::header::CONTENT_TYPE,
                        "text/event-stream".parse().unwrap(),
                    );
                    return out;
                }
                let converted = crate::protocol::convert_response(
                    &endpoint.protocol,
                    client_protocol,
                    &upstream_json,
                    &resolved.model.name,
                );
                match serde_json::to_vec(&converted) {
                    Ok(new_body_bytes) => {
                        Response::from_parts(parts, axum::body::Body::from(new_body_bytes))
                    }
                    Err(_) => Response::from_parts(parts, axum::body::Body::from(body_bytes)),
                }
            } else {
                Response::from_parts(parts, axum::body::Body::from(body_bytes))
            }
        }
        Err(_) => Response::from_parts(parts, axum::body::Body::empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Bytes, to_bytes};
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::post;
    use axum::{Json, Router};
    use cab_core::types::{ApiKeyConfig, Model, ProviderEndpoint};
    use std::sync::{Arc, Mutex};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    #[derive(Clone, Default)]
    struct Recorder {
        bodies: Arc<Mutex<Vec<serde_json::Value>>>,
    }

    struct TestServer {
        base_url: String,
        recorder: Recorder,
        shutdown: Option<oneshot::Sender<()>>,
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            if let Some(shutdown) = self.shutdown.take() {
                let _ = shutdown.send(());
            }
        }
    }

    async fn spawn_router(app: Router, recorder: Recorder) -> TestServer {
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
            recorder,
            shutdown: Some(tx),
        }
    }

    async fn anthropic_success(State(recorder): State<Recorder>, body: Bytes) -> impl IntoResponse {
        let json = serde_json::from_slice::<serde_json::Value>(&body).unwrap();
        recorder.bodies.lock().unwrap().push(json);
        Json(serde_json::json!({
            "id": "msg_1",
            "model": "native-model",
            "content": [{"type": "text", "text": "anthropic answer"}],
            "usage": {"input_tokens": 2, "output_tokens": 3}
        }))
    }

    async fn openai_success(State(recorder): State<Recorder>, body: Bytes) -> impl IntoResponse {
        let json = serde_json::from_slice::<serde_json::Value>(&body).unwrap();
        recorder.bodies.lock().unwrap().push(json);
        Json(serde_json::json!({
            "id": "chatcmpl_1",
            "model": "native-model",
            "choices": [{"message": {"role": "assistant", "content": "openai answer"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 5, "completion_tokens": 7}
        }))
    }

    async fn openai_responses_success(
        State(recorder): State<Recorder>,
        body: Bytes,
    ) -> impl IntoResponse {
        let json = serde_json::from_slice::<serde_json::Value>(&body).unwrap();
        recorder.bodies.lock().unwrap().push(json);
        Json(serde_json::json!({
            "id": "resp_1",
            "model": "native-model",
            "output_text": "responses answer",
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "responses answer"}]
            }],
            "usage": {"input_tokens": 4, "output_tokens": 6}
        }))
    }

    async fn openai_stream() -> impl IntoResponse {
        (
            [("content-type", "text/event-stream")],
            "data: {\"choices\":[{\"delta\":{\"content\":\"stream \"},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\"answer\"},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"completion_tokens\":2}}\n\n\
data: [DONE]\n\n",
        )
    }

    async fn bad_gateway() -> impl IntoResponse {
        (StatusCode::BAD_GATEWAY, "temporary upstream failure")
    }

    async fn rate_limited() -> impl IntoResponse {
        (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "5")],
            "quota",
        )
    }

    fn endpoint(id: &str, protocol: &str, url: &str) -> ProviderEndpoint {
        ProviderEndpoint {
            id: id.into(),
            protocol: protocol.into(),
            url: url.into(),
            label: None,
            priority: 50,
            enabled: true,
        }
    }

    fn model(name: &str, protocol: &str, endpoints: Vec<ProviderEndpoint>) -> ResolvedModel {
        ResolvedModel {
            model: Model {
                id: name.replace('/', "-"),
                name: name.into(),
                display_name: name.into(),
                provider_id: "provider-1".into(),
                protocol: protocol.into(),
                context_length: 128000,
                input_cost: Some(1.0),
                output_cost: Some(1.0),
                enabled: true,
                overall_intelligence: Some(50.0),
                coding_index: Some(50.0),
                agentic_index: Some(50.0),
                math_index: Some(50.0),
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
                links: Some(serde_json::json!({"native_model_id": "native-model"})),
            },
            provider_id: "provider-1".into(),
            endpoint_candidates: endpoints,
            api_keys: vec![ApiKeyConfig {
                key: "key-1".into(),
                enabled: true,
                subscribed: false,
                quota_reset_at: None,
            }],
            provider_api_key: "".into(),
            model_protocol: protocol.into(),
            provider_name: "Provider One".into(),
            provider_routing: vec![],
        }
    }

    async fn response_json(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn chat_request_can_use_anthropic_endpoint_and_convert_response() {
        let recorder = Recorder::default();
        let server = spawn_router(
            Router::new()
                .route("/v1/messages", post(anthropic_success))
                .with_state(recorder.clone()),
            recorder,
        )
        .await;
        let primary = model(
            "provider/model",
            "anthropic",
            vec![endpoint("anthropic", "anthropic", &server.base_url)],
        );
        let request = ProxyRequest {
            body: Bytes::from_static(br#"{"model":"provider/model","messages":[{"role":"system","content":"sys"},{"role":"user","content":"hi"}]}"#),
            headers: HeaderMap::new(),
            stream: false,
            path_suffix: "chat/completions".into(),
        };

        let (response, provider, routed_model) = execute_with_fallback(
            &reqwest::Client::new(),
            &cab_db::InMemoryStore::new(),
            &primary,
            &[],
            &request,
        )
        .await
        .unwrap();
        let json = response_json(response).await;

        assert_eq!(provider, "Provider One");
        assert_eq!(routed_model, "provider/model");
        assert_eq!(json["choices"][0]["message"]["content"], "anthropic answer");
        let bodies = server.recorder.bodies.lock().unwrap();
        assert_eq!(bodies[0]["model"], "native-model");
        assert_eq!(bodies[0]["system"], "sys");
        assert_eq!(bodies[0]["messages"][0]["role"], "user");
    }

    #[tokio::test]
    async fn messages_request_can_use_openai_endpoint_and_convert_response() {
        let recorder = Recorder::default();
        let server = spawn_router(
            Router::new()
                .route("/v1/chat/completions", post(openai_success))
                .with_state(recorder.clone()),
            recorder,
        )
        .await;
        let primary = model(
            "provider/model",
            "openai-chat",
            vec![endpoint("chat", "openai-chat", &server.base_url)],
        );
        let request = ProxyRequest {
            body: Bytes::from_static(
                br#"{"model":"provider/model","system":"sys","messages":[{"role":"user","content":"hi"}]}"#,
            ),
            headers: HeaderMap::new(),
            stream: false,
            path_suffix: "v1/messages".into(),
        };

        let (response, _, _) = execute_with_fallback(
            &reqwest::Client::new(),
            &cab_db::InMemoryStore::new(),
            &primary,
            &[],
            &request,
        )
        .await
        .unwrap();
        let json = response_json(response).await;

        assert_eq!(json["type"], "message");
        assert_eq!(json["content"][0]["text"], "openai answer");
        let bodies = server.recorder.bodies.lock().unwrap();
        assert_eq!(bodies[0]["model"], "native-model");
        assert_eq!(bodies[0]["messages"][0]["role"], "system");
        assert_eq!(bodies[0]["messages"][1]["role"], "user");
    }

    #[tokio::test]
    async fn messages_request_can_use_responses_endpoint_with_input_conversion() {
        let recorder = Recorder::default();
        let server = spawn_router(
            Router::new()
                .route("/v1/responses", post(openai_responses_success))
                .with_state(recorder.clone()),
            recorder,
        )
        .await;
        let primary = model(
            "provider/model",
            "openai-responses",
            vec![endpoint(
                "responses",
                "openai-responses",
                &server.base_url,
            )],
        );
        let request = ProxyRequest {
            body: Bytes::from_static(
                br#"{"model":"provider/model","system":"sys","messages":[{"role":"user","content":"hi"}]}"#,
            ),
            headers: HeaderMap::new(),
            stream: false,
            path_suffix: "v1/messages".into(),
        };

        let (response, _, _) = execute_with_fallback(
            &reqwest::Client::new(),
            &cab_db::InMemoryStore::new(),
            &primary,
            &[],
            &request,
        )
        .await
        .unwrap();
        let json = response_json(response).await;

        assert_eq!(json["content"][0]["text"], "responses answer");
        let bodies = server.recorder.bodies.lock().unwrap();
        assert!(bodies[0].get("input").is_some());
        assert!(bodies[0].get("messages").is_none());
        assert_eq!(bodies[0]["instructions"], "sys");
        assert_eq!(bodies[0]["input"][0]["content"], "hi");
        assert_eq!(bodies[0]["stream"], false);
    }

    #[tokio::test]
    async fn messages_stream_request_can_use_responses_endpoint_with_anthropic_sse() {
        let recorder = Recorder::default();
        let server = spawn_router(
            Router::new()
                .route("/v1/responses", post(openai_responses_success))
                .with_state(recorder.clone()),
            recorder,
        )
        .await;
        let primary = model(
            "provider/model",
            "openai-responses",
            vec![endpoint(
                "responses",
                "openai-responses",
                &server.base_url,
            )],
        );
        let request = ProxyRequest {
            body: Bytes::from_static(
                br#"{"model":"provider/model","messages":[{"role":"user","content":"hi"}],"stream":true}"#,
            ),
            headers: HeaderMap::new(),
            stream: true,
            path_suffix: "v1/messages".into(),
        };

        let (response, _, _) = execute_with_fallback(
            &reqwest::Client::new(),
            &cab_db::InMemoryStore::new(),
            &primary,
            &[],
            &request,
        )
        .await
        .unwrap();

        assert_eq!(response.headers()["content-type"], "text/event-stream");
        let bytes = to_bytes(response.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();
        let sse = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(sse.contains("event: message_start"));
        assert!(sse.contains(r#""text":"responses answer""#));
        assert!(sse.contains("event: message_stop"));
    }

    #[tokio::test]
    async fn messages_stream_request_converts_openai_sse_to_anthropic() {
        let recorder = Recorder::default();
        let server = spawn_router(
            Router::new()
                .route("/v1/chat/completions", post(openai_stream))
                .with_state(recorder.clone()),
            recorder,
        )
        .await;
        let primary = model(
            "provider/model",
            "openai-chat",
            vec![endpoint("chat", "openai-chat", &server.base_url)],
        );
        let request = ProxyRequest {
            body: Bytes::from_static(
                br#"{"model":"provider/model","system":"sys","messages":[{"role":"user","content":"hi"}],"stream":true}"#,
            ),
            headers: HeaderMap::new(),
            stream: true,
            path_suffix: "v1/messages".into(),
        };

        let (response, _, _) = execute_with_fallback(
            &reqwest::Client::new(),
            &cab_db::InMemoryStore::new(),
            &primary,
            &[],
            &request,
        )
        .await
        .unwrap();

        assert_eq!(response.headers()["content-type"], "text/event-stream");
        let bytes = to_bytes(response.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();
        let sse = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(sse.contains("event: message_start"));
        assert!(sse.contains("event: content_block_delta"));
        assert!(sse.contains(r#""text":"stream ""#));
        assert!(sse.contains(r#""text":"answer""#));
        assert!(sse.contains("event: message_stop"));
    }

    #[tokio::test]
    async fn responses_request_can_use_chat_endpoint_with_non_streaming_shim() {
        let recorder = Recorder::default();
        let server = spawn_router(
            Router::new()
                .route("/v1/chat/completions", post(openai_success))
                .with_state(recorder.clone()),
            recorder,
        )
        .await;
        let primary = model(
            "provider/model",
            "openai-chat",
            vec![endpoint("chat", "openai-chat", &server.base_url)],
        );
        let request = ProxyRequest {
            body: Bytes::from_static(
                br#"{"model":"provider/model","input":"hello","stream":true}"#,
            ),
            headers: HeaderMap::new(),
            stream: true,
            path_suffix: "responses".into(),
        };

        let (response, _, _) = execute_with_fallback(
            &reqwest::Client::new(),
            &cab_db::InMemoryStore::new(),
            &primary,
            &[],
            &request,
        )
        .await
        .unwrap();

        assert_eq!(response.headers()["content-type"], "text/event-stream");
        let bytes = to_bytes(response.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();
        let sse = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(sse.contains("response.completed"));
        assert!(sse.contains("openai answer"));
        let bodies = server.recorder.bodies.lock().unwrap();
        assert_eq!(bodies[0]["stream"], false);
        assert_eq!(bodies[0]["messages"][0]["content"], "hello");
    }

    #[tokio::test]
    async fn retries_next_endpoint_after_server_error() {
        let recorder = Recorder::default();
        let server = spawn_router(
            Router::new()
                .route("/bad/v1/chat/completions", post(bad_gateway))
                .route("/good/v1/chat/completions", post(openai_success))
                .with_state(recorder.clone()),
            recorder,
        )
        .await;
        let primary = model(
            "plain-model",
            "openai-chat",
            vec![
                endpoint("bad", "openai-chat", &format!("{}/bad/v1", server.base_url)),
                endpoint(
                    "good",
                    "openai-chat",
                    &format!("{}/good/v1", server.base_url),
                ),
            ],
        );
        let request = ProxyRequest {
            body: Bytes::from_static(
                br#"{"model":"plain-model","messages":[{"role":"user","content":"hi"}]}"#,
            ),
            headers: HeaderMap::new(),
            stream: false,
            path_suffix: "chat/completions".into(),
        };

        let (response, _, _) = execute_with_fallback(
            &reqwest::Client::new(),
            &cab_db::InMemoryStore::new(),
            &primary,
            &[],
            &request,
        )
        .await
        .unwrap();
        let json = response_json(response).await;

        assert_eq!(json["choices"][0]["message"]["content"], "openai answer");
        assert_eq!(server.recorder.bodies.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn subscribed_provider_429_falls_back_to_next_provider() {
        let sub_recorder = Recorder::default();
        let sub_server = spawn_router(
            Router::new()
                .route("/v1/chat/completions", post(rate_limited))
                .with_state(sub_recorder.clone()),
            sub_recorder,
        )
        .await;
        let payg_recorder = Recorder::default();
        let payg_server = spawn_router(
            Router::new()
                .route("/v1/chat/completions", post(openai_success))
                .with_state(payg_recorder.clone()),
            payg_recorder,
        )
        .await;

        let mut primary = model(
            "minimax/MiniMax-M3",
            "openai-chat",
            vec![endpoint("chat", "openai-chat", &sub_server.base_url)],
        );
        primary.provider_id = "minimax".into();
        primary.provider_name = "MiniMax".into();
        primary.api_keys = vec![ApiKeyConfig {
            key: "sub-key".into(),
            enabled: true,
            subscribed: true,
            quota_reset_at: None,
        }];

        let mut fallback = model(
            "deepseek/deepseek-v4-flash",
            "openai-chat",
            vec![endpoint("chat", "openai-chat", &payg_server.base_url)],
        );
        fallback.provider_id = "opencode-go".into();
        fallback.provider_name = "OpenCode Go".into();

        let request = ProxyRequest {
            body: Bytes::from_static(br#"{"model":"auto","messages":[{"role":"user","content":"hi"}]}"#),
            headers: HeaderMap::new(),
            stream: false,
            path_suffix: "chat/completions".into(),
        };

        let (response, provider, routed_model) = execute_with_fallback(
            &reqwest::Client::new(),
            &cab_db::InMemoryStore::new(),
            &primary,
            std::slice::from_ref(&fallback),
            &request,
        )
        .await
        .unwrap();

        assert_eq!(provider, "OpenCode Go");
        assert_eq!(routed_model, "deepseek/deepseek-v4-flash");
        let json = response_json(response).await;
        assert_eq!(json["choices"][0]["message"]["content"], "openai answer");
    }

    #[tokio::test]
    async fn returns_last_error_when_no_endpoint_or_no_key_or_rate_limited() {
        let no_endpoint = model("plain-model", "openai-chat", vec![]);
        let request = ProxyRequest {
            body: Bytes::from_static(b"{}"),
            headers: HeaderMap::new(),
            stream: false,
            path_suffix: "chat/completions".into(),
        };
        let err = execute_with_fallback(
            &reqwest::Client::new(),
            &cab_db::InMemoryStore::new(),
            &no_endpoint,
            &[],
            &request,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CabError::Proxy(message) if message.contains("no endpoint")));

        let mut no_key = model(
            "plain-model",
            "openai-chat",
            vec![endpoint("chat", "openai-chat", "http://127.0.0.1:1")],
        );
        no_key.api_keys.clear();
        no_key.provider_api_key.clear();
        let err = execute_with_fallback(
            &reqwest::Client::new(),
            &cab_db::InMemoryStore::new(),
            &no_key,
            &[],
            &request,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CabError::Proxy(message) if message.contains("no usable API keys")));

        let recorder = Recorder::default();
        let server = spawn_router(
            Router::new()
                .route("/v1/chat/completions", post(rate_limited))
                .with_state(recorder.clone()),
            recorder,
        )
        .await;
        let rate_limited_model = model(
            "plain-model",
            "openai-chat",
            vec![endpoint("chat", "openai-chat", &server.base_url)],
        );
        let err = execute_with_fallback(
            &reqwest::Client::new(),
            &cab_db::InMemoryStore::new(),
            &rate_limited_model,
            &[],
            &request,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CabError::ProviderError { status: 429, .. }));
    }
}
