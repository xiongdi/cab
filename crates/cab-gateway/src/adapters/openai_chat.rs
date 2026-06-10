use super::ProtocolAdapter;

pub struct OpenAiChatAdapter;

impl ProtocolAdapter for OpenAiChatAdapter {
    fn protocol(&self) -> &'static str {
        "openai-chat"
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
        let input = usage
            .get("prompt_tokens")
            .or_else(|| usage.get("input_tokens"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let output = usage
            .get("completion_tokens")
            .or_else(|| usage.get("output_tokens"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        (input, output)
    }
}
