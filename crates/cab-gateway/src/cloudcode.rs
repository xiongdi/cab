//! Google Cloud Code / Antigravity `v1internal:*` traffic shim for CAB proxy mode.
//!
//! Antigravity CLI talks to `daily-cloudcode-pa.googleapis.com` over HTTPS with
//! proprietary JSON bodies. Proxy mode hijacks DNS to CAB; this module provides
//! minimal compatibility stubs plus routing for generate-style calls.

use axum::body::Body;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use cab_core::CabError;
use cab_core::types::RequestLog;
use chrono::Utc;
use std::sync::Arc;
use uuid::Uuid;

use crate::fallback::{ProxyRequest, execute_with_fallback};
use crate::router::{ResolvedModel, pick_endpoints_for_protocol, resolve_route};
use crate::state::GatewayState;

/// Model ID agy recognizes internally; CAB still routes via agent `auto` strategy at inference time.
const PROXY_MODEL_ID: &str = "gemini-3-flash-preview";

/// Catch-all for hijacked Cloud Code host traffic (path like `/v1internal:generateChat`).
pub async fn handle_v1internal(
    State(state): State<Arc<GatewayState>>,
    axum::extract::Path(path): axum::extract::Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, CabError> {
    let start = std::time::Instant::now();
    let path = path.trim_start_matches('/');
    if !path.starts_with("v1internal:") {
        return Err(CabError::NotFound(format!("Unknown path: {path}")));
    }
    let rpc = path
        .trim_start_matches("v1internal:")
        .split('?')
        .next()
        .unwrap_or(path);

    tracing::info!("CloudCode proxy RPC: {rpc}");

    if matches!(
        rpc,
        "loadCodeAssist"
            | "fetchUserInfo"
            | "fetchAvailableModels"
            | "listModelConfigs"
            | "listExperiments"
            | "fetchAdminControls"
            | "getCodeAssistGlobalUserSetting"
            | "onboardUser"
            | "checkUrlDenylist"
            | "retrieveUserQuotaSummary"
            | "setUserSettings"
    ) {
        return Ok(stub_response(&state, rpc, stub_for_setup(rpc), start));
    }

    if rpc.contains("generate") || rpc.contains("Chat") || rpc.contains("Content") {
        let wants_sse = rpc.starts_with("streamGenerate")
            || headers
                .get(header::ACCEPT)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.contains("text/event-stream"))
                .unwrap_or(false);
        return generate_via_cab(&state, &headers, body, rpc, wants_sse, start).await;
    }

    Ok(stub_response(&state, rpc, serde_json::json!({}), start))
}

