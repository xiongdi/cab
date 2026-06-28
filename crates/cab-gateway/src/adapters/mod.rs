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
use cab_core::types::{RequestLog, UsageRecord};
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

use crate::fallback::{ProxyRequest, execute_with_fallback};
use crate::router::{
    ResolvedRoute, pick_endpoints_for_protocol, resolve_model_on_provider, resolve_route,
};
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

    let (cache_affinity_enabled, cache_request_shaping_enabled) = state
        .pool
        .inner
        .read()
        .map(|d| {
            (
                d.settings.cache_affinity_enabled,
                d.settings.cache_request_shaping_enabled,
            )
        })
        .unwrap_or((true, true));
    let session_key = if cache_affinity_enabled || cache_request_shaping_enabled {
        cab_core::session_key(&agent, &body_json)
    } else {
        None
    };

    // Tool-schema weight diagnostics: record per-tool token estimates so the
    // dashboard can surface which schemas dominate the cacheable prefix.
    if cache_request_shaping_enabled {
        let costs = cab_core::tool_schema_costs(&body_json);
        if !costs.is_empty() {
            state.pool.tool_weights.record(&agent, costs);
        }
    }

    // Prefix-shape diagnostics: attribute likely prompt-cache misses to the
    // region (system / tools) that changed since this session's previous turn.
    if let Some(key) = session_key {
        let reasons = state
            .pool
            .prefix_shapes
            .record(key, cab_core::prefix_shape(&body_json));
        if !reasons.is_empty() {
            tracing::info!(
                agent = %agent,
                session = key,
                changed = %reasons.join(","),
                "prompt-cache prefix changed; upstream cache likely cold for this turn"
            );
        }
    }

    let resolved = resolve_route(
        &state.pool,
        &agent,
        requested_model.as_deref(),
        Some(&body_json),
    )
    .await?;

    let resolved = match session_key {
        Some(key) if cache_affinity_enabled => {
            apply_session_affinity(&state.pool, key, resolved).await
        }
        _ => resolved,
    };

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
        shape_requests: cache_request_shaping_enabled,
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
            let mut cache_read_tokens = 0;
            let mut cache_creation_tokens = 0;
            let mut final_response = response;
            let mut usage_json: Option<serde_json::Value> = None;

            if stream {
                // Stream path: wrap the body in a TokenTrackingStream that
                // accumulates token counts (including cache tokens) from SSE
                // events and persists the full RequestLog on Drop.  Do NOT
                // spawn a separate insert here — that would race with the Drop
                // and routinely wipe out the cache-token data.
                let log = RequestLog {
                    id: log_id,
                    timestamp: Utc::now().to_rfc3339(),
                    agent: agent.clone(),
                    provider: provider_name,
                    model: model_name.clone(),
                    input_tokens: 0,
                    output_tokens: 0,
                    total_tokens: 0,
                    cache_read_tokens: 0,
                    cache_creation_tokens: 0,
                    latency_ms,
                    status: 200,
                    error: None,
                    path: adapter.log_path().to_string(),
                    stream: true,
                };
                let (parts, body) = final_response.into_parts();
                let tracking_stream = crate::protocol::TokenTrackingStream::new(
                    body.into_data_stream(),
                    state.pool.clone(),
                    log,
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
                    (cache_read_tokens, cache_creation_tokens) = extract_cache_tokens(usage);
                    usage_json = Some(usage.clone());
                }
                final_response = Response::from_parts(parts, axum::body::Body::from(body_bytes));

                let log = RequestLog {
                    id: log_id,
                    timestamp: Utc::now().to_rfc3339(),
                    agent: agent.clone(),
                    provider: provider_name,
                    model: model_name.clone(),
                    input_tokens,
                    output_tokens,
                    total_tokens: input_tokens + output_tokens,
                    cache_read_tokens,
                    cache_creation_tokens,
                    latency_ms,
                    status: 200,
                    error: None,
                    path: adapter.log_path().to_string(),
                    stream: false,
                };
                let pool = state.pool.clone();
                let usage_record = build_usage_record(
                    usage_json.as_ref(),
                    &agent,
                    &resolved.provider_id,
                    &model_name,
                    input_tokens,
                    output_tokens,
                    &resolved.model,
                );
                tokio::spawn(async move {
                    if let Err(e) = cab_db::log::insert(&pool, &log).await {
                        tracing::error!("Failed to log request: {e}");
                    }
                    if let Some(record) = usage_record
                        && let Some(sqlite_pool) = pool.sqlite()
                        && let Ok(conn) = sqlite_pool.get()
                        && let Err(e) = cab_db::sqlite::insert_usage(&conn, &record)
                    {
                        tracing::warn!("Failed to record usage: {e}");
                    }
                });
            }

            Ok(final_response)
        }
        Err(e) => {
            state.pool.health.record_failure(&resolved.provider_id);
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
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                latency_ms,
                status: status_code,
                error: Some(cab_core::redact_secrets(&e.to_string())),
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

/// Keep a conversation on its pinned provider+model so the upstream prefix
/// cache keeps hitting across turns. Honors an existing, still-usable pin
/// (demoting the freshly scored route to a fallback); otherwise re-pins to the
/// route just resolved.
async fn apply_session_affinity(
    pool: &cab_db::InMemoryStore,
    key: u64,
    resolved: ResolvedRoute,
) -> ResolvedRoute {
    if let Some(pin) = pool.affinity.get(key) {
        if pin.provider_id == resolved.provider_id && pin.model_name == resolved.model.name {
            return resolved;
        }
        match resolve_model_on_provider(pool, &pin.model_name, &pin.provider_id).await {
            Ok(Some(mut pinned)) => {
                let mut fallbacks = Vec::with_capacity(resolved.fallback_models.len() + 1);
                fallbacks.push(resolved.as_primary_model());
                fallbacks.extend(
                    resolved
                        .fallback_models
                        .into_iter()
                        .filter(|fallback| fallback.provider_id != pinned.provider_id),
                );
                pinned.fallback_models = fallbacks;
                return pinned;
            }
            Ok(None) => {}
            Err(e) => tracing::debug!("session affinity re-resolve failed: {e}"),
        }
    }
    pool.affinity.set(
        key,
        resolved.provider_id.clone(),
        resolved.model.name.clone(),
    );
    resolved
}

/// Pull cache-read / cache-creation token counts out of an upstream `usage`
/// object, covering both Anthropic (`cache_*_input_tokens`) and OpenAI
/// (`prompt_tokens_details.cached_tokens`) shapes.
fn extract_cache_tokens(usage: &serde_json::Value) -> (i64, i64) {
    let cache_read = usage
        .get("cache_read_input_tokens")
        .or_else(|| usage.get("cache_read_tokens"))
        .or_else(|| {
            usage
                .get("prompt_tokens_details")
                .and_then(|details| details.get("cached_tokens"))
        })
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        .max(0);
    let cache_creation = usage
        .get("cache_creation_input_tokens")
        .or_else(|| usage.get("cache_creation_tokens"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        .max(0);
    (cache_read, cache_creation)
}

/// Best-effort request cost in USD from catalog pricing (per-1M-token rates).
///
/// Handles the two cache accounting conventions: Anthropic reports `input_tokens`
/// *excluding* cached tokens, while OpenAI's `prompt_tokens` *includes* them. When
/// the catalog has no explicit cache-read price, cached input is billed at ~10% of
/// the input rate (the typical prefix-cache discount); cache writes at ~1.25x input.
fn compute_cost_usd(
    usage: Option<&serde_json::Value>,
    model: &cab_core::types::Model,
    input_tokens: i64,
    output_tokens: i64,
    cache_read: i64,
    cache_creation: i64,
) -> f64 {
    let per_million =
        |tokens: i64, price: f64| (tokens.max(0) as f64) / 1_000_000.0 * price.max(0.0);
    let input_price = model.input_cost.unwrap_or(0.0);
    let output_price = model.output_cost.unwrap_or(0.0);
    let cache_read_price = cab_core::cache_read_cost_from_model(model).unwrap_or(input_price * 0.1);

    let anthropic_style = usage
        .map(|u| {
            u.get("cache_read_input_tokens").is_some()
                || u.get("cache_creation_input_tokens").is_some()
        })
        .unwrap_or(false);
    let full_input = if anthropic_style {
        input_tokens
    } else {
        (input_tokens - cache_read).max(0)
    };

    per_million(full_input, input_price)
        + per_million(cache_read, cache_read_price)
        + per_million(cache_creation, input_price * 1.25)
        + per_million(output_tokens, output_price)
}

fn build_usage_record(
    usage: Option<&serde_json::Value>,
    agent: &str,
    provider_id: &str,
    model_name: &str,
    input_tokens: i64,
    output_tokens: i64,
    model: &cab_core::types::Model,
) -> Option<UsageRecord> {
    if input_tokens == 0 && output_tokens == 0 {
        return None;
    }

    let (cache_read, cache_creation) = usage.map(extract_cache_tokens).unwrap_or((0, 0));
    let cost_usd = compute_cost_usd(
        usage,
        model,
        input_tokens,
        output_tokens,
        cache_read,
        cache_creation,
    );

    Some(UsageRecord {
        id: Uuid::new_v4().to_string(),
        timestamp: Utc::now().to_rfc3339(),
        provider_id: provider_id.to_string(),
        model_id: model_name.to_string(),
        service_provider_id: provider_id.to_string(),
        agent_id: agent.to_string(),
        input_tokens,
        output_tokens,
        cache_read_tokens: cache_read,
        cache_creation_tokens: cache_creation,
        cost_usd,
        subscription: false,
        request_id: None,
    })
}
