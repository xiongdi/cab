//! Three-protocol conversion hub (Anthropic Messages, OpenAI Chat, OpenAI Responses).

mod engine;
mod ir;
mod legacy;
mod stream;

pub use engine::{
    PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI_CHAT, PROTOCOL_OPENAI_RESPONSES, Protocol, convert_request,
    convert_response, convert_sse_stream, synthesize_sse_from_response,
};
pub use legacy::{
    StreamUsageMeta, TokenTrackingStream, anthropic_to_openai, anthropic_to_openai_chat_request,
    anthropic_to_responses_request, chat_request_to_responses, chat_to_responses,
    openai_chat_to_anthropic_messages, openai_to_anthropic, responses_text_from_body,
    responses_to_anthropic_messages, responses_to_anthropic_request,
    responses_to_anthropic_sse_stream, responses_to_chat_request, responses_to_sse_stream,
    transform_openai_chat_sse_to_anthropic,
};

#[cfg(test)]
mod tests {
    use super::ir::{decode_openai_chat_request, encode_anthropic_request};
    use super::*;
    use serde_json::json;

    #[test]
    fn ir_openai_tool_message_becomes_anthropic_tool_result() {
        let body = json!({
            "model": "gpt-test",
            "messages": [
                {"role": "user", "content": "hello"},
                {"role": "tool", "tool_call_id": "call_1", "content": "result"}
            ]
        });
        let ir = decode_openai_chat_request(&body);
        let anthropic = encode_anthropic_request(&ir);
        let msgs = anthropic["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[1]["content"][0]["type"], "tool_result");
    }

    #[test]
    fn engine_convert_request_roundtrip_tools() {
        let body = json!({
            "model": "claude-test",
            "max_tokens": 100,
            "tools": [{"name": "Read", "description": "d", "input_schema": {"type": "object"}}],
            "tool_choice": {"type": "any"},
            "messages": [{"role": "user", "content": "hi"}]
        });
        let chat = convert_request(PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI_CHAT, &body);
        assert_eq!(chat["tool_choice"], "required");
        let back = convert_request(PROTOCOL_OPENAI_CHAT, PROTOCOL_ANTHROPIC, &chat);
        assert_eq!(back["tool_choice"]["type"], "any");
    }

    #[test]
    fn ir_response_empty_openai_has_no_choices() {
        let ir = super::ir::IrResponse::default();
        let openai = super::ir::encode_openai_chat_response(&ir);
        assert_eq!(openai["choices"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn ir_anthropic_response_with_tool_use_maps_to_openai() {
        let body = json!({
            "id": "msg_1",
            "model": "claude",
            "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {"path": "/a"}}],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 1, "output_tokens": 2}
        });
        let openai = convert_response(PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI_CHAT, &body, "m");
        assert_eq!(openai["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(
            openai["choices"][0]["message"]["tool_calls"][0]["function"]["name"],
            "Read"
        );
    }

    #[test]
    fn responses_parallel_tool_calls_bundle_into_single_assistant_turn_in_openai_chat() {
        // Parallel tool calls arrive as consecutive top-level `function_call`
        // items, then their `function_call_output`s. Converting to OpenAI Chat
        // must bundle both `tool_calls` into ONE assistant message, followed by
        // the `tool` responses — otherwise upstream rejects the sequence.
        let body = json!({
            "model": "deepseek/deepseek-v4-flash",
            "input": [
                {"type": "function_call", "name": "get_goal", "arguments": "{}", "call_id": "call_1"},
                {"type": "function_call", "name": "update_plan", "arguments": "{\"p\":1}", "call_id": "call_2"},
                {"type": "function_call_output", "call_id": "call_1", "output": "out1"},
                {"type": "function_call_output", "call_id": "call_2", "output": "out2"}
            ]
        });
        let chat = convert_request(PROTOCOL_OPENAI_RESPONSES, PROTOCOL_OPENAI_CHAT, &body);
        let msgs = chat["messages"].as_array().unwrap();
        // One assistant message carrying both tool_calls, then two tool messages.
        assert_eq!(msgs.len(), 3, "messages: {chat}");
        assert_eq!(msgs[0]["role"], "assistant");
        assert_eq!(msgs[0]["tool_calls"].as_array().unwrap().len(), 2);
        assert_eq!(msgs[0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(msgs[0]["tool_calls"][1]["id"], "call_2");
        assert_eq!(msgs[1]["role"], "tool");
        assert_eq!(msgs[1]["tool_call_id"], "call_1");
        assert_eq!(msgs[2]["role"], "tool");
        assert_eq!(msgs[2]["tool_call_id"], "call_2");
    }

    #[test]
    fn responses_single_tool_call_turn_stays_well_formed_in_openai_chat() {
        let body = json!({
            "model": "deepseek/deepseek-v4-flash",
            "input": [
                {"type": "function_call", "name": "get_goal", "arguments": "{}", "call_id": "call_1"},
                {"type": "function_call_output", "call_id": "call_1", "output": "out1"}
            ]
        });
        let chat = convert_request(PROTOCOL_OPENAI_RESPONSES, PROTOCOL_OPENAI_CHAT, &body);
        let msgs = chat["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2, "messages: {chat}");
        assert_eq!(msgs[0]["role"], "assistant");
        assert_eq!(msgs[0]["tool_calls"].as_array().unwrap().len(), 1);
        assert_eq!(msgs[1]["role"], "tool");
    }

    #[test]
    fn responses_nameless_tool_is_dropped_when_converting_to_openai_chat() {
        // Codex sends Responses-style tools; a special type like
        // `external_web_access` carries no `name` and cannot be expressed as an
        // OpenAI `function`. It must be dropped, not emitted as `function.name: ""`.
        let body = json!({
            "model": "deepseek/deepseek-v4-flash",
            "input": [{"type": "message", "role": "user", "content": [{"type": "input_text", "text": "hi"}]}],
            "tools": [
                {"type": "function", "name": "exec_command", "description": "run", "parameters": {"type": "object"}},
                {"type": "external_web_access"}
            ]
        });
        let chat = convert_request(PROTOCOL_OPENAI_RESPONSES, PROTOCOL_OPENAI_CHAT, &body);
        let tools = chat["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1, "nameless tool must be dropped: {chat}");
        assert_eq!(tools[0]["function"]["name"], "exec_command");
    }

    #[test]
    fn codex_view_image_tool_result_forwards_image_to_openai_chat() {
        // Codex sends a `view_image` result as a `function_call_output` whose
        // `output` is an array of `{"type":"input_image","image_url":"data:image/..."}`.
        // After conversion to OpenAI Chat (the path used for OpenAI-compatible
        // upstreams such as deepseek), the tool message must carry a real
        // `image_url` content part, not a JSON-encoded string.
        let body = json!({
            "model": "cab/auto",
            "input": [{
                "type": "function_call_output",
                "call_id": "call_1",
                "output": [
                    {"type": "input_image", "image_url": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUg="}
                ]
            }]
        });
        let chat = convert_request(PROTOCOL_OPENAI_RESPONSES, PROTOCOL_OPENAI_CHAT, &body);
        let msgs = chat["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1, "messages: {chat}");
        assert_eq!(msgs[0]["role"], "tool");
        let content = msgs[0]["content"]
            .as_array()
            .expect("tool content must be an array");
        let img = content
            .iter()
            .find(|p| p.get("type").and_then(|t| t.as_str()) == Some("image_url"))
            .expect("expected an image_url content part");
        let url = img["image_url"]["url"].as_str().unwrap();
        assert!(url.starts_with("data:image/"), "image url: {url}");
    }

    #[test]
    fn codex_view_image_tool_result_roundtrips_to_responses() {
        let body = json!({
            "model": "cab/auto",
            "input": [{
                "type": "function_call_output",
                "call_id": "call_1",
                "output": [
                    {"type": "input_image", "image_url": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUg="}
                ]
            }]
        });
        let responses =
            convert_request(PROTOCOL_OPENAI_RESPONSES, PROTOCOL_OPENAI_RESPONSES, &body);
        let items = responses["input"].as_array().unwrap();
        let out = items
            .iter()
            .find(|i| i.get("type").and_then(|t| t.as_str()) == Some("function_call_output"))
            .expect("expected function_call_output item");
        let output = out["output"].as_array().expect("output must be an array");
        let img = output
            .iter()
            .find(|p| p.get("type").and_then(|t| t.as_str()) == Some("input_image"))
            .expect("expected input_image part");
        assert!(
            img["image_url"]
                .as_str()
                .unwrap()
                .starts_with("data:image/")
        );
    }

    #[tokio::test]
    async fn anthropic_sse_to_openai_chat_emits_content_and_done() {
        use super::stream::transform_anthropic_sse_to_openai_chat;
        use futures::StreamExt;

        let anthropic_sse = concat!(
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        );
        let upstream = futures::stream::iter(vec![Ok::<bytes::Bytes, std::convert::Infallible>(
            bytes::Bytes::from(anthropic_sse),
        )]);
        let mut out = transform_anthropic_sse_to_openai_chat(upstream);
        let mut chunks = Vec::new();
        while let Some(item) = out.next().await {
            chunks.push(String::from_utf8(item.unwrap().to_vec()).unwrap());
        }
        let joined = chunks.join("");
        assert!(joined.contains("\"content\":\"hi\""));
        assert!(joined.contains("[DONE]"));
        let finish_idx = joined
            .find("\"finish_reason\":\"stop\"")
            .expect("finish_reason");
        let done_idx = joined.find("[DONE]").expect("[DONE]");
        assert!(
            finish_idx < done_idx,
            "finish_reason must precede [DONE], got: {joined}"
        );
    }

    #[test]
    fn chat_json_to_responses_sse_emits_function_call_lifecycle() {
        let chat = json!({
            "id": "chatcmpl_1",
            "model": "mimo-v2.5",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "checking",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {"name": "exec_command", "arguments": "{\"cmd\":\"ls\"}"}
                    }]
                },
                "finish_reason": "tool_calls"
            }]
        });
        let responses = convert_response(
            PROTOCOL_OPENAI_CHAT,
            PROTOCOL_OPENAI_RESPONSES,
            &chat,
            "mimo-v2.5",
        );
        let sse = String::from_utf8(responses_to_sse_stream(&responses).to_vec()).unwrap();
        assert!(sse.contains("event: response.output_item.added"));
        assert!(sse.contains("\"type\":\"function_call\""));
        assert!(sse.contains("\"call_id\":\"call_1\""));
        assert!(sse.contains("\"name\":\"exec_command\""));
        assert!(sse.contains("event: response.function_call_arguments.delta"));
        assert!(sse.contains("event: response.function_call_arguments.done"));
        assert!(sse.contains("event: response.output_item.done"));
        assert!(sse.contains("event: response.completed"));
        let item_ids: Vec<&str> = sse
            .split("\"item_id\":\"")
            .skip(1)
            .filter_map(|rest| rest.split('"').next())
            .collect();
        assert!(
            item_ids.iter().any(|id| id.starts_with("fc_")),
            "argument events must use generated fc_* item_id, got {item_ids:?} in {sse}"
        );
        assert!(
            !item_ids.contains(&"call_1"),
            "Chat tool_calls[].id must not be reused as Responses item_id: {sse}"
        );
    }

    #[tokio::test]
    async fn chat_sse_to_responses_emits_official_function_call_lifecycle() {
        use super::stream::transform_openai_chat_sse_to_responses;
        use futures::StreamExt;

        let chat_sse = concat!(
            "data: {\"id\":\"chatcmpl_1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"let me check\",\"tool_calls\":[{\"index\":0,\"id\":\"call_abc\",\"type\":\"function\",\"function\":{\"name\":\"exec_command\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"cmd\\\":\\\"ls\\\"}\"}}]},\"finish_reason\":null}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: [DONE]\n\n",
        );
        let upstream = futures::stream::iter(vec![Ok::<bytes::Bytes, std::convert::Infallible>(
            bytes::Bytes::from(chat_sse),
        )]);
        let mut out = transform_openai_chat_sse_to_responses(upstream, "mimo-v2.5".into());
        let mut chunks = Vec::new();
        while let Some(item) = out.next().await {
            chunks.push(String::from_utf8(item.unwrap().to_vec()).unwrap());
        }
        let joined = chunks.join("");

        let added = joined.find("event: response.output_item.added").unwrap();
        let fc_added = joined.find("\"type\":\"function_call\"").unwrap();
        let args_delta = joined
            .find("event: response.function_call_arguments.delta")
            .unwrap();
        let args_done = joined
            .find("event: response.function_call_arguments.done")
            .unwrap();
        let item_done = joined.rfind("event: response.output_item.done").unwrap();
        let completed = joined.find("event: response.completed").unwrap();
        assert!(added < fc_added);
        assert!(fc_added < args_delta);
        assert!(args_delta < args_done);
        assert!(args_done < item_done);
        assert!(item_done < completed);
        assert!(joined.contains("\"call_id\":\"call_abc\""));
        assert!(joined.contains("\"name\":\"exec_command\""));
        assert!(joined.contains(r#""delta":"let me check""#));
        assert!(
            joined.contains(r#""delta":"{\"cmd\":\"ls\"}""#)
                || joined.contains("\"delta\":\"{\\\"cmd\\\":\\\"ls\\\"}\"")
        );

        let item_ids: Vec<&str> = joined
            .split("\"item_id\":\"")
            .skip(1)
            .filter_map(|rest| rest.split('"').next())
            .collect();
        assert!(
            item_ids.iter().any(|id| id.starts_with("fc_")),
            "argument events must use generated fc_* item_id, got {item_ids:?} in {joined}"
        );
        assert!(
            !item_ids.contains(&"call_abc"),
            "Chat tool_calls[].id must map to call_id, not item_id: {joined}"
        );
        assert!(
            !joined.contains("event: response.completed")
                || joined.match_indices("event: response.completed").count() == 1,
            "finish_reason:null must not emit extra completed events: {joined}"
        );
    }

    #[test]
    fn synthesize_anthropic_sse_keeps_tool_use_thinking_and_usage() {
        use super::stream::synthesize_anthropic_sse_from_response;

        let message = json!({
            "id": "msg_keep",
            "model": "mimo-v2.5",
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 41, "output_tokens": 12},
            "content": [
                {"type": "thinking", "thinking": "need a tool", "signature": "sig_1"},
                {"type": "tool_use", "id": "toolu_1", "name": "Read", "input": {"path": "/tmp"}}
            ]
        });
        let sse = String::from_utf8(
            synthesize_anthropic_sse_from_response(&message, "mimo-v2.5".into()).to_vec(),
        )
        .unwrap();
        assert!(sse.contains(r#""input_tokens":41"#));
        assert!(sse.contains(r#""output_tokens":12"#));
        assert!(sse.contains(r#""type":"thinking_delta""#));
        assert!(sse.contains("need a tool"));
        assert!(sse.contains(r#""type":"input_json_delta""#));
        assert!(sse.contains("toolu_1"));
        assert!(sse.contains("Read"));
        assert!(!sse.contains("need a tool\"") || sse.contains("thinking_delta"));
    }

    #[test]
    fn responses_reasoning_item_maps_to_anthropic_thinking() {
        let body = json!({
            "id": "resp_1",
            "model": "gpt-5.6-luna",
            "output": [
                {
                    "type": "reasoning",
                    "summary": [{"type": "summary_text", "text": "plan first"}]
                },
                {
                    "type": "message",
                    "content": [{"type": "output_text", "text": "done"}]
                }
            ],
            "usage": {"input_tokens": 8, "output_tokens": 3}
        });
        let anthropic = convert_response(
            PROTOCOL_OPENAI_RESPONSES,
            PROTOCOL_ANTHROPIC,
            &body,
            "gpt-5.6-luna",
        );
        assert_eq!(anthropic["content"][0]["type"], "thinking");
        assert_eq!(anthropic["content"][0]["thinking"], "plan first");
        assert_eq!(anthropic["content"][1]["type"], "text");
        assert_eq!(anthropic["content"][1]["text"], "done");
        assert_eq!(anthropic["usage"]["input_tokens"], 8);
    }

    #[tokio::test]
    async fn responses_sse_to_anthropic_maps_tool_use_lifecycle() {
        use super::stream::transform_responses_sse_to_anthropic;
        use futures::StreamExt;

        let responses_sse = concat!(
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":1,\"item\":{\"id\":\"rs_msg\",\"type\":\"message\",\"status\":\"in_progress\",\"role\":\"assistant\",\"content\":[]}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"output_index\":1,\"item_id\":\"rs_msg\",\"delta\":\"Checking files for you.\"}\n\n",
            "event: response.output_text.done\n",
            "data: {\"type\":\"response.output_text.done\",\"output_index\":1,\"item_id\":\"rs_msg\",\"text\":\"Checking files for you.\"}\n\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":2,\"item\":{\"id\":\"fc_abc\",\"type\":\"function_call\",\"status\":\"in_progress\",\"name\":\"Bash\",\"call_id\":\"call_abc\",\"arguments\":\"\"}}\n\n",
            "event: response.function_call_arguments.delta\n",
            "data: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":2,\"item_id\":\"fc_abc\",\"delta\":\"{\\\"command\\\":\\\"ls\\\"}\"}\n\n",
            "event: response.function_call_arguments.done\n",
            "data: {\"type\":\"response.function_call_arguments.done\",\"output_index\":2,\"item_id\":\"fc_abc\",\"arguments\":\"{\\\"command\\\":\\\"ls\\\"}\",\"name\":\"Bash\"}\n\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":2,\"item\":{\"id\":\"fc_abc\",\"type\":\"function_call\",\"status\":\"completed\",\"name\":\"Bash\",\"call_id\":\"call_abc\",\"arguments\":\"{\\\"command\\\":\\\"ls\\\"}\"}}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"output\":[{\"id\":\"fc_abc\",\"type\":\"function_call\",\"status\":\"completed\",\"name\":\"Bash\",\"call_id\":\"call_abc\",\"arguments\":\"{\\\"command\\\":\\\"ls\\\"}\"}],\"usage\":{\"input_tokens\":500,\"output_tokens\":50,\"input_tokens_details\":{\"cached_tokens\":100}}}}\n\n",
        );
        let upstream = futures::stream::iter(vec![Ok::<bytes::Bytes, std::convert::Infallible>(
            bytes::Bytes::from(responses_sse),
        )]);
        let mut out = transform_responses_sse_to_anthropic(upstream, "muse-spark".into());
        let mut chunks = Vec::new();
        while let Some(item) = out.next().await {
            chunks.push(String::from_utf8(item.unwrap().to_vec()).unwrap());
        }
        let joined = chunks.join("");

        // 1. Must contain text delta
        assert!(joined.contains("\"text\":\"Checking files for you.\""));

        // 2. Must contain tool_use block with call_id and name
        assert!(joined.contains("\"type\":\"tool_use\""));
        assert!(joined.contains("\"id\":\"call_abc\""));
        assert!(joined.contains("\"name\":\"Bash\""));
        assert!(joined.contains("\"input_json_delta\""));

        // 3. Must have stop_reason: tool_use
        assert!(joined.contains("\"stop_reason\":\"tool_use\""));

        // 4. Must carry usage
        assert!(joined.contains("\"input_tokens\":500"));
        assert!(joined.contains("\"output_tokens\":50"));
        assert!(joined.contains("\"cache_read_input_tokens\":100"));

        // 5. Text content_block_stop must occur BEFORE tool_use content_block_start
        let text_stop = joined
            .find("\"index\":0,\"type\":\"content_block_stop\"")
            .expect("text stop");
        let tool_start = joined
            .find("\"index\":1,\"type\":\"content_block_start\"")
            .expect("tool start");
        assert!(
            text_stop < tool_start,
            "Text block must be stopped before tool block starts: {joined}"
        );

        // 6. Ensure input_json_delta is not duplicated
        let delta_count = joined.matches("\"type\":\"input_json_delta\"").count();
        assert_eq!(
            delta_count, 1,
            "input_json_delta should appear exactly once"
        );
    }
}
