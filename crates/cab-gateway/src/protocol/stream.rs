//! SSE stream transformers between wire protocols.

use bytes::Bytes;
use futures::Stream;
use serde_json::Value;
use std::collections::HashMap;
use std::pin::Pin;
use std::task::Poll;

use super::legacy::anthropic_stream_event;

fn sse_line_payload(line: &str) -> Option<&str> {
    let line = line.trim();
    if line.is_empty() || line.starts_with(':') {
        return None;
    }
    line.strip_prefix("data:").map(str::trim)
}

struct LineBuffer {
    buffer: String,
}

impl LineBuffer {
    fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.buffer.push_str(&String::from_utf8_lossy(bytes));
        let mut lines = Vec::new();
        while let Some(pos) = self.buffer.find('\n') {
            let line = self.buffer[..pos].trim().to_string();
            self.buffer.drain(..=pos);
            if !line.is_empty() {
                lines.push(line);
            }
        }
        lines
    }

    fn flush(&mut self) -> Option<String> {
        if self.buffer.trim().is_empty() {
            None
        } else {
            let line = self.buffer.trim().to_string();
            self.buffer.clear();
            Some(line)
        }
    }
}

struct ToolTracker {
    block_index: u32,
    id: String,
    name: String,
    pending_args: String,
    started: bool,
    stopped: bool,
    streamed_args: bool,
}

struct AnthropicSseEmitter {
    model: String,
    message_id: String,
    pending: Vec<Bytes>,
    message_started: bool,
    thinking_index: Option<u32>,
    thinking_signature_emitted: bool,
    text_index: Option<u32>,
    next_index: u32,
    tools: HashMap<String, ToolTracker>,
    alias_to_tool: HashMap<String, String>,
    finished: bool,
    output_tokens: u64,
}

impl AnthropicSseEmitter {
    fn new(model: String) -> Self {
        Self {
            model,
            message_id: format!("msg_{}", uuid::Uuid::new_v4().simple()),
            pending: Vec::new(),
            message_started: false,
            thinking_index: None,
            thinking_signature_emitted: false,
            text_index: None,
            next_index: 0,
            tools: HashMap::new(),
            alias_to_tool: HashMap::new(),
            finished: false,
            output_tokens: 0,
        }
    }

    fn alloc_index(&mut self) -> u32 {
        let i = self.next_index;
        self.next_index += 1;
        i
    }

    fn ensure_message(&mut self) {
        if self.message_started {
            return;
        }
        self.message_started = true;
        self.pending.push(anthropic_stream_event(
            "message_start",
            serde_json::json!({
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
            }),
        ));
    }

    fn start_block(&mut self, _block_type: &str, content_block: Value) -> u32 {
        self.ensure_message();
        let index = self.alloc_index();
        self.pending.push(anthropic_stream_event(
            "content_block_start",
            serde_json::json!({
                "type": "content_block_start",
                "index": index,
                "content_block": content_block,
            }),
        ));
        index
    }

    fn push_delta(&mut self, index: u32, delta: Value) {
        self.pending.push(anthropic_stream_event(
            "content_block_delta",
            serde_json::json!({
                "type": "content_block_delta",
                "index": index,
                "delta": delta,
            }),
        ));
    }

    fn stop_block(&mut self, index: u32) {
        self.pending.push(anthropic_stream_event(
            "content_block_stop",
            serde_json::json!({"type": "content_block_stop", "index": index}),
        ));
    }

    fn close_thinking(&mut self) {
        if let Some(idx) = self.thinking_index.take() {
            self.ensure_thinking_signature_for(idx);
            self.stop_block(idx);
        }
    }

