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

pub async fn list_routable_model_entries(
    store: &InMemoryStore,
) -> Result<Vec<RoutableModelEntry>, String> {
    let active = active_provider_ids(store).await?;
    let models = crate::model::list(store).await?;
    let inner = store.inner.read().map_err(|e| e.to_string())?;

    let mut entries = Vec::new();
    for model in models {
        if !model.enabled {
            continue;
        }

        let mut added_providers = HashSet::new();
        for ep in inner.model_endpoints.values() {
            if ep.model_id != model.name || !ep.enabled {
                continue;
            }
            if !active.contains(&ep.provider_tag) {
                continue;
            }
            let Some((input, output)) = known_pricing(ep.input_cost, ep.output_cost) else {
                continue;
            };
            if !added_providers.insert(ep.provider_tag.clone()) {
                continue;
            }
            entries.push(RoutableModelEntry {
                model: model.clone(),
                service_provider_id: ep.provider_tag.clone(),
                endpoint_input_cost: Some(input),
                endpoint_output_cost: Some(output),
                endpoint_cache_read_cost: ep.cache_read_cost,
            });
        }

        // Native vendor reachable without a catalog endpoint row (tests / partial sync).
        if active.contains(&model.provider_id) && added_providers.insert(model.provider_id.clone())
        {
            if let Some((input, output)) = known_pricing(model.input_cost, model.output_cost) {
                entries.push(RoutableModelEntry {
                    model: model.clone(),
                    service_provider_id: model.provider_id.clone(),
                    endpoint_input_cost: Some(input),
                    endpoint_output_cost: Some(output),
                    endpoint_cache_read_cost: cache_read_from_model(&model),
                });
            }
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
    let mut seen = HashSet::new();
    let mut models = Vec::new();
    for entry in list_routable_model_entries(store).await? {
        if seen.insert(entry.model.id.clone()) {
            models.push(entry.model);
        }
    }
    Ok(models)
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
            catalog_models: vec![],
        }
    }

    fn model(id: &str, name: &str, provider_id: &str, enabled: bool) -> Model {
        Model {
            id: id.into(),
            name: name.into(),
            display_name: name.into(),
            provider_id: provider_id.into(),
            protocol: "openai-chat".into(),
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
    async fn lists_one_entry_per_active_provider_for_same_model() {
        let store = InMemoryStore::new();
        {
            let mut inner = store.inner.write().unwrap();
            inner
                .providers
                .insert("minimax".into(), provider("minimax", true, "k"));
            inner
                .providers
                .insert("opencode-go".into(), provider("opencode-go", true, "k2"));
            inner
                .models
                .insert("m1".into(), model("m1", "minimax/m3", "minimax", true));
            inner.model_endpoints.insert(
                "ep1".into(),
                crate::endpoint::ModelEndpoint {
                    id: "ep1".into(),
                    model_id: "minimax/m3".into(),
                    canonical_slug: "minimax/m3".into(),
                    provider_name: "MiniMax".into(),
                    provider_tag: "minimax".into(),
                    native_model_id: "m3".into(),
                    upstream_protocol: None,
                    quantization: "unknown".into(),
                    input_cost: Some(1.0),
                    output_cost: Some(2.0),
                    cache_read_cost: None,
                    context_length: Some(128000),
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
            inner.model_endpoints.insert(
                "ep2".into(),
                crate::endpoint::ModelEndpoint {
                    id: "ep2".into(),
                    model_id: "minimax/m3".into(),
                    canonical_slug: "minimax/m3".into(),
                    provider_name: "OpenCode Go".into(),
                    provider_tag: "opencode-go".into(),
                    native_model_id: "m3".into(),
                    upstream_protocol: None,
                    quantization: "unknown".into(),
                    input_cost: Some(0.1),
                    output_cost: Some(0.4),
                    cache_read_cost: None,
                    context_length: Some(128000),
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

        let entries = list_routable_model_entries(&store).await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].endpoint_input_cost, Some(1.0));
        let providers: HashSet<_> = entries
            .iter()
            .map(|e| e.service_provider_id.as_str())
            .collect();
        assert!(providers.contains("minimax"));
        assert!(providers.contains("opencode-go"));
    }

    #[tokio::test]
    async fn service_provider_prefers_reseller_when_native_vendor_disabled() {
        let store = InMemoryStore::new();
        {
            let mut inner = store.inner.write().unwrap();
            inner
                .providers
                .insert("deepseek".into(), provider("deepseek", false, "k"));
            inner
                .providers
                .insert("opencode-go".into(), provider("opencode-go", true, "k2"));
            inner
                .models
                .insert("m1".into(), model("m1", "deepseek/v4", "deepseek", true));
            inner.model_endpoints.insert(
                "ep-reseller".into(),
                crate::endpoint::ModelEndpoint {
                    id: "ep-reseller".into(),
                    model_id: "deepseek/v4".into(),
                    canonical_slug: "deepseek/v4".into(),
                    provider_name: "OpenCode Go".into(),
                    provider_tag: "opencode-go".into(),
                    native_model_id: "v4".into(),
                    upstream_protocol: None,
                    quantization: "unknown".into(),
                    input_cost: Some(0.5),
                    output_cost: Some(1.0),
                    cache_read_cost: None,
                    context_length: Some(128000),
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

        let entries = list_routable_model_entries(&store).await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].service_provider_id, "opencode-go");
        assert_eq!(entries[0].endpoint_input_cost, Some(0.5));
    }
}