fn stub_for_setup(rpc: &str) -> serde_json::Value {
    let reset_time = chrono::Utc::now().to_rfc3339();
    let model_entry = serde_json::json!({
        "displayName": "Gemini 3 Flash Preview",
        "model": PROXY_MODEL_ID,
        "label": "Gemini 3 Flash Preview",
        "modelId": PROXY_MODEL_ID,
        "modelName": PROXY_MODEL_ID,
        "vertexModelId": PROXY_MODEL_ID,
        "quotaInfo": {
            "remainingFraction": 1.0,
            "resetTime": reset_time,
            "isExhausted": false
        },
        "maxTokens": 1048576,
        "maxOutputTokens": 8192,
        "recommended": true,
        "supportsImages": true,
        "supportsThinking": true,
        "modelProvider": "google"
    });
    match rpc {
        "loadCodeAssist" => serde_json::json!({
            "codeAssistEnabled": true,
            "cloudaicompanionProject": "projects/cab-proxy",
            "currentTier": {
                "id": "free-tier",
                "name": "Free",
                "description": "CAB Proxy",
                "isDefault": true,
                "userDefinedCloudaicompanionProject": false
            },
            "allowedTiers": [{
                "id": "free-tier",
                "name": "Free",
                "isDefault": true
            }],
            "planInfo": {
                "planType": "FREE",
                "monthlyPromptCredits": 1000
            },
            "availablePromptCredits": 1000
        }),
        "fetchAvailableModels" => serde_json::json!({
            "models": {
                PROXY_MODEL_ID: model_entry
            },
            "defaultAgentModelId": PROXY_MODEL_ID,
            "commandModelIds": [PROXY_MODEL_ID],
            "tabModelIds": [PROXY_MODEL_ID],
            "agentModelSorts": [{
                "displayName": "Recommended",
                "groups": [{
                    "displayName": "Gemini",
                    "modelIds": [PROXY_MODEL_ID]
                }]
            }]
        }),
        "fetchUserInfo" => serde_json::json!({
            "regionCode": "US",
            "userSettings": {},
            "userTags": []
        }),
        "listExperiments" => serde_json::json!({
            "experimentIds": [],
            "flags": []
        }),
        "listModelConfigs" => serde_json::json!({
            "allowedModelConfigs": [{
                "id": PROXY_MODEL_ID,
                "displayName": "Gemini 3 Flash Preview",
                "descriptionText": "CAB intelligent routing",
                "vertexModelId": PROXY_MODEL_ID
            }],
            "defaultAgentModelConfig": {
                "id": PROXY_MODEL_ID,
                "displayName": "Gemini 3 Flash Preview",
                "vertexModelId": PROXY_MODEL_ID
            }
        }),
        "retrieveUserQuotaSummary" => serde_json::json!({
            "buckets": [{
                "bucketId": "gemini-flash",
                "displayName": "Gemini Flash",
                "remaining": 1.0,
                "window": "DAILY"
            }]
        }),
        "setUserSettings" => serde_json::json!({}),
        "fetchAdminControls" | "checkUrlDenylist" | "getCodeAssistGlobalUserSetting" => {
            serde_json::json!({})
        }
        _ => serde_json::json!({}),
    }
}

async fn generate_via_cab(
    state: &Arc<GatewayState>,
    headers: &HeaderMap,
    body: Bytes,
    rpc: &str,
    wants_sse: bool,
    start: std::time::Instant,
) -> Result<Response, CabError> {
    let body_json: serde_json::Value =
        serde_json::from_slice(&body).unwrap_or_else(|_| serde_json::json!({}));

    let strategy = cab_db::agent::get_by_id(&state.pool, "antigravity")
        .await
        .ok()
        .flatten()
        .and_then(|a| a.model_id)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "auto".to_string());

    let prompt = extract_prompt_text(&body_json).unwrap_or_else(|| " ".to_string());
    let openai_body = serde_json::json!({
        "model": strategy,
        "messages": [{"role": "user", "content": prompt}],
        "stream": false,
        "max_tokens": 4096
    });

    let resolved = resolve_route(
        &state.pool,
        "antigravity",
        Some(strategy.as_str()),
        Some(&openai_body),
    )
    .await?;

    let provider = cab_db::provider::get_by_id(&state.pool, &resolved.model.provider_id)
        .await
        .map_err(CabError::Database)?
        .ok_or_else(|| {
            CabError::NotFound(format!("Provider {} not found", resolved.model.provider_id))
        })?;

    let primary = ResolvedModel {
        model: resolved.model.clone(),
        endpoint_candidates: pick_endpoints_for_protocol(&provider, &resolved.model.protocol),
        provider_api_key: resolved.provider_api_key.clone(),
        model_protocol: resolved.model.protocol.clone(),
        provider_name: resolved.provider_name.clone(),
        provider_routing: resolved.provider_routing.clone(),
    };

    let proxy_req = ProxyRequest {
        body: Bytes::from(serde_json::to_vec(&openai_body).unwrap_or_default()),
        headers: headers.clone(),
        stream: false,
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
            let (_parts, resp_body) = response.into_parts();
            let body_bytes = axum::body::to_bytes(resp_body, 10 * 1024 * 1024)
                .await
                .unwrap_or_default();
            let reply = serde_json::from_slice::<serde_json::Value>(&body_bytes)
                .ok()
                .and_then(|v| {
                    v.get("choices")?
                        .as_array()?
                        .first()?
                        .get("message")?
                        .get("content")?
                        .as_str()
                        .map(|s| s.to_string())
                })
                .unwrap_or_default();

            log_request(
                state,
                "antigravity",
                &provider_name,
                &model_name,
                latency_ms,
                200,
                None,
                &format!("/v1internal:{rpc}"),
            );

            let cloudcode_resp = serde_json::json!({
                "candidates": [{
                    "content": { "role": "model", "parts": [{ "text": reply }] },
                    "finishReason": "STOP"
                }],
                "modelVersion": model_name,
                "responseId": Uuid::new_v4().to_string()
            });

            if wants_sse {
                // Vertex GenerateContentResponse chunks over SSE (agy streamGenerateContent?alt=sse)
                let sse_body = format!(
                    "data: {}\n\ndata: [DONE]\n\n",
                    serde_json::to_string(&cloudcode_resp).unwrap_or_else(|_| "{}".to_string())
                );
                return Ok((
                    StatusCode::OK,
                    [
                        (header::CONTENT_TYPE, "text/event-stream"),
                        (header::CACHE_CONTROL, "no-cache"),
                    ],
                    Body::from(sse_body),
                )
                    .into_response());
            }

            Ok((
                StatusCode::OK,
                [("content-type", "application/json")],
                axum::Json(cloudcode_resp),
            )
                .into_response())
        }
        Err(e) => {
            log_request(
                state,
                "antigravity",
                &resolved.provider_name,
                &resolved.model.name,
                latency_ms,
                502,
                Some(e.to_string()),
                &format!("/v1internal:{rpc}"),
            );
            Err(e)
        }
    }
}

