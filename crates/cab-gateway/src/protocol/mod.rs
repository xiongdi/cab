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
    TokenTrackingStream, anthropic_to_openai, anthropic_to_openai_chat_request,
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
}
