use axum::response::Response;
use cab_core::types::ApiKeyConfig;
use cab_core::{CabError, ordered_api_keys, resolve_quota_reset_at};
use reqwest::Client;

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

fn target_model_name(resolved: &ResolvedModel, fallback: &str) -> String {
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

    for resolved in all_models {
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

                let mut rewritten_body = request.body.clone();
                if let Ok(json_val) = serde_json::from_slice::<serde_json::Value>(&request.body) {
                    let mut converted_val = if endpoint.protocol == "anthropic"
                        && request.path_suffix == "chat/completions"
                    {
                        crate::protocol::openai_to_anthropic(&json_val)
                    } else if endpoint.protocol != "anthropic" && is_messages_path {
                        crate::protocol::anthropic_to_openai_chat_request(&json_val)
                    } else if endpoint.protocol != "openai-responses" && is_responses_path {
                        crate::protocol::responses_to_chat_request(&json_val)
                    } else {
                        json_val.clone()
                    };

                    if let Some(obj) = converted_val.as_object_mut() {
                        let target_model_name = target_model_name(resolved, &resolved.model.name);
                        obj.insert(
                            "model".to_string(),
                            serde_json::Value::String(target_model_name),
                        );
                        if needs_responses_shim {
                            obj.insert("stream".to_string(), serde_json::Value::Bool(false));
                        }
                    }
                    if let Ok(new_body_bytes) = serde_json::to_vec(&converted_val) {
                        rewritten_body = bytes::Bytes::from(new_body_bytes);
                    }
                }

                let upstream_stream = if needs_responses_shim {
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
                            "Provider {} key returned 429 for model {}, trying next key/model",
                            resolved.provider_name,
                            resolved.model.name
                        );
                        endpoint_error = Some(CabError::ProviderError {
                            status: 429,
                            body,
                            retry_after,
                        });
                        break;
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
                if matches!(
                    &model_error,
                    Some(CabError::ProviderError { status: 429, .. })
                ) {
                    continue 'keys;
                }
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

    if endpoint.protocol == "anthropic"
        && request.path_suffix == "chat/completions"
        && !request.stream
    {
        let (parts, body) = response.into_parts();
        return match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
            Ok(body_bytes) => {
                if let Ok(anthropic_json) = serde_json::from_slice::<serde_json::Value>(&body_bytes)
                {
                    let openai_json = crate::protocol::anthropic_to_openai(&anthropic_json);
                    match serde_json::to_vec(&openai_json) {
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
        };
    }

    if endpoint.protocol != "anthropic" && is_messages_path && !request.stream {
        let (parts, body) = response.into_parts();
        return match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
            Ok(body_bytes) => {
                if let Ok(openai_json) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                    let anthropic_json =
                        crate::protocol::openai_chat_to_anthropic_messages(&openai_json);
                    match serde_json::to_vec(&anthropic_json) {
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
        };
    }

    if endpoint.protocol != "openai-responses" && is_responses_path {
        let (parts, body) = response.into_parts();
        return match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
            Ok(body_bytes) => {
                if let Ok(openai_json) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                    let responses_json =
                        crate::protocol::chat_to_responses(&openai_json, &resolved.model.name);
                    if request.stream {
                        let sse = crate::protocol::responses_to_sse_stream(&responses_json);
                        let mut response = Response::from_parts(parts, axum::body::Body::from(sse));
                        response.headers_mut().insert(
                            axum::http::header::CONTENT_TYPE,
                            "text/event-stream".parse().unwrap(),
                        );
                        response
                    } else {
                        match serde_json::to_vec(&responses_json) {
                            Ok(new_body_bytes) => {
                                Response::from_parts(parts, axum::body::Body::from(new_body_bytes))
                            }
                            Err(_) => {
                                Response::from_parts(parts, axum::body::Body::from(body_bytes))
                            }
                        }
                    }
                } else {
                    Response::from_parts(parts, axum::body::Body::from(body_bytes))
                }
            }
            Err(_) => Response::from_parts(parts, axum::body::Body::empty()),
        };
    }

    response
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
                overall_intelligence: 50.0,
                coding_index: 50.0,
                agentic_index: 50.0,
                math_index: 50.0,
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
