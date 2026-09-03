use super::ProtocolAdapter;

pub struct OpenAiChatAdapter;

impl ProtocolAdapter for OpenAiChatAdapter {
    fn protocol(&self) -> &'static str {
        "openai-compatible"
    }

    fn path_suffix(&self) -> &'static str {
        "chat/completions"
    }

    fn log_path(&self) -> &'static str {
        "/v1/chat/completions"
    }

    fn default_stream(&self, _body: &serde_json::Value) -> bool {
        false
    }

    fn extract_usage(&self, usage: &serde_json::Value) -> (i64, i64) {
        // OpenAI Chat Completions `CompletionUsage`: prompt_tokens / completion_tokens.
        // Do not fall back to Responses/Anthropic field names — some compat providers
        // emit both, with input_tokens/output_tokens stuck at 0.
        let input = usage
            .get("prompt_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let output = usage
            .get("completion_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        (input, output)
    }
}