fn extract_prompt_text(value: &serde_json::Value) -> Option<String> {
    const KEYS: &[&str] = &[
        "text",
        "prompt",
        "content",
        "message",
        "input",
        "query",
        "userInput",
        "instruction",
        "parts",
        "contents",
        "request",
    ];

    fn walk(v: &serde_json::Value, keys: &[&str], out: &mut Vec<String>) {
        match v {
            serde_json::Value::String(s) if !s.trim().is_empty() => out.push(s.clone()),
            serde_json::Value::Array(arr) => {
                for item in arr {
                    walk(item, keys, out);
                }
            }
            serde_json::Value::Object(map) => {
                for key in keys {
                    if let Some(val) = map.get(*key) {
                        walk(val, keys, out);
                    }
                }
                for val in map.values() {
                    walk(val, keys, out);
                }
            }
            _ => {}
        }
    }

    let mut parts = Vec::new();
    walk(value, KEYS, &mut parts);
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

fn stub_response(
    state: &Arc<GatewayState>,
    rpc: &str,
    json: serde_json::Value,
    start: std::time::Instant,
) -> Response {
    log_request(
        state,
        "antigravity",
        "CAB-Proxy",
        "stub",
        start.elapsed().as_millis() as i64,
        200,
        None,
        &format!("/v1internal:{rpc}"),
    );
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        axum::Json(json),
    )
        .into_response()
}

fn log_request(
    state: &Arc<GatewayState>,
    agent: &str,
    provider: &str,
    model: &str,
    latency_ms: i64,
    status: i64,
    error: Option<String>,
    path: &str,
) {
    let log = RequestLog {
        id: Uuid::new_v4().to_string(),
        timestamp: Utc::now().to_rfc3339(),
        agent: agent.to_string(),
        provider: provider.to_string(),
        model: model.to_string(),
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        latency_ms,
        status: status as i32,
        error,
        path: path.to_string(),
        stream: false,
    };
    let pool = state.pool.clone();
    tokio::spawn(async move {
        if let Err(e) = cab_db::log::insert(&pool, &log).await {
            tracing::error!("Failed to log cloudcode proxy request: {e}");
        }
    });
}
