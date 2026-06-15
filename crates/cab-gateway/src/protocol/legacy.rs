//! Protocol conversion between CAB's three upstream wire formats.
//!
//! Official references:
//! - Anthropic Messages API — <https://docs.anthropic.com/en/api/messages>
//!   SSE: `message_start` → `content_block_*` → `message_delta` → `message_stop`
//!   Content: `text`, `thinking`, `tool_use`, `tool_result`
//!   Tools: `{name, description, input_schema}`; tool_choice: `{type: auto|any|tool, name?}`
//! - OpenAI Chat Completions — <https://platform.openai.com/docs/api-reference/chat>
//!   Stream: `data: {choices[0].delta}` … `data: [DONE]`
//!   Tools: `{type: function, function: {name, description, parameters}}`
//!   tool_choice: `"auto"|"none"|"required"` or `{type: function, function: {name}}`
//! - OpenAI Responses API — <https://developers.openai.com/api/docs/guides/migrate-to-responses>
//!   Input/output are typed Items: `message`, `function_call`, `function_call_output`
//!   Tools: `{type: function, name, description, parameters}` (flat, no nested `function`)
//!
//! Conversion strategy: normalize through explicit field mapping tables; for streaming tool
//! calls accumulate deltas by OpenAI `index` and emit Anthropic `input_json_delta` only after
//! `function.name` is known (OpenAI-compatible providers may send arguments before name).

use bytes::Bytes;
use futures::Stream;
use serde_json::Value;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Convert an OpenAI chat completion request body to Anthropic Messages format.
///
/// Maps:
/// - `messages` array with role mappings (system → separate field, assistant/user preserved)
/// - `model` → `model`
/// - `max_tokens` → `max_tokens`
/// - `temperature` → `temperature`
/// - `stream` → `stream`
pub fn openai_to_anthropic(openai_body: &Value) -> Value {
    super::ir::encode_anthropic_request(&super::ir::decode_openai_chat_request(openai_body))
}

/// Convert an Anthropic Messages response to OpenAI chat completion format.
pub fn anthropic_to_openai(anthropic_resp: &Value) -> Value {
    super::ir::encode_openai_chat_response(&super::ir::decode_anthropic_response(anthropic_resp))
}

/// Convert an Anthropic Messages request to OpenAI chat completion format.
pub fn anthropic_to_openai_chat_request(anthropic_body: &Value) -> Value {
    super::ir::encode_openai_chat_request(&super::ir::decode_anthropic_request(anthropic_body))
}

/// Convert an OpenAI chat completion response to Anthropic Messages format.
pub fn openai_chat_to_anthropic_messages(openai_resp: &Value) -> Value {
    super::ir::encode_anthropic_response(&super::ir::decode_openai_chat_response(openai_resp))
}

pub(crate) fn anthropic_stream_event(event_type: &str, data: Value) -> Bytes {
    Bytes::from(format!("event: {event_type}\ndata: {data}\n\n"))
}

struct StreamingToolCall {
    block_index: u32,
    id: String,
    name: String,
    pending_args: String,
    started: bool,
    stopped: bool,
}

struct OpenAiChatStreamConverter {
    model: String,
    message_id: String,
    line_buffer: String,
    pending: Vec<Bytes>,
    message_started: bool,
    thinking_block_started: bool,
    thinking_block_index: u32,
    text_block_started: bool,
    text_block_index: u32,
    next_block_index: u32,
    tool_calls: std::collections::HashMap<u64, StreamingToolCall>,
    finished: bool,
    output_tokens: u64,
}

impl OpenAiChatStreamConverter {
    fn new(model: String) -> Self {
        Self {
            model,
            message_id: format!("msg_{}", uuid::Uuid::new_v4().simple()),
            line_buffer: String::new(),
            pending: Vec::new(),
            message_started: false,
            thinking_block_started: false,
            thinking_block_index: 0,
            text_block_started: false,
            text_block_index: 0,
            next_block_index: 0,
            tool_calls: std::collections::HashMap::new(),
            finished: false,
            output_tokens: 0,
        }
    }

    fn ensure_message_started(&mut self) {
        if self.message_started {
            return;
        }
        self.message_started = true;
        let data = serde_json::json!({
            "type": "message_start",
            "message": {
                "id": self.message_id,
                "type": "message",
                "role": "assistant",
                "model": self.model,
                "content": [],
                "stop_reason": null,
                "stop_sequence": null,
                "usage": {"input_tokens": 0, "output_tokens": 0}
            }
        });
        self.pending
            .push(anthropic_stream_event("message_start", data));
    }

    fn allocate_block_index(&mut self) -> u32 {
        let index = self.next_block_index;
        self.next_block_index += 1;
        index
    }

