use super::ProtocolAdapter;

pub struct AnthropicAdapter;

impl ProtocolAdapter for AnthropicAdapter {
    fn protocol(&self) -> &'static str {
        "anthropic"
    }

    fn path_suffix(&self) -> &'static str {
        "v1/messages"
    }

    fn log_path(&self) -> &'static str {
        "/v1/messages"
    }

    fn default_stream(&self, _body: &serde_json::Value) -> bool {
        false
    }

    fn extract_usage(&self, usage: &serde_json::Value) -> (i64, i64) {
        let input = usage
            .get("input_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let output = usage
            .get("output_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        (input, output)
    }
}
