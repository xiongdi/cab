//! OpenAI Responses API over WebSocket (Codex `supports_websockets`).
//!
//! Codex speaks flat `response.create` frames on `GET /v1/responses`, expects the
//! same `response.*` event objects as HTTP SSE (one JSON text frame each), and
//! reuses the socket across warmup (`generate:false`) and many turns. Dropping
//! the socket without a Close handshake triggers Codex's HTTPS fallback.

use axum::body::Bytes;
use axum::extract::ws::{CloseFrame, Message, WebSocket};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use cab_core::CabError;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

use crate::adapters::{OpenAiResponsesAdapter, handle_proxied_request};
use crate::agent_id::extract_agent_id;
use crate::protocol::responses_to_sse_stream;
use crate::state::GatewayState;

static OPENAI_RESPONSES: OpenAiResponsesAdapter = OpenAiResponsesAdapter;

const WS_ONLY_FIELDS: &[&str] = &[
    "type",
    "generate",
    "stream_id",
    "client_metadata",
    "previous_response_id",
    "stream",
    "background",
];

#[derive(Clone, Default)]
struct CachedTurn {
    /// Fully expanded input array used for this turn (for `previous_response_id`).
    full_input: Value,
    output: Vec<Value>,
}

pub async fn handle_ws_socket(mut socket: WebSocket, state: Arc<GatewayState>, headers: HeaderMap) {
    let mut cache: HashMap<String, CachedTurn> = HashMap::new();

    loop {
        let msg = match socket.recv().await {
            Some(Ok(Message::Text(text))) => text,
            Some(Ok(Message::Ping(payload))) => {
                let _ = socket.send(Message::Pong(payload)).await;
                continue;
            }
            Some(Ok(Message::Pong(_))) => continue,
            Some(Ok(Message::Close(_))) | None => break,
            Some(Ok(Message::Binary(_))) => continue,
            Some(Err(e)) => {
                let msg = e.to_string();
                // Codex often drops TCP after the turn without a Close frame.
                if msg.contains("Connection reset") || msg.contains("without closing handshake") {
                    tracing::debug!("WebSocket client disconnected: {msg}");
                } else {
                    tracing::warn!("WebSocket recv error: {msg}");
                }
                break;
            }
        };

        if let Err(e) = handle_ws_turn(&mut socket, &state, &headers, &msg, &mut cache).await {
            tracing::warn!("WebSocket turn error: {e}");
            let _ = send_error(&mut socket, StatusCode::BAD_GATEWAY, &e).await;
            // Keep the socket open so Codex can retry or fall back cleanly.
        }
    }

    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code: 1000,
            reason: "done".into(),
        })))
        .await;
}

async fn handle_ws_turn(
    socket: &mut WebSocket,
    state: &Arc<GatewayState>,
    headers: &HeaderMap,
    msg: &str,
    cache: &mut HashMap<String, CachedTurn>,
) -> Result<(), String> {
    let create = parse_response_create(msg)?;
    let generate = create
        .get("generate")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let model = create
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("cab")
        .to_string();
    let previous_response_id = create
        .get("previous_response_id")
        .and_then(|v| v.as_str())
        .map(str::to_string);

    let new_input = create.get("input").cloned().unwrap_or(json!([]));
    let full_input = match expand_input(cache, previous_response_id.as_deref(), &new_input) {
        Ok(v) => v,
        Err(code) if code.starts_with("previous_response_not_found") => {
            let frame = json!({
                "type": "error",
                "status": 400,
                "error": {
                    "type": "invalid_request_error",
                    "code": "previous_response_not_found",
                    "message": code,
                    "param": "previous_response_id",
                }
            });
            socket
                .send(Message::Text(frame.to_string().into()))
                .await
                .map_err(|e| format!("WebSocket send error: {e}"))?;
            return Ok(());
        }
        Err(e) => return Err(e),
    };

    if !generate {
        let response = synthetic_completed_response(&model, &full_input);
        let response_id = response
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("resp_warmup")
            .to_string();
        cache.insert(
            response_id,
            CachedTurn {
                full_input,
                output: vec![],
            },
        );
        return emit_response_events(socket, &response).await;
    }

    let mut body = create.clone();
    if let Some(obj) = body.as_object_mut() {
        for key in WS_ONLY_FIELDS {
            obj.remove(*key);
        }
        obj.insert("input".to_string(), full_input.clone());
        obj.insert("stream".to_string(), Value::Bool(false));
    }

    let body_bytes = serde_json::to_vec(&body).map_err(|e| format!("Serialize error: {e}"))?;

    let mut proxy_headers = headers.clone();
    ensure_codex_agent_header(&mut proxy_headers, &create);

    let http_resp = handle_proxied_request(
        &OPENAI_RESPONSES,
        Arc::clone(state),
        proxy_headers,
        Bytes::from(body_bytes),
    )
    .await
    .map_err(|e| cab_error_message(&e))?;

    let (parts, body) = http_resp.into_parts();
    let status = parts.status;
    let bytes = axum::body::to_bytes(body, 10 * 1024 * 1024)
        .await
        .map_err(|e| format!("Read response body: {e}"))?;

    if !status.is_success() {
        let message = String::from_utf8_lossy(&bytes).to_string();
        return send_error(socket, status, &message).await;
    }

    let content_type = parts
        .headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if content_type.contains("text/event-stream") {
        return emit_sse_as_ws_frames(socket, &String::from_utf8_lossy(&bytes)).await;
    }

    let mut resp_val: Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid JSON response: {e}"))?;

    // Ensure a stable id for previous_response_id chaining.
    let response_id = resp_val
        .get("id")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| format!("resp_{}", uuid::Uuid::new_v4().simple()));
    if let Some(obj) = resp_val.as_object_mut() {
        obj.entry("id".to_string())
            .or_insert_with(|| Value::String(response_id.clone()));
    }

    let output = resp_val
        .get("output")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    cache.insert(response_id, CachedTurn { full_input, output });

    emit_response_events(socket, &resp_val).await
}