    fn push_tool_input_delta(&mut self, openai_index: u64, partial_json: &str) {
        if partial_json.is_empty() {
            return;
        }
        let (block_index, id, name, started) = {
            let Some(tool) = self.tool_calls.get_mut(&openai_index) else {
                return;
            };
            if tool.name.is_empty() {
                return;
            }
            if tool.id.is_empty() {
                tool.id = format!("toolu_{}", uuid::Uuid::new_v4().simple());
            }
            (
                tool.block_index,
                tool.id.clone(),
                tool.name.clone(),
                tool.started,
            )
        };
        if !started {
            if let Some(tool) = self.tool_calls.get_mut(&openai_index) {
                tool.started = true;
            }
            self.ensure_message_started();
            self.pending.push(anthropic_stream_event(
                "content_block_start",
                serde_json::json!({
                    "type": "content_block_start",
                    "index": block_index,
                    "content_block": {
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": {}
                    }
                }),
            ));
        }
        self.output_tokens = self.output_tokens.saturating_add(partial_json.len() as u64);
        self.pending.push(anthropic_stream_event(
            "content_block_delta",
            serde_json::json!({
                "type": "content_block_delta",
                "index": block_index,
                "delta": {"type": "input_json_delta", "partial_json": partial_json}
            }),
        ));
    }

    fn ensure_tool_block_started(&mut self, openai_index: u64) {
        let Some((block_index, id, name)) =
            self.tool_calls.get_mut(&openai_index).and_then(|tool| {
                if tool.started || tool.name.is_empty() {
                    return None;
                }
                if tool.id.is_empty() {
                    tool.id = format!("toolu_{}", uuid::Uuid::new_v4().simple());
                }
                tool.started = true;
                Some((tool.block_index, tool.id.clone(), tool.name.clone()))
            })
        else {
            return;
        };
        self.ensure_message_started();
        self.pending.push(anthropic_stream_event(
            "content_block_start",
            serde_json::json!({
                "type": "content_block_start",
                "index": block_index,
                "content_block": {
                    "type": "tool_use",
                    "id": id,
                    "name": name,
                    "input": {}
                }
            }),
        ));
    }

    fn stop_tool_blocks(&mut self) {
        let mut indices: Vec<u32> = self
            .tool_calls
            .values_mut()
            .filter(|tool| tool.started && !tool.stopped)
            .map(|tool| {
                tool.stopped = true;
                tool.block_index
            })
            .collect();
        indices.sort_unstable();
        for index in indices {
            self.pending.push(anthropic_stream_event(
                "content_block_stop",
                serde_json::json!({"type": "content_block_stop", "index": index}),
            ));
        }
    }

