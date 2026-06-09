//! Protocol conversion utilities between OpenAI and Anthropic formats.

use serde_json::Value;

/// Convert an OpenAI chat completion request body to Anthropic Messages format.
///
/// Maps:
/// - `messages` array with role mappings (system → separate field, assistant/user preserved)
/// - `model` → `model`
/// - `max_tokens` → `max_tokens`
/// - `temperature` → `temperature`
/// - `stream` → `stream`
pub fn openai_to_anthropic(openai_body: &Value) -> Value {
    let mut anthropic = serde_json::Map::new();

    // Model
    if let Some(model) = openai_body.get("model") {
        anthropic.insert("model".to_string(), model.clone());
    }

    // max_tokens (required for Anthropic)
    if let Some(max_tokens) = openai_body.get("max_tokens") {
        anthropic.insert("max_tokens".to_string(), max_tokens.clone());
    } else {
        anthropic.insert("max_tokens".to_string(), Value::Number(4096.into()));
    }

    // Temperature
    if let Some(temp) = openai_body.get("temperature") {
        anthropic.insert("temperature".to_string(), temp.clone());
    }

    // Stream
    if let Some(stream) = openai_body.get("stream") {
        anthropic.insert("stream".to_string(), stream.clone());
    }

    // Messages — split system messages out
    if let Some(Value::Array(messages)) = openai_body.get("messages") {
        let mut system_parts: Vec<String> = Vec::new();
        let mut user_messages: Vec<Value> = Vec::new();

        for msg in messages {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
            match role {
                "system" => {
                    if let Some(content) = msg.get("content").and_then(|c| c.as_str()) {
                        system_parts.push(content.to_string());
                    }
                }
                "user" | "assistant" => {
                    user_messages.push(msg.clone());
                }
                _ => {
                    // Pass through other roles as-is
                    user_messages.push(msg.clone());
                }
            }
        }

        if !system_parts.is_empty() {
            anthropic.insert(
                "system".to_string(),
                Value::String(system_parts.join("\n\n")),
            );
        }
        anthropic.insert("messages".to_string(), Value::Array(user_messages));
    }

    Value::Object(anthropic)
}

/// Convert an Anthropic Messages response to OpenAI chat completion format.
pub fn anthropic_to_openai(anthropic_resp: &Value) -> Value {
    let mut result = serde_json::Map::new();

    result.insert(
        "id".to_string(),
        anthropic_resp
            .get("id")
            .cloned()
            .unwrap_or(Value::String("chatcmpl-converted".to_string())),
    );
    result.insert(
        "object".to_string(),
        Value::String("chat.completion".to_string()),
    );
    result.insert("created".to_string(), Value::Number(0.into()));

    if let Some(model) = anthropic_resp.get("model") {
        result.insert("model".to_string(), model.clone());
    }

    // Convert content blocks to choices
    let mut choices = Vec::new();
    if let Some(Value::Array(content)) = anthropic_resp.get("content") {
        let mut text_parts: Vec<String> = Vec::new();
        for block in content {
            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                text_parts.push(text.to_string());
            }
        }

        let finish_reason = anthropic_resp
            .get("stop_reason")
            .and_then(|r| r.as_str())
            .map(|r| match r {
                "end_turn" => "stop",
                "max_tokens" => "length",
                "stop_sequence" => "stop",
                _ => "stop",
            })
            .unwrap_or("stop");

        choices.push(serde_json::json!({
            "index": 0,
            "message": {
                "role": "assistant",
                "content": text_parts.join(""),
            },
            "finish_reason": finish_reason,
        }));
    }
    result.insert("choices".to_string(), Value::Array(choices));

    // Convert usage
    if let Some(usage) = anthropic_resp.get("usage") {
        result.insert("usage".to_string(), serde_json::json!({
            "prompt_tokens": usage.get("input_tokens").cloned().unwrap_or(Value::Number(0.into())),
            "completion_tokens": usage.get("output_tokens").cloned().unwrap_or(Value::Number(0.into())),
            "total_tokens": 0,
        }));
    }

    Value::Object(result)
}

