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
    thinking_signature_emitted: bool,
    text_block_started: bool,
    text_block_index: u32,
    next_block_index: u32,
    tool_calls: std::collections::HashMap<u64, StreamingToolCall>,
    finished: bool,
    output_tokens: u64,
    /// Latched from a usage-only final chunk (common for OpenAI-compat providers
    /// like LongCat that send finish_reason and usage on separate SSE events).
    last_usage: Option<Value>,
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
            thinking_signature_emitted: false,
            text_block_started: false,
            text_block_index: 0,
            next_block_index: 0,
            tool_calls: std::collections::HashMap::new(),
            finished: false,
            output_tokens: 0,
            last_usage: None,
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
            if let Some(id) = call.get("id").and_then(|v| v.as_str())
                && let Some(tool) = self.tool_calls.get_mut(&openai_index)
            {
                tool.id = id.to_string();
            }
            if let Some(name) = call
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                && let Some(tool) = self.tool_calls.get_mut(&openai_index)
            {
                tool.name = name.to_string();
                if !tool.pending_args.is_empty() {
                    let buffered = std::mem::take(&mut tool.pending_args);
                    self.push_tool_input_delta(openai_index, &buffered);
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
        self.thinking_signature_emitted = false;
        self.pending.push(anthropic_stream_event(
            "content_block_start",
            serde_json::json!({
                "type": "content_block_start",
                "index": self.thinking_block_index,
                "content_block": {"type": "thinking", "thinking": ""}
            }),
        ));
    }

    fn ensure_thinking_signature(&mut self) {
        if !self.thinking_block_started || self.thinking_signature_emitted {
            return;
        }
        self.thinking_signature_emitted = true;
        // Anthropic clients (Claude Code) require a signature_delta before
        // content_block_stop to retain the thinking block for later turns.
        // OpenAI-compat providers only send reasoning text — mint a stable-looking
        // opaque signature so tool-call multi-turns can replay CoT upstream.
        let signature = format!("cab_{}", uuid::Uuid::new_v4().simple());
        self.pending.push(anthropic_stream_event(
            "content_block_delta",
            serde_json::json!({
                "type": "content_block_delta",
                "index": self.thinking_block_index,
                "delta": {"type": "signature_delta", "signature": signature}
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

        // Upstream is OpenAI Chat Completions (`CompletionUsage`): only
        // prompt_tokens / completion_tokens are authoritative.
        let output_tokens = usage
            .and_then(|u| u.get("completion_tokens"))
            .and_then(|v| v.as_u64())
            .filter(|v| *v > 0)
            .unwrap_or(self.output_tokens);
        let input_tokens = usage
            .and_then(|u| u.get("prompt_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        if self.thinking_block_started {
            self.ensure_thinking_signature();
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
        let mut delta_usage = serde_json::json!({"output_tokens": output_tokens});
        if input_tokens > 0 {
            delta_usage["input_tokens"] = serde_json::json!(input_tokens);
        }
        self.pending.push(anthropic_stream_event(
            "message_delta",
            serde_json::json!({
                "type": "message_delta",
                "delta": {"stop_reason": stop_reason, "stop_sequence": null},
                "usage": delta_usage
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
            let usage = self.last_usage.clone();
            self.finish_with_reason(None, usage.as_ref());
            return;
        }
        let Ok(chunk) = serde_json::from_str::<Value>(payload) else {
            return;
        };
        if let Some(usage) = chunk.get("usage") {
            self.last_usage = Some(usage.clone());
        }
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
            // Prefer usage on this chunk; otherwise the latched usage-only chunk.
            let usage = chunk
                .get("usage")
                .cloned()
                .or_else(|| self.last_usage.clone());
            self.finish_with_reason(finish_reason, usage.as_ref());
        } else if chunk.get("usage").is_some()
            && choice
                .and_then(|c| c.get("delta"))
                .map(|d| d.as_object().map(|o| o.is_empty()).unwrap_or(true))
                .unwrap_or(true)
        {
            // Usage-only final chunk with empty/missing choices (OpenAI
            // stream_options.include_usage) — finalize with CompletionUsage.
            self.finish_with_reason(None, chunk.get("usage"));
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
        let usage = self.last_usage.clone();
        self.finish_with_reason(None, usage.as_ref());
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

use cab_core::types::RequestLog;

pub struct TokenTrackingStream<S> {
    inner: S,
    pool: cab_db::InMemoryStore,
    /// Full request log (all fields except token counts are pre-filled). Token
    /// counts are accumulated during streaming and the log is persisted on Drop.
    log: RequestLog,
    buffer: Vec<u8>,
    accumulated_response: Vec<u8>,
}

impl<S> TokenTrackingStream<S> {
    pub fn new(inner: S, pool: cab_db::InMemoryStore, log: RequestLog) -> Self {
        Self {
            inner,
            pool,
            log,
            buffer: Vec::new(),
            accumulated_response: Vec::new(),
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
                    self.track_protocol_event(&json_val);
                }
            }
        }
    }

    /// Dispatch usage parsing by the client-facing protocol (`log.path`).
    ///
    /// Field names follow the official schemas and are not mixed across protocols:
    /// - Chat Completions (`CompletionUsage`): `prompt_tokens` / `completion_tokens`
    /// - Responses (`ResponseUsage`): `response.usage.input_tokens` / `output_tokens`
    /// - Anthropic Messages: `message_start.message.usage` + `message_delta.usage`
    fn track_protocol_event(&mut self, json_val: &serde_json::Value) {
        match self.log.path.as_str() {
            "/v1/chat/completions" => {
                if let Some(usage) = json_val.get("usage") {
                    self.apply_openai_chat_usage(usage);
                }
            }
            "/v1/responses" => {
                if let Some(usage) = json_val.get("response").and_then(|r| r.get("usage")) {
                    self.apply_openai_responses_usage(usage);
                }
            }
            "/v1/messages" => match json_val.get("type").and_then(|t| t.as_str()) {
                Some("message_start") => {
                    if let Some(usage) = json_val.get("message").and_then(|m| m.get("usage")) {
                        self.apply_anthropic_usage(usage);
                    }
                }
                Some("message_delta") => {
                    if let Some(usage) = json_val.get("usage") {
                        self.apply_anthropic_usage(usage);
                    }
                }
                _ => {
                    // Tolerant fallback when `type` is absent on the data payload.
                    if let Some(usage) = json_val.get("message").and_then(|m| m.get("usage")) {
                        self.apply_anthropic_usage(usage);
                    }
                    if let Some(usage) = json_val.get("usage") {
                        self.apply_anthropic_usage(usage);
                    }
                }
            },
            _ => {
                if let Some(usage) = json_val.get("response").and_then(|r| r.get("usage")) {
                    self.apply_openai_responses_usage(usage);
                } else if let Some(usage) = json_val.get("message").and_then(|m| m.get("usage")) {
                    self.apply_anthropic_usage(usage);
                } else if let Some(usage) = json_val.get("usage") {
                    if usage.get("prompt_tokens").is_some()
                        || usage.get("completion_tokens").is_some()
                    {
                        self.apply_openai_chat_usage(usage);
                    } else {
                        self.apply_anthropic_usage(usage);
                    }
                }
            }
        }
    }

    /// OpenAI Chat Completions `CompletionUsage` only.
    ///
    /// Wire `prompt_tokens` includes cache; we store disjoint CAB fields.
    fn apply_openai_chat_usage(&mut self, usage: &serde_json::Value) {
        let reported_input = usage
            .get("prompt_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(self.log.input_tokens + self.log.cache_read_tokens);
        let reported_output = usage
            .get("completion_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(self.log.output_tokens);
        let details = usage.get("prompt_tokens_details");
        let cache_read = details
            .and_then(|d| d.get("cached_tokens"))
            .and_then(|v| v.as_i64())
            .or_else(|| {
                usage
                    .get("prompt_cache_hit_tokens")
                    .and_then(|v| v.as_i64())
            })
            .unwrap_or(self.log.cache_read_tokens);
        let cache_creation = details
            .and_then(|d| d.get("cache_write_tokens"))
            .and_then(|v| v.as_i64())
            .unwrap_or(self.log.cache_creation_tokens);
        let n = cab_core::normalize_stored_tokens(
            reported_input,
            reported_output,
            cache_read,
            cache_creation,
            true,
        );
        self.log.input_tokens = n.input_tokens;
        self.log.output_tokens = n.output_tokens;
        self.log.cache_read_tokens = n.cache_read_tokens;
        self.log.cache_creation_tokens = n.cache_creation_tokens;
        self.log.total_tokens = n.total_tokens;
    }

    /// OpenAI Responses API `ResponseUsage` only.
    ///
    /// Wire `input_tokens` includes cache; we store disjoint CAB fields.
    fn apply_openai_responses_usage(&mut self, usage: &serde_json::Value) {
        let reported_input = usage
            .get("input_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(self.log.input_tokens + self.log.cache_read_tokens);
        let reported_output = usage
            .get("output_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(self.log.output_tokens);
        let details = usage.get("input_tokens_details");
        let cache_read = details
            .and_then(|d| d.get("cached_tokens"))
            .and_then(|v| v.as_i64())
            .unwrap_or(self.log.cache_read_tokens);
        let cache_creation = details
            .and_then(|d| d.get("cache_write_tokens"))
            .and_then(|v| v.as_i64())
            .unwrap_or(self.log.cache_creation_tokens);
        let n = cab_core::normalize_stored_tokens(
            reported_input,
            reported_output,
            cache_read,
            cache_creation,
            true,
        );
        self.log.input_tokens = n.input_tokens;
        self.log.output_tokens = n.output_tokens;
        self.log.cache_read_tokens = n.cache_read_tokens;
        self.log.cache_creation_tokens = n.cache_creation_tokens;
        self.log.total_tokens = n.total_tokens;
    }

    /// Anthropic Messages `Usage` only (`input_tokens` / `output_tokens` + cache_*).
    ///
    /// Wire input normally excludes cache — but some relays report the total
    /// prompt (inclusive of cache reads); detect that layout and normalize it
    /// back to CAB's disjoint storage so cache reads are never double-counted.
    fn apply_anthropic_usage(&mut self, usage: &serde_json::Value) {
        let mut reported_input = self.log.input_tokens;
        if let Some(v) = usage.get("input_tokens").and_then(|v| v.as_i64()) {
            // message_start carries input; later empty stubs must not wipe it.
            if v > 0 || self.log.input_tokens == 0 {
                reported_input = v;
            }
        }
        let reported_output = usage
            .get("output_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(self.log.output_tokens);
        let cache_read = usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(self.log.cache_read_tokens);
        let cache_creation = usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(self.log.cache_creation_tokens);
        let n = cab_core::normalize_stored_tokens(
            reported_input,
            reported_output,
            cache_read,
            cache_creation,
            cab_core::anthropic_input_includes_cache(reported_input, cache_read, cache_creation),
        );
        self.log.input_tokens = n.input_tokens;
        self.log.output_tokens = n.output_tokens;
        self.log.cache_read_tokens = n.cache_read_tokens;
        self.log.cache_creation_tokens = n.cache_creation_tokens;
        self.log.total_tokens = n.total_tokens;
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
                this.accumulated_response.extend_from_slice(&bytes);
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
        // Physical token total — cache write is only added when it is a disjoint
        // Anthropic prompt part (`/v1/messages`). On OpenAI it overlays input.
        self.log.total_tokens = match self.log.path.as_str() {
            "/v1/messages" => {
                self.log.input_tokens
                    + self.log.cache_read_tokens
                    + self.log.cache_creation_tokens
                    + self.log.output_tokens
            }
            _ => self.log.input_tokens + self.log.cache_read_tokens + self.log.output_tokens,
        };
        if let Ok(resp_str) = String::from_utf8(self.accumulated_response.clone()) {
            self.log.response_body = Some(resp_str);
        }
        let pool = self.pool.clone();
        let log = self.log.clone();

        // Update the in-memory ring buffer synchronously — no async, no race.
        // This is the canonical insert for the request log; we must NOT rely on
        // a concurrent `tokio::spawn` in the caller to win a race against us.
        if let Ok(mut data) = pool.inner.write() {
            if let Some(pos) = data.request_logs.iter().position(|l| l.id == log.id) {
                data.request_logs[pos] = log.clone();
            } else {
                data.request_logs.push(log.clone());
                if data.request_logs.len() > 500 {
                    let overflow = data.request_logs.len() - 500;
                    data.request_logs.drain(0..overflow);
                }
            }
        }

        // Best-effort SQLite persist when a tokio runtime is available (the
        // normal case — streaming happens inside an active axum request).
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if let Some(sqlite_pool) = pool.sqlite()
                    && let Ok(conn) = sqlite_pool.get()
                    && let Err(e) = cab_db::sqlite::append_log(&conn, &log)
                {
                    tracing::warn!("Failed to persist streamed log to SQLite: {e}");
                }
            });
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
    fn anthropic_to_openai_chat_request_injects_empty_reasoning_for_tool_calls() {
        // Claude Code strips unsigned thinking; multi-turn tool history arrives
        // without it. DeepSeek still requires the reasoning_content field.
        let body = serde_json::json!({
            "model": "deepseek/deepseek-v4-flash",
            "messages": [
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "toolu_1", "name": "Read", "input": {"file_path": "/tmp/a"}}
                ]},
                {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "toolu_1", "content": "file data"}]}
            ]
        });

        let converted = anthropic_to_openai_chat_request(&body);

        assert_eq!(converted["messages"][0]["reasoning_content"], "");
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
        assert!(sse.contains(r#""type":"signature_delta""#));
        assert!(sse.contains(r#""signature":"cab_"#));
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
    async fn token_tracking_stream_persists_anthropic_usage_on_drop() {
        let pool = cab_db::InMemoryStore::new();
        let log_id = "log-1".to_string();

        let initial_log = RequestLog {
            id: log_id.clone(),
            timestamp: "now".into(),
            agent: "claude-code".into(),
            provider: "test".into(),
            model: "model".into(),
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            latency_ms: 42,
            status: 200,
            error: None,
            path: "/v1/messages".into(),
            stream: true,
            request_body: None,
            response_body: None,
        };

        // Anthropic streaming: input on message_start, cumulative output on message_delta.
        let chunks = futures::stream::iter(vec![
            Ok(Bytes::from_static(
                br#"data: {"type":"message_start","message":{"usage":{"input_tokens":25,"output_tokens":1,"cache_read_input_tokens":42,"cache_creation_input_tokens":9}}}
"#,
            )),
            Ok(Bytes::from_static(
                br#"data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":15}}
data: {"type":"message_stop"}
"#,
            )),
        ]);
        let mut stream = TokenTrackingStream::new(chunks, pool.clone(), initial_log);
        while stream.next().await.is_some() {}
        drop(stream);

        let data = pool.inner.read().unwrap();
        let log = data
            .request_logs
            .iter()
            .find(|entry| entry.id == log_id)
            .unwrap();
        assert_eq!(log.input_tokens, 25);
        assert_eq!(log.output_tokens, 15);
        // Anthropic-style cache tokens are added into total_tokens.
        assert_eq!(log.total_tokens, 25 + 42 + 9 + 15);
        assert_eq!(log.cache_read_tokens, 42);
        assert_eq!(log.cache_creation_tokens, 9);
        assert_eq!(log.latency_ms, 42);
        assert_eq!(log.agent, "claude-code");
        assert_eq!(log.status, 200);
    }

    #[tokio::test]
    async fn token_tracking_anthropic_normalizes_inclusive_input() {
        // Relay reports the total prompt as input_tokens (29047) alongside a
        // separate cache_read (28928); CAB must store disjoint legs so the
        // cache read is not double-counted.
        let pool = cab_db::InMemoryStore::new();
        let log_id = "log-anthropic-inclusive".to_string();
        let initial_log = RequestLog {
            id: log_id.clone(),
            timestamp: "now".into(),
            agent: "claude-code".into(),
            provider: "OpenCode Go".into(),
            model: "deepseek/deepseek-v4-flash".into(),
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            latency_ms: 42,
            status: 200,
            error: None,
            path: "/v1/messages".into(),
            stream: true,
            request_body: None,
            response_body: None,
        };

        let chunks = futures::stream::iter(vec![
            Ok(Bytes::from_static(
                br#"data: {"type":"message_start","message":{"usage":{"input_tokens":29047,"output_tokens":1,"cache_read_input_tokens":28928}}}
"#,
            )),
            Ok(Bytes::from_static(
                br#"data: {"type":"message_delta","delta":{"stop_reason":"max_tokens"},"usage":{"output_tokens":64}}
data: {"type":"message_stop"}
"#,
            )),
        ]);
        let mut stream = TokenTrackingStream::new(chunks, pool.clone(), initial_log);
        while stream.next().await.is_some() {}
        drop(stream);

        let data = pool.inner.read().unwrap();
        let log = data
            .request_logs
            .iter()
            .find(|entry| entry.id == log_id)
            .unwrap();
        assert_eq!(log.input_tokens, 119); // 29047 - 28928
        assert_eq!(log.cache_read_tokens, 28928);
        assert_eq!(log.output_tokens, 64);
        assert_eq!(log.total_tokens, 119 + 28928 + 64); // no double count
    }

    #[tokio::test]
    async fn token_tracking_openai_chat_maps_deepseek_cache_hit() {
        let pool = cab_db::InMemoryStore::new();
        let log_id = "log-deepseek-cache".to_string();
        let initial_log = RequestLog {
            id: log_id.clone(),
            timestamp: "now".into(),
            agent: "opencode".into(),
            provider: "deepseek".into(),
            model: "deepseek-chat".into(),
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            latency_ms: 10,
            status: 200,
            error: None,
            path: "/v1/chat/completions".into(),
            stream: true,
            request_body: None,
            response_body: None,
        };

        let chunks = futures::stream::iter(vec![Ok(Bytes::from_static(
            br#"data: {"choices":[],"usage":{"prompt_tokens":100,"completion_tokens":5,"prompt_cache_hit_tokens":40,"prompt_cache_miss_tokens":60}}
data: [DONE]
"#,
        ))]);
        let mut stream = TokenTrackingStream::new(chunks, pool.clone(), initial_log);
        while stream.next().await.is_some() {}
        drop(stream);

        let data = pool.inner.read().unwrap();
        let log = data
            .request_logs
            .iter()
            .find(|entry| entry.id == log_id)
            .unwrap();
        assert_eq!(log.input_tokens, 60); // prompt(100) - cache hit(40)
        assert_eq!(log.cache_read_tokens, 40);
        assert_eq!(log.output_tokens, 5);
        assert_eq!(log.total_tokens, 105);
    }

    #[tokio::test]
    async fn token_tracking_openai_chat_ignores_nonstandard_zero_aliases() {
        // Compat providers may also emit input_tokens/output_tokens=0 alongside
        // standard CompletionUsage fields — Chat Completions must ignore them.
        let pool = cab_db::InMemoryStore::new();
        let log_id = "log-longcat".to_string();
        let initial_log = RequestLog {
            id: log_id.clone(),
            timestamp: "now".into(),
            agent: "grok-build".into(),
            provider: "LongCat".into(),
            model: "meituan/longcat-2.0".into(),
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            latency_ms: 10,
            status: 200,
            error: None,
            path: "/v1/chat/completions".into(),
            stream: true,
            request_body: None,
            response_body: None,
        };

        let chunks = futures::stream::iter(vec![Ok(Bytes::from_static(
            br#"data: {"choices":[],"usage":{"prompt_tokens":13450,"completion_tokens":32,"input_tokens":0,"output_tokens":0}}
data: [DONE]
"#,
        ))]);
        let mut stream = TokenTrackingStream::new(chunks, pool.clone(), initial_log);
        while stream.next().await.is_some() {}
        drop(stream);

        let data = pool.inner.read().unwrap();
        let log = data
            .request_logs
            .iter()
            .find(|entry| entry.id == log_id)
            .unwrap();
        assert_eq!(log.input_tokens, 13450);
        assert_eq!(log.output_tokens, 32);
        assert_eq!(log.total_tokens, 13482);
    }

    #[tokio::test]
    async fn token_tracking_openai_chat_records_prompt_cache_details() {
        let pool = cab_db::InMemoryStore::new();
        let log_id = "log-chat-cache".to_string();
        let initial_log = RequestLog {
            id: log_id.clone(),
            timestamp: "now".into(),
            agent: "opencode".into(),
            provider: "openai".into(),
            model: "gpt".into(),
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            latency_ms: 10,
            status: 200,
            error: None,
            path: "/v1/chat/completions".into(),
            stream: true,
            request_body: None,
            response_body: None,
        };

        let chunks = futures::stream::iter(vec![Ok(Bytes::from_static(
            br#"data: {"choices":[],"usage":{"prompt_tokens":100,"completion_tokens":5,"total_tokens":105,"prompt_tokens_details":{"cached_tokens":40,"cache_write_tokens":10}}}
data: [DONE]
"#,
        ))]);
        let mut stream = TokenTrackingStream::new(chunks, pool.clone(), initial_log);
        while stream.next().await.is_some() {}
        drop(stream);

        let data = pool.inner.read().unwrap();
        let log = data
            .request_logs
            .iter()
            .find(|entry| entry.id == log_id)
            .unwrap();
        assert_eq!(log.input_tokens, 60);
        assert_eq!(log.output_tokens, 5);
        assert_eq!(log.cache_read_tokens, 40);
        assert_eq!(log.cache_creation_tokens, 10);
        // prompt(100)+output(5); write is overlay, not added again.
        assert_eq!(log.total_tokens, 105);
    }

    #[tokio::test]
    async fn token_tracking_responses_records_input_cache_details() {
        let pool = cab_db::InMemoryStore::new();
        let log_id = "log-resp-cache".to_string();
        let initial_log = RequestLog {
            id: log_id.clone(),
            timestamp: "now".into(),
            agent: "codex".into(),
            provider: "openai".into(),
            model: "gpt".into(),
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            latency_ms: 10,
            status: 200,
            error: None,
            path: "/v1/responses".into(),
            stream: true,
            request_body: None,
            response_body: None,
        };

        let chunks = futures::stream::iter(vec![Ok(Bytes::from_static(
            br#"data: {"type":"response.completed","response":{"usage":{"input_tokens":100,"output_tokens":5,"total_tokens":105,"input_tokens_details":{"cached_tokens":40,"cache_write_tokens":10}}}}

"#,
        ))]);
        let mut stream = TokenTrackingStream::new(chunks, pool.clone(), initial_log);
        while stream.next().await.is_some() {}
        drop(stream);

        let data = pool.inner.read().unwrap();
        let log = data
            .request_logs
            .iter()
            .find(|entry| entry.id == log_id)
            .unwrap();
        assert_eq!(log.input_tokens, 60);
        assert_eq!(log.output_tokens, 5);
        assert_eq!(log.cache_read_tokens, 40);
        assert_eq!(log.cache_creation_tokens, 10);
        assert_eq!(log.total_tokens, 105);
    }

    #[tokio::test]
    async fn token_tracking_reads_responses_nested_usage() {
        let pool = cab_db::InMemoryStore::new();
        let log_id = "log-responses".to_string();
        let initial_log = RequestLog {
            id: log_id.clone(),
            timestamp: "now".into(),
            agent: "codex".into(),
            provider: "LongCat".into(),
            model: "meituan/longcat-2.0".into(),
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            latency_ms: 10,
            status: 200,
            error: None,
            path: "/v1/responses".into(),
            stream: true,
            request_body: None,
            response_body: None,
        };

        let chunks = futures::stream::iter(vec![Ok(Bytes::from_static(
            br#"event: response.completed
data: {"type":"response.completed","response":{"usage":{"input_tokens":10206,"output_tokens":7,"total_tokens":10213}}}

"#,
        ))]);
        let mut stream = TokenTrackingStream::new(chunks, pool.clone(), initial_log);
        while stream.next().await.is_some() {}
        drop(stream);

        let data = pool.inner.read().unwrap();
        let log = data
            .request_logs
            .iter()
            .find(|entry| entry.id == log_id)
            .unwrap();
        assert_eq!(log.input_tokens, 10206);
        assert_eq!(log.output_tokens, 7);
        assert_eq!(log.total_tokens, 10213);
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
    fn convert_request_anthropic_thinking_becomes_responses_reasoning_effort() {
        use crate::protocol::{PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI_RESPONSES, convert_request};
        let body = serde_json::json!({
            "model": "gpt-5.6-luna",
            "max_tokens": 256,
            "thinking": {"type": "enabled", "budget_tokens": 1024},
            "messages": [{"role": "user", "content": "hi"}]
        });
        let converted = convert_request(PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI_RESPONSES, &body);
        // thinking must NOT be passed through verbatim — Responses API rejects it.
        assert!(converted.get("thinking").is_none());
        // It should be converted to reasoning.effort.
        assert_eq!(converted["reasoning"]["effort"], "low");

        // Larger budget maps to higher effort.
        let big = serde_json::json!({
            "model": "gpt-5.6-luna",
            "max_tokens": 256,
            "thinking": {"type": "enabled", "budget_tokens": 16384},
            "messages": [{"role": "user", "content": "hi"}]
        });
        let converted_big = convert_request(PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI_RESPONSES, &big);
        assert_eq!(converted_big["reasoning"]["effort"], "high");

        // Disabled thinking → no reasoning field.
        let disabled = serde_json::json!({
            "model": "gpt-5.6-luna",
            "max_tokens": 256,
            "thinking": {"type": "disabled"},
            "messages": [{"role": "user", "content": "hi"}]
        });
        let converted_disabled =
            convert_request(PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI_RESPONSES, &disabled);
        assert!(converted_disabled.get("reasoning").is_none());
        assert!(converted_disabled.get("thinking").is_none());
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
