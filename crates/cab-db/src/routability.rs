use std::collections::HashSet;

use cab_core::types::Model;
use serde::{Deserialize, Serialize};

use crate::InMemoryStore;

/// Providers that can forward requests (enabled with a configured API key).
pub async fn active_provider_ids(store: &InMemoryStore) -> Result<HashSet<String>, String> {
    Ok(crate::provider::list(store)
        .await?
        .into_iter()
        .map(|provider| provider.id)
        .collect())
}

/// Resolve which enabled provider would actually serve this model (native vendor or reseller).
pub async fn resolve_service_provider_id(
    store: &InMemoryStore,
    model: &Model,
) -> Result<Option<String>, String> {
    if !model.enabled {
        return Ok(None);
    }

    let active = active_provider_ids(store).await?;
    if active.contains(&model.provider_id) {
        return Ok(Some(model.provider_id.clone()));
    }

    let tags = crate::endpoint::enabled_provider_tags_for_model(store, &model.name).await?;
    for tag in tags {
        if tag != model.provider_id && active.contains(&tag) {
            return Ok(Some(tag));
        }
    }

    Ok(None)
}

/// A model is routable when enabled and reachable via its native provider or an enabled reseller endpoint.
pub async fn is_model_routable(store: &InMemoryStore, model: &Model) -> Result<bool, String> {
    Ok(resolve_service_provider_id(store, model)
        .await?
        .is_some())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutableModelEntry {
    #[serde(flatten)]
    pub model: Model,
    pub service_provider_id: String,
}

pub async fn list_routable_model_entries(
    store: &InMemoryStore,
) -> Result<Vec<RoutableModelEntry>, String> {
    let models = crate::model::list(store).await?;
    let mut entries = Vec::new();
    for model in models {
        let Some(service_provider_id) = resolve_service_provider_id(store, &model).await? else {
            continue;
        };
        entries.push(RoutableModelEntry {
            model,
            service_provider_id,
        });
    }
    Ok(entries)
}

pub async fn list_routable_models(store: &InMemoryStore) -> Result<Vec<Model>, String> {
    Ok(list_routable_model_entries(store)
        .await?
        .into_iter()
        .map(|entry| entry.model)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cab_core::types::{ApiKeyConfig, Model, Provider, ProviderEndpoint};

    fn provider(id: &str, enabled: bool, api_key: &str) -> Provider {
        Provider {
            id: id.into(),
            name: id.into(),
            endpoints: vec![ProviderEndpoint {
                id: format!("{id}-ep"),
                protocol: "openai-chat".into(),
                url: "https://example.test/v1".into(),
                label: None,
                priority: 50,
                enabled: true,
            }],
            api_key: api_key.into(),
            enabled,
            created_at: "now".into(),
            updated_at: "now".into(),
            privacy_policy_url: None,
            terms_of_service_url: None,
            status_page_url: None,
            headquarters: None,
            datacenters: None,
            api_keys: vec![ApiKeyConfig {
                key: api_key.into(),
                enabled: true,
                subscribed: false,
                quota_reset_at: None,
            }],
            api: None,
            doc: None,
            env: None,
            npm: None,
            model_count: 0,
            catalog_models: vec![],
        }
    }

    fn model(name: &str, vendor: &str) -> Model {
        Model {
            id: format!("id-{name}"),
            name: name.into(),
            display_name: name.into(),
            provider_id: vendor.into(),
            protocol: "openai-chat".into(),
            context_length: 128000,
            input_cost: Some(1.0),
            output_cost: Some(2.0),
            enabled: true,
            overall_intelligence: 50.0,
            coding_index: 50.0,
            agentic_index: 50.0,
            math_index: 50.0,
            output_speed_tps: None,
            time_to_first_token_secs: None,
            created_at: "now".into(),
            updated_at: "now".into(),
            canonical_slug: None,
            hugging_face_id: None,
            created: None,
            description: None,
            architecture: None,
            pricing: None,
            top_provider: None,
            per_request_limits: None,
            supported_parameters: None,
            default_parameters: None,
            supported_voices: None,
            knowledge_cutoff: None,
            expiration_date: None,
            links: None,
        }
    }

    #[tokio::test]
    async fn service_provider_prefers_reseller_when_native_vendor_disabled() {
        let store = InMemoryStore::new();
        {
            let mut inner = store.inner.write().unwrap();
            inner.providers.insert(
                "deepseek".into(),
                provider("deepseek", false, ""),
            );
            inner.providers.insert(
                "opencode-go".into(),
                provider("opencode-go", true, "sk-test"),
            );
            inner.models.insert(
                "model-1".into(),
                model("deepseek/deepseek-v4-pro", "deepseek"),
            );
            inner.model_endpoints.insert(
                "deepseek/deepseek-v4-pro::opencode-go".into(),
                crate::endpoint::ModelEndpoint {
                    id: "deepseek/deepseek-v4-pro::opencode-go".into(),
                    model_id: "deepseek/deepseek-v4-pro".into(),
                    canonical_slug: "deepseek/deepseek-v4-pro".into(),
                    provider_name: "OpenCode Go".into(),
                    provider_tag: "opencode-go".into(),
                    native_model_id: "deepseek-v4-pro".into(),
                    quantization: "unknown".into(),
                    input_cost: 1.0,
                    output_cost: 2.0,
                    cache_read_cost: None,
                    context_length: 128000,
                    max_completion_tokens: None,
                    status: 0,
                    uptime_30m: None,
                    uptime_5m: None,
                    uptime_1d: None,
                    supports_tools: true,
                    supports_streaming: true,
                    enabled: true,
                    updated_at: "now".into(),
                },
            );
        }

        let entry = list_routable_model_entries(&store).await.unwrap();
        assert_eq!(entry.len(), 1);
        assert_eq!(entry[0].model.name, "deepseek/deepseek-v4-pro");
        assert_eq!(entry[0].service_provider_id, "opencode-go");
    }
}
