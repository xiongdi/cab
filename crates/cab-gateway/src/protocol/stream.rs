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
}

struct AnthropicSseEmitter {
    model: String,
    message_id: String,
    pending: Vec<Bytes>,
    message_started: bool,
    thinking_index: Option<u32>,
    text_index: Option<u32>,
    next_index: u32,
    tools: HashMap<String, ToolTracker>,
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
            text_index: None,
            next_index: 0,
            tools: HashMap::new(),
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

    fn push_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
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
        }
        self.output_tokens = self.output_tokens.saturating_add(text.len() as u64);
        self.push_delta(
            self.thinking_index.unwrap(),
            serde_json::json!({"type": "thinking_delta", "thinking": text}),
        );
    }

    fn push_tool_args(&mut self, key: &str, partial: &str) {
        if partial.is_empty() {
            return;
        }
        let Some(tool) = self.tools.get(key) else {
            return;
        };
        if tool.name.is_empty() {
            return;
        }
        let (block_index, id, name, started) = (
            tool.block_index,
            tool.id.clone(),
            tool.name.clone(),
            tool.started,
        );
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
                        "id": if id.is_empty() { format!("toolu_{}", uuid::Uuid::new_v4().simple()) } else { id.clone() },
                        "name": name,
                        "input": {}
                    }
                }),
            ));
        }
        self.output_tokens = self.output_tokens.saturating_add(partial.len() as u64);
        self.push_delta(
            block_index,
            serde_json::json!({"type": "input_json_delta", "partial_json": partial}),
        );
    }

    fn finish(&mut self, stop_reason: &str, output_tokens: Option<u64>) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.ensure_message();
        let pending: Vec<(String, String)> = self
            .tools
            .iter_mut()
            .filter(|(_, tool)| !tool.pending_args.is_empty() && !tool.name.is_empty())
            .map(|(key, tool)| (key.clone(), std::mem::take(&mut tool.pending_args)))
            .collect();
        for (key, args) in pending {
            self.push_tool_args(&key, &args);
        }
        let stop_indices: Vec<u32> = self
            .tools
            .values_mut()
            .filter(|tool| tool.started && !tool.stopped)
            .map(|tool| {
                tool.stopped = true;
                tool.block_index
            })
            .collect();
        for index in stop_indices {
            self.stop_block(index);
        }
        if let Some(idx) = self.thinking_index {
            self.stop_block(idx);
        }
        if let Some(idx) = self.text_index {
            self.stop_block(idx);
        }
        let out = output_tokens.unwrap_or(self.output_tokens);
        self.pending.push(anthropic_stream_event(
            "message_delta",
            serde_json::json!({
                "type": "message_delta",
                "delta": {"stop_reason": stop_reason, "stop_sequence": null},
                "usage": {"output_tokens": out}
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
                    emitter.finish("end_turn", None);
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
                    "response.reasoning_text.delta" | "response.reasoning_summary_text.delta" => {
                        if let Some(delta) = event.get("delta").and_then(|d| d.as_str()) {
                            emitter.push_thinking(delta);
                        }
                    }
                    "response.output_item.added" => {
                        if let Some(item) = event.get("item")
                            && item.get("type").and_then(|t| t.as_str()) == Some("function_call") {
                                let call_id = item
                                    .get("call_id")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let name = item
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let block_index = emitter.alloc_index();
                                let id = if call_id.is_empty() {
                                    format!("toolu_{}", uuid::Uuid::new_v4().simple())
                                } else {
                                    call_id.clone()
                                };
                                emitter.tools.insert(
                                    call_id.clone(),
                                    ToolTracker {
                                        block_index,
                                        id,
                                        name,
                                        pending_args: String::new(),
                                        started: false,
                                        stopped: false,
                                    },
                                );
                            }
                    }
                    "response.function_call_arguments.delta" => {
                        let call_id = event
                            .get("item_id")
                            .or_else(|| event.get("call_id"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let key = if call_id.is_empty() {
                            "idx:0".into()
                        } else {
                            call_id.clone()
                        };
                        if !emitter.tools.contains_key(&key) {
                            let block_index = emitter.alloc_index();
                            emitter.tools.insert(
                                key.clone(),
                                ToolTracker {
                                    block_index,
                                    id: call_id.clone(),
                                    name: String::new(),
                                    pending_args: String::new(),
                                    started: false,
                                    stopped: false,
                                },
                            );
                        }
                        if let Some(delta) = event.get("delta").and_then(|d| d.as_str()) {
                            if emitter
                                .tools
                                .get(&key)
                                .map(|t| t.name.is_empty())
                                .unwrap_or(true)
                            {
                                if let Some(t) = emitter.tools.get_mut(&key) {
                                    t.pending_args.push_str(delta);
                                }
                            } else {
                                emitter.push_tool_args(&key, delta);
                            }
                        }
                    }
                    "response.function_call_arguments.done" => {
                        if let Some(item) = event.get("item") {
                            let call_id = item
                                .get("call_id")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let key = if call_id.is_empty() {
                                "idx:0".into()
                            } else {
                                call_id
                            };
                            if let Some(t) = emitter.tools.get_mut(&key) {
                                if t.name.is_empty() {
                                    t.name = item
                                        .get("name")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                }
                                if !t.pending_args.is_empty() {
                                    let args = std::mem::take(&mut t.pending_args);
                                    emitter.push_tool_args(&key, &args);
                                }
                                if let Some(args) = item.get("arguments").and_then(|a| a.as_str()) {
                                    emitter.push_tool_args(&key, args);
                                }
                            }
                        }
                    }
                    "response.completed" => {
                        let stop = if emitter.tools.values().any(|t| t.started) {
                            "tool_use"
                        } else {
                            "end_turn"
                        };
                        let usage = event
                            .get("response")
                            .and_then(|r| r.get("usage"))
                            .and_then(|u| u.get("output_tokens"))
                            .and_then(|v| v.as_u64());
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
                        emitter.finish("end_turn", None);
                    }
                    done = true;
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    })
}

/// OpenAI Chat SSE → OpenAI Responses SSE (for Codex client + chat upstream).
pub fn transform_openai_chat_sse_to_responses<S, E>(
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
    let mut text_started = false;
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

            let emit = |pending: &mut Vec<Bytes>, event_type: &str, data: Value| {
                pending.push(Bytes::from(format!(
                    "event: {event_type}\ndata: {data}\n\n"
                )));
            };

            let process = |line: &str,
                           pending: &mut Vec<Bytes>,
                           started: &mut bool,
                           text_started: &mut bool,
                           accumulated: &mut String| {
                let Some(payload) = sse_line_payload(line) else {
                    return false;
                };
                if payload == "[DONE]" {
                    return true;
                }
                let Ok(chunk) = serde_json::from_str::<Value>(payload) else {
                    return false;
                };
                if !*started {
                    *started = true;
                    emit(
                        pending,
                        "response.created",
                        serde_json::json!({
                            "type": "response.created",
                            "response": {"id": response_id, "object": "response", "model": model, "status": "in_progress"}
                        }),
                    );
                    emit(
                        pending,
                        "response.output_item.added",
                        serde_json::json!({
                            "type": "response.output_item.added",
                            "output_index": 0,
                            "item": {"id": item_id, "type": "message", "role": "assistant", "status": "in_progress"}
                        }),
                    );
                }
                let delta = chunk
                    .get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("delta"));
                if let Some(text) = delta
                    .and_then(|d| d.get("content"))
                    .and_then(|c| c.as_str())
                {
                    *text_started = true;
                    accumulated.push_str(text);
                    emit(
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
                if let Some(Value::Array(tool_calls)) = delta.and_then(|d| d.get("tool_calls")) {
                    for call in tool_calls {
                        if let Some(name) = call
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                        {
                            let call_id =
                                call.get("id").and_then(|v| v.as_str()).unwrap_or("call_0");
                            emit(
                                pending,
                                "response.output_item.added",
                                serde_json::json!({
                                    "type": "response.output_item.added",
                                    "output_index": 1,
                                    "item": {"type": "function_call", "call_id": call_id, "name": name, "arguments": ""}
                                }),
                            );
                        }
                        if let Some(args) = call
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(|a| a.as_str())
                        {
                            let call_id =
                                call.get("id").and_then(|v| v.as_str()).unwrap_or("call_0");
                            emit(
                                pending,
                                "response.function_call_arguments.delta",
                                serde_json::json!({
                                    "type": "response.function_call_arguments.delta",
                                    "item_id": call_id,
                                    "delta": args,
                                }),
                            );
                        }
                    }
                }
                if chunk
                    .get("choices")
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.get("finish_reason"))
                    .is_some()
                {
                    return true;
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
                            &mut text_started,
                            &mut accumulated,
                        ) {
                            emit(
                                &mut pending,
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
                            &mut text_started,
                            &mut accumulated,
                        );
                    }
                    if !done {
                        emit(
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
                            && item.get("type").and_then(|t| t.as_str()) == Some("function_call") {
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
                            && block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
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
                                    && let Some((_, _, tool_idx)) = block_tools.get(&index) {
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
                        if let Some(usage) = event.get("message").and_then(|m| m.get("usage")) {
                            if let Some(in_tok) = usage.get("input_tokens").and_then(|v| v.as_i64()) {
                                *input_tokens = Some(in_tok);
                            }
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
                        if let Some(usage) = event.get("usage") {
                            if let Some(out_tok) = usage.get("output_tokens").and_then(|v| v.as_i64()) {
                                *output_tokens = Some(out_tok);
                            }
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
                            && block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
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
                                    && let Some(call_id) = block_tools.get(&index) {
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
        && !reasoning.is_empty() {
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
        && !content.is_empty() {
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
pub fn synthesize_anthropic_sse_from_response(message: &Value, model: String) -> Bytes {
    let text = message
        .get("content")
        .and_then(|c| c.as_array())
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|b| {
                    b.get("text")
                        .and_then(|t| t.as_str())
                        .or_else(|| b.get("thinking").and_then(|t| t.as_str()))
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();
    super::legacy::responses_to_anthropic_sse_stream(
        &serde_json::json!({"output_text": text}),
        model,
    )
}

/// Synthesize Responses SSE from a complete Responses JSON (fallback when upstream is non-streaming).
pub fn synthesize_responses_sse_from_response(responses: &Value) -> Bytes {
    super::legacy::responses_to_sse_stream(responses)
}