fn anthropic_content_to_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| {
                block
                    .get("text")
                    .and_then(|t| t.as_str())
                    .or_else(|| block.get("content").and_then(|c| c.as_str()))
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// Convert an Anthropic Messages request to OpenAI chat completion format.
pub fn anthropic_to_openai_chat_request(anthropic_body: &Value) -> Value {
    let mut openai = serde_json::Map::new();

    if let Some(model) = anthropic_body.get("model") {
        openai.insert("model".to_string(), model.clone());
    }
    if let Some(max_tokens) = anthropic_body.get("max_tokens") {
        openai.insert("max_tokens".to_string(), max_tokens.clone());
    }
    if let Some(temp) = anthropic_body.get("temperature") {
        openai.insert("temperature".to_string(), temp.clone());
    }
    if let Some(stream) = anthropic_body.get("stream") {
        openai.insert("stream".to_string(), stream.clone());
    }

    let mut messages = Vec::new();
    if let Some(system) = anthropic_body.get("system") {
        let text = anthropic_content_to_text(system);
        if !text.is_empty() {
            messages.push(serde_json::json!({"role": "system", "content": text}));
        }
    }
    if let Some(Value::Array(msgs)) = anthropic_body.get("messages") {
        for msg in msgs {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
            let content = msg
                .get("content")
                .map(anthropic_content_to_text)
                .unwrap_or_default();
            if content.is_empty() {
                continue;
            }
            messages.push(serde_json::json!({"role": role, "content": content}));
        }
    }
    openai.insert("messages".to_string(), Value::Array(messages));
    Value::Object(openai)
}

/// Convert an OpenAI chat completion response to Anthropic Messages format.
pub fn openai_chat_to_anthropic_messages(openai_resp: &Value) -> Value {
    let id = openai_resp
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("msg-converted");
    let model = openai_resp
        .get("model")
        .cloned()
        .unwrap_or(Value::String("unknown".to_string()));
    let text = openai_resp
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or_default();
    let finish_reason = openai_resp
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("finish_reason"))
        .and_then(|r| r.as_str())
        .map(|r| match r {
            "length" => "max_tokens",
            _ => "end_turn",
        })
        .unwrap_or("end_turn");
    let usage = openai_resp.get("usage");
    let input_tokens = usage
        .and_then(|u| u.get("prompt_tokens"))
        .cloned()
        .unwrap_or(Value::Number(0.into()));
    let output_tokens = usage
        .and_then(|u| u.get("completion_tokens"))
        .cloned()
        .unwrap_or(Value::Number(0.into()));

    serde_json::json!({
        "id": id,
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": [{"type": "text", "text": text}],
        "stop_reason": finish_reason,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": output_tokens
        }
    })
}

