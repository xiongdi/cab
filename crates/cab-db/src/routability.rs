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

/// Resolve which enabled provider would serve this model. Under the provider-first
/// binding, a model is served exclusively by its native provider (`model.provider_id`)
/// when that provider is active — there are no reseller endpoints.
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
    Ok(None)
}

/// A model is routable when enabled and its native provider is active.
pub async fn is_model_routable(store: &InMemoryStore, model: &Model) -> Result<bool, String> {
    Ok(resolve_service_provider_id(store, model).await?.is_some())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutableModelEntry {
    #[serde(flatten)]
    pub model: Model,
    /// Gateway provider that would serve this route (e.g. opencode-go, minimax).
    pub service_provider_id: String,
    /// Per-provider pricing from the models.dev endpoint row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_input_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_output_cost: Option<f64>,
    pub endpoint_cache_read_cost: Option<f64>,
}

fn cache_read_from_model(model: &Model) -> Option<f64> {
    model
        .pricing
        .as_ref()
        .and_then(|pricing| pricing.get("cache_read"))
        .and_then(|value| value.as_f64())
        .filter(|cost| *cost >= 0.0)
}

fn known_pricing(input: Option<f64>, output: Option<f64>) -> Option<(f64, f64)> {
    match (input, output) {
        (Some(i), Some(o)) if i >= 0.0 && o >= 0.0 => Some((i, o)),
        _ => None,
    }
}

/// Pricing for routing comes from the provider-bound model row (models.dev
/// provider cost), not the global catalog reference table.
fn provider_model_pricing(model: &Model) -> Option<(f64, f64)> {
    if let Some(pricing) = known_pricing(model.input_cost, model.output_cost) {
        return Some(pricing);
    }
    let pricing = model.pricing.as_ref()?;
    let input = pricing.get("input")?.as_f64()?;
    let output = pricing.get("output")?.as_f64()?;
    known_pricing(Some(input), Some(output))
}

pub async fn list_routable_model_entries(
    store: &InMemoryStore,
) -> Result<Vec<RoutableModelEntry>, String> {
    let active = active_provider_ids(store).await?;
    let inner = store.inner.read().map_err(|e| e.to_string())?;

    let mut entries = Vec::new();
    for provider in inner.providers.values() {
        if !active.contains(&provider.id) {
            continue;
        }
        for bound in &provider.models {
            let model = &bound.model;
            if !model.enabled {
                continue;
            }
            let Some((input, output)) = provider_model_pricing(model) else {
                continue;
            };
            entries.push(RoutableModelEntry {
                model: model.clone(),
                service_provider_id: provider.id.clone(),
                endpoint_input_cost: Some(input),
                endpoint_output_cost: Some(output),
                endpoint_cache_read_cost: cache_read_from_model(model),
            });
        }
    }

    entries.sort_by(|a, b| {
        a.model
            .name
            .cmp(&b.model.name)
            .then_with(|| a.service_provider_id.cmp(&b.service_provider_id))
    });
    Ok(entries)
}

pub async fn list_routable_models(store: &InMemoryStore) -> Result<Vec<Model>, String> {
    let entries = list_routable_model_entries(store).await?;
    Ok(entries.into_iter().map(|entry| entry.model).collect())
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
                url: format!("https://{id}.example/v1"),
                label: None,
                priority: 10,
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
                quota_reset_at: None,
            }],
            api: None,
            doc: None,
            env: None,
            npm: None,
            model_count: 0,
            logo: None,
            catalog_models: vec![],
            models: vec![],
        }
    }

    fn model(id: &str, name: &str, provider_id: &str, enabled: bool) -> Model {
        Model {
            id: id.into(),
            name: name.into(),
            display_name: name.into(),
            provider_id: provider_id.into(),
            protocol: "openai-chat".into(),
            upstream_protocol: None,
            context_length: 128000,
            input_cost: Some(1.0),
            output_cost: Some(2.0),
            enabled,
            overall_intelligence: Some(50.0),
            coding_index: Some(50.0),
            agentic_index: Some(50.0),
            math_index: Some(50.0),
            output_speed_tps: None,
            time_to_first_token_secs: None,
            created_at: "now".into(),
            updated_at: "now".into(),
            canonical_slug: Some(name.into()),
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
    async fn lists_one_entry_per_active_provider_for_bound_model() {
        let store = InMemoryStore::new();
        {
            let mut inner = store.inner.write().unwrap();
            let mut minimax = provider("minimax", true, "k");
            minimax.models = vec![cab_core::ProviderModel {
                model: model("m1", "minimax/m3", "minimax", true),
            }];
            inner.providers.insert("minimax".into(), minimax);
        }

        let entries = list_routable_model_entries(&store).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].service_provider_id, "minimax");
        assert_eq!(entries[0].endpoint_input_cost, Some(1.0));
    }

    #[tokio::test]
    async fn model_not_routable_when_native_provider_disabled() {
        let store = InMemoryStore::new();
        {
            let mut inner = store.inner.write().unwrap();
            let mut deepseek = provider("deepseek", false, "k");
            deepseek.models = vec![cab_core::ProviderModel {
                model: model("m1", "deepseek/v4", "deepseek", true),
            }];
            inner.providers.insert("deepseek".into(), deepseek);
        }

        let entries = list_routable_model_entries(&store).await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn routable_uses_provider_pricing_json_when_scalar_fields_missing() {
        let store = InMemoryStore::new();
        {
            let mut inner = store.inner.write().unwrap();
            let mut ogo = provider("opencode-go", true, "k");
            let mut bound = model("m1", "vendor/model-a", "opencode-go", true);
            bound.input_cost = None;
            bound.output_cost = None;
            bound.pricing = Some(serde_json::json!({
                "input": 0.2,
                "output": 1.2,
                "cache_read": 0.02
            }));
            ogo.models = vec![cab_core::ProviderModel { model: bound }];
            inner.providers.insert("opencode-go".into(), ogo);
        }

        let entries = list_routable_model_entries(&store).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].endpoint_input_cost, Some(0.2));
        assert_eq!(entries[0].endpoint_output_cost, Some(1.2));
        assert_eq!(entries[0].endpoint_cache_read_cost, Some(0.02));
    }
}
