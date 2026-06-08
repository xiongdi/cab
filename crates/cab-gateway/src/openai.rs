use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use cab_core::CabError;
use cab_core::types::RequestLog;
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

use crate::fallback::{ProxyRequest, execute_with_fallback};
use crate::router::{ResolvedModel, pick_endpoints_for_protocol, resolve_route};
use crate::state::GatewayState;

/// POST /v1/chat/completions
///
/// OpenAI-compatible chat completions proxy.
pub async fn handle_chat_completions(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, CabError> {
    let start = std::time::Instant::now();
    let agent = extract_agent(&headers);

    // Parse body to extract the requested model and stream flag
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

    let primary = ResolvedModel {
        model: resolved.model.clone(),
        endpoint_candidates: resolved.endpoint_candidates.clone(),
        provider_api_key: resolved.provider_api_key.clone(),
        model_protocol: resolved.model_protocol.clone(),
        provider_name: resolved.provider_name.clone(),
        provider_routing: resolved.provider_routing.clone(),
    };

    let proxy_req = ProxyRequest {
        body,
        headers: headers.clone(),
        stream,
        path_suffix: "chat/completions".to_string(),
        url_model: None,
    };

    let result = execute_with_fallback(
        &state.client,
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
                        axum::body::Bytes::new()
                    }
                };
                if let Ok(json_val) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                    if let Some(usage) = json_val.get("usage") {
                        input_tokens = usage
                            .get("prompt_tokens")
                            .or_else(|| usage.get("input_tokens"))
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        output_tokens = usage
                            .get("completion_tokens")
                            .or_else(|| usage.get("output_tokens"))
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                    }
                }
                final_response = Response::from_parts(parts, axum::body::Body::from(body_bytes));
            }

            // Log successful request
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
                path: "/v1/chat/completions".to_string(),
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
            // Log failed request
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
                path: "/v1/chat/completions".to_string(),
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

/// POST /v1/responses
///
/// OpenAI Responses API proxy for clients like Codex CLI.
pub async fn handle_responses(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, CabError> {
    let start = std::time::Instant::now();
    let agent = extract_agent(&headers);

    let body_json: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| CabError::InvalidRequest(format!("Invalid JSON body: {e}")))?;

    let requested_model = body_json
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let stream = body_json
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

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

    let endpoint_candidates = pick_endpoints_for_protocol(&provider, "openai-responses");

    let primary = ResolvedModel {
        model: resolved.model.clone(),
        endpoint_candidates,
        provider_api_key: resolved.provider_api_key.clone(),
        model_protocol: "openai-responses".to_string(),
        provider_name: resolved.provider_name.clone(),
        provider_routing: resolved.provider_routing.clone(),
    };

    let proxy_req = ProxyRequest {
        body,
        headers: headers.clone(),
        stream,
        path_suffix: "responses".to_string(),
        url_model: None,
    };

    let result = execute_with_fallback(
        &state.client,
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
                        axum::body::Bytes::new()
                    }
                };
                if let Ok(json_val) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                    if let Some(usage) = json_val.get("usage") {
                        input_tokens = usage
                            .get("input_tokens")
                            .or_else(|| usage.get("prompt_tokens"))
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        output_tokens = usage
                            .get("output_tokens")
                            .or_else(|| usage.get("completion_tokens"))
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
                path: "/v1/responses".to_string(),
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
                path: "/v1/responses".to_string(),
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

/// GET /v1/models — list all enabled models in OpenAI format.
pub async fn handle_list_models(
    State(state): State<Arc<GatewayState>>,
) -> Result<impl IntoResponse, CabError> {
    let mut models = cab_db::model::list(&state.pool)
        .await
        .map_err(CabError::Database)?;

    // Sort models by overall_intelligence descending (highest intelligence/capability first)
    models.sort_by(|a, b| {
        b.overall_intelligence
            .partial_cmp(&a.overall_intelligence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let providers = cab_db::provider::list(&state.pool)
        .await
        .map_err(CabError::Database)?;
    let active_provider_ids: std::collections::HashSet<String> = providers
        .into_iter()
        .filter(|p| p.enabled && !p.api_key.trim().is_empty())
        .map(|p| p.id)
        .collect();

    let mut model_list = Vec::new();
    for model in models {
        if !model.enabled || !active_provider_ids.contains(&model.provider_id) {
            continue;
        }
        let provider_tags =
            cab_db::endpoint::enabled_provider_tags_for_model(&state.pool, &model.name)
                .await
                .map_err(CabError::Database)?;
        if provider_tags.is_empty() {
            continue;
        }

        // Format owned_by to include provider, description, and pricing
        let mut owned_by_parts = Vec::new();
        owned_by_parts.push(model.provider_id.clone());

        if let Some(ref desc) = model.description {
            if !desc.trim().is_empty() && desc.to_lowercase() != model.provider_id.to_lowercase() {
                let clean_desc = if desc.len() > 60 {
                    format!("{}...", &desc[..57])
                } else {
                    desc.clone()
                };
                owned_by_parts.push(clean_desc);
            }
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

/// Extract agent identifier from User-Agent header.
fn extract_agent(headers: &HeaderMap) -> String {
    headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|ua| {
            // Try to extract a meaningful agent name from User-Agent
            if ua.contains("cursor") || ua.contains("Cursor") {
                "cursor".to_string()
            } else if ua.contains("copilot") || ua.contains("Copilot") {
                "copilot".to_string()
            } else if ua.contains("continue") || ua.contains("Continue") {
                "continue".to_string()
            } else if ua.contains("cline") || ua.contains("Cline") {
                "cline".to_string()
            } else if ua.contains("aider") || ua.contains("Aider") {
                "aider".to_string()
            } else if ua.contains("claude") || ua.contains("Claude") {
                "claude-code".to_string()
            } else {
                // Use first 64 chars of User-Agent
                ua.chars().take(64).collect()
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}
