use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::Response;
use cab_core::CabError;
use cab_core::types::RequestLog;
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

use crate::fallback::{ProxyRequest, execute_with_fallback};
use crate::router::{ResolvedModel, pick_endpoints_for_protocol, resolve_route};
use crate::state::GatewayState;

/// POST /v1beta/models/{model}:generateContent
/// POST /v1beta/models/{model}:streamGenerateContent
pub async fn handle_model_action(
    State(state): State<Arc<GatewayState>>,
    Path(model_action): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, CabError> {
    let (requested_model, stream) = parse_model_action(&model_action)?;
    handle_gemini_request(state, headers, body, requested_model, stream).await
}

fn parse_model_action(model_action: &str) -> Result<(String, bool), CabError> {
    if let Some(model) = model_action.strip_suffix(":streamGenerateContent") {
        return Ok((model.to_string(), true));
    }
    if let Some(model) = model_action.strip_suffix(":generateContent") {
        return Ok((model.to_string(), false));
    }
    Err(CabError::InvalidRequest(format!(
        "Unsupported Gemini model action: {model_action}"
    )))
}

async fn handle_gemini_request(
    state: Arc<GatewayState>,
    headers: HeaderMap,
    body: Bytes,
    requested_model: String,
    stream: bool,
) -> Result<Response, CabError> {
    let start = std::time::Instant::now();
    let agent = extract_agent(&headers);

    let body_json: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| CabError::InvalidRequest(format!("Invalid JSON body: {e}")))?;

    let resolved = resolve_route(
        &state.pool,
        &agent,
        Some(requested_model.as_str()),
        Some(&body_json),
    )
    .await?;

    let provider = cab_db::provider::get_by_id(&state.pool, &resolved.model.provider_id)
        .await
        .map_err(CabError::Database)?
        .ok_or_else(|| {
            CabError::NotFound(format!("Provider {} not found", resolved.model.provider_id))
        })?;

    let endpoint_candidates = pick_endpoints_for_protocol(&provider, "gemini");
    let upstream_model = target_model_name(&resolved, &requested_model);

    let primary = ResolvedModel {
        model: resolved.model.clone(),
        endpoint_candidates,
        provider_api_key: resolved.provider_api_key.clone(),
        model_protocol: "gemini".to_string(),
        provider_name: resolved.provider_name.clone(),
        provider_routing: resolved.provider_routing.clone(),
    };

    let path_suffix = if stream {
        "streamGenerateContent".to_string()
    } else {
        "generateContent".to_string()
    };

    let proxy_req = ProxyRequest {
        body,
        headers: headers.clone(),
        stream,
        path_suffix,
        url_model: Some(upstream_model),
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
                        Bytes::new()
                    }
                };
                if let Ok(json_val) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
                    if let Some(usage) = json_val.get("usageMetadata") {
                        input_tokens = usage
                            .get("promptTokenCount")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        output_tokens = usage
                            .get("candidatesTokenCount")
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
                path: if stream {
                    "/v1beta/models/:streamGenerateContent".to_string()
                } else {
                    "/v1beta/models/:generateContent".to_string()
                },
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
                path: if stream {
                    "/v1beta/models/:streamGenerateContent".to_string()
                } else {
                    "/v1beta/models/:generateContent".to_string()
                },
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

fn target_model_name(resolved: &crate::router::ResolvedRoute, requested_model: &str) -> String {
    if matches!(
        requested_model,
        "auto" | "cheapest" | "intelligent" | "balanced" | "price"
    ) {
        return resolved.model.name.clone();
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
                requested_model.to_string()
            }
        })
}

fn extract_agent(headers: &HeaderMap) -> String {
    headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|ua| {
            let lower = ua.to_lowercase();
            if lower.contains("antigravity") || lower.starts_with("agy/") || lower.contains(" agy")
            {
                "antigravity".to_string()
            } else if lower.contains("gemini") {
                "antigravity".to_string()
            } else if lower.contains("cursor") {
                "cursor".to_string()
            } else if lower.contains("copilot") {
                "copilot".to_string()
            } else if lower.contains("continue") {
                "continue".to_string()
            } else if lower.contains("cline") {
                "cline".to_string()
            } else if lower.contains("aider") {
                "aider".to_string()
            } else if lower.contains("claude") {
                "claude-code".to_string()
            } else {
                ua.chars().take(64).collect()
            }
        })
        .unwrap_or_else(|| "unknown".to_string())
}
