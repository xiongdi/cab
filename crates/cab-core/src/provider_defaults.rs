use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;
use uuid::Uuid;

use crate::types::{ProviderEndpoint, ProviderUserSettings, Settings};

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderDefaultsCatalog {
    pub providers: HashMap<String, ProviderDefaultsEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderDefaultsEntry {
    pub default_protocol: String,
    pub endpoints: Vec<DefaultEndpointTemplate>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DefaultEndpointTemplate {
    pub protocol: String,
    pub url: String,
    pub label: Option<String>,
    pub priority: i32,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

const EMBEDDED_DEFAULTS: &str = include_str!("../../../config/provider-endpoints.defaults.json");

/// Load bundled default provider protocol/endpoint definitions.
pub fn load_provider_defaults() -> ProviderDefaultsCatalog {
    let candidates = [
        Path::new("config/provider-endpoints.defaults.json"),
        Path::new("cab/config/provider-endpoints.defaults.json"),
    ];

    for path in candidates {
        if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(catalog) => return catalog,
                    Err(e) => tracing::warn!(
                        "Failed to parse provider defaults at {}: {e}",
                        path.display()
                    ),
                },
                Err(e) => tracing::warn!(
                    "Failed to read provider defaults at {}: {e}",
                    path.display()
                ),
            }
        }
    }

    serde_json::from_str(EMBEDDED_DEFAULTS).unwrap_or_else(|e| {
        tracing::error!("Failed to parse embedded provider defaults: {e}");
        ProviderDefaultsCatalog {
            providers: HashMap::new(),
        }
    })
}

/// Resolve endpoints for a provider.
/// User settings take precedence; bundled defaults fill in any missing protocol/url pairs.
pub fn resolve_provider_endpoints(
    provider_id: &str,
    defaults: &ProviderDefaultsCatalog,
    settings: &Settings,
) -> Vec<ProviderEndpoint> {
    let bundled = defaults
        .providers
        .get(provider_id)
        .map(|entry| templates_to_endpoints(&entry.endpoints))
        .unwrap_or_default();

    if let Some(endpoints) = settings.providers.get(provider_id).and_then(|u| u.endpoints.as_ref()) {
        return merge_endpoints(endpoints, &bundled);
    }

    bundled
}

fn merge_endpoints(
    user: &[ProviderEndpoint],
    bundled: &[ProviderEndpoint],
) -> Vec<ProviderEndpoint> {
    if bundled.is_empty() {
        return user.to_vec();
    }

    let mut merged = user.to_vec();
    for endpoint in bundled {
        let exists = merged
            .iter()
            .any(|e| e.protocol == endpoint.protocol && e.url == endpoint.url);
        if !exists {
            merged.push(endpoint.clone());
        }
    }
    merged.sort_by_key(|b| std::cmp::Reverse(b.priority));
    merged
}

/// Resolve the model protocol for a provider.
pub fn resolve_provider_default_protocol(
    provider_id: &str,
    defaults: &ProviderDefaultsCatalog,
    settings: &Settings,
) -> String {
    if let Some(primary) = settings.providers.get(provider_id)
        .and_then(|u| u.endpoints.as_ref())
        .and_then(|es| es.iter().max_by_key(|e| e.priority))
    {
        return primary.protocol.clone();
    }

    defaults
        .providers
        .get(provider_id)
        .map(|entry| entry.default_protocol.clone())
        .unwrap_or_else(|| "openai-chat".to_string())
}

pub fn templates_to_endpoints(templates: &[DefaultEndpointTemplate]) -> Vec<ProviderEndpoint> {
    templates
        .iter()
        .map(|t| ProviderEndpoint {
            id: Uuid::new_v4().to_string(),
            protocol: t.protocol.clone(),
            url: t.url.clone(),
            label: t.label.clone(),
            priority: t.priority,
            enabled: t.enabled,
        })
        .collect()
}

pub fn provider_user_settings_from_provider(
    enabled: bool,
    api_key: &str,
    api_keys: &[crate::types::ApiKeyConfig],
    endpoints: &[ProviderEndpoint],
) -> ProviderUserSettings {
    ProviderUserSettings {
        enabled: Some(enabled),
        api_key: if api_key.is_empty() {
            None
        } else {
            Some(api_key.to_string())
        },
        api_keys: if api_keys.is_empty() {
            None
        } else {
            Some(api_keys.to_vec())
        },
        endpoints: if endpoints.is_empty() {
            None
        } else {
            Some(endpoints.to_vec())
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ProviderUserSettings;
    use std::collections::HashMap;

    fn sample_endpoint(protocol: &str, url: &str, priority: i32) -> ProviderEndpoint {
        ProviderEndpoint {
            id: Uuid::new_v4().to_string(),
            protocol: protocol.to_string(),
            url: url.to_string(),
            label: None,
            priority,
            enabled: true,
        }
    }

    #[test]
    fn merge_endpoints_adds_missing_bundled_entries() {
        let user = vec![sample_endpoint(
            "anthropic",
            "https://api.minimaxi.com/anthropic/v1",
            50,
        )];
        let bundled = vec![
            sample_endpoint("openai-chat", "https://api.minimaxi.com/v1", 60),
            sample_endpoint("anthropic", "https://api.minimaxi.com/anthropic/v1", 50),
            sample_endpoint("openai-responses", "https://api.minimaxi.com/v1", 70),
        ];

        let merged = merge_endpoints(&user, &bundled);
        assert_eq!(merged.len(), 3);
        assert!(
            merged
                .iter()
                .any(|e| e.protocol == "openai-chat" && e.url.contains("minimaxi.com/v1"))
        );
    }

    #[test]
    fn resolve_provider_endpoints_merges_user_and_bundled_defaults() {
        let mut providers = HashMap::new();
        providers.insert(
            "minimax-cn-coding-plan".to_string(),
            ProviderDefaultsEntry {
                default_protocol: "anthropic".to_string(),
                endpoints: vec![DefaultEndpointTemplate {
                    protocol: "openai-chat".to_string(),
                    url: "https://api.minimaxi.com/v1".to_string(),
                    label: Some("OpenAI Chat".to_string()),
                    priority: 60,
                    enabled: true,
                }],
            },
        );
        let defaults = ProviderDefaultsCatalog { providers };

        let mut settings = Settings {
            gateway_port: 3125,
            log_retention_days: 30,
            gateway_key: String::new(),
            artificial_analysis_api_key: None,
            providers: HashMap::new(),
            models: HashMap::new(),
        };
        settings.providers.insert(
            "minimax-cn-coding-plan".to_string(),
            ProviderUserSettings {
                enabled: Some(true),
                api_key: None,
                api_keys: None,
                endpoints: Some(vec![sample_endpoint(
                    "anthropic",
                    "https://api.minimaxi.com/anthropic/v1",
                    50,
                )]),
            },
        );

        let resolved = resolve_provider_endpoints("minimax-cn-coding-plan", &defaults, &settings);
        assert_eq!(resolved.len(), 2);
    }
}