/// Parse a Codex / OpenAI Responses WS create frame into the create payload.
///
/// Accepts:
/// - flat `{ "type":"response.create", "model":..., "input":... }` (current Codex)
/// - nested `{ "type":"responses.create", "response":{...} }` (legacy CAB)
pub fn parse_response_create(msg: &str) -> Result<Value, String> {
    let json: Value = serde_json::from_str(msg).map_err(|e| format!("Invalid JSON: {e}"))?;

    let ty = json.get("type").and_then(|v| v.as_str()).unwrap_or("");
    match ty {
        "response.create" | "" => {
            if json.get("response").is_some() && json.get("model").is_none() {
                // Nested legacy with wrong/missing type.
                json.get("response")
                    .cloned()
                    .ok_or_else(|| "Missing 'response' field".to_string())
            } else {
                Ok(json)
            }
        }
        "responses.create" => json
            .get("response")
            .cloned()
            .ok_or_else(|| "Missing 'response' field".to_string()),
        other => Err(format!("Unsupported WebSocket frame type: {other}")),
    }
}

fn expand_input(
    cache: &HashMap<String, CachedTurn>,
    previous_response_id: Option<&str>,
    new_input: &Value,
) -> Result<Value, String> {
    let Some(prev_id) = previous_response_id else {
        return Ok(normalize_input_array(new_input));
    };

    let prior = cache
        .get(prev_id)
        .ok_or_else(|| format!("previous_response_not_found: no cached response id '{prev_id}'"))?;

    let mut items = Vec::new();
    if let Some(arr) = prior.full_input.as_array() {
        items.extend(arr.iter().cloned());
    }
    for item in &prior.output {
        items.push(item.clone());
    }
    match new_input {
        Value::Array(arr) => items.extend(arr.iter().cloned()),
        Value::String(s) => items.push(json!({"role": "user", "content": s})),
        Value::Null => {}
        other => items.push(other.clone()),
    }
    Ok(Value::Array(items))
}

fn normalize_input_array(input: &Value) -> Value {
    match input {
        Value::Array(_) => input.clone(),
        Value::String(s) => json!([{"role": "user", "content": s}]),
        Value::Null => json!([]),
        other => json!([other]),
    }
}

fn synthetic_completed_response(model: &str, input: &Value) -> Value {
    let id = format!("resp_{}", uuid::Uuid::new_v4().simple());
    json!({
        "id": id,
        "object": "response",
        "created_at": chrono::Utc::now().timestamp(),
        "status": "completed",
        "model": model,
        "output": [],
        "usage": {
            "input_tokens": 0,
            "output_tokens": 0,
            "total_tokens": 0,
        },
        // Keep a hint of the expanded input for debugging; Codex ignores unknown fields.
        "metadata": {"cab_ws_warmup": true, "input_items": input.as_array().map(|a| a.len()).unwrap_or(0)},
    })
}