/// Convert OpenAI Responses request to chat completion format (tools omitted).
pub fn responses_to_chat_request(responses_body: &Value) -> Value {
    let mut chat = serde_json::Map::new();
    if let Some(model) = responses_body.get("model") {
        chat.insert("model".to_string(), model.clone());
    }
    chat.insert("stream".to_string(), Value::Bool(false));
    if let Some(max_tokens) = responses_body
        .get("max_output_tokens")
        .or_else(|| responses_body.get("max_tokens"))
    {
        chat.insert("max_tokens".to_string(), max_tokens.clone());
    }

    let mut messages = Vec::new();
    if let Some(instructions) = responses_body.get("instructions") {
        let text = match instructions {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if !text.trim().is_empty() {
            messages.push(serde_json::json!({"role": "system", "content": text}));
        }
    }

    match responses_body.get("input") {
        Some(Value::String(s)) if !s.trim().is_empty() => {
            messages.push(serde_json::json!({"role": "user", "content": s}));
        }
        Some(Value::Array(items)) => {
            for item in items {
                if let Some(text) = item.as_str() {
                    if !text.trim().is_empty() {
                        messages.push(serde_json::json!({"role": "user", "content": text}));
                    }
                    continue;
                }
                let role = item.get("role").and_then(|v| v.as_str()).unwrap_or("user");
                let mapped_role = match role {
                    "developer" | "system" => "system",
                    "assistant" => "assistant",
                    "tool" => "tool",
                    _ => "user",
                };
                let content = item
                    .get("content")
                    .map(|c| match c {
                        Value::String(s) => s.clone(),
                        Value::Array(blocks) => blocks
                            .iter()
                            .filter_map(|b| {
                                b.get("text")
                                    .and_then(|t| t.as_str())
                                    .or_else(|| b.as_str())
                            })
                            .collect::<Vec<_>>()
                            .join(""),
                        other => other.to_string(),
                    })
                    .unwrap_or_default();
                if content.trim().is_empty() {
                    continue;
                }
                messages.push(serde_json::json!({"role": mapped_role, "content": content}));
            }
        }
        _ => {}
    }

    if messages.is_empty() {
        messages.push(serde_json::json!({"role": "user", "content": " "}));
    }

    chat.insert("messages".to_string(), Value::Array(messages));
    Value::Object(chat)
}

/// Encode a Responses API payload as SSE events expected by Codex / OpenAI clients.
pub fn responses_to_sse_stream(responses: &Value) -> bytes::Bytes {
    let response_id = responses
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("resp_shim");
    let text = responses
        .get("output_text")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let created = responses
        .get("created")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| chrono::Utc::now().timestamp());
    let model = responses
        .get("model")
        .cloned()
        .unwrap_or(Value::String("unknown".to_string()));
    let item_id = format!("msg_{}", uuid::Uuid::new_v4().simple());

    let mut sse = String::new();

    let created_event = serde_json::json!({
        "type": "response.created",
        "response": {
            "id": response_id,
            "object": "response",
            "created_at": created,
            "status": "in_progress",
            "model": model,
        }
    });
    sse.push_str(&format!(
        "event: response.created\ndata: {}\n\n",
        created_event
    ));

    let item_added_event = serde_json::json!({
        "type": "response.output_item.added",
        "output_index": 0,
        "item": {
            "id": item_id,
            "type": "message",
            "role": "assistant",
            "status": "in_progress",
        }
    });
    sse.push_str(&format!(
        "event: response.output_item.added\ndata: {}\n\n",
        item_added_event
    ));

    if !text.is_empty() {
        let delta_event = serde_json::json!({
            "type": "response.output_text.delta",
            "output_index": 0,
            "content_index": 0,
            "item_id": item_id,
            "delta": text,
        });
        sse.push_str(&format!(
            "event: response.output_text.delta\ndata: {}\n\n",
            delta_event
        ));
    }

    let item_done_event = serde_json::json!({
        "type": "response.output_item.done",
        "output_index": 0,
        "item": {
            "id": item_id,
            "type": "message",
            "role": "assistant",
            "status": "completed",
            "content": [{"type": "output_text", "text": text}],
        }
    });
    sse.push_str(&format!(
        "event: response.output_item.done\ndata: {}\n\n",
        item_done_event
    ));

    let mut completed_response = responses.clone();
    if let Some(obj) = completed_response.as_object_mut() {
        obj.insert("status".to_string(), Value::String("completed".to_string()));
    }
    let completed_event = serde_json::json!({
        "type": "response.completed",
        "response": completed_response,
    });
    sse.push_str(&format!(
        "event: response.completed\ndata: {}\n\n",
        completed_event
    ));

    bytes::Bytes::from(sse)
}

