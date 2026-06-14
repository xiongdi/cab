//! Protocol conversion engine — all paths route through IR.

use super::ir::{
    decode_anthropic_request, decode_anthropic_response, decode_openai_chat_request,
    decode_openai_chat_response, decode_responses_request, decode_responses_response,
    encode_anthropic_request, encode_anthropic_response, encode_openai_chat_request,
    encode_openai_chat_response, encode_responses_request, encode_responses_response,
};
use super::legacy::transform_openai_chat_sse_to_anthropic;
use super::stream::{
    synthesize_anthropic_sse_from_response, synthesize_openai_chat_sse_from_response,
    synthesize_responses_sse_from_response, transform_anthropic_sse_to_openai_chat,
    transform_anthropic_sse_to_responses, transform_openai_chat_sse_to_responses,
    transform_responses_sse_to_anthropic, transform_responses_sse_to_openai_chat,
};
use bytes::Bytes;
use futures::Stream;
use serde_json::Value;

pub const PROTOCOL_ANTHROPIC: &str = "anthropic";
pub const PROTOCOL_OPENAI_CHAT: &str = "openai-chat";
pub const PROTOCOL_OPENAI_RESPONSES: &str = "openai-responses";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Anthropic,
    OpenAiChat,
    OpenAiResponses,
}

impl Protocol {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            PROTOCOL_ANTHROPIC => Some(Self::Anthropic),
            PROTOCOL_OPENAI_CHAT => Some(Self::OpenAiChat),
            PROTOCOL_OPENAI_RESPONSES => Some(Self::OpenAiResponses),
            _ => None,
        }
    }
}

fn decode_request(protocol: Protocol, body: &Value) -> super::ir::IrRequest {
    match protocol {
        Protocol::Anthropic => decode_anthropic_request(body),
        Protocol::OpenAiChat => decode_openai_chat_request(body),
        Protocol::OpenAiResponses => decode_responses_request(body),
    }
}

fn encode_request(protocol: Protocol, ir: &super::ir::IrRequest) -> Value {
    match protocol {
        Protocol::Anthropic => encode_anthropic_request(ir),
        Protocol::OpenAiChat => encode_openai_chat_request(ir),
        Protocol::OpenAiResponses => encode_responses_request(ir),
    }
}

fn decode_response(protocol: Protocol, body: &Value) -> super::ir::IrResponse {
    match protocol {
        Protocol::Anthropic => decode_anthropic_response(body),
        Protocol::OpenAiChat => decode_openai_chat_response(body),
        Protocol::OpenAiResponses => decode_responses_response(body),
    }
}

fn encode_response(protocol: Protocol, ir: &super::ir::IrResponse, model_fallback: &str) -> Value {
    match protocol {
        Protocol::Anthropic => encode_anthropic_response(ir),
        Protocol::OpenAiChat => encode_openai_chat_response(ir),
        Protocol::OpenAiResponses => encode_responses_response(ir, model_fallback),
    }
}

/// Convert a request body from one protocol to another via IR.
pub fn convert_request(from: &str, to: &str, body: &Value) -> Value {
    if from == to {
        return body.clone();
    }
    let Some(from_p) = Protocol::parse(from) else {
        return body.clone();
    };
    let Some(to_p) = Protocol::parse(to) else {
        return body.clone();
    };
    let ir = decode_request(from_p, body);
    encode_request(to_p, &ir)
}

/// Convert a non-streaming response body from one protocol to another via IR.
pub fn convert_response(from: &str, to: &str, body: &Value, model_fallback: &str) -> Value {
    if from == to {
        return body.clone();
    }
    let Some(from_p) = Protocol::parse(from) else {
        return body.clone();
    };
    let Some(to_p) = Protocol::parse(to) else {
        return body.clone();
    };
    let ir = decode_response(from_p, body);
    encode_response(to_p, &ir, model_fallback)
}

/// Convert an upstream SSE byte stream between protocols.
pub fn convert_sse_stream<S, E>(
    from: &str,
    to: &str,
    upstream: S,
    model: String,
) -> std::pin::Pin<Box<dyn Stream<Item = Result<Bytes, E>> + Send>>
where
    S: Stream<Item = Result<Bytes, E>> + Unpin + Send + 'static,
    E: Send + 'static,
{
    match (from, to) {
        (PROTOCOL_OPENAI_CHAT, PROTOCOL_ANTHROPIC) => {
            Box::pin(transform_openai_chat_sse_to_anthropic(upstream, model))
        }
        (PROTOCOL_OPENAI_RESPONSES, PROTOCOL_ANTHROPIC) => {
            Box::pin(transform_responses_sse_to_anthropic(upstream, model))
        }
        (PROTOCOL_OPENAI_CHAT, PROTOCOL_OPENAI_RESPONSES) => {
            Box::pin(transform_openai_chat_sse_to_responses(upstream, model))
        }
        (PROTOCOL_OPENAI_RESPONSES, PROTOCOL_OPENAI_CHAT) => {
            Box::pin(transform_responses_sse_to_openai_chat(upstream, model))
        }
        (PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI_CHAT) => {
            Box::pin(transform_anthropic_sse_to_openai_chat(upstream))
        }
        (PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI_RESPONSES) => {
            Box::pin(transform_anthropic_sse_to_responses(upstream, model))
        }
        _ => Box::pin(upstream),
    }
}

/// Synthesize client SSE when upstream returned a complete JSON body (non-streaming fallback).
pub fn synthesize_sse_from_response(
    from: &str,
    to: &str,
    body: &Value,
    model: String,
) -> Bytes {
    if from == to {
        return Bytes::new();
    }
    match (from, to) {
        (PROTOCOL_OPENAI_RESPONSES, PROTOCOL_ANTHROPIC) => {
            let anthropic = convert_response(from, to, body, &model);
            synthesize_anthropic_sse_from_response(&anthropic, model)
        }
        (PROTOCOL_OPENAI_CHAT, PROTOCOL_OPENAI_RESPONSES) => {
            let responses = convert_response(from, to, body, &model);
            synthesize_responses_sse_from_response(&responses)
        }
        (PROTOCOL_OPENAI_CHAT, PROTOCOL_ANTHROPIC) => {
            let anthropic = convert_response(from, to, body, &model);
            synthesize_anthropic_sse_from_response(&anthropic, model)
        }
        (PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI_CHAT) => {
            let chat = convert_response(from, to, body, &model);
            synthesize_openai_chat_sse_from_response(&chat)
        }
        (PROTOCOL_ANTHROPIC, PROTOCOL_OPENAI_RESPONSES) => {
            let responses = convert_response(from, to, body, &model);
            synthesize_responses_sse_from_response(&responses)
        }
        (PROTOCOL_OPENAI_RESPONSES, PROTOCOL_OPENAI_CHAT) => {
            let chat = convert_response(from, to, body, &model);
            synthesize_openai_chat_sse_from_response(&chat)
        }
        _ => Bytes::new(),
    }
}