    fn close_text(&mut self) {
        if let Some(idx) = self.text_index.take() {
            self.stop_block(idx);
        }
    }

    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.close_thinking();
        if self.text_index.is_none() {
            let idx = self.start_block("text", serde_json::json!({"type": "text", "text": ""}));
            self.text_index = Some(idx);
        }
        self.output_tokens = self.output_tokens.saturating_add(text.len() as u64);
        self.push_delta(
            self.text_index.unwrap(),
            serde_json::json!({"type": "text_delta", "text": text}),
        );
    }

    fn push_thinking(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.thinking_index.is_none() {
            let idx = self.start_block(
                "thinking",
                serde_json::json!({"type": "thinking", "thinking": ""}),
            );
            self.thinking_index = Some(idx);
            self.thinking_signature_emitted = false;
        }
        self.output_tokens = self.output_tokens.saturating_add(text.len() as u64);
        self.push_delta(
            self.thinking_index.unwrap(),
            serde_json::json!({"type": "thinking_delta", "thinking": text}),
        );
    }

    fn ensure_thinking_signature_for(&mut self, idx: u32) {
        if self.thinking_signature_emitted {
            return;
        }
        self.thinking_signature_emitted = true;
        self.push_delta(
            idx,
            serde_json::json!({
                "type": "signature_delta",
                "signature": format!("cab_{}", uuid::Uuid::new_v4().simple())
            }),
        );
    }

    fn ensure_tool(
        &mut self,
        item_id: &str,
        call_id: &str,
        output_index: Option<u64>,
        name: &str,
    ) -> String {
        let existing_key = (!call_id.is_empty() && self.alias_to_tool.contains_key(call_id))
            .then(|| self.alias_to_tool.get(call_id).cloned())
            .flatten()
            .or_else(|| {
                (!item_id.is_empty() && self.alias_to_tool.contains_key(item_id))
                    .then(|| self.alias_to_tool.get(item_id).cloned())
                    .flatten()
            })
            .or_else(|| {
                output_index.and_then(|idx| self.alias_to_tool.get(&format!("idx:{idx}")).cloned())
            });

        let key = if let Some(k) = existing_key {
            k
        } else {
            let canonical = if !call_id.is_empty() {
                call_id.to_string()
            } else if !item_id.is_empty() {
                item_id.to_string()
            } else if let Some(idx) = output_index {
                format!("idx:{idx}")
            } else {
                format!("tool_{}", self.tools.len())
            };
            let block_index = self.alloc_index();
            let display_id = if !call_id.is_empty() {
                call_id.to_string()
            } else if !item_id.is_empty() {
                item_id.to_string()
            } else {
                format!("toolu_{}", uuid::Uuid::new_v4().simple())
            };
            self.tools.insert(
                canonical.clone(),
                ToolTracker {
                    block_index,
                    id: display_id,
                    name: name.to_string(),
                    pending_args: String::new(),
                    started: false,
                    stopped: false,
                    streamed_args: false,
                },
            );
            canonical
        };

        if !call_id.is_empty() {
            self.alias_to_tool.insert(call_id.to_string(), key.clone());
        }
        if !item_id.is_empty() {
            self.alias_to_tool.insert(item_id.to_string(), key.clone());
        }
        if let Some(idx) = output_index {
            self.alias_to_tool.insert(format!("idx:{idx}"), key.clone());
        }

        if let Some(tool) = self.tools.get_mut(&key) {
            if tool.name.is_empty() && !name.is_empty() {
                tool.name = name.to_string();
            }
            if !call_id.is_empty()
                && (tool.id.is_empty()
                    || tool.id.starts_with("toolu_")
                    || tool.id.starts_with("fc_"))
            {
                tool.id = call_id.to_string();
            }
        }

        key
    }

    fn push_tool_args(&mut self, key: &str, partial: &str) {
        if partial.is_empty() {
            return;
        }
        let Some(tool) = self.tools.get(key) else {
            return;
        };
        if tool.stopped {
            return;
        }
        if tool.name.is_empty() {
            if let Some(t) = self.tools.get_mut(key) {
                t.pending_args.push_str(partial);
            }
            return;
        }
        let (block_index, id, name, started) = (
            tool.block_index,
            tool.id.clone(),
            tool.name.clone(),
            tool.started,
        );
        self.close_thinking();
        self.close_text();
        if !started {
            if let Some(t) = self.tools.get_mut(key) {
                t.started = true;
            }
            self.ensure_message();
            self.pending.push(anthropic_stream_event(
                "content_block_start",
                serde_json::json!({
                    "type": "content_block_start",
                    "index": block_index,
                    "content_block": {
                        "type": "tool_use",
                        "id": if id.is_empty() { format!("toolu_{}", uuid::Uuid::new_v4().simple()) } else { id },
                        "name": name,
                        "input": {}
                    }
                }),
            ));
        }
        if let Some(t) = self.tools.get_mut(key) {
            t.streamed_args = true;
        }
        self.output_tokens = self.output_tokens.saturating_add(partial.len() as u64);
        self.push_delta(
            block_index,
            serde_json::json!({"type": "input_json_delta", "partial_json": partial}),
        );
    }

    fn flush_tool_pending_args(&mut self, key: &str) {
        let pending = self
            .tools
            .get_mut(key)
            .filter(|t| !t.name.is_empty() && !t.pending_args.is_empty())
            .map(|t| std::mem::take(&mut t.pending_args));
        if let Some(args) = pending {
            self.push_tool_args(key, &args);
        }
    }

    fn stop_tool(&mut self, key: &str) {
        self.flush_tool_pending_args(key);
        let stopped_index = self
            .tools
            .get_mut(key)
            .filter(|tool| tool.started && !tool.stopped)
            .map(|tool| {
                tool.stopped = true;
                tool.block_index
            });
        if let Some(index) = stopped_index {
            self.stop_block(index);
        }
    }

    fn finish(&mut self, stop_reason: &str, usage: Option<&Value>) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.ensure_message();

        let keys: Vec<String> = self.tools.keys().cloned().collect();
        for key in keys {
            self.flush_tool_pending_args(&key);
            let need_start = self
                .tools
                .get(&key)
                .map(|t| !t.started && !t.name.is_empty())
                .unwrap_or(false);
            if need_start {
                self.push_tool_args(&key, "{}");
            }
            self.stop_tool(&key);
        }

        self.close_thinking();
        self.close_text();

        let mut delta_usage = serde_json::json!({
            "output_tokens": usage
                .and_then(|u| u.get("output_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(self.output_tokens)
        });
        if let Some(u) = usage {
            if let Some(in_tok) = u.get("input_tokens").and_then(|v| v.as_u64()) {
                delta_usage["input_tokens"] = serde_json::json!(in_tok);
            }
            if let Some(cached) = u
                .get("input_tokens_details")
                .and_then(|d| d.get("cached_tokens"))
                .and_then(|v| v.as_u64())
                .filter(|&v| v > 0)
            {
                delta_usage["cache_read_input_tokens"] = serde_json::json!(cached);
            }
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

    fn pop(&mut self) -> Option<Bytes> {
        if self.pending.is_empty() {
            None
        } else {
            Some(self.pending.remove(0))
        }
    }
}

/// Real-time OpenAI Responses SSE → Anthropic Messages SSE.
pub fn transform_responses_sse_to_anthropic<S, E>(
    upstream: S,
    model: String,
) -> impl Stream<Item = Result<Bytes, E>>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
{
    let mut upstream = upstream;
    let mut lines = LineBuffer::new();
    let mut emitter = AnthropicSseEmitter::new(model);
    let mut done = false;

    futures::stream::poll_fn(move |cx| {
        loop {
            if let Some(out) = emitter.pop() {
                return Poll::Ready(Some(Ok(out)));
            }
            if done {
                return Poll::Ready(None);
            }

            let process_line = |line: &str, emitter: &mut AnthropicSseEmitter| {
                let Some(payload) = sse_line_payload(line) else {
                    return;
                };
                if payload == "[DONE]" {
                    let stop = if emitter.tools.values().any(|t| t.started) {
                        "tool_use"
                    } else {
                        "end_turn"
                    };
                    emitter.finish(stop, None);
                    return;
                }
                let Ok(event) = serde_json::from_str::<Value>(payload) else {
                    return;
                };
                let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
                match event_type {
                    "response.output_text.delta" => {
                        if let Some(delta) = event.get("delta").and_then(|d| d.as_str()) {
                            emitter.push_text(delta);
                        }
                    }
                    "response.output_text.done" | "response.content_part.done" => {
                        emitter.close_text();
                    }
                    "response.reasoning_text.delta" | "response.reasoning_summary_text.delta" => {
                        if let Some(delta) = event.get("delta").and_then(|d| d.as_str()) {
                            emitter.push_thinking(delta);
                        }
                    }
                    "response.reasoning_text.done" => {
                        emitter.close_thinking();
                    }
                    "response.output_item.added" => {
                        if let Some(item) = event.get("item") {
                            let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
                            if item_type == "function_call" {
                                let item_id = item.get("id").and_then(|v| v.as_str()).unwrap_or("");
                                let call_id =
                                    item.get("call_id").and_then(|v| v.as_str()).unwrap_or("");
                                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                                let output_index =
                                    event.get("output_index").and_then(|v| v.as_u64());
                                let key = emitter.ensure_tool(item_id, call_id, output_index, name);
                                if let Some(args) = item.get("arguments").and_then(|a| a.as_str())
                                    && !args.is_empty()
                                {
                                    emitter.push_tool_args(&key, args);
                                }
                            } else if item_type == "message" {
                                emitter.close_thinking();
                            }
                        }
                    }
                    "response.function_call_arguments.delta" => {
                        let item_id = event
                            .get("item_id")
                            .or_else(|| event.get("call_id"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let output_index = event.get("output_index").and_then(|v| v.as_u64());
                        let key = emitter.ensure_tool(item_id, "", output_index, "");
                        if let Some(delta) = event.get("delta").and_then(|d| d.as_str()) {
                            emitter.push_tool_args(&key, delta);
                        }
                    }
                    "response.function_call_arguments.done" => {
                        let item_id = event
                            .get("item_id")
                            .or_else(|| event.get("call_id"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let output_index = event.get("output_index").and_then(|v| v.as_u64());
                        let name = event.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        let key = emitter.ensure_tool(item_id, "", output_index, name);
                        let streamed = emitter
                            .tools
                            .get(&key)
                            .map(|t| t.streamed_args)
                            .unwrap_or(false);
                        if !streamed
                            && let Some(args) = event.get("arguments").and_then(|a| a.as_str())
                            && !args.is_empty()
                        {
                            emitter.push_tool_args(&key, args);
                        }
                        emitter.stop_tool(&key);
                    }
                    "response.output_item.done" => {
                        if let Some(item) = event.get("item") {
                            let item_type = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
                            if item_type == "function_call" {
                                let item_id = item.get("id").and_then(|v| v.as_str()).unwrap_or("");
                                let call_id =
                                    item.get("call_id").and_then(|v| v.as_str()).unwrap_or("");
                                let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                                let output_index =
                                    event.get("output_index").and_then(|v| v.as_u64());
                                let key = emitter.ensure_tool(item_id, call_id, output_index, name);
                                let streamed = emitter
                                    .tools
                                    .get(&key)
                                    .map(|t| t.streamed_args)
                                    .unwrap_or(false);
                                if !streamed {
                                    let args = item
                                        .get("arguments")
                                        .and_then(|a| a.as_str())
                                        .unwrap_or("{}");
                                    emitter.push_tool_args(
                                        &key,
                                        if args.is_empty() { "{}" } else { args },
                                    );
                                }
                                emitter.stop_tool(&key);
                            } else if item_type == "message" {
                                emitter.close_text();
                            } else if item_type == "reasoning" {
                                emitter.close_thinking();
                            }
                        }
                    }
                    "response.completed" => {
                        if let Some(outputs) = event
                            .get("response")
                            .and_then(|r| r.get("output"))
                            .and_then(|o| o.as_array())
                        {
                            for item in outputs {
                                if item.get("type").and_then(|t| t.as_str())
                                    == Some("function_call")
                                {
                                    let item_id =
                                        item.get("id").and_then(|v| v.as_str()).unwrap_or("");
                                    let call_id =
                                        item.get("call_id").and_then(|v| v.as_str()).unwrap_or("");
                                    let name =
                                        item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                                    let key = emitter.ensure_tool(item_id, call_id, None, name);
                                    let args = item
                                        .get("arguments")
                                        .and_then(|a| a.as_str())
                                        .unwrap_or("{}");
                                    let tool_started =
                                        emitter.tools.get(&key).map(|t| t.started).unwrap_or(false);
                                    if !tool_started {
                                        emitter.push_tool_args(
                                            &key,
                                            if args.is_empty() { "{}" } else { args },
                                        );
                                    }
                                    emitter.stop_tool(&key);
                                }
                            }
                        }
                        let stop = if emitter.tools.values().any(|t| t.started) {
                            "tool_use"
                        } else {
                            "end_turn"
                        };
                        let usage = event.get("response").and_then(|r| r.get("usage"));
                        emitter.finish(stop, usage);
                    }
                    _ => {}
                }
            };

            match Pin::new(&mut upstream).poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => {
                    for line in lines.push(&bytes) {
                        process_line(&line, &mut emitter);
                    }
                }
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e))),
                Poll::Ready(None) => {
                    if let Some(line) = lines.flush() {
                        process_line(&line, &mut emitter);
                    }
                    if !emitter.finished {
                        let stop = if emitter.tools.values().any(|t| t.started) {
                            "tool_use"
                        } else {
                            "end_turn"
                        };
                        emitter.finish(stop, None);
                    }
                    done = true;
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    })
}

/// OpenAI Chat SSE → OpenAI Responses SSE (for Codex client + chat upstream).
struct ChatFunctionCall {
    output_index: u32,
    item_id: String,
    call_id: String,
    name: String,
    arguments: String,
    added: bool,
}

/// Chat Completions SSE → Responses SSE.
///
/// Field mapping follows the official APIs, not an IR convenience layer:
/// Chat `tool_calls[].id` → Responses `item.call_id`;
/// Responses `item.id` is a generated `fc_*` used as `item_id` on argument events;
/// `finish_reason: null` is not a terminal signal.
struct ChatToResponsesConverter {
    model: String,
    response_id: String,
    pending: Vec<Bytes>,
    started: bool,
    finishing: bool,
    completed: bool,
    message_item_id: String,
    message_output_index: u32,
    message_added: bool,
    message_done: bool,
    accumulated_text: String,
    next_output_index: u32,
    tools: HashMap<u64, ChatFunctionCall>,
    tool_order: Vec<u64>,
    usage: Option<Value>,
}

impl ChatToResponsesConverter {
    fn new(model: String) -> Self {
        Self {
            model,
            response_id: format!("resp_{}", uuid::Uuid::new_v4().simple()),
            pending: Vec::new(),
            started: false,
            finishing: false,
            completed: false,
            message_item_id: format!("msg_{}", uuid::Uuid::new_v4().simple()),
            message_output_index: 0,
            message_added: false,
            message_done: false,
            accumulated_text: String::new(),
            next_output_index: 0,
            tools: HashMap::new(),
            tool_order: Vec::new(),
            usage: None,
        }
    }

    fn emit(&mut self, event_type: &str, data: Value) {
        self.pending.push(Bytes::from(format!(
            "event: {event_type}\ndata: {data}\n\n"
        )));
    }

    fn alloc_output_index(&mut self) -> u32 {
        let i = self.next_output_index;
        self.next_output_index += 1;
        i
    }

    fn ensure_started(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        let response = serde_json::json!({
            "id": self.response_id,
            "object": "response",
            "model": self.model,
            "status": "in_progress",
            "output": [],
        });
        self.emit(
            "response.created",
            serde_json::json!({"type": "response.created", "response": response}),
        );
        self.emit(
            "response.in_progress",
            serde_json::json!({"type": "response.in_progress", "response": response}),
        );
    }

    fn ensure_message_started(&mut self) {
        if self.message_added {
            return;
        }
        self.ensure_started();
        self.message_output_index = self.alloc_output_index();
        self.message_added = true;
        self.emit(
            "response.output_item.added",
            serde_json::json!({
                "type": "response.output_item.added",
                "output_index": self.message_output_index,
                "item": {
                    "id": self.message_item_id,
                    "type": "message",
                    "role": "assistant",
                    "status": "in_progress",
                    "content": [],
                }
            }),
        );
        self.emit(
            "response.content_part.added",
            serde_json::json!({
                "type": "response.content_part.added",
                "item_id": self.message_item_id,
                "output_index": self.message_output_index,
                "content_index": 0,
                "part": {"type": "output_text", "text": "", "annotations": [], "logprobs": []},
            }),
        );
    }

    fn on_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.ensure_message_started();
        self.accumulated_text.push_str(text);
        self.emit(
            "response.output_text.delta",
            serde_json::json!({
                "type": "response.output_text.delta",
                "item_id": self.message_item_id,
                "output_index": self.message_output_index,
                "content_index": 0,
                "delta": text,
            }),
        );
    }

    fn tool_entry(&mut self, chat_index: u64) -> &mut ChatFunctionCall {
        if !self.tools.contains_key(&chat_index) {
            self.tool_order.push(chat_index);
            self.tools.insert(
                chat_index,
                ChatFunctionCall {
                    output_index: 0,
                    item_id: format!("fc_{}", uuid::Uuid::new_v4().simple()),
                    call_id: String::new(),
                    name: String::new(),
                    arguments: String::new(),
                    added: false,
                },
            );
        }
        self.tools.get_mut(&chat_index).unwrap()
    }

    fn ensure_tool_added(&mut self, chat_index: u64) {
        let (name, call_id, item_id, buffered) = {
            let Some(tool) = self.tools.get(&chat_index) else {
                return;
            };
            if tool.added || tool.name.is_empty() {
                return;
            }
            (
                tool.name.clone(),
                tool.call_id.clone(),
                tool.item_id.clone(),
                tool.arguments.clone(),
            )
        };
        self.close_message();
        let output_index = self.alloc_output_index();
        if let Some(tool) = self.tools.get_mut(&chat_index) {
            tool.output_index = output_index;
            if tool.call_id.is_empty() {
                tool.call_id = format!("call_{chat_index}");
            }
            tool.added = true;
        }
        let call_id = self
            .tools
            .get(&chat_index)
            .map(|t| t.call_id.clone())
            .unwrap_or(call_id);
        self.emit(
            "response.output_item.added",
            serde_json::json!({
                "type": "response.output_item.added",
                "output_index": output_index,
                "item": {
                    "id": item_id,
                    "type": "function_call",
                    "status": "in_progress",
                    "call_id": call_id,
                    "name": name,
                    "arguments": "",
                }
            }),
        );
        if !buffered.is_empty() {
            self.emit(
                "response.function_call_arguments.delta",
                serde_json::json!({
                    "type": "response.function_call_arguments.delta",
                    "item_id": item_id,
                    "output_index": output_index,
                    "delta": buffered,
                }),
            );
        }
    }

    fn on_tool_calls(&mut self, tool_calls: &[Value]) {
        self.ensure_started();
        for call in tool_calls {
            let chat_index = call.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
            if let Some(id) = call.get("id").and_then(|v| v.as_str())
                && !id.is_empty()
            {
                self.tool_entry(chat_index).call_id = id.to_string();
            }
            if let Some(name) = call
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                && !name.is_empty()
            {
                self.tool_entry(chat_index).name = name.to_string();
                self.ensure_tool_added(chat_index);
            }
            if let Some(args) = call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|a| a.as_str())
                && !args.is_empty()
            {
                let added = self.tool_entry(chat_index).added;
                if added {
                    let (item_id, output_index) = {
                        let tool = self.tools.get(&chat_index).unwrap();
                        (tool.item_id.clone(), tool.output_index)
                    };
                    self.tools
                        .get_mut(&chat_index)
                        .unwrap()
                        .arguments
                        .push_str(args);
                    self.emit(
                        "response.function_call_arguments.delta",
                        serde_json::json!({
                            "type": "response.function_call_arguments.delta",
                            "item_id": item_id,
                            "output_index": output_index,
                            "delta": args,
                        }),
                    );
                } else {
                    self.tool_entry(chat_index).arguments.push_str(args);
                    self.ensure_tool_added(chat_index);
                }
            }
        }
    }

    fn close_message(&mut self) {
        if !self.message_added || self.message_done {
            return;
        }
        self.message_done = true;
        let text = self.accumulated_text.clone();
        if !text.is_empty() {
            self.emit(
                "response.output_text.done",
                serde_json::json!({
                    "type": "response.output_text.done",
                    "item_id": self.message_item_id,
                    "output_index": self.message_output_index,
                    "content_index": 0,
                    "text": text,
                }),
            );
            self.emit(
                "response.content_part.done",
                serde_json::json!({
                    "type": "response.content_part.done",
                    "item_id": self.message_item_id,
                    "output_index": self.message_output_index,
                    "content_index": 0,
                    "part": {"type": "output_text", "text": text, "annotations": [], "logprobs": []},
                }),
            );
        }
        self.emit(
            "response.output_item.done",
            serde_json::json!({
                "type": "response.output_item.done",
                "output_index": self.message_output_index,
                "item": {
                    "id": self.message_item_id,
                    "type": "message",
                    "role": "assistant",
                    "status": "completed",
                    "content": [{"type": "output_text", "text": text}],
                }
            }),
        );
    }

    fn close_tools(&mut self) {
        let order = self.tool_order.clone();
        for chat_index in order {
            self.ensure_tool_added(chat_index);
            let Some(tool) = self.tools.get(&chat_index) else {
                continue;
            };
            if !tool.added {
                continue;
            }
            let item_id = tool.item_id.clone();
            let call_id = tool.call_id.clone();
            let name = tool.name.clone();
            let arguments = tool.arguments.clone();
            let output_index = tool.output_index;
            self.emit(
                "response.function_call_arguments.done",
                serde_json::json!({
                    "type": "response.function_call_arguments.done",
                    "item_id": item_id,
                    "output_index": output_index,
                    "name": name,
                    "arguments": arguments,
                }),
            );
            self.emit(
                "response.output_item.done",
                serde_json::json!({
                    "type": "response.output_item.done",
                    "output_index": output_index,
                    "item": {
                        "id": item_id,
                        "type": "function_call",
                        "status": "completed",
                        "call_id": call_id,
                        "name": name,
                        "arguments": arguments,
                    }
                }),
            );
        }
    }

    fn on_usage(&mut self, usage: &Value) {
        let input = usage
            .get("prompt_tokens")
            .or_else(|| usage.get("input_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let output = usage
            .get("completion_tokens")
            .or_else(|| usage.get("output_tokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        self.usage = Some(serde_json::json!({
            "input_tokens": input,
            "output_tokens": output,
            "total_tokens": input + output,
        }));
    }

    fn on_finish(&mut self) {
        if self.finishing {
            return;
        }
        self.finishing = true;
        self.ensure_started();
        self.close_message();
        self.close_tools();
    }

    fn emit_completed(&mut self) {
        if self.completed {
            return;
        }
        self.completed = true;
        if !self.finishing {
            self.on_finish();
        }
        let mut output = Vec::new();
        if self.message_added {
            output.push(serde_json::json!({
                "id": self.message_item_id,
                "type": "message",
                "role": "assistant",
                "status": "completed",
                "content": [{"type": "output_text", "text": self.accumulated_text}],
            }));
        }
        for chat_index in &self.tool_order {
            if let Some(tool) = self.tools.get(chat_index)
                && tool.added
            {
                output.push(serde_json::json!({
                    "id": tool.item_id,
                    "type": "function_call",
                    "status": "completed",
                    "call_id": tool.call_id,
                    "name": tool.name,
                    "arguments": tool.arguments,
                }));
            }
        }
        let mut response = serde_json::json!({
            "id": self.response_id,
            "object": "response",
            "model": self.model,
            "status": "completed",
            "output_text": self.accumulated_text,
            "output": output,
        });
        if let Some(usage) = &self.usage {
            response["usage"] = usage.clone();
        }
        self.emit(
            "response.completed",
            serde_json::json!({"type": "response.completed", "response": response}),
        );
    }

    fn process_line(&mut self, line: &str) {
        let Some(payload) = sse_line_payload(line) else {
            return;
        };
        if payload == "[DONE]" {
            self.emit_completed();
            return;
        }
        let Ok(chunk) = serde_json::from_str::<Value>(payload) else {
            return;
        };
        self.ensure_started();
        let choices = chunk.get("choices").and_then(|c| c.as_array());
        if let Some(choices) = choices {
            for choice in choices {
                let delta = choice.get("delta");
                if let Some(text) = delta
                    .and_then(|d| d.get("content"))
                    .and_then(|c| c.as_str())
                {
                    self.on_text(text);
                }
                if let Some(Value::Array(tool_calls)) = delta.and_then(|d| d.get("tool_calls")) {
                    self.on_tool_calls(tool_calls);
                }
                if let Some(reason) = choice
                    .get("finish_reason")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                {
                    let _ = reason;
                    self.on_finish();
                }
            }
        }
        if let Some(usage) = chunk.get("usage").filter(|u| !u.is_null()) {
            self.on_usage(usage);
        }
        if self.finishing && self.usage.is_some() {
            self.emit_completed();
        }
    }
}

pub fn transform_openai_chat_sse_to_responses<S, E>(
    upstream: S,
    model: String,
) -> impl Stream<Item = Result<Bytes, E>>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
{
    let mut upstream = upstream;
    let mut lines = LineBuffer::new();
    let mut converter = ChatToResponsesConverter::new(model);

    futures::stream::poll_fn(move |cx| {
        loop {
            if let Some(out) = pop_pending_front(&mut converter.pending) {
                return Poll::Ready(Some(Ok(out)));
            }
            if converter.completed {
                return Poll::Ready(None);
            }

            match Pin::new(&mut upstream).poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => {
                    for line in lines.push(&bytes) {
                        converter.process_line(&line);
                    }
                }
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e))),
                Poll::Ready(None) => {
                    if let Some(line) = lines.flush() {
                        converter.process_line(&line);
                    }
                    converter.emit_completed();
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    })
}

/// Responses SSE → OpenAI Chat SSE.
pub fn transform_responses_sse_to_openai_chat<S, E>(
    upstream: S,
    _model: String,
) -> impl Stream<Item = Result<Bytes, E>>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
{
    let mut upstream = upstream;
    let mut lines = LineBuffer::new();
    let mut pending: Vec<Bytes> = Vec::new();
    let mut tool_args: HashMap<String, String> = HashMap::new();
    let mut tool_names: HashMap<String, String> = HashMap::new();
    let mut tool_index: HashMap<String, u64> = HashMap::new();
    let mut next_tool_idx = 0u64;
    let mut done = false;

    futures::stream::poll_fn(move |cx| {
        loop {
            if let Some(out) = pop_pending_front(&mut pending) {
                return Poll::Ready(Some(Ok(out)));
            }
            if done {
                return Poll::Ready(None);
            }

            let push_chat = |pending: &mut Vec<Bytes>, data: Value| {
                pending.push(Bytes::from(format!("data: {data}\n\n")));
            };

            let process = |line: &str,
                           pending: &mut Vec<Bytes>,
                           tool_args: &mut HashMap<String, String>,
                           tool_names: &mut HashMap<String, String>,
                           tool_index: &mut HashMap<String, u64>,
                           next_tool_idx: &mut u64| {
                let Some(payload) = sse_line_payload(line) else {
                    return false;
                };
                if payload == "[DONE]" {
                    return true;
                }
                let Ok(event) = serde_json::from_str::<Value>(payload) else {
                    return false;
                };
                match event.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                    "response.output_text.delta" => {
                        if let Some(delta) = event.get("delta").and_then(|d| d.as_str()) {
                            push_chat(
                                pending,
                                serde_json::json!({
                                    "choices": [{"index": 0, "delta": {"content": delta}, "finish_reason": null}]
                                }),
                            );
                        }
                    }
                    "response.output_item.added" => {
                        if let Some(item) = event.get("item")
                            && item.get("type").and_then(|t| t.as_str()) == Some("function_call")
                        {
                            let call_id = item
                                .get("call_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("call_0")
                                .to_string();
                            let name = item
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let idx = *next_tool_idx;
                            *next_tool_idx += 1;
                            tool_index.insert(call_id.clone(), idx);
                            if let Some(item_id) = item.get("id").and_then(|v| v.as_str()) {
                                tool_index.insert(item_id.to_string(), idx);
                            }
                            tool_names.insert(call_id.clone(), name.clone());
                            push_chat(
                                pending,
                                serde_json::json!({
                                    "choices": [{"index": 0, "delta": {"tool_calls": [{
                                        "index": idx, "id": call_id, "type": "function",
                                        "function": {"name": name, "arguments": ""}
                                    }]}, "finish_reason": null}]
                                }),
                            );
                        }
                    }
                    "response.function_call_arguments.delta" => {
                        let call_id = event
                            .get("item_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("call_0")
                            .to_string();
                        if let Some(delta) = event.get("delta").and_then(|d| d.as_str()) {
                            tool_args
                                .entry(call_id.clone())
                                .or_default()
                                .push_str(delta);
                            let idx = tool_index.get(&call_id).copied().unwrap_or(0);
                            push_chat(
                                pending,
                                serde_json::json!({
                                    "choices": [{"index": 0, "delta": {"tool_calls": [{
                                        "index": idx,
                                        "function": {"arguments": delta}
                                    }]}, "finish_reason": null}]
                                }),
                            );
                        }
                    }
                    "response.completed" => {
                        let stop = if tool_names.is_empty() {
                            "stop"
                        } else {
                            "tool_calls"
                        };
                        push_chat(
                            pending,
                            serde_json::json!({
                                "choices": [{"index": 0, "delta": {}, "finish_reason": stop}]
                            }),
                        );
                        push_chat(pending, serde_json::json!("[DONE]"));
                        return true;
                    }
                    _ => {}
                }
                false
            };

            match Pin::new(&mut upstream).poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => {
                    for line in lines.push(&bytes) {
                        if process(
                            &line,
                            &mut pending,
                            &mut tool_args,
                            &mut tool_names,
                            &mut tool_index,
                            &mut next_tool_idx,
                        ) {
                            done = true;
                            break;
                        }
                    }
                }
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e))),
                Poll::Ready(None) => {
                    if let Some(line) = lines.flush() {
                        let _ = process(
                            &line,
                            &mut pending,
                            &mut tool_args,
                            &mut tool_names,
                            &mut tool_index,
                            &mut next_tool_idx,
                        );
                    }
                    if !done {
                        push_chat(
                            &mut pending,
                            serde_json::json!({"choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]}),
                        );
                        push_chat(&mut pending, serde_json::json!("[DONE]"));
                    }
                    done = true;
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    })
}

fn pop_pending_front(pending: &mut Vec<Bytes>) -> Option<Bytes> {
    if pending.is_empty() {
        None
    } else {
        Some(pending.remove(0))
    }
}

fn push_openai_chat_sse(pending: &mut Vec<Bytes>, delta: Value, finish_reason: Option<&str>) {
    pending.push(Bytes::from(format!(
        "data: {}\n\n",
        serde_json::json!({
            "choices": [{"index": 0, "delta": delta, "finish_reason": finish_reason}]
        })
    )));
}

fn push_responses_sse(pending: &mut Vec<Bytes>, event_type: &str, data: Value) {
    pending.push(Bytes::from(format!(
        "event: {event_type}\ndata: {data}\n\n"
    )));
}

fn anthropic_stop_to_openai_finish(stop: &str) -> &str {
    match stop {
        "tool_use" => "tool_calls",
        "max_tokens" => "length",
        _ => "stop",
    }
}

/// Anthropic Messages SSE → OpenAI Chat SSE.
pub fn transform_anthropic_sse_to_openai_chat<S, E>(
    upstream: S,
) -> impl Stream<Item = Result<Bytes, E>>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
{
    let mut upstream = upstream;
    let mut lines = LineBuffer::new();
    let mut pending: Vec<Bytes> = Vec::new();
    let mut block_tools: HashMap<u32, (String, String, u64)> = HashMap::new();
    let mut next_tool_idx = 0u64;
    let mut finish_reason = "stop".to_string();
    let mut done = false;
    let mut input_tokens: Option<i64> = None;
    let mut output_tokens: Option<i64> = None;

    futures::stream::poll_fn(move |cx| {
        loop {
            if let Some(out) = pop_pending_front(&mut pending) {
                return Poll::Ready(Some(Ok(out)));
            }
            if done {
                return Poll::Ready(None);
            }

            let process = |line: &str,
                           pending: &mut Vec<Bytes>,
                           block_tools: &mut HashMap<u32, (String, String, u64)>,
                           next_tool_idx: &mut u64,
                           finish_reason: &mut String,
                           input_tokens: &mut Option<i64>,
                           output_tokens: &mut Option<i64>| {
                let Some(payload) = sse_line_payload(line) else {
                    return false;
                };
                let Ok(event) = serde_json::from_str::<Value>(payload) else {
                    return false;
                };
                match event.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                    "content_block_start" => {
                        let index = event.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        if let Some(block) = event.get("content_block")
                            && block.get("type").and_then(|t| t.as_str()) == Some("tool_use")
                        {
                            let id = block
                                .get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("call_0")
                                .to_string();
                            let name = block
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let tool_idx = *next_tool_idx;
                            *next_tool_idx += 1;
                            block_tools.insert(index, (id.clone(), name.clone(), tool_idx));
                            push_openai_chat_sse(
                                pending,
                                serde_json::json!({
                                    "tool_calls": [{
                                        "index": tool_idx,
                                        "id": id,
                                        "type": "function",
                                        "function": {"name": name, "arguments": ""}
                                    }]
                                }),
                                None,
                            );
                        }
                    }
                    "content_block_delta" => {
                        let index = event.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let delta = event.get("delta").unwrap_or(&Value::Null);
                        match delta.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                            "text_delta" => {
                                if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                    push_openai_chat_sse(
                                        pending,
                                        serde_json::json!({"content": text}),
                                        None,
                                    );
                                }
                            }
                            "thinking_delta" => {
                                if let Some(text) = delta.get("thinking").and_then(|t| t.as_str()) {
                                    push_openai_chat_sse(
                                        pending,
                                        serde_json::json!({"reasoning_content": text}),
                                        None,
                                    );
                                }
                            }
                            "input_json_delta" => {
                                if let Some(partial) =
                                    delta.get("partial_json").and_then(|t| t.as_str())
                                    && let Some((_, _, tool_idx)) = block_tools.get(&index)
                                {
                                    push_openai_chat_sse(
                                        pending,
                                        serde_json::json!({
                                            "tool_calls": [{
                                                "index": tool_idx,
                                                "function": {"arguments": partial}
                                            }]
                                        }),
                                        None,
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                    "message_start" => {
                        if let Some(in_tok) = event
                            .get("message")
                            .and_then(|m| m.get("usage"))
                            .and_then(|usage| usage.get("input_tokens"))
                            .and_then(|v| v.as_i64())
                        {
                            *input_tokens = Some(in_tok);
                        }
                    }
                    "message_delta" => {
                        if let Some(stop) = event
                            .get("delta")
                            .and_then(|d| d.get("stop_reason"))
                            .and_then(|s| s.as_str())
                        {
                            *finish_reason = anthropic_stop_to_openai_finish(stop).to_string();
                        }
                        if let Some(out_tok) = event
                            .get("usage")
                            .and_then(|usage| usage.get("output_tokens"))
                            .and_then(|v| v.as_i64())
                        {
                            *output_tokens = Some(out_tok);
                        }
                    }
                    "message_stop" => {
                        let mut final_chunk = serde_json::json!({
                            "choices": [{"index": 0, "delta": {}, "finish_reason": finish_reason}]
                        });
                        let mut usage = serde_json::json!({});
                        if let Some(inp) = input_tokens {
                            usage["prompt_tokens"] = serde_json::json!(inp);
                        }
                        if let Some(out) = output_tokens {
                            usage["completion_tokens"] = serde_json::json!(out);
                        }
                        if !usage.as_object().map(|m| m.is_empty()).unwrap_or(true) {
                            final_chunk["usage"] = usage;
                        }
                        pending.push(Bytes::from(format!("data: {}\n\n", final_chunk)));
                        pending.push(Bytes::from("data: [DONE]\n\n".to_string()));
                        return true;
                    }
                    _ => {}
                }
                false
            };

            match Pin::new(&mut upstream).poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => {
                    for line in lines.push(&bytes) {
                        if process(
                            &line,
                            &mut pending,
                            &mut block_tools,
                            &mut next_tool_idx,
                            &mut finish_reason,
                            &mut input_tokens,
                            &mut output_tokens,
                        ) {
                            done = true;
                            break;
                        }
                    }
                }
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e))),
                Poll::Ready(None) => {
                    if let Some(line) = lines.flush() {
                        let _ = process(
                            &line,
                            &mut pending,
                            &mut block_tools,
                            &mut next_tool_idx,
                            &mut finish_reason,
                            &mut input_tokens,
                            &mut output_tokens,
                        );
                    }
                    if !done {
                        push_openai_chat_sse(
                            &mut pending,
                            serde_json::json!({}),
                            Some(&finish_reason),
                        );
                        pending.push(Bytes::from("data: [DONE]\n\n".to_string()));
                    }
                    done = true;
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    })
}

/// Anthropic Messages SSE → OpenAI Responses SSE.
pub fn transform_anthropic_sse_to_responses<S, E>(
    upstream: S,
    model: String,
) -> impl Stream<Item = Result<Bytes, E>>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin,
{
    let mut upstream = upstream;
    let mut lines = LineBuffer::new();
    let response_id = format!("resp_{}", uuid::Uuid::new_v4().simple());
    let item_id = format!("msg_{}", uuid::Uuid::new_v4().simple());
    let mut pending: Vec<Bytes> = Vec::new();
    let mut started = false;
    let mut block_tools: HashMap<u32, String> = HashMap::new();
    let mut accumulated = String::new();
    let mut done = false;

    futures::stream::poll_fn(move |cx| {
        loop {
            if let Some(out) = pop_pending_front(&mut pending) {
                return Poll::Ready(Some(Ok(out)));
            }
            if done {
                return Poll::Ready(None);
            }

            let process = |line: &str,
                           pending: &mut Vec<Bytes>,
                           started: &mut bool,
                           block_tools: &mut HashMap<u32, String>,
                           accumulated: &mut String| {
                let Some(payload) = sse_line_payload(line) else {
                    return false;
                };
                let Ok(event) = serde_json::from_str::<Value>(payload) else {
                    return false;
                };
                if !*started {
                    *started = true;
                    push_responses_sse(
                        pending,
                        "response.created",
                        serde_json::json!({
                            "type": "response.created",
                            "response": {"id": response_id, "object": "response", "model": model, "status": "in_progress"}
                        }),
                    );
                    push_responses_sse(
                        pending,
                        "response.output_item.added",
                        serde_json::json!({
                            "type": "response.output_item.added",
                            "output_index": 0,
                            "item": {"id": item_id, "type": "message", "role": "assistant", "status": "in_progress"}
                        }),
                    );
                }
                match event.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                    "content_block_start" => {
                        let index = event.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        if let Some(block) = event.get("content_block")
                            && block.get("type").and_then(|t| t.as_str()) == Some("tool_use")
                        {
                            let call_id = block
                                .get("id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("call_0")
                                .to_string();
                            let name = block
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            block_tools.insert(index, call_id.clone());
                            push_responses_sse(
                                pending,
                                "response.output_item.added",
                                serde_json::json!({
                                    "type": "response.output_item.added",
                                    "output_index": 1,
                                    "item": {"type": "function_call", "call_id": call_id, "name": name, "arguments": ""}
                                }),
                            );
                        }
                    }
                    "content_block_delta" => {
                        let index = event.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let delta = event.get("delta").unwrap_or(&Value::Null);
                        match delta.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                            "text_delta" => {
                                if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                    accumulated.push_str(text);
                                    push_responses_sse(
                                        pending,
                                        "response.output_text.delta",
                                        serde_json::json!({
                                            "type": "response.output_text.delta",
                                            "output_index": 0,
                                            "content_index": 0,
                                            "item_id": item_id,
                                            "delta": text,
                                        }),
                                    );
                                }
                            }
                            "thinking_delta" => {
                                if let Some(text) = delta.get("thinking").and_then(|t| t.as_str()) {
                                    push_responses_sse(
                                        pending,
                                        "response.reasoning_text.delta",
                                        serde_json::json!({
                                            "type": "response.reasoning_text.delta",
                                            "delta": text,
                                        }),
                                    );
                                }
                            }
                            "input_json_delta" => {
                                if let Some(partial) =
                                    delta.get("partial_json").and_then(|t| t.as_str())
                                    && let Some(call_id) = block_tools.get(&index)
                                {
                                    push_responses_sse(
                                        pending,
                                        "response.function_call_arguments.delta",
                                        serde_json::json!({
                                            "type": "response.function_call_arguments.delta",
                                            "item_id": call_id,
                                            "delta": partial,
                                        }),
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                    "message_stop" => {
                        push_responses_sse(
                            pending,
                            "response.completed",
                            serde_json::json!({
                                "type": "response.completed",
                                "response": {
                                    "id": response_id,
                                    "status": "completed",
                                    "output_text": accumulated,
                                }
                            }),
                        );
                        return true;
                    }
                    _ => {}
                }
                false
            };

            match Pin::new(&mut upstream).poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => {
                    for line in lines.push(&bytes) {
                        if process(
                            &line,
                            &mut pending,
                            &mut started,
                            &mut block_tools,
                            &mut accumulated,
                        ) {
                            done = true;
                            break;
                        }
                    }
                }
                Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e))),
                Poll::Ready(None) => {
                    if let Some(line) = lines.flush() {
                        let _ = process(
                            &line,
                            &mut pending,
                            &mut started,
                            &mut block_tools,
                            &mut accumulated,
                        );
                    }
                    if !done {
                        push_responses_sse(
                            &mut pending,
                            "response.completed",
                            serde_json::json!({
                                "type": "response.completed",
                                "response": {"id": response_id, "status": "completed", "output_text": accumulated}
                            }),
                        );
                    }
                    done = true;
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    })
}

/// Synthesize OpenAI Chat SSE from a complete chat completion JSON.
pub fn synthesize_openai_chat_sse_from_response(body: &Value) -> Bytes {
    let choice = body.get("choices").and_then(|c| c.get(0));
    let message = choice.and_then(|c| c.get("message"));
    let finish = choice
        .and_then(|c| c.get("finish_reason"))
        .and_then(|f| f.as_str())
        .unwrap_or("stop");

    let mut sse = String::new();
    if let Some(reasoning) = message
        .and_then(|m| m.get("reasoning_content"))
        .and_then(|c| c.as_str())
        && !reasoning.is_empty()
    {
        sse.push_str(&format!(
                "data: {}\n\n",
                serde_json::json!({
                    "choices": [{"index": 0, "delta": {"reasoning_content": reasoning}, "finish_reason": null}]
                })
            ));
    }
    if let Some(content) = message
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        && !content.is_empty()
    {
        sse.push_str(&format!(
            "data: {}\n\n",
            serde_json::json!({
                "choices": [{"index": 0, "delta": {"content": content}, "finish_reason": null}]
            })
        ));
    }
    if let Some(Value::Array(tool_calls)) = message.and_then(|m| m.get("tool_calls")) {
        for call in tool_calls {
            let idx = call.get("index").and_then(|v| v.as_u64()).unwrap_or(0);
            let id = call.get("id").and_then(|v| v.as_str()).unwrap_or("call_0");
            let name = call
                .get("function")
                .and_then(|f| f.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let args = call
                .get("function")
                .and_then(|f| f.get("arguments"))
                .and_then(|a| a.as_str())
                .unwrap_or("");
            sse.push_str(&format!(
                "data: {}\n\n",
                serde_json::json!({
                    "choices": [{"index": 0, "delta": {"tool_calls": [{
                        "index": idx, "id": id, "type": "function",
                        "function": {"name": name, "arguments": args}
                    }]}, "finish_reason": null}]
                })
            ));
        }
    }
    sse.push_str(&format!(
        "data: {}\n\n",
        serde_json::json!({
            "choices": [{"index": 0, "delta": {}, "finish_reason": finish}]
        })
    ));
    sse.push_str("data: [DONE]\n\n");
    Bytes::from(sse)
}

/// Synthesize Anthropic SSE from a complete Anthropic message JSON (fallback when upstream is non-streaming).
///
/// Emits official Messages streaming events (`message_start` → per-block start/delta/stop
/// → `message_delta` → `message_stop`), including `thinking` and `tool_use` blocks and
/// real usage. Flattening to a single text delta drops tool calls and makes Claude Code wait
/// for the full JSON then see a one-liner.
pub fn synthesize_anthropic_sse_from_response(message: &Value, model: String) -> Bytes {
    let message_id = message
        .get("id")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("msg_{}", uuid::Uuid::new_v4().simple()));
    let model = message
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or(&model)
        .to_string();
    let usage = message
        .get("usage")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({"input_tokens": 0, "output_tokens": 0}));
    let input_tokens = usage
        .get("input_tokens")
        .cloned()
        .unwrap_or(serde_json::json!(0));
    let output_tokens = usage
        .get("output_tokens")
        .cloned()
        .unwrap_or(serde_json::json!(0));
    let stop_reason = message
        .get("stop_reason")
        .cloned()
        .unwrap_or(serde_json::json!("end_turn"));
    let blocks = message
        .get("content")
        .and_then(|c| c.as_array())
        .cloned()
        .unwrap_or_default();

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
                "usage": {
                    "input_tokens": input_tokens,
                    "output_tokens": 0,
                    "cache_creation_input_tokens": usage.get("cache_creation_input_tokens").cloned().unwrap_or(serde_json::json!(0)),
                    "cache_read_input_tokens": usage.get("cache_read_input_tokens").cloned().unwrap_or(serde_json::json!(0))
                }
            }
        }),
    ));

    if blocks.is_empty() {
        chunks.push(anthropic_stream_event(
            "content_block_start",
            serde_json::json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": {"type": "text", "text": ""}
            }),
        ));
        chunks.push(anthropic_stream_event(
            "content_block_stop",
            serde_json::json!({"type": "content_block_stop", "index": 0}),
        ));
    }

    for (index, block) in blocks.iter().enumerate() {
        let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("text");
        match block_type {
            "thinking" => {
                let thinking = block.get("thinking").and_then(|t| t.as_str()).unwrap_or("");
                let signature = block
                    .get("signature")
                    .and_then(|t| t.as_str())
                    .unwrap_or("");
                chunks.push(anthropic_stream_event(
                    "content_block_start",
                    serde_json::json!({
                        "type": "content_block_start",
                        "index": index,
                        "content_block": {"type": "thinking", "thinking": ""}
                    }),
                ));
                if !thinking.is_empty() {
                    chunks.push(anthropic_stream_event(
                        "content_block_delta",
                        serde_json::json!({
                            "type": "content_block_delta",
                            "index": index,
                            "delta": {"type": "thinking_delta", "thinking": thinking}
                        }),
                    ));
                }
                chunks.push(anthropic_stream_event(
                    "content_block_delta",
                    serde_json::json!({
                        "type": "content_block_delta",
                        "index": index,
                        "delta": {
                            "type": "signature_delta",
                            "signature": if signature.is_empty() {
                                format!("cab_{}", uuid::Uuid::new_v4().simple())
                            } else {
                                signature.to_string()
                            }
                        }
                    }),
                ));
                chunks.push(anthropic_stream_event(
                    "content_block_stop",
                    serde_json::json!({"type": "content_block_stop", "index": index}),
                ));
            }
            "tool_use" => {
                let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let input = block.get("input").cloned().unwrap_or(serde_json::json!({}));
                chunks.push(anthropic_stream_event(
                    "content_block_start",
                    serde_json::json!({
                        "type": "content_block_start",
                        "index": index,
                        "content_block": {
                            "type": "tool_use",
                            "id": id,
                            "name": name,
                            "input": {}
                        }
                    }),
                ));
                let partial = serde_json::to_string(&input).unwrap_or_else(|_| "{}".into());
                chunks.push(anthropic_stream_event(
                    "content_block_delta",
                    serde_json::json!({
                        "type": "content_block_delta",
                        "index": index,
                        "delta": {"type": "input_json_delta", "partial_json": partial}
                    }),
                ));
                chunks.push(anthropic_stream_event(
                    "content_block_stop",
                    serde_json::json!({"type": "content_block_stop", "index": index}),
                ));
            }
            _ => {
                let text = block.get("text").and_then(|t| t.as_str()).unwrap_or("");
                chunks.push(anthropic_stream_event(
                    "content_block_start",
                    serde_json::json!({
                        "type": "content_block_start",
                        "index": index,
                        "content_block": {"type": "text", "text": ""}
                    }),
                ));
                if !text.is_empty() {
                    chunks.push(anthropic_stream_event(
                        "content_block_delta",
                        serde_json::json!({
                            "type": "content_block_delta",
                            "index": index,
                            "delta": {"type": "text_delta", "text": text}
                        }),
                    ));
                }
                chunks.push(anthropic_stream_event(
                    "content_block_stop",
                    serde_json::json!({"type": "content_block_stop", "index": index}),
                ));
            }
        }
    }

    let mut message_delta_usage = serde_json::json!({"output_tokens": output_tokens});
    if let Some(cache_read) = usage.get("cache_read_input_tokens") {
        message_delta_usage["cache_read_input_tokens"] = cache_read.clone();
    }
    if let Some(cache_write) = usage.get("cache_creation_input_tokens") {
        message_delta_usage["cache_creation_input_tokens"] = cache_write.clone();
    }
    chunks.push(anthropic_stream_event(
        "message_delta",
        serde_json::json!({
            "type": "message_delta",
            "delta": {"stop_reason": stop_reason, "stop_sequence": null},
            "usage": message_delta_usage
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
    Bytes::from(sse)
}

/// Synthesize Responses SSE from a complete Responses JSON (fallback when upstream is non-streaming).
pub fn synthesize_responses_sse_from_response(responses: &Value) -> Bytes {
    super::legacy::responses_to_sse_stream(responses)
}
