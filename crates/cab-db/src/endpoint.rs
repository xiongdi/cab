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
    /// Upstream wire protocol for this model on this provider (from models.dev per-model npm).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_protocol: Option<String>,
    pub quantization: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_cost: Option<f64>,
    pub cache_read_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_length: Option<i64>,
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

pub async fn find_for_model_provider(
    store: &InMemoryStore,
    model_id: &str,
    provider_tag: &str,
) -> Result<Option<ModelEndpoint>, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    Ok(inner
        .model_endpoints
        .values()
        .find(|ep| ep.model_id == model_id && ep.provider_tag == provider_tag)
        .cloned())
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
    drop(inner);
    if let Some(pool) = &store.pool {
        let conn = pool.get().map_err(|e| e.to_string())?;
        crate::sqlite::clear_model_endpoints(&conn)?;
    }
    Ok(())
}

pub async fn upsert(store: &InMemoryStore, ep: &ModelEndpoint) -> Result<(), String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    inner.model_endpoints.insert(ep.id.clone(), ep.clone());
    drop(inner);
    if let Some(pool) = &store.pool {
        let conn = pool.get().map_err(|e| e.to_string())?;
        crate::sqlite::upsert_model_endpoint(&conn, ep)?;
    }
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
    drop(inner);
    if deleted > 0
        && let Some(pool) = &store.pool
    {
        let conn = pool.get().map_err(|e| e.to_string())?;
        crate::sqlite::delete_model_endpoints_for(&conn, model_id)?;
    }
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
        let updated = ep.clone();
        drop(inner);
        if let Some(pool) = &store.pool {
            let conn = pool.get().map_err(|e| e.to_string())?;
            crate::sqlite::upsert_model_endpoint(&conn, &updated)?;
        }
        Ok(Some(updated))
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
    let mut changed: Vec<ModelEndpoint> = Vec::new();
    for ep in inner.model_endpoints.values_mut() {
        if ep.provider_name == provider_name || ep.provider_tag == provider_name {
            ep.enabled = enabled;
            changed.push(ep.clone());
        }
    }
    drop(inner);
    if !changed.is_empty()
        && let Some(pool) = &store.pool
    {
        let conn = pool.get().map_err(|e| e.to_string())?;
        for ep in &changed {
            crate::sqlite::upsert_model_endpoint(&conn, ep)?;
        }
    }
    Ok(())
}

pub async fn set_provider_tag_enabled(
    store: &InMemoryStore,
    provider_tag: &str,
    enabled: bool,
) -> Result<(), String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    let mut changed: Vec<ModelEndpoint> = Vec::new();
    for ep in inner.model_endpoints.values_mut() {
        if ep.provider_tag == provider_tag {
            ep.enabled = enabled;
            changed.push(ep.clone());
        }
    }
    drop(inner);
    if !changed.is_empty()
        && let Some(pool) = &store.pool
    {
        let conn = pool.get().map_err(|e| e.to_string())?;
        for ep in &changed {
            crate::sqlite::upsert_model_endpoint(&conn, ep)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn endpoint(id: &str, model_id: &str, provider_name: &str, enabled: bool) -> ModelEndpoint {
        ModelEndpoint {
            id: id.into(),
            model_id: model_id.into(),
            canonical_slug: model_id.into(),
            provider_name: provider_name.into(),
            provider_tag: format!("{provider_name}/{model_id}"),
            native_model_id: model_id.into(),
            upstream_protocol: None,
            quantization: "unknown".into(),
            input_cost: Some(1.0),
            output_cost: Some(2.0),
            cache_read_cost: Some(0.5),
            context_length: Some(128000),
            max_completion_tokens: Some(4096),
            status: 1,
            uptime_30m: Some(99.0),
            uptime_5m: Some(98.0),
            uptime_1d: Some(97.0),
            supports_tools: true,
            supports_streaming: true,
            enabled,
            updated_at: "now".into(),
        }
    }

    #[tokio::test]
    async fn endpoint_store_covers_summary_listing_toggles_and_delete() {
        let store = InMemoryStore::new();
        upsert(&store, &endpoint("b", "model-1", "Beta", true))
            .await
            .unwrap();
        upsert(&store, &endpoint("a", "model-1", "Alpha", false))
            .await
            .unwrap();
        upsert(&store, &endpoint("c", "model-2", "Beta", true))
            .await
            .unwrap();

        let summaries = provider_summary(&store).await.unwrap();
        let beta = summaries
            .iter()
            .find(|summary| summary.provider_name == "Beta")
            .unwrap();
        assert_eq!(beta.model_count, 2);

        let listed = list_for_model(&store, "model-1").await.unwrap();
        assert_eq!(
            listed
                .iter()
                .map(|ep| ep.provider_name.as_str())
                .collect::<Vec<_>>(),
            vec!["Alpha", "Beta"]
        );

        assert_eq!(
            enabled_provider_tags_for_model(&store, "model-1")
                .await
                .unwrap(),
            vec!["Beta/model-1"]
        );

        let toggled = set_enabled(&store, "a", true).await.unwrap().unwrap();
        assert!(toggled.enabled);
        assert!(
            set_enabled(&store, "missing", true)
                .await
                .unwrap()
                .is_none()
        );

        set_provider_enabled(&store, "Beta", false).await.unwrap();
        assert!(
            !list_for_model(&store, "model-1")
                .await
                .unwrap()
                .into_iter()
                .find(|ep| ep.provider_name == "Beta")
                .unwrap()
                .enabled
        );

        assert_eq!(delete_for_model(&store, "model-1").await.unwrap(), 2);
        assert!(list_for_model(&store, "model-1").await.unwrap().is_empty());

        clear_all(&store).await.unwrap();
        assert!(provider_summary(&store).await.unwrap().is_empty());
    }
}
