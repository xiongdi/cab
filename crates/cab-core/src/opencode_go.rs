//! OpenCode Go per-model protocol lookup.
//!
//! Official map: https://opencode.ai/docs/go/
//! Live model list: https://opencode.ai/zen/go/v1/models
//!
//! Handshake uses this table instead of trying Responses / Chat / Anthropic
//! endpoints in priority order. Unknown Go ids are sniffed by family prefix.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

const EMBEDDED_TABLE: &str = include_str!("../../../config/opencode-go-protocols.json");

const OPENCODE_GO_PROVIDER_ID: &str = "opencode-go";

#[derive(Debug, Deserialize)]
struct GoProtocolTable {
    models: HashMap<String, String>,
    family_sniff: Vec<FamilySniff>,
}

#[derive(Debug, Deserialize)]
struct FamilySniff {
    prefix: String,
    protocol: String,
}

fn table() -> &'static GoProtocolTable {
    static TABLE: OnceLock<GoProtocolTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str(EMBEDDED_TABLE).unwrap_or_else(|e| {
            tracing::error!("Failed to parse embedded OpenCode Go protocol table: {e}");
            GoProtocolTable {
                models: HashMap::new(),
                family_sniff: Vec::new(),
            }
        })
    })
}

/// Native model id as used by OpenCode Go (`mimo-v2.5`), stripped of vendor prefixes.
pub fn normalize_go_model_id(model_id: &str) -> &str {
    model_id
        .rsplit('/')
        .next()
        .unwrap_or(model_id)
        .trim()
        .trim_start_matches("opencode-go-")
}

pub fn is_opencode_go_provider(provider_id: &str) -> bool {
    provider_id == OPENCODE_GO_PROVIDER_ID
}

/// Look up the official Go wire protocol for a model id.
///
/// Exact ids from the published endpoint table win; otherwise family prefixes
/// cover newer Go models (e.g. `kimi-k2.5`, `hy3-preview`) without a round trip.
pub fn sniff_opencode_go_protocol(model_id: &str) -> Option<String> {
    let id = normalize_go_model_id(model_id);
    if id.is_empty() {
        return None;
    }
    let table = table();
    if let Some(protocol) = table.models.get(id) {
        return Some(protocol.clone());
    }
    let id_lower = id.to_ascii_lowercase();
    let mut best: Option<(&str, &str)> = None;
    for row in &table.family_sniff {
        if id_lower.starts_with(&row.prefix.to_ascii_lowercase())
            && best.is_none_or(|(prefix, _)| row.prefix.len() > prefix.len())
        {
            best = Some((row.prefix.as_str(), row.protocol.as_str()));
        }
    }
    best.map(|(_, protocol)| protocol.to_string())
}

/// Provider-scoped sniff: only OpenCode Go uses the published endpoint table.
pub fn sniff_provider_model_protocol(provider_id: &str, model_id: &str) -> Option<String> {
    if is_opencode_go_provider(provider_id) {
        sniff_opencode_go_protocol(model_id)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_go_table_matches_published_endpoints() {
        assert_eq!(
            sniff_opencode_go_protocol("gpt-5.6-luna").as_deref(),
            Some("openai-responses")
        );
        assert_eq!(
            sniff_opencode_go_protocol("grok-4.5").as_deref(),
            Some("openai-responses")
        );
        assert_eq!(
            sniff_opencode_go_protocol("xiaomi/mimo-v2.5").as_deref(),
            Some("openai-chat")
        );
        assert_eq!(
            sniff_opencode_go_protocol("kimi-k3").as_deref(),
            Some("openai-chat")
        );
        assert_eq!(
            sniff_opencode_go_protocol("deepseek/deepseek-v4-flash").as_deref(),
            Some("openai-chat")
        );
        assert_eq!(
            sniff_opencode_go_protocol("minimax-m3").as_deref(),
            Some("anthropic")
        );
        assert_eq!(
            sniff_opencode_go_protocol("qwen3.8-max").as_deref(),
            Some("anthropic")
        );
        assert_eq!(
            sniff_opencode_go_protocol("hy3").as_deref(),
            Some("openai-chat")
        );
    }

    #[test]
    fn family_sniff_covers_unpublished_go_ids() {
        assert_eq!(
            sniff_opencode_go_protocol("kimi-k2.5").as_deref(),
            Some("openai-chat")
        );
        assert_eq!(
            sniff_opencode_go_protocol("hy3-preview").as_deref(),
            Some("openai-chat")
        );
        assert_eq!(
            sniff_opencode_go_protocol("qwen3.5-plus").as_deref(),
            Some("anthropic")
        );
        assert_eq!(
            sniff_opencode_go_protocol("glm-5").as_deref(),
            Some("openai-chat")
        );
    }

    #[test]
    fn sniff_is_scoped_to_opencode_go() {
        assert!(sniff_provider_model_protocol("opencode-go", "mimo-v2.5").is_some());
        assert!(sniff_provider_model_protocol("opencode", "mimo-v2.5").is_none());
        assert!(sniff_provider_model_protocol("openai", "gpt-5.6-luna").is_none());
    }
}