    fn process_tool_call_delta(&mut self, tool_calls: &[Value]) {
        for call in tool_calls {
            let openai_index = call.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
            if !self.tool_calls.contains_key(&openai_index) {
                let block_index = self.allocate_block_index();
                self.tool_calls.insert(
                    openai_index,
                    StreamingToolCall {
                        block_index,
                        id: String::new(),
                        name: String::new(),
                        pending_args: String::new(),
                        started: false,
                        stopped: false,
                    },
                );
            }
            if let Some(id) = call.get("id").and_then(|v| v.as_str()) {
                if let Some(tool) = self.tool_calls.get_mut(&openai_index) {
                    tool.id = id.to_string();
                }
            }
            if let Some(name) = call
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
            {
                if let Some(tool) = self.tool_calls.get_mut(&openai_index) {
                    tool.name = name.to_string();
                    if !tool.pending_args.is_empty() {
                        let buffered = std::mem::take(&mut tool.pending_args);
                        self.push_tool_input_delta(openai_index, &buffered);
                    }
                }
            }
            if let Some(args) = call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|a| a.as_str())
            {
                if self
                    .tool_calls
                    .get(&openai_index)
                    .map(|t| t.name.is_empty())
                    .unwrap_or(true)
                {
                    if let Some(tool) = self.tool_calls.get_mut(&openai_index) {
                        tool.pending_args.push_str(args);
                    }
                } else {
                    self.push_tool_input_delta(openai_index, args);
                }
            } else {
                self.ensure_tool_block_started(openai_index);
            }
        }
    }

    fn ensure_thinking_block_started(&mut self) {
        if self.thinking_block_started {
            return;
        }
        self.ensure_message_started();
        self.thinking_block_index = self.allocate_block_index();
        self.thinking_block_started = true;
        self.pending.push(anthropic_stream_event(
            "content_block_start",
            serde_json::json!({
                "type": "content_block_start",
                "index": self.thinking_block_index,
                "content_block": {"type": "thinking", "thinking": ""}
            }),
        ));
    }

    fn ensure_text_block_started(&mut self) {
        if self.text_block_started {
            return;
        }
        self.ensure_message_started();
        self.text_block_index = self.allocate_block_index();
        self.text_block_started = true;
        self.pending.push(anthropic_stream_event(
            "content_block_start",
            serde_json::json!({
                "type": "content_block_start",
                "index": self.text_block_index,
                "content_block": {"type": "text", "text": ""}
            }),
        ));
    }

    fn push_thinking_delta(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.ensure_thinking_block_started();
        self.output_tokens = self.output_tokens.saturating_add(text.len() as u64);
        self.pending.push(anthropic_stream_event(
            "content_block_delta",
            serde_json::json!({
                "type": "content_block_delta",
                "index": self.thinking_block_index,
                "delta": {"type": "thinking_delta", "thinking": text}
            }),
        ));
    }

    fn push_text_delta(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.ensure_text_block_started();
        self.output_tokens = self.output_tokens.saturating_add(text.len() as u64);
        self.pending.push(anthropic_stream_event(
            "content_block_delta",
            serde_json::json!({
                "type": "content_block_delta",
                "index": self.text_block_index,
                "delta": {"type": "text_delta", "text": text}
            }),
        ));
    }

    fn finish_with_reason(&mut self, finish_reason: Option<&str>, usage: Option<&Value>) {
        if self.finished {
            return;
        }
        self.finished = true;

        if !self.message_started {
            self.ensure_message_started();
        }
        if !self.thinking_block_started && !self.text_block_started && self.tool_calls.is_empty() {
            self.ensure_text_block_started();
        }

        let pending_indices: Vec<u64> = self.tool_calls.keys().copied().collect();
        for idx in pending_indices {
            let flush = self
                .tool_calls
                .get(&idx)
                .map(|tool| {
                    (
                        !tool.pending_args.is_empty() && !tool.name.is_empty(),
                        tool.pending_args.clone(),
                    )
                })
                .unwrap_or((false, String::new()));
            if flush.0 {
                self.push_tool_input_delta(idx, &flush.1);
                if let Some(tool) = self.tool_calls.get_mut(&idx) {
                    tool.pending_args.clear();
                }
            }
        }

        self.stop_tool_blocks();

        let stop_reason = match finish_reason {
            Some("length") => "max_tokens",
            Some("tool_calls") => "tool_use",
            _ => "end_turn",
        };

        let output_tokens = usage
            .and_then(|u| u.get("completion_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(self.output_tokens);

        if self.thinking_block_started {
            self.pending.push(anthropic_stream_event(
                "content_block_stop",
                serde_json::json!({"type": "content_block_stop", "index": self.thinking_block_index}),
            ));
        }
        if self.text_block_started {
            self.pending.push(anthropic_stream_event(
                "content_block_stop",
                serde_json::json!({"type": "content_block_stop", "index": self.text_block_index}),
            ));
        }
        self.pending.push(anthropic_stream_event(
            "message_delta",
            serde_json::json!({
                "type": "message_delta",
                "delta": {"stop_reason": stop_reason, "stop_sequence": null},
                "usage": {"output_tokens": output_tokens}
            }),
        ));
        self.pending.push(anthropic_stream_event(
            "message_stop",
            serde_json::json!({"type": "message_stop"}),
        ));
    }

    fn process_openai_line(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() || line.starts_with(':') {
            return;
        }
        let Some(payload) = line.strip_prefix("data:").map(str::trim) else {
            return;
        };
        if payload == "[DONE]" {
            self.finish_with_reason(None, None);
            return;
        }
        let Ok(chunk) = serde_json::from_str::<Value>(payload) else {
            return;
        };
        let choice = chunk.get("choices").and_then(|c| c.get(0));
        let delta = choice.and_then(|c| c.get("delta"));
        if let Some(reasoning) = delta
            .and_then(|d| d.get("reasoning_content"))
            .and_then(|c| c.as_str())
        {
            self.push_thinking_delta(reasoning);
        }
        if let Some(text) = delta
            .and_then(|d| d.get("content"))
            .and_then(|c| c.as_str())
        {
            self.push_text_delta(text);
        }
        if let Some(Value::Array(tool_calls)) = delta.and_then(|d| d.get("tool_calls")) {
            self.process_tool_call_delta(tool_calls);
        }
        let finish_reason = choice
            .and_then(|c| c.get("finish_reason"))
            .and_then(|r| r.as_str());
        if finish_reason.is_some() {
            self.finish_with_reason(finish_reason, chunk.get("usage"));
        }
    }

    fn push_input(&mut self, bytes: &[u8]) {
        self.line_buffer.push_str(&String::from_utf8_lossy(bytes));
        while let Some(pos) = self.line_buffer.find('\n') {
            let line = self.line_buffer[..pos].to_string();
            self.line_buffer.drain(..=pos);
            self.process_openai_line(&line);
        }
    }

    fn finish(&mut self) {
        if !self.line_buffer.trim().is_empty() {
            let line = self.line_buffer.trim().to_string();
            self.line_buffer.clear();
            self.process_openai_line(&line);
        }
        self.finish_with_reason(None, None);
    }

    fn pop_output(&mut self) -> Option<Bytes> {
        if self.pending.is_empty() {
            None
        } else {
            Some(self.pending.remove(0))
        }
    }
}

/// Transform an upstream OpenAI chat SSE stream into Anthropic Messages SSE events.
pub fn transform_openai_chat_sse_to_anthropic<S, E>(
    upstream: S,
    model: String,
) -> impl Stream<Item = Result<Bytes, E>>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
{
    let mut converter = OpenAiChatStreamConverter::new(model);
    let mut upstream = upstream;
    let mut finished_upstream = false;

    futures::stream::poll_fn(move |cx| {
        loop {
            if let Some(out) = converter.pop_output() {
                return Poll::Ready(Some(Ok(out)));
            }
            if finished_upstream {
                return Poll::Ready(None);
            }

            match Pin::new(&mut upstream).poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => converter.push_input(&bytes),
                Poll::Ready(Some(Err(err))) => return Poll::Ready(Some(Err(err))),
                Poll::Ready(None) => {
                    converter.finish();
                    finished_upstream = true;
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    })
}

/// Convert OpenAI Responses request to chat completion format.
pub fn responses_to_chat_request(responses_body: &Value) -> Value {
    super::ir::encode_openai_chat_request(&super::ir::decode_responses_request(responses_body))
}

/// Convert OpenAI chat completion request to Responses API format.
pub fn chat_request_to_responses(chat_body: &Value) -> Value {
    super::ir::encode_responses_request(&super::ir::decode_openai_chat_request(chat_body))
}

/// Convert Anthropic Messages request directly to OpenAI Responses format.
pub fn anthropic_to_responses_request(anthropic_body: &Value) -> Value {
    super::ir::encode_responses_request(&super::ir::decode_anthropic_request(anthropic_body))
}

/// Convert OpenAI Responses request directly to Anthropic Messages format.
pub fn responses_to_anthropic_request(responses_body: &Value) -> Value {
    super::ir::encode_anthropic_request(&super::ir::decode_responses_request(responses_body))
}

/// Extract assistant text from a Responses API payload.
pub fn responses_text_from_body(responses: &Value) -> String {
    if let Some(text) = responses.get("output_text").and_then(|t| t.as_str()) {
        return text.to_string();
    }

    responses
        .get("output")
        .and_then(|output| output.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("content").and_then(|content| match content {
                        Value::String(s) => Some(s.clone()),
                        Value::Array(blocks) => Some(
                            blocks
                                .iter()
                                .filter_map(|block| {
                                    block.get("text").and_then(|t| t.as_str()).or_else(|| {
                                        block.get("output_text").and_then(|t| t.as_str())
                                    })
                                })
                                .collect::<Vec<_>>()
                                .join(""),
                        ),
                        _ => None,
                    })
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

/// Convert Responses API payload to Anthropic Messages format.
pub fn responses_to_anthropic_messages(responses: &Value) -> Value {
    super::ir::encode_anthropic_response(&super::ir::decode_responses_response(responses))
}

/// Encode a Responses API payload as Anthropic Messages SSE (for streaming clients).
pub fn responses_to_anthropic_sse_stream(responses: &Value, model: String) -> bytes::Bytes {
    let text = responses_text_from_body(responses);
    let message_id = format!("msg_{}", uuid::Uuid::new_v4().simple());
    let output_tokens = responses
        .get("usage")
        .and_then(|u| u.get("output_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(text.len() as u64);

    let mut chunks = Vec::new();
    chunks.push(anthropic_stream_event(
        "message_start",
        serde_json::json!({
            "type": "message_start",
            "message": {
                "id": message_id,
                "type": "message",
                "role": "assistant",
                "model": model,
                "content": [],
                "stop_reason": null,
                "stop_sequence": null,
                "usage": {"input_tokens": 0, "output_tokens": 0}
            }
        }),
    ));
    chunks.push(anthropic_stream_event(
        "content_block_start",
        serde_json::json!({
            "type": "content_block_start",
            "index": 0,
            "content_block": {"type": "text", "text": ""}
        }),
    ));
    if !text.is_empty() {
        chunks.push(anthropic_stream_event(
            "content_block_delta",
            serde_json::json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": {"type": "text_delta", "text": text}
            }),
        ));
    }
    chunks.push(anthropic_stream_event(
        "content_block_stop",
        serde_json::json!({"type": "content_block_stop", "index": 0}),
    ));
    chunks.push(anthropic_stream_event(
        "message_delta",
        serde_json::json!({
            "type": "message_delta",
            "delta": {"stop_reason": "end_turn", "stop_sequence": null},
            "usage": {"output_tokens": output_tokens}
        }),
    ));
    chunks.push(anthropic_stream_event(
        "message_stop",
        serde_json::json!({"type": "message_stop"}),
    ));

    let mut sse = Vec::new();
    for chunk in chunks {
        sse.extend_from_slice(&chunk);
    }
    bytes::Bytes::from(sse)
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

/// Convert OpenAI chat completion response to Responses API format.
pub fn chat_to_responses(openai_resp: &Value, model_name: &str) -> Value {
    super::ir::encode_responses_response(
        &super::ir::decode_openai_chat_response(openai_resp),
        model_name,
    )
}

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
            if let Some(data_content) = trimmed.strip_prefix("data:") {
                let data_content = data_content.trim();
                if data_content != "[DONE]"
                    && !data_content.is_empty()
                    && let Ok(json_val) = serde_json::from_str::<serde_json::Value>(data_content)
                {
                    // Anthropic message_start event: message.usage.input_tokens
                    if let Some(usage) = json_val.get("message").and_then(|m| m.get("usage"))
                        && let Some(in_tokens) = usage.get("input_tokens").and_then(|v| v.as_i64())
                    {
                        self.input_tokens = in_tokens;
                    }
                    // Anthropic message_delta event: usage.output_tokens
                    // OpenAI stream chunk usage: usage.prompt_tokens, usage.completion_tokens
                    if let Some(usage) = json_val.get("usage") {
                        if let Some(in_tokens) = usage.get("prompt_tokens").and_then(|v| v.as_i64())
                        {
                            self.input_tokens = in_tokens;
                        }
                        if let Some(in_tokens) = usage.get("input_tokens").and_then(|v| v.as_i64())
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
        if let Ok(mut data) = pool.inner.write()
            && let Some(log) = data.request_logs.iter_mut().find(|l| l.id == log_id)
        {
            log.input_tokens = input_tokens;
            log.output_tokens = output_tokens;
            log.total_tokens = input_tokens + output_tokens;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cab_core::types::RequestLog;
    use futures::StreamExt;

    #[test]
    fn openai_to_anthropic_moves_system_and_defaults_max_tokens() {
        let body = serde_json::json!({
            "model": "gpt-test",
            "temperature": 0.2,
            "stream": true,
            "messages": [
                {"role": "system", "content": "be terse"},
                {"role": "system", "content": "be exact"},
                {"role": "user", "content": "hello"},
                {"role": "tool", "content": "tool payload"}
            ]
        });

        let converted = openai_to_anthropic(&body);

        assert_eq!(converted["model"], "gpt-test");
        assert_eq!(converted["max_tokens"], 4096);
        assert_eq!(converted["temperature"], 0.2);
        assert_eq!(converted["stream"], true);
        assert_eq!(converted["system"], "be terse\n\nbe exact");
        assert_eq!(converted["messages"].as_array().unwrap().len(), 2);
        assert_eq!(converted["messages"][0]["role"], "user");
        assert_eq!(
            converted["messages"][1]["content"][0]["type"],
            "tool_result"
        );
    }

    #[test]
    fn anthropic_to_openai_maps_content_finish_reason_and_usage() {
        let body = serde_json::json!({
            "id": "msg_1",
            "model": "claude-test",
            "content": [
                {"type": "text", "text": "hello "},
                {"type": "text", "text": "world"}
            ],
            "stop_reason": "max_tokens",
            "usage": {"input_tokens": 3, "output_tokens": 5}
        });

        let converted = anthropic_to_openai(&body);

        assert_eq!(converted["id"], "msg_1");
        assert_eq!(converted["object"], "chat.completion");
        assert_eq!(converted["model"], "claude-test");
        assert_eq!(converted["choices"][0]["message"]["content"], "hello world");
        assert_eq!(converted["choices"][0]["finish_reason"], "length");
        assert_eq!(converted["usage"]["prompt_tokens"], 3);
        assert_eq!(converted["usage"]["completion_tokens"], 5);
    }

    #[test]
    fn anthropic_to_openai_uses_defaults_when_fields_are_missing() {
        let converted = anthropic_to_openai(&serde_json::json!({}));

        assert_eq!(converted["id"], "msg-converted");
        assert_eq!(converted["choices"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn anthropic_to_openai_chat_request_flattens_system_and_blocks() {
        let body = serde_json::json!({
            "model": "claude-test",
            "max_tokens": 100,
            "temperature": 0.4,
            "stream": false,
            "system": [{"type": "text", "text": "system text"}],
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "hello"}, {"content": " world"}]},
                {"role": "assistant", "content": "done"},
                {"role": "user", "content": []}
            ]
        });

        let converted = anthropic_to_openai_chat_request(&body);

        assert_eq!(converted["model"], "claude-test");
        assert_eq!(converted["max_tokens"], 100);
        assert_eq!(converted["temperature"], 0.4);
        assert_eq!(converted["stream"], false);
        assert_eq!(converted["messages"].as_array().unwrap().len(), 3);
        assert_eq!(converted["messages"][0]["role"], "system");
        assert_eq!(converted["messages"][0]["content"], "system text");
        assert_eq!(converted["messages"][1]["content"], "hello world");
        assert_eq!(converted["messages"][2]["role"], "assistant");
    }

    #[test]
    fn openai_chat_to_anthropic_messages_maps_usage_and_finish_reason() {
        let body = serde_json::json!({
            "id": "chatcmpl_1",
            "model": "gpt-test",
            "choices": [{"message": {"content": "done"}, "finish_reason": "length"}],
            "usage": {"prompt_tokens": 11, "completion_tokens": 13}
        });

        let converted = openai_chat_to_anthropic_messages(&body);

        assert_eq!(converted["id"], "chatcmpl_1");
        assert_eq!(converted["type"], "message");
        assert_eq!(converted["model"], "gpt-test");
        assert_eq!(converted["content"][0]["text"], "done");
        assert_eq!(converted["stop_reason"], "max_tokens");
        assert_eq!(converted["usage"]["input_tokens"], 11);
        assert_eq!(converted["usage"]["output_tokens"], 13);
    }

    #[test]
    fn anthropic_to_openai_chat_request_preserves_tools_and_tool_results() {
        let body = serde_json::json!({
            "model": "claude-test",
            "tools": [{"name": "Read", "description": "read file", "input_schema": {"type": "object"}}],
            "messages": [
                {"role": "assistant", "content": [{"type": "tool_use", "id": "toolu_1", "name": "Read", "input": {"file_path": "/tmp/a"}}]},
                {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "toolu_1", "content": "file data"}]}
            ]
        });

        let converted = anthropic_to_openai_chat_request(&body);

        assert_eq!(converted["tools"][0]["function"]["name"], "Read");
        assert_eq!(converted["messages"][0]["role"], "assistant");
        assert_eq!(
            converted["messages"][0]["tool_calls"][0]["function"]["name"],
            "Read"
        );
        assert_eq!(converted["messages"][1]["role"], "tool");
        assert_eq!(converted["messages"][1]["tool_call_id"], "toolu_1");
    }

    #[test]
    fn anthropic_to_openai_chat_request_maps_thinking_to_reasoning_content() {
        let body = serde_json::json!({
            "model": "deepseek/deepseek-v4-flash",
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "Need to read the file first."},
                    {"type": "tool_use", "id": "toolu_1", "name": "Read", "input": {"file_path": "/tmp/a"}}
                ]},
                {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "toolu_1", "content": "file data"}]}
            ]
        });

        let converted = anthropic_to_openai_chat_request(&body);

        assert_eq!(
            converted["messages"][0]["reasoning_content"],
            "Need to read the file first."
        );
        assert_eq!(
            converted["messages"][0]["tool_calls"][0]["function"]["name"],
            "Read"
        );
    }

    #[test]
    fn openai_chat_to_anthropic_messages_maps_reasoning_content_to_thinking() {
        let body = serde_json::json!({
            "id": "chatcmpl_1",
            "model": "deepseek/deepseek-v4-flash",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "reasoning_content": "Let me answer briefly.",
                    "content": "Hello"
                },
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 1, "completion_tokens": 2}
        });

        let converted = openai_chat_to_anthropic_messages(&body);

        assert_eq!(converted["content"][0]["type"], "thinking");
        assert_eq!(
            converted["content"][0]["thinking"],
            "Let me answer briefly."
        );
        assert_eq!(converted["content"][1]["type"], "text");
        assert_eq!(converted["content"][1]["text"], "Hello");
    }

    #[tokio::test]
    async fn transform_openai_chat_sse_maps_reasoning_content_to_thinking_delta() {
        let openai_sse = "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"Think\"},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n";
        let upstream = futures::stream::iter(vec![Ok::<Bytes, std::convert::Infallible>(
            Bytes::from(openai_sse),
        )]);
        let mut out = transform_openai_chat_sse_to_anthropic(upstream, "test-model".into());
        let mut sse = String::new();
        while let Some(chunk) = out.next().await {
            sse.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
        }

        assert!(sse.contains(r#""type":"thinking""#));
        assert!(sse.contains(r#""type":"thinking_delta""#));
        assert!(sse.contains(r#""thinking":"Think""#));
        assert!(sse.contains(r#""text":"Hi""#));
    }

    #[tokio::test]
    async fn transform_openai_chat_sse_to_anthropic_emits_message_events() {
        let openai_sse = "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\" there\"},\"finish_reason\":null}]}\n\n\
data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"completion_tokens\":2}}\n\n\
data: [DONE]\n\n";
        let upstream = futures::stream::iter(vec![Ok::<Bytes, std::convert::Infallible>(
            Bytes::from(openai_sse),
        )]);
        let mut out = transform_openai_chat_sse_to_anthropic(upstream, "test-model".into());
        let mut sse = String::new();
        while let Some(chunk) = out.next().await {
            sse.push_str(&String::from_utf8_lossy(&chunk.unwrap()));
        }

        assert!(sse.contains("event: message_start"));
        assert!(sse.contains("event: content_block_delta"));
        assert!(sse.contains(r#""text":"Hi""#));
        assert!(sse.contains(r#""text":" there""#));
        assert!(sse.contains("event: message_stop"));
        assert!(sse.contains(r#""stop_reason":"end_turn""#));
    }

    #[test]
    fn chat_request_to_responses_maps_messages_and_instructions() {
        let body = serde_json::json!({
            "model": "gpt-test",
            "max_tokens": 1024,
            "messages": [
                {"role": "system", "content": "be terse"},
                {"role": "user", "content": "hello"}
            ]
        });

        let converted = chat_request_to_responses(&body);

        assert_eq!(converted["model"], "gpt-test");
        assert_eq!(converted["max_output_tokens"], 1024);
        assert_eq!(converted["instructions"], "be terse");
        assert_eq!(converted["input"].as_array().unwrap().len(), 1);
        assert_eq!(converted["input"][0]["role"], "user");
        assert_eq!(converted["input"][0]["content"], "hello");
        assert!(!converted.as_object().unwrap().contains_key("messages"));
    }

    #[test]
    fn responses_to_anthropic_messages_maps_output_text() {
        let body = serde_json::json!({
            "id": "resp_1",
            "model": "test-model",
            "output_text": "hello world",
            "usage": {"input_tokens": 3, "output_tokens": 5}
        });

        let converted = responses_to_anthropic_messages(&body);

        assert_eq!(converted["type"], "message");
        assert_eq!(converted["content"][0]["text"], "hello world");
        assert_eq!(converted["usage"]["input_tokens"], 3);
        assert_eq!(converted["usage"]["output_tokens"], 5);
    }

    #[test]
    fn responses_to_chat_request_handles_string_input_and_instructions() {
        let body = serde_json::json!({
            "model": "resp-test",
            "instructions": "be helpful",
            "input": "hello",
            "max_output_tokens": 20
        });

        let converted = responses_to_chat_request(&body);

        assert_eq!(converted["model"], "resp-test");
        assert_eq!(converted["stream"], false);
        assert_eq!(converted["max_tokens"], 20);
        assert_eq!(
            converted["messages"][0],
            serde_json::json!({"role": "system", "content": "be helpful"})
        );
        assert_eq!(
            converted["messages"][1],
            serde_json::json!({"role": "user", "content": "hello"})
        );
    }

    #[test]
    fn responses_to_chat_request_handles_array_input_roles_and_empty_fallback() {
        let body = serde_json::json!({
            "instructions": {"kind": "json"},
            "input": [
                "plain text",
                {"role": "developer", "content": "dev note"},
                {"role": "assistant", "content": [{"text": "assistant "}, "text"]},
                {"role": "tool", "content": {"value": 1}},
                {"role": "unknown", "content": ""}
            ],
            "max_tokens": 30
        });

        let converted = responses_to_chat_request(&body);

        assert_eq!(converted["max_tokens"], 30);
        assert_eq!(converted["messages"][0]["role"], "system");
        assert_eq!(
            converted["messages"][1],
            serde_json::json!({"role": "user", "content": "plain text"})
        );
        assert_eq!(
            converted["messages"][2],
            serde_json::json!({"role": "system", "content": "dev note"})
        );
        assert_eq!(
            converted["messages"][3],
            serde_json::json!({"role": "assistant", "content": "assistant text"})
        );
        assert_eq!(converted["messages"][4]["role"], "tool");

        let empty = responses_to_chat_request(&serde_json::json!({"input": []}));
        assert_eq!(
            empty["messages"][0],
            serde_json::json!({"role": "user", "content": " "})
        );
    }

    #[test]
    fn chat_to_responses_maps_text_and_token_usage() {
        let body = serde_json::json!({
            "model": "gpt-test",
            "choices": [{"message": {"content": "answer"}}],
            "usage": {"input_tokens": 7, "output_tokens": 9}
        });

        let converted = chat_to_responses(&body, "fallback-model");

        assert_eq!(converted["object"], "response");
        assert_eq!(converted["model"], "gpt-test");
        assert_eq!(converted["output"][0]["content"][0]["text"], "answer");
        assert_eq!(converted["output_text"], "answer");
        assert_eq!(converted["usage"]["input_tokens"], 7);
        assert_eq!(converted["usage"]["output_tokens"], 9);
        assert_eq!(converted["usage"]["total_tokens"], 16);
    }

    #[test]
    fn chat_to_responses_uses_fallback_model_and_prompt_token_names() {
        let body = serde_json::json!({
            "choices": [{"message": {"content": "answer"}}],
            "usage": {"prompt_tokens": 2, "completion_tokens": 4}
        });

        let converted = chat_to_responses(&body, "fallback-model");

        assert_eq!(converted["model"], "fallback-model");
        assert_eq!(converted["usage"]["input_tokens"], 2);
        assert_eq!(converted["usage"]["output_tokens"], 4);
        assert_eq!(converted["usage"]["total_tokens"], 6);
    }

    #[test]
    fn responses_to_sse_stream_emits_expected_events_with_and_without_text() {
        let body = serde_json::json!({
            "id": "resp_1",
            "created": 123,
            "model": "resp-model",
            "output_text": "hello"
        });

        let sse = String::from_utf8(responses_to_sse_stream(&body).to_vec()).unwrap();

        assert!(sse.contains("event: response.created"));
        assert!(sse.contains("\"id\":\"resp_1\""));
        assert!(sse.contains("event: response.output_text.delta"));
        assert!(sse.contains("\"delta\":\"hello\""));
        assert!(sse.contains("event: response.completed"));
        assert!(sse.contains("\"status\":\"completed\""));

        let empty =
            String::from_utf8(responses_to_sse_stream(&serde_json::json!({})).to_vec()).unwrap();
        assert!(empty.contains("\"id\":\"resp_shim\""));
        assert!(empty.contains("\"model\":\"unknown\""));
        assert!(!empty.contains("response.output_text.delta"));
    }

    #[tokio::test]
    async fn token_tracking_stream_updates_request_log_on_drop() {
        let pool = cab_db::InMemoryStore::new();
        let log_id = "log-1".to_string();
        {
            let mut data = pool.inner.write().unwrap();
            data.request_logs.push(RequestLog {
                id: log_id.clone(),
                timestamp: "now".into(),
                agent: "codex".into(),
                provider: "test".into(),
                model: "model".into(),
                input_tokens: 0,
                output_tokens: 0,
                total_tokens: 0,
                latency_ms: 0,
                status: 200,
                error: None,
                path: "/v1/chat/completions".into(),
                stream: true,
            });
        }

        let chunks = futures::stream::iter(vec![
            Ok(Bytes::from_static(
                br#"data: {"message":{"usage":{"input_tokens":3}}}
"#,
            )),
            Ok(Bytes::from_static(
                br#"data: {"usage":{"prompt_tokens":5,"completion_tokens":8}}
data: {"usage":{"input_tokens":7,"output_tokens":11}}
data: [DONE]
"#,
            )),
        ]);
        let mut stream = TokenTrackingStream::new(chunks, pool.clone(), log_id.clone());
        while stream.next().await.is_some() {}
        drop(stream);

        let data = pool.inner.read().unwrap();
        let log = data
            .request_logs
            .iter()
            .find(|entry| entry.id == log_id)
            .unwrap();
        assert_eq!(log.input_tokens, 7);
        assert_eq!(log.output_tokens, 11);
        assert_eq!(log.total_tokens, 18);
    }

    #[tokio::test]
    async fn openai_stream_converter_emits_tool_use_blocks() {
        use futures::StreamExt;

        let chunks: Vec<Result<Bytes, std::convert::Infallible>> = vec![
            Ok(Bytes::from_static(
                br#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_abc","function":{"name":"Glob","arguments":""}}]}}]}
"#,
            )),
            Ok(Bytes::from_static(
                br#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"pattern\":"}}]}}]}
"#,
            )),
            Ok(Bytes::from_static(
                br#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"**/*\"}"}}]}}]}
"#,
            )),
            Ok(Bytes::from_static(
                br#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}
data: [DONE]
"#,
            )),
        ];

        let out = transform_openai_chat_sse_to_anthropic(
            futures::stream::iter(chunks),
            "deepseek/deepseek-v4-flash".into(),
        )
        .map(|result| result.unwrap())
        .collect::<Vec<_>>()
        .await;
        let combined = out
            .iter()
            .map(|chunk| String::from_utf8_lossy(chunk))
            .collect::<String>();

        assert!(combined.contains(r#""type":"tool_use""#));
        assert!(combined.contains(r#""name":"Glob""#));
        assert!(combined.contains("input_json_delta"));
        assert!(combined.contains(r#""stop_reason":"tool_use""#));
    }

    #[test]
    fn convert_request_routes_anthropic_to_responses_with_tools() {
        use crate::protocol::{PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI_RESPONSES, convert_request};
        let body = serde_json::json!({
            "model": "claude-test",
            "max_tokens": 100,
            "tools": [{"name": "Read", "description": "read", "input_schema": {"type": "object"}}],
            "tool_choice": {"type": "any"},
            "messages": [
                {"role": "assistant", "content": [{"type": "tool_use", "id": "toolu_1", "name": "Read", "input": {"path": "/tmp"}}]},
                {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "toolu_1", "content": "ok"}]}
            ]
        });
        let converted = convert_request(PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI_RESPONSES, &body);
        assert_eq!(converted["tools"][0]["name"], "Read");
        assert_eq!(converted["tool_choice"], "required");
        assert_eq!(converted["input"][0]["type"], "function_call");
        assert_eq!(converted["input"][1]["type"], "function_call_output");
    }

    #[test]
    fn responses_to_anthropic_messages_maps_function_call_output() {
        let body = serde_json::json!({
            "id": "resp_1",
            "model": "gpt-test",
            "output": [{
                "type": "function_call",
                "call_id": "call_1",
                "name": "Glob",
                "arguments": "{\"pattern\":\"**/*\"}"
            }],
            "usage": {"input_tokens": 1, "output_tokens": 2}
        });
        let converted = responses_to_anthropic_messages(&body);
        assert_eq!(converted["stop_reason"], "tool_use");
        assert_eq!(converted["content"][0]["type"], "tool_use");
        assert_eq!(converted["content"][0]["name"], "Glob");
    }

    #[test]
    fn chat_request_to_responses_maps_assistant_tool_calls() {
        let body = serde_json::json!({
            "model": "gpt-test",
            "messages": [
                {"role": "assistant", "tool_calls": [{
                    "id": "call_1",
                    "type": "function",
                    "function": {"name": "Read", "arguments": "{\"path\":\"/a\"}"}
                }]},
                {"role": "tool", "tool_call_id": "call_1", "content": "data"}
            ]
        });
        let converted = chat_request_to_responses(&body);
        assert_eq!(converted["input"][0]["type"], "function_call");
        assert_eq!(converted["input"][1]["type"], "function_call_output");
    }

    #[tokio::test]
    async fn openai_stream_converter_buffers_args_before_name() {
        use futures::StreamExt;

        let chunks: Vec<Result<Bytes, std::convert::Infallible>> = vec![
            Ok(Bytes::from_static(
                br#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_x","function":{"arguments":"{\"a\":"}}]}}]}
"#,
            )),
            Ok(Bytes::from_static(
                br#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"Test","arguments":"1}"}}]}}]}
"#,
            )),
            Ok(Bytes::from_static(
                br#"data: {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}
data: [DONE]
"#,
            )),
        ];

        let out = transform_openai_chat_sse_to_anthropic(
            futures::stream::iter(chunks),
            "test-model".into(),
        )
        .map(|result| result.unwrap())
        .collect::<Vec<_>>()
        .await;
        let combined = out
            .iter()
            .map(|chunk| String::from_utf8_lossy(chunk))
            .collect::<String>();

        assert!(combined.contains(r#""name":"Test""#));
        assert!(combined.contains(r#""partial_json":"{\"a\":"#));
        assert!(combined.contains(r#""partial_json":"1}""#));
    }
}
