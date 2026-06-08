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
    let agent = extract_agent(&headers);

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
        url_model: None,
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

/// Extract agent identifier from headers — checks x-api-key owner, User-Agent, etc.
fn extract_agent(headers: &HeaderMap) -> String {
    headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|ua| {
            let lower = ua.to_lowercase();
            if lower.contains("cursor") {
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