fn openai_content_to_text(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// Convert OpenAI chat completion response to Responses API format.
pub fn chat_to_responses(openai_resp: &Value, model_name: &str) -> Value {
    let text = openai_resp
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or_default();
    let usage = openai_resp.get("usage");
    let input_tokens = usage
        .and_then(|u| u.get("prompt_tokens"))
        .or_else(|| usage.and_then(|u| u.get("input_tokens")))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let output_tokens = usage
        .and_then(|u| u.get("completion_tokens"))
        .or_else(|| usage.and_then(|u| u.get("output_tokens")))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    serde_json::json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "object": "response",
        "created": chrono::Utc::now().timestamp(),
        "model": openai_resp.get("model").cloned().unwrap_or(Value::String(model_name.to_string())),
        "output": [{
            "type": "message",
            "role": "assistant",
            "content": [{"type": "output_text", "text": text}]
        }],
        "output_text": text,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
            "total_tokens": input_tokens + output_tokens
        }
    })
}

use bytes::Bytes;
use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};

pub struct TokenTrackingStream<S> {
    inner: S,
    pool: cab_db::InMemoryStore,
    log_id: String,
    buffer: Vec<u8>,
    input_tokens: i64,
    output_tokens: i64,
}

impl<S> TokenTrackingStream<S> {
    pub fn new(inner: S, pool: cab_db::InMemoryStore, log_id: String) -> Self {
        Self {
            inner,
            pool,
            log_id,
            buffer: Vec::new(),
            input_tokens: 0,
            output_tokens: 0,
        }
    }

    fn process_bytes(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
        while let Some(pos) = self.buffer.iter().position(|&b| b == b'\n') {
            let line_bytes = self.buffer.drain(..=pos).collect::<Vec<u8>>();
            let line = String::from_utf8_lossy(&line_bytes);
            let trimmed = line.trim();
            if trimmed.starts_with("data:") {
                let data_content = trimmed["data:".len()..].trim();
                if data_content != "[DONE]" && !data_content.is_empty() {
                    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(data_content) {
                        // Anthropic message_start event: message.usage.input_tokens
                        if let Some(usage) = json_val.get("message").and_then(|m| m.get("usage")) {
                            if let Some(in_tokens) =
                                usage.get("input_tokens").and_then(|v| v.as_i64())
                            {
                                self.input_tokens = in_tokens;
                            }
                        }
                        // Anthropic message_delta event: usage.output_tokens
                        // OpenAI stream chunk usage: usage.prompt_tokens, usage.completion_tokens
                        if let Some(usage) = json_val.get("usage") {
                            if let Some(in_tokens) =
                                usage.get("prompt_tokens").and_then(|v| v.as_i64())
                            {
                                self.input_tokens = in_tokens;
                            }
                            if let Some(in_tokens) =
                                usage.get("input_tokens").and_then(|v| v.as_i64())
                            {
                                self.input_tokens = in_tokens;
                            }
                            if let Some(out_tokens) =
                                usage.get("completion_tokens").and_then(|v| v.as_i64())
                            {
                                self.output_tokens = out_tokens;
                            }
                            if let Some(out_tokens) =
                                usage.get("output_tokens").and_then(|v| v.as_i64())
                            {
                                self.output_tokens = out_tokens;
                            }
                        }
                    }
                }
            }
        }
    }
}

impl<S> Stream for TokenTrackingStream<S>
where
    S: Stream<Item = Result<Bytes, axum::Error>> + Unpin,
{
    type Item = Result<Bytes, axum::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                this.process_bytes(&bytes);
                Poll::Ready(Some(Ok(bytes)))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<S> Drop for TokenTrackingStream<S> {
    fn drop(&mut self) {
        let pool = self.pool.clone();
        let log_id = self.log_id.clone();
        let input_tokens = self.input_tokens;
        let output_tokens = self.output_tokens;
        if let Ok(mut data) = pool.inner.write() {
            if let Some(log) = data.request_logs.iter_mut().find(|l| l.id == log_id) {
                log.input_tokens = input_tokens;
                log.output_tokens = output_tokens;
                log.total_tokens = input_tokens + output_tokens;
            }
        }
    }
}
