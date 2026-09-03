use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use cab_core::CabError;
use cab_core::types::{LogQuery, Model, PaginatedLogs, Provider, RequestLog};

use crate::ApiState;

fn canonical_protocol(protocol: &str) -> Option<String> {
    let protocol = match protocol {
        "openai-chat" => "openai-compatible",
        "anthropic" => "anthropic-messages",
        value => value,
    };
    (!protocol.trim().is_empty()).then(|| protocol.to_string())
}

fn agent_protocol_for_path(path: &str) -> Option<String> {
    match path {
        "/v1/messages" => Some("anthropic-messages".to_string()),
        "/v1/responses" => Some("openai-responses".to_string()),
        "/v1/chat/completions" => Some("openai-compatible".to_string()),
        _ => None,
    }
}

fn native_model_id(model: &Model) -> Option<&str> {
    model
        .links
        .as_ref()
        .and_then(|links| links.get("native_model_id"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
}

fn model_matches(model: &Model, requested: &str) -> bool {
    let requested = requested.trim();
    if requested.is_empty() {
        return false;
    }

    [
        Some(model.name.as_str()),
        Some(model.id.as_str()),
        Some(model.display_name.as_str()),
        native_model_id(model),
    ]
    .into_iter()
    .flatten()
    .any(|candidate| candidate.eq_ignore_ascii_case(requested))
        || model
            .name
            .rsplit_once('/')
            .is_some_and(|(_, native)| native.eq_ignore_ascii_case(requested))
}

fn provider_protocol_for_log(provider: &Provider, log: &RequestLog) -> Option<String> {
    provider
        .models
        .iter()
        .find(|bound| model_matches(&bound.model, &log.model))
        .and_then(|bound| {
            bound
                .model
                .upstream_protocol
                .as_deref()
                .and_then(canonical_protocol)
                .or_else(|| canonical_protocol(&bound.model.protocol))
        })
        .or_else(|| {
            let mut enabled_endpoints = provider
                .endpoints
                .iter()
                .filter(|endpoint| endpoint.enabled);
            let endpoint = enabled_endpoints.next()?;
            enabled_endpoints
                .next()
                .is_none()
                .then(|| canonical_protocol(&endpoint.protocol))
                .flatten()
        })
}

fn enrich_log_protocols(logs: &mut PaginatedLogs, providers: &[Provider]) {
    for log in &mut logs.data {
        if log.agent_protocol.is_none() {
            log.agent_protocol = agent_protocol_for_path(&log.path);
        }
        if log.provider_protocol.is_some() {
            continue;
        }

        let provider = providers.iter().find(|provider| {
            provider.id.eq_ignore_ascii_case(&log.provider)
                || provider.name.eq_ignore_ascii_case(&log.provider)
        });
        log.provider_protocol =
            provider.and_then(|provider| provider_protocol_for_log(provider, log));
    }
}

pub async fn query_logs(
    State(state): State<ApiState>,
    Query(query): Query<LogQuery>,
) -> Result<impl IntoResponse, CabError> {
    let mut logs = cab_db::log::query(&state.pool, &query)
        .await
        .map_err(CabError::Database)?;
    // Older rows predate the protocol columns. Resolve those values from the
    // provider-bound catalog so the UI does not show a misleading dash, while
    // newer rows keep the exact protocol selected by fallback execution.
    let providers = cab_db::provider::list_catalog(&state.pool)
        .await
        .map_err(CabError::Database)?;
    enrich_log_protocols(&mut logs, &providers);
    Ok(Json(logs))
}

pub async fn delete_logs(State(state): State<ApiState>) -> Result<impl IntoResponse, CabError> {
    let deleted = cab_db::log::clear(&state.pool)
        .await
        .map_err(CabError::Database)?;
    Ok(Json(serde_json::json!({ "deleted": deleted })))
}
