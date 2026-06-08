use crate::InMemoryStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEndpoint {
    pub id: String,
    pub model_id: String,
    pub canonical_slug: String,
    pub provider_name: String,
    pub provider_tag: String,
    pub native_model_id: String,
    pub quantization: String,
    pub input_cost: f64,
    pub output_cost: f64,
    pub cache_read_cost: Option<f64>,
    pub context_length: i64,
    pub max_completion_tokens: Option<i64>,
    pub status: i64,
    pub uptime_30m: Option<f64>,
    pub uptime_5m: Option<f64>,
    pub uptime_1d: Option<f64>,
    pub supports_tools: bool,
    pub supports_streaming: bool,
    pub enabled: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointProviderSummary {
    pub provider_name: String,
    pub model_count: i64,
}

pub async fn provider_summary(
    store: &InMemoryStore,
) -> Result<Vec<EndpointProviderSummary>, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    let mut counts = HashMap::new();
    for ep in inner.model_endpoints.values() {
        *counts.entry(ep.provider_name.clone()).or_insert(0) += 1;
    }
    let list = counts
        .into_iter()
        .map(|(provider_name, model_count)| EndpointProviderSummary {
            provider_name,
            model_count,
        })
        .collect();
    Ok(list)
}

pub async fn list_for_model(
    store: &InMemoryStore,
    model_id: &str,
) -> Result<Vec<ModelEndpoint>, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    let mut matched: Vec<ModelEndpoint> = inner
        .model_endpoints
        .values()
        .filter(|ep| ep.model_id == model_id)
        .cloned()
        .collect();
    matched.sort_by(|a, b| a.provider_name.cmp(&b.provider_name));
    Ok(matched)
}

pub async fn clear_all(store: &InMemoryStore) -> Result<(), String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    inner.model_endpoints.clear();
    Ok(())
}

pub async fn upsert(store: &InMemoryStore, ep: &ModelEndpoint) -> Result<(), String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    inner.model_endpoints.insert(ep.id.clone(), ep.clone());
    Ok(())
}

pub async fn delete_for_model(store: &InMemoryStore, model_id: &str) -> Result<u64, String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    let mut deleted = 0;
    inner.model_endpoints.retain(|_, ep| {
        if ep.model_id == model_id {
            deleted += 1;
            false
        } else {
            true
        }
    });
    Ok(deleted)
}

pub async fn set_enabled(
    store: &InMemoryStore,
    id: &str,
    enabled: bool,
) -> Result<Option<ModelEndpoint>, String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    if let Some(ep) = inner.model_endpoints.get_mut(id) {
        ep.enabled = enabled;
        Ok(Some(ep.clone()))
    } else {
        Ok(None)
    }
}

pub async fn set_provider_enabled(
    store: &InMemoryStore,
    provider_name: &str,
    enabled: bool,
) -> Result<(), String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    for ep in inner.model_endpoints.values_mut() {
        if ep.provider_name == provider_name {
            ep.enabled = enabled;
        }
    }
    Ok(())
}

pub async fn enabled_provider_tags_for_model(
    store: &InMemoryStore,
    model_name: &str,
) -> Result<Vec<String>, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    let matched: Vec<String> = inner
        .model_endpoints
        .values()
        .filter(|ep| ep.model_id == model_name && ep.enabled)
        .map(|ep| ep.provider_tag.clone())
        .collect();
    Ok(matched)
}
