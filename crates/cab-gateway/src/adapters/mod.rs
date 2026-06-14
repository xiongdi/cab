//! Protocol adapters for gateway proxy handlers.

mod anthropic;
mod openai_chat;
mod openai_responses;

pub use anthropic::AnthropicAdapter;
pub use openai_chat::OpenAiChatAdapter;
pub use openai_responses::OpenAiResponsesAdapter;

use axum::body::Bytes;
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

/// Wire-format adapter for a single upstream protocol.
pub trait ProtocolAdapter: Send + Sync {
    fn protocol(&self) -> &'static str;
    fn path_suffix(&self) -> &'static str;
    fn log_path(&self) -> &'static str;
    fn default_stream(&self, body: &serde_json::Value) -> bool;
    fn extract_usage(&self, usage: &serde_json::Value) -> (i64, i64);
}

pub async fn handle_proxied_request(
    adapter: &dyn ProtocolAdapter,
    state: Arc<GatewayState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, CabError> {
    let start = std::time::Instant::now();
    let agent = crate::agent_id::extract_agent_id(&headers);

    let body_json: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| CabError::InvalidRequest(format!("Invalid JSON body: {e}")))?;

    let requested_model = body_json
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let stream = body_json
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or_else(|| adapter.default_stream(&body_json));

    let resolved = resolve_route(
        &state.pool,
        &agent,
        requested_model.as_deref(),
        Some(&body_json),
    )
    .await?;

    let provider = cab_db::provider::get_by_id(&state.pool, &resolved.provider_id)
        .await
        .map_err(CabError::Database)?
        .ok_or_else(|| {
            CabError::NotFound(format!("Provider {} not found", resolved.provider_id))
        })?;

    let endpoint_meta = cab_db::endpoint::find_for_model_provider(
        &state.pool,
        &resolved.model.name,
        &resolved.provider_id,
    )
    .await
    .map_err(CabError::Database)?;

    let upstream_protocol = endpoint_meta
        .as_ref()
        .and_then(|ep| ep.upstream_protocol.as_deref())
        .unwrap_or_else(|| adapter.protocol());

    let endpoint_candidates = pick_endpoints_for_protocol(&provider, upstream_protocol);

    let mut primary = resolved.as_primary_model();
    primary.endpoint_candidates = endpoint_candidates;
    primary.model_protocol = adapter.protocol().to_string();

    let proxy_req = ProxyRequest {
        body,
        headers: headers.clone(),
        stream,
        path_suffix: adapter.path_suffix().to_string(),
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
                        axum::body::Bytes::new()
                    }
                };
                if let Ok(json_val) = serde_json::from_slice::<serde_json::Value>(&body_bytes)
                    && let Some(usage) = json_val.get("usage")
                {
                    (input_tokens, output_tokens) = adapter.extract_usage(usage);
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
                path: adapter.log_path().to_string(),
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
                path: adapter.log_path().to_string(),
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
