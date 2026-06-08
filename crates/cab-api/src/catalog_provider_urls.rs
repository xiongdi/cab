//! Read provider endpoint defaults from the read-only models.dev catalog cache.
//! Source: `~/.cab/catalog/models.dev/api.json`
//! Schema: `{ "<slug>": { "api": "https://...", "npm": "@ai-sdk/...", "name": "...", ... } }`

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ModelsDevProvider {
    api: Option<String>,
    npm: Option<String>,
}

/// Infer the protocol from model.dev hints. Order of precedence:
/// 1. `npm` field (e.g. `@ai-sdk/anthropic` => anthropic)
/// 2. URL pattern fallback (`/anthropic` => anthropic, `/responses` => openai-responses)
/// 3. Default to openai-chat
pub fn infer_protocol(npm: Option<&str>, api_url: Option<&str>) -> String {
    if let Some(npm) = npm {
        let lower = npm.to_ascii_lowercase();
        if lower.contains("responses") {
            return "openai-responses".to_string();
        }
        if lower.contains("anthropic") {
            return "anthropic".to_string();
        }
        if lower.contains("openai") {
            return "openai-chat".to_string();
        }
    }
    if let Some(url) = api_url {
        let lower = url.to_ascii_lowercase();
        if lower.contains("/anthropic") {
            return "anthropic".to_string();
        }
        if lower.contains("/responses") {
            return "openai-responses".to_string();
        }
    }
    "openai-chat".to_string()
}

#[derive(Debug, Clone)]
pub struct ProviderDefaultEndpoint {
    pub protocol: String,
    pub url: String,
    pub label: Option<String>,
}

/// Load a single provider's default endpoint from the cached models.dev file.
/// Returns `None` if slug not found or file unreadable/malformed (NEVER panics).
pub fn load_provider_default_endpoint(
    slug: &str,
    cache_dir: &Path,
) -> Option<ProviderDefaultEndpoint> {
    let cache_dir = cache_dir.to_path_buf();
    let file_path = cache_dir.join("models.dev").join("api.json");

    let content = std::fs::read_to_string(&file_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;

    // Normalize slug: lowercase, _ -> -
    let normalized_slug = slug.to_lowercase().replace('_', "-");

    // Look up the slug key (try both normalized and original)
    let entry = json.get(&normalized_slug).or_else(|| json.get(slug))?;

    let api_url = entry.get("api").and_then(|v| v.as_str());
    let npm = entry.get("npm").and_then(|v| v.as_str());

    let protocol = infer_protocol(npm, api_url);
    let url = api_url?.trim_end_matches('/').to_string();

    Some(ProviderDefaultEndpoint {
        protocol,
        url,
        label: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_anthropic_from_npm() {
        assert_eq!(infer_protocol(Some("@ai-sdk/anthropic"), None), "anthropic");
    }

    #[test]
    fn infers_openai_responses_from_npm() {
        assert_eq!(
            infer_protocol(Some("@ai-sdk/openai-responses"), None),
            "openai-responses"
        );
    }

    #[test]
    fn infers_openai_chat_from_npm() {
        assert_eq!(infer_protocol(Some("@ai-sdk/openai"), None), "openai-chat");
    }

    #[test]
    fn infers_anthropic_from_url_pattern() {
        assert_eq!(
            infer_protocol(None, Some("https://api.minimaxi.com/anthropic")),
            "anthropic"
        );
    }

    #[test]
    fn infers_openai_responses_from_url_pattern() {
        assert_eq!(
            infer_protocol(None, Some("https://api.openai.com/v1/responses")),
            "openai-responses"
        );
    }

    #[test]
    fn defaults_to_openai_chat() {
        assert_eq!(
            infer_protocol(None, Some("https://api.deepseek.com/v1")),
            "openai-chat"
        );
        assert_eq!(infer_protocol(None, None), "openai-chat");
    }

    #[test]
    fn npm_takes_precedence_over_url() {
        // @ai-sdk/openai wins over /anthropic in URL
        assert_eq!(
            infer_protocol(
                Some("@ai-sdk/openai"),
                Some("https://example.com/anthropic")
            ),
            "openai-chat"
        );
    }

    #[test]
    fn load_returns_none_for_missing_file() {
        let dir = std::env::temp_dir().join("cab_test_missing_file");
        let result = load_provider_default_endpoint("minimax", &dir);
        assert!(result.is_none());
    }

    #[test]
    fn load_returns_endpoint_for_known_slug() {
        // Write a temp models.dev/api.json with a minimax entry
        let temp_dir = std::env::temp_dir().join("cab_test_provider_urls");
        let _ = std::fs::create_dir_all(temp_dir.join("models.dev"));
        let catalog_path = temp_dir.join("models.dev").join("api.json");
        std::fs::write(
            &catalog_path,
            serde_json::to_string_pretty(&serde_json::json!({
                "minimax": {
                    "api": "https://api.minimax.chat/anthropic/v1",
                    "npm": "@ai-sdk/anthropic"
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let result = load_provider_default_endpoint("minimax", &temp_dir);
        assert!(result.is_some());
        let ep = result.unwrap();
        assert_eq!(ep.protocol, "anthropic");
        assert_eq!(ep.url, "https://api.minimax.chat/anthropic/v1");

        // Cleanup
        let _ = std::fs::remove_dir_all(temp_dir);
    }
}