async fn emit_response_events(socket: &mut WebSocket, response: &Value) -> Result<(), String> {
    let sse = responses_to_sse_stream(response);
    emit_sse_as_ws_frames(socket, &String::from_utf8_lossy(&sse)).await
}

async fn emit_sse_as_ws_frames(socket: &mut WebSocket, sse: &str) -> Result<(), String> {
    for data in sse_data_payloads(sse) {
        socket
            .send(Message::Text(data.into()))
            .await
            .map_err(|e| format!("WebSocket send error: {e}"))?;
    }
    Ok(())
}

/// Extract `data:` payloads from an SSE document (ignore `event:` lines; JSON has `type`).
pub fn sse_data_payloads(sse: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut data_lines: Vec<&str> = Vec::new();
    for line in sse.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            data_lines.push(rest.trim_start());
        } else if line.is_empty() && !data_lines.is_empty() {
            out.push(data_lines.join("\n"));
            data_lines.clear();
        }
    }
    if !data_lines.is_empty() {
        out.push(data_lines.join("\n"));
    }
    out
}

async fn send_error(
    socket: &mut WebSocket,
    status: StatusCode,
    message: &str,
) -> Result<(), String> {
    let truncated: String = message.chars().take(2000).collect();
    let frame = json!({
        "type": "error",
        "status": status.as_u16(),
        "error": {
            "type": "server_error",
            "message": truncated,
        }
    });
    socket
        .send(Message::Text(frame.to_string().into()))
        .await
        .map_err(|e| format!("WebSocket send error: {e}"))
}

fn cab_error_message(err: &CabError) -> String {
    err.to_string()
}

/// CAB's Codex auto mode writes placeholder model `gpt-5.5`. When the WS upgrade
/// lacks a recognizable agent signal, still route as Codex.
fn ensure_codex_agent_header(headers: &mut HeaderMap, create: &Value) {
    if extract_agent_id(headers) != "unknown" {
        return;
    }
    let model = create.get("model").and_then(|v| v.as_str()).unwrap_or("");
    if model == "gpt-5.5"
        && let Ok(value) = HeaderValue::from_str("codex")
    {
        headers.insert("x-cab-agent", value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_flat_response_create() {
        let msg = r#"{"type":"response.create","model":"gpt-5.5","input":[{"role":"user","content":"hi"}],"generate":false}"#;
        let v = parse_response_create(msg).unwrap();
        assert_eq!(v["model"], "gpt-5.5");
        assert_eq!(v["generate"], false);
        assert!(v.get("input").is_some());
    }

    #[test]
    fn parse_nested_legacy_responses_create() {
        let msg = r#"{"type":"responses.create","response":{"model":"x","input":"hi"}}"#;
        let v = parse_response_create(msg).unwrap();
        assert_eq!(v["model"], "x");
    }

    #[test]
    fn expand_input_chains_previous_output() {
        let mut cache = HashMap::new();
        cache.insert(
            "resp_1".into(),
            CachedTurn {
                full_input: json!([{"role":"user","content":"q1"}]),
                output: vec![json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"a1"}]})],
            },
        );
        let expanded = expand_input(
            &cache,
            Some("resp_1"),
            &json!([{"role":"user","content":"q2"}]),
        )
        .unwrap();
        let arr = expanded.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0]["content"], "q1");
        assert_eq!(arr[2]["content"], "q2");
    }

    #[test]
    fn expand_input_missing_previous_errors() {
        let cache = HashMap::new();
        let err = expand_input(&cache, Some("missing"), &json!([])).unwrap_err();
        assert!(err.contains("previous_response_not_found"));
    }

    #[test]
    fn sse_data_payloads_extracts_json_frames() {
        let sse = "\
event: response.created\n\
data: {\"type\":\"response.created\"}\n\
\n\
event: response.completed\n\
data: {\"type\":\"response.completed\"}\n\
\n";
        let frames = sse_data_payloads(sse);
        assert_eq!(frames.len(), 2);
        assert!(frames[0].contains("response.created"));
        assert!(frames[1].contains("response.completed"));
    }

    #[test]
    fn responses_to_sse_includes_completed_for_warmup_shape() {
        let response = synthetic_completed_response("gpt-5.5", &json!([]));
        let sse = String::from_utf8(responses_to_sse_stream(&response).to_vec()).unwrap();
        let frames = sse_data_payloads(&sse);
        assert!(frames.iter().any(|f| f.contains("response.created")));
        assert!(frames.iter().any(|f| f.contains("response.completed")));
    }
}
