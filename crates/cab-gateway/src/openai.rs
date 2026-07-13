use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use cab_core::CabError;
use std::sync::Arc;

use crate::adapters::{OpenAiChatAdapter, OpenAiResponsesAdapter, handle_proxied_request};
use crate::state::GatewayState;

static OPENAI_CHAT: OpenAiChatAdapter = OpenAiChatAdapter;
static OPENAI_RESPONSES: OpenAiResponsesAdapter = OpenAiResponsesAdapter;

/// POST /v1/chat/completions
///
/// OpenAI-compatible chat completions proxy.
pub async fn handle_chat_completions(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, CabError> {
    handle_proxied_request(&OPENAI_CHAT, state, headers, body).await
}

/// GET /v1/responses — WebSocket upgrade for Codex Responses API.
///
/// Codex uses WebSocket to connect for real-time Responses API.
/// This handler accepts the upgrade, routes the request through CAB's
/// existing HTTP proxy logic, and sends the response back as a WS message.
pub async fn handle_responses_ws(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_socket(socket, state, headers))
}

async fn handle_ws_socket(mut socket: WebSocket, state: Arc<GatewayState>, headers: HeaderMap) {
    // Wait for the first text message (the responses.create event)
    let msg = match socket.recv().await {
        Some(Ok(Message::Text(text))) => text,
        Some(Ok(Message::Close(_))) | None => return,
        Some(Ok(_)) => return, // ignore binary/ping/pong
        Some(Err(e)) => {
            tracing::warn!("WebSocket recv error: {e}");
            return;
        }
    };

    // Parse the responses.create event and extract the inner response request
    let body_bytes = match parse_ws_message(&msg) {
        Ok(body) => body,
        Err(e) => {
            let _ = socket
                .send(Message::Text(
                    format!(r#"{{"type":"error","error":{{"message":"{e}"}}}}"#).into(),
                ))
                .await;
            return;
        }
    };

    // Route through the existing responses proxy logic
    match handle_proxied_request(&OPENAI_RESPONSES, state, headers, body_bytes).await {
        Ok(http_resp) => {
            // Read the response body and send it over WebSocket
            let parts = http_resp.into_parts();
            let body_bytes = axum::body::to_bytes(parts.1, 10 * 1024 * 1024).await;
            match body_bytes {
                Ok(bytes) => {
                    // Send response.created event (using the response body)
                    if let Ok(resp_val) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                        // Send response.created
                        let created = serde_json::json!({
                            "type": "response.created",
                            "response": resp_val,
                        });
                        let _ = socket.send(Message::Text(created.to_string().into())).await;

                        // Send response.completed
                        let completed = serde_json::json!({
                            "type": "response.completed",
                            "response": resp_val,
                        });
                        let _ = socket
                            .send(Message::Text(completed.to_string().into()))
                            .await;
                    } else {
                        // If response is not valid JSON, send it raw
                        let _ = socket
                            .send(Message::Text(
                                String::from_utf8_lossy(&bytes).to_string().into(),
                            ))
                            .await;
                    }
                }
                Err(e) => {
                    let _ = socket
                        .send(Message::Text(
                            format!(r#"{{"type":"error","error":{{"message":"{e}"}}}}"#).into(),
                        ))
                        .await;
                }
            }
        }
        Err(err) => {
            let err_msg = err.to_string().replace('"', "\\\"");
            let _ = socket
                .send(Message::Text(
                    format!(r#"{{"type":"error","error":{{"message":"{err_msg}"}}}}"#).into(),
                ))
                .await;
        }
    }
}

/// Parse a WebSocket text message (responses.create event) and return
/// the inner response body as bytes suitable for handle_proxied_request.
fn parse_ws_message(msg: &str) -> Result<Bytes, String> {
    let json: serde_json::Value =
        serde_json::from_str(msg).map_err(|e| format!("Invalid JSON: {e}"))?;

    // Expect: {"type":"responses.create","response":{...}}
    let resp = json
        .get("response")
        .ok_or_else(|| "Missing 'response' field".to_string())?;

    // Ensure stream=false for WS (already streaming via the socket)
    let mut body = resp.clone();
    if body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        body["stream"] = serde_json::Value::Bool(false);
    }

    serde_json::to_vec(&body)
        .map(Bytes::from)
        .map_err(|e| format!("Serialize error: {e}"))
}

/// POST /v1/responses
///
/// OpenAI Responses API proxy for clients like Codex CLI.
pub async fn handle_responses(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, CabError> {
    handle_proxied_request(&OPENAI_RESPONSES, state, headers, body).await
}

/// GET /v1/models — list all enabled models in OpenAI format.
pub async fn handle_list_models(
    State(state): State<Arc<GatewayState>>,
) -> Result<impl IntoResponse, CabError> {
    let mut models = cab_db::model::list(&state.pool)
        .await
        .map_err(CabError::Database)?;

    let providers = cab_db::provider::list(&state.pool)
        .await
        .map_err(CabError::Database)?;
    let active_provider_ids: std::collections::HashSet<String> = providers
        .iter()
        .filter(|provider| provider.enabled && cab_core::provider_has_configured_key(provider))
        .map(|provider| provider.id.clone())
        .collect();

    // Sort by intelligence descending; models without AA scores sort last.
    models.sort_by(
        |a, b| match (b.overall_intelligence, a.overall_intelligence) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (Some(b_score), Some(a_score)) => b_score
                .partial_cmp(&a_score)
                .unwrap_or(std::cmp::Ordering::Equal),
        },
    );

    let mut model_list = Vec::new();
    for model in models {
        if !model.enabled {
            continue;
        }
        let native_active = active_provider_ids.contains(&model.provider_id);
        let provider_tags =
            cab_db::endpoint::enabled_provider_tags_for_model(&state.pool, &model.name)
                .await
                .map_err(CabError::Database)?;
        if provider_tags.is_empty() {
            continue;
        }
        let reseller_active = provider_tags
            .iter()
            .any(|tag| active_provider_ids.contains(tag));
        if !native_active && !reseller_active {
            continue;
        }

        // Format owned_by to include provider, description, and pricing
        let mut owned_by_parts = Vec::new();
        owned_by_parts.push(model.provider_id.clone());

        if let Some(ref desc) = model.description
            && !desc.trim().is_empty()
            && desc.to_lowercase() != model.provider_id.to_lowercase()
        {
            let clean_desc = if desc.len() > 60 {
                format!("{}...", &desc[..57])
            } else {
                desc.clone()
            };
            owned_by_parts.push(clean_desc);
        }

        if model.input_cost.is_some() || model.output_cost.is_some() {
            owned_by_parts.push(format!(
                "${}/${} per Mtok",
                format_cost(model.input_cost),
                format_cost(model.output_cost)
            ));
        }

        let formatted_owned_by = owned_by_parts.join(" · ");

        model_list.push(codex_compatible_model(
            &model.name,
            &model.display_name,
            &formatted_owned_by,
        ));
        model_list.push(codex_compatible_model(
            &claude_code_discovery_alias(&model.name),
            &model.display_name,
            &formatted_owned_by,
        ));
        if let Some(pos) = model.name.find('/') {
            let suffix = &model.name[pos + 1..];
            model_list.push(codex_compatible_model(
                suffix,
                &model.display_name,
                &formatted_owned_by,
            ));
        }
    }

    for (id, display_name) in claude_code_gateway_model_stubs() {
        model_list.push(codex_compatible_model(
            id,
            display_name,
            "cab · CAB auto-routes requests in auto mode",
        ));
    }

    Ok(Json(serde_json::json!({
        "object": "list",
        "data": model_list,
        "models": model_list,
        "has_more": false,
    })))
}

fn format_cost(cost: Option<f64>) -> String {
    match cost {
        Some(val) => {
            if val == 0.0 {
                "0".to_string()
            } else if val.fract() == 0.0 {
                format!("{:.0}", val)
            } else if (val * 10.0).fract() == 0.0 {
                format!("{:.1}", val)
            } else if (val * 100.0).fract() == 0.0 {
                format!("{:.2}", val)
            } else {
                format!("{:.4}", val)
            }
        }
        None => "-".to_string(),
    }
}

fn claude_code_discovery_alias(model_name: &str) -> String {
    format!("claude/cab/{model_name}")
}

/// Native Claude Code model IDs accepted for gateway validation (requests auto-route in CAB).
fn claude_code_gateway_model_stubs() -> &'static [(&'static str, &'static str)] {
    &[
        ("claude-opus-4-8", "Opus 4.8 (CAB auto)"),
        ("claude-opus-4-8[1m]", "Opus 4.8 1M (CAB auto)"),
        ("claude-opus-4-7", "Opus 4.7 (CAB auto)"),
        ("claude-opus-4-6", "Opus 4.6 (CAB auto)"),
        ("claude-opus-4-5", "Opus 4.5 (CAB auto)"),
        ("claude-sonnet-4-6", "Sonnet 4.6 (CAB auto)"),
        ("claude-sonnet-4-5", "Sonnet 4.5 (CAB auto)"),
        ("claude-haiku-4-5", "Haiku 4.5 (CAB auto)"),
        ("claude/cab/auto", "CAB Auto strategy"),
    ]
}

