use super::ProtocolAdapter;

pub struct OpenAiResponsesAdapter;

impl ProtocolAdapter for OpenAiResponsesAdapter {
    fn protocol(&self) -> &'static str {
        "openai-responses"
    }

    fn path_suffix(&self) -> &'static str {
        "responses"
    }

    fn log_path(&self) -> &'static str {
        "/v1/responses"
    }

    fn default_stream(&self, _body: &serde_json::Value) -> bool {
        true
    }

    fn extract_usage(&self, usage: &serde_json::Value) -> (i64, i64) {
        let input = usage
            .get("input_tokens")
            .or_else(|| usage.get("prompt_tokens"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let output = usage
            .get("output_tokens")
            .or_else(|| usage.get("completion_tokens"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        (input, output)
    }
}