fn codex_compatible_model(id: &str, display_name: &str, owned_by: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "slug": id,
        "display_name": display_name,
        "name": display_name,
        "object": "model",
        "type": "model",
        "created": 0,
        "created_at": "1970-01-01T00:00:00Z",
        "owned_by": owned_by,
        "supported_reasoning_levels": [],
        "shell_type": "shell_command",
        "visibility": "list",
        "supported_in_api": true,
        "priority": 1,
        "base_instructions": "",
        "supports_reasoning_summaries": false,
        "default_reasoning_summary": "none",
        "support_verbosity": false,
        "supports_parallel_tool_calls": false,
        "supports_image_detail_original": false,
        "context_window": 128000,
        "effective_context_window_percent": 95,
        "experimental_supported_tools": [],
        "used_fallback_model_metadata": false,
        "supports_search_tool": false,
        "truncation_policy": {
            "type": "bytes",
            "limit": 10000000
        },
        "web_search_tool_type": "disabled",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::to_bytes;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::post;
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

    async fn openai_chat_success() -> impl axum::response::IntoResponse {
        Json(serde_json::json!({
            "id": "chatcmpl_1",
            "object": "chat.completion",
            "model": "native-model",
            "choices": [{"message": {"role": "assistant", "content": "chat answer"}}],
            "usage": {"prompt_tokens": 3, "completion_tokens": 5, "total_tokens": 8}
        }))
    }

    async fn openai_responses_success() -> impl axum::response::IntoResponse {
        Json(serde_json::json!({
            "id": "resp_1",
            "object": "response",
            "model": "native-model",
            "output": [{"type": "message", "content": [{"type": "output_text", "text": "response answer"}]}],
            "usage": {"input_tokens": 7, "output_tokens": 11, "total_tokens": 18}
        }))
    }

    async fn openai_chat_stream() -> impl axum::response::IntoResponse {
        (
            [("content-type", "text/event-stream")],
            "data: {\"usage\":{\"prompt_tokens\":2}}\n\
data: {\"usage\":{\"completion_tokens\":4}}\n\
data: [DONE]\n",
        )
    }

    async fn openai_error() -> impl axum::response::IntoResponse {
        (StatusCode::TOO_MANY_REQUESTS, "rate limited")
    }

    fn provider(id: &str, enabled: bool, api_key: &str) -> Provider {
        Provider {
            id: id.into(),
            name: id.into(),
            endpoints: vec![ProviderEndpoint {
                id: format!("{id}-endpoint"),
                protocol: "openai-chat".into(),
                url: "https://example.test/v1".into(),
                label: None,
                priority: 50,
                enabled: true,
            }],
            api_key: api_key.into(),
            enabled,
            created_at: "now".into(),
            updated_at: "now".into(),
            privacy_policy_url: None,
            terms_of_service_url: None,
            status_page_url: None,
            headquarters: None,
            datacenters: None,
            api_keys: vec![ApiKeyConfig {
                key: api_key.into(),
                enabled: !api_key.is_empty(),
                quota_reset_at: None,
            }],
            api: None,
            doc: None,
            env: None,
            npm: None,
            model_count: 0,
            logo: None,
            catalog_models: vec![],
        }
    }

    fn provider_with_endpoint(protocol: &str, base_url: &str) -> Provider {
        Provider {
            id: "provider-1".into(),
            name: "Provider One".into(),
            endpoints: vec![ProviderEndpoint {
                id: protocol.into(),
                protocol: protocol.into(),
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
                quota_reset_at: None,
            }],
            api: None,
            doc: None,
            env: None,
            npm: None,
            model_count: 0,
            logo: None,
            catalog_models: vec![],
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn model(
        id: &str,
        name: &str,
        provider_id: &str,
        enabled: bool,
        overall_intelligence: f64,
        description: Option<&str>,
        input_cost: Option<f64>,
        output_cost: Option<f64>,
    ) -> Model {
        Model {
            id: id.into(),
            name: name.into(),
            display_name: format!("Display {name}"),
            provider_id: provider_id.into(),
            protocol: "openai-chat".into(),
            context_length: 128000,
            input_cost,
            output_cost,
            enabled,
            overall_intelligence: Some(overall_intelligence),
            coding_index: Some(0.0),
            agentic_index: Some(0.0),
            math_index: Some(0.0),
            output_speed_tps: None,
            time_to_first_token_secs: None,
            created_at: "now".into(),
            updated_at: "now".into(),
            canonical_slug: None,
            hugging_face_id: None,
            created: None,
            description: description.map(str::to_string),
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

    fn endpoint(id: &str, model_id: &str, enabled: bool) -> cab_db::endpoint::ModelEndpoint {
        cab_db::endpoint::ModelEndpoint {
            id: id.into(),
            model_id: model_id.into(),
            canonical_slug: model_id.into(),
            provider_name: "provider".into(),
            provider_tag: "provider/tag".into(),
            native_model_id: model_id.into(),
            upstream_protocol: None,
            quantization: "unknown".into(),
            input_cost: Some(0.0),
            output_cost: Some(0.0),
            cache_read_cost: None,
            context_length: Some(128000),
            max_completion_tokens: None,
            status: 1,
            uptime_30m: None,
            uptime_5m: None,
            uptime_1d: None,
            supports_tools: true,
            supports_streaming: true,
            enabled,
            updated_at: "now".into(),
        }
    }

    fn state(base_url: &str, provider_protocol: &str, model_protocol: &str) -> Arc<GatewayState> {
        let pool = cab_db::InMemoryStore::new();
        {
            let mut data = pool.inner.write().unwrap();
            data.providers.insert(
                "provider-1".into(),
                provider_with_endpoint(provider_protocol, base_url),
            );
            data.models.insert(
                "provider-model".into(),
                model(
                    "provider-model",
                    "provider/model",
                    "provider-1",
                    true,
                    50.0,
                    None,
                    Some(1.0),
                    Some(2.0),
                ),
            );
            data.models.get_mut("provider-model").unwrap().protocol = model_protocol.into();
        }
        Arc::new(GatewayState {
            pool,
            client: reqwest::Client::new(),
        })
    }

    async fn response_json(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn chat_completions_success_returns_response_and_logs_usage() {
        let server =
            spawn_router(Router::new().route("/v1/chat/completions", post(openai_chat_success)))
                .await;
        let state = state(&server.base_url, "openai-chat", "openai-chat");

        let response = handle_chat_completions(
            State(state.clone()),
            HeaderMap::new(),
            Bytes::from_static(
                br#"{"model":"provider/model","messages":[{"role":"user","content":"hi"}]}"#,
            ),
        )
        .await
        .unwrap();
        let json = response_json(response).await;

        assert_eq!(json["choices"][0]["message"]["content"], "chat answer");
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let data = state.pool.inner.read().unwrap();
        let log = data.request_logs.last().unwrap();
        assert_eq!(log.provider, "Provider One");
        assert_eq!(log.model, "provider/model");
        assert_eq!(log.path, "/v1/chat/completions");
        assert_eq!(log.input_tokens, 3);
        assert_eq!(log.output_tokens, 5);
        assert_eq!(log.total_tokens, 8);
        assert_eq!(log.status, 200);
    }

    #[tokio::test]
    async fn chat_completions_stream_tracks_usage() {
        let server =
            spawn_router(Router::new().route("/v1/chat/completions", post(openai_chat_stream)))
                .await;
        let state = state(&server.base_url, "openai-chat", "openai-chat");

        let response = handle_chat_completions(
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
        assert!(log.stream);
        assert_eq!(log.input_tokens, 2);
        assert_eq!(log.output_tokens, 4);
    }

    #[tokio::test]
    async fn responses_success_returns_response_and_logs_usage() {
        let server =
            spawn_router(Router::new().route("/v1/responses", post(openai_responses_success)))
                .await;
        let state = state(&server.base_url, "openai-responses", "openai-responses");

        let response = handle_responses(
            State(state.clone()),
            HeaderMap::new(),
            Bytes::from_static(br#"{"model":"provider/model","input":"hi","stream":false}"#),
        )
        .await
        .unwrap();
        let json = response_json(response).await;

        assert_eq!(json["output"][0]["content"][0]["text"], "response answer");
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let data = state.pool.inner.read().unwrap();
        let log = data.request_logs.last().unwrap();
        assert_eq!(log.provider, "Provider One");
        assert_eq!(log.model, "provider/model");
        assert_eq!(log.path, "/v1/responses");
        assert_eq!(log.input_tokens, 7);
        assert_eq!(log.output_tokens, 11);
        assert_eq!(log.total_tokens, 18);
        assert_eq!(log.status, 200);
    }

    #[tokio::test]
    async fn chat_completions_error_logs_provider_failure() {
        let server =
            spawn_router(Router::new().route("/v1/chat/completions", post(openai_error))).await;
        let state = state(&server.base_url, "openai-chat", "openai-chat");

        let err = handle_chat_completions(
            State(state.clone()),
            HeaderMap::new(),
            Bytes::from_static(
                br#"{"model":"provider/model","messages":[{"role":"user","content":"hi"}]}"#,
            ),
        )
        .await
        .unwrap_err();

        assert!(matches!(err, CabError::ProviderError { status: 429, .. }));
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let data = state.pool.inner.read().unwrap();
        let log = data.request_logs.last().unwrap();
        assert_eq!(log.status, 429);
        assert!(log.error.as_ref().unwrap().contains("rate limited"));
    }

    #[tokio::test]
    async fn responses_invalid_json_returns_invalid_request_without_log() {
        let state = state("http://127.0.0.1:1", "openai-responses", "openai-responses");

        let err = handle_responses(
            State(state.clone()),
            HeaderMap::new(),
            Bytes::from_static(b"{"),
        )
        .await
        .unwrap_err();

        assert!(matches!(err, CabError::InvalidRequest(_)));
        assert!(state.pool.inner.read().unwrap().request_logs.is_empty());
    }

    #[tokio::test]
    async fn list_models_returns_enabled_models_aliases_and_codex_fields() {
        let pool = cab_db::InMemoryStore::new();
        {
            let mut data = pool.inner.write().unwrap();
            data.providers
                .insert("p1".into(), provider("p1", true, "key"));
            data.providers
                .insert("p2".into(), provider("p2", false, "key"));
            data.providers.insert("p3".into(), provider("p3", true, ""));
            data.models.insert(
                "high".into(),
                model(
                    "high",
                    "p1/high-model",
                    "p1",
                    true,
                    90.0,
                    Some("A very capable model with a description that is intentionally longer than sixty characters."),
                    Some(1.25),
                    Some(2.0),
                ),
            );
            data.models.insert(
                "low".into(),
                model(
                    "low",
                    "p1/low-model",
                    "p1",
                    true,
                    10.0,
                    Some("p1"),
                    None,
                    None,
                ),
            );
            data.models.insert(
                "disabled".into(),
                model(
                    "disabled",
                    "p1/disabled",
                    "p1",
                    false,
                    100.0,
                    None,
                    None,
                    None,
                ),
            );
            data.models.insert(
                "disabled-provider".into(),
                model(
                    "disabled-provider",
                    "p2/model",
                    "p2",
                    true,
                    100.0,
                    None,
                    None,
                    None,
                ),
            );
            data.models.insert(
                "no-key".into(),
                model("no-key", "p3/model", "p3", true, 100.0, None, None, None),
            );
            data.model_endpoints
                .insert("high-ep".into(), endpoint("high-ep", "p1/high-model", true));
            data.model_endpoints
                .insert("low-ep".into(), endpoint("low-ep", "p1/low-model", true));
            data.model_endpoints.insert(
                "disabled-ep".into(),
                endpoint("disabled-ep", "p1/disabled", true),
            );
            data.model_endpoints.insert(
                "disabled-provider-ep".into(),
                endpoint("disabled-provider-ep", "p2/model", true),
            );
            data.model_endpoints
                .insert("no-key-ep".into(), endpoint("no-key-ep", "p3/model", true));
        }
        let state = Arc::new(GatewayState {
            pool,
            client: reqwest::Client::new(),
        });

        let response = handle_list_models(State(state))
            .await
            .unwrap()
            .into_response();
        let bytes = to_bytes(response.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let data = json["data"].as_array().unwrap();
        let ids = data
            .iter()
            .map(|item| item["id"].as_str().unwrap())
            .collect::<Vec<_>>();

        assert_eq!(
            ids[..6],
            [
                "p1/high-model",
                "claude/cab/p1/high-model",
                "high-model",
                "p1/low-model",
                "claude/cab/p1/low-model",
                "low-model"
            ]
        );
        assert!(ids.contains(&"claude-opus-4-8[1m]"));
        assert!(ids.contains(&"claude/cab/auto"));
        assert_eq!(json["models"], json["data"]);
        assert_eq!(json["has_more"], false);
        assert_eq!(data[0]["slug"], "p1/high-model");
        assert_eq!(data[0]["display_name"], "Display p1/high-model");
        assert!(
            data[0]["owned_by"]
                .as_str()
                .unwrap()
                .contains("$1.25/$2 per Mtok")
        );
        assert_eq!(data[0]["supported_in_api"], true);
        assert_eq!(data[0]["truncation_policy"]["type"], "bytes");
        assert_eq!(data[0]["web_search_tool_type"], "disabled");
    }

    #[test]
    fn format_cost_covers_integer_decimal_zero_and_missing() {
        assert_eq!(format_cost(Some(0.0)), "0");
        assert_eq!(format_cost(Some(2.0)), "2");
        assert_eq!(format_cost(Some(2.5)), "2.5");
        assert_eq!(format_cost(Some(2.25)), "2.25");
        assert_eq!(format_cost(Some(2.125)), "2.1250");
        assert_eq!(format_cost(None), "-");
    }

    #[test]
    fn discovery_alias_and_codex_model_shape_are_stable() {
        assert_eq!(
            claude_code_discovery_alias("provider/model"),
            "claude/cab/provider/model"
        );

        let item = codex_compatible_model("id", "Display", "owner");
        assert_eq!(item["id"], "id");
        assert_eq!(item["name"], "Display");
        assert_eq!(item["owned_by"], "owner");
        assert_eq!(item["context_window"], 128000);
        assert_eq!(item["supports_parallel_tool_calls"], false);
    }
}
