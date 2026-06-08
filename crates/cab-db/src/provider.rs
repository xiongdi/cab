use crate::InMemoryStore;
use cab_core::provider_defaults::{
    load_provider_defaults, resolve_provider_default_protocol, resolve_provider_endpoints,
};
use cab_core::types::{
    select_preferred_api_key, CreateProvider, Provider, ProviderEndpoint, Settings, UpdateProvider,
};
use uuid::Uuid;

pub async fn list(store: &InMemoryStore) -> Result<Vec<Provider>, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    let list: Vec<Provider> = inner
        .providers
        .values()
        .filter(|p| p.enabled)
        .cloned()
        .collect();
    Ok(list)
}

pub async fn list_catalog(store: &InMemoryStore) -> Result<Vec<Provider>, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    let mut list: Vec<Provider> = inner.providers.values().cloned().collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(list)
}

#[allow(clippy::too_many_arguments)]
pub async fn upsert_catalog_provider(
    store: &InMemoryStore,
    id: &str,
    name: &str,
    default_endpoint: Option<(&str, &str)>, // (protocol, url)
    privacy_policy_url: Option<&str>,
    terms_of_service_url: Option<&str>,
    status_page_url: Option<&str>,
    headquarters: Option<&str>,
    datacenters: Option<&[String]>,
    api: Option<&str>,
    doc: Option<&str>,
    env: Option<&[String]>,
    npm: Option<&str>,
    model_count: usize,
    catalog_models: &[String],
) -> Result<(), String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;

    let now = chrono::Utc::now().to_rfc3339();

    if let Some(existing) = inner.providers.get_mut(id) {
        existing.name = name.to_string();
        existing.privacy_policy_url = privacy_policy_url.map(|s| s.to_string());
        existing.terms_of_service_url = terms_of_service_url.map(|s| s.to_string());
        existing.status_page_url = status_page_url.map(|s| s.to_string());
        existing.headquarters = headquarters.map(|s| s.to_string());
        existing.datacenters = datacenters.map(|d| d.to_vec());
        existing.api = api.map(|s| s.to_string());
        existing.doc = doc.map(|s| s.to_string());
        existing.env = env.map(|v| v.to_vec());
        existing.npm = npm.map(|s| s.to_string());
        existing.model_count = model_count;
        existing.catalog_models = catalog_models.to_vec();
        existing.updated_at = now;
    } else {
        let mut endpoints = Vec::new();
        if let Some((protocol, url)) = default_endpoint {
            endpoints.push(ProviderEndpoint {
                id: Uuid::new_v4().to_string(),
                protocol: protocol.to_string(),
                url: url.to_string(),
                label: Some("Default Endpoint".to_string()),
                priority: 50,
                enabled: true,
            });
        }
        let provider = Provider {
            id: id.to_string(),
            name: name.to_string(),
            endpoints,
            api_key: "".to_string(),
            enabled: false,
            created_at: now.clone(),
            updated_at: now,
            privacy_policy_url: privacy_policy_url.map(|s| s.to_string()),
            terms_of_service_url: terms_of_service_url.map(|s| s.to_string()),
            status_page_url: status_page_url.map(|s| s.to_string()),
            headquarters: headquarters.map(|s| s.to_string()),
            datacenters: datacenters.map(|d| d.to_vec()),
            api_keys: Vec::new(),
            api: api.map(|s| s.to_string()),
            doc: doc.map(|s| s.to_string()),
            env: env.map(|v| v.to_vec()),
            npm: npm.map(|s| s.to_string()),
            model_count,
            catalog_models: catalog_models.to_vec(),
        };
        inner.providers.insert(id.to_string(), provider);
    }

    Ok(())
}

/// Ensure catalog providers have known extra endpoints (e.g. MiniMax Responses API).
pub async fn ensure_extra_endpoints(
    store: &InMemoryStore,
    id: &str,
    endpoints: &[(&str, &str, Option<&str>, i32)],
) -> Result<(), String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    let Some(provider) = inner.providers.get_mut(id) else {
        return Ok(());
    };

    for (protocol, url, label, priority) in endpoints {
        let exists = provider
            .endpoints
            .iter()
            .any(|e| e.protocol == *protocol && e.url == *url);
        if exists {
            continue;
        }
        provider.endpoints.push(ProviderEndpoint {
            id: Uuid::new_v4().to_string(),
            protocol: protocol.to_string(),
            url: url.to_string(),
            label: label.map(|s| s.to_string()),
            priority: *priority,
            enabled: true,
        });
    }

    provider.updated_at = chrono::Utc::now().to_rfc3339();
    Ok(())
}

/// Apply bundled defaults merged with user overrides from settings.json.
pub async fn apply_provider_config(
    store: &InMemoryStore,
    provider_id: &str,
    settings: &Settings,
    defaults: &cab_core::ProviderDefaultsCatalog,
) -> Result<(), String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    let Some(provider) = inner.providers.get_mut(provider_id) else {
        return Ok(());
    };

    let resolved_endpoints = resolve_provider_endpoints(provider_id, defaults, settings);
    if !resolved_endpoints.is_empty() {
        provider.endpoints = resolved_endpoints;
    }

    if let Some(user) = settings.providers.get(provider_id) {
        if let Some(enabled) = user.enabled {
            provider.enabled = enabled;
        }
        if let Some(api_keys) = &user.api_keys {
            provider.api_keys = api_keys.clone();
            provider.api_key = select_preferred_api_key(api_keys).unwrap_or_default();
        } else if let Some(api_key) = &user.api_key {
            provider.api_key = api_key.clone();
        }
    }

    provider.updated_at = chrono::Utc::now().to_rfc3339();
    Ok(())
}

pub fn default_protocol_for_provider(
    provider_id: &str,
    settings: &Settings,
    defaults: &cab_core::ProviderDefaultsCatalog,
) -> String {
    resolve_provider_default_protocol(provider_id, defaults, settings)
}

pub async fn apply_all_provider_configs(store: &InMemoryStore) -> Result<(), String> {
    let defaults = load_provider_defaults();
    let settings = {
        let inner = store.inner.read().map_err(|e| e.to_string())?;
        inner.settings.clone()
    };
    let provider_ids: Vec<String> = {
        let inner = store.inner.read().map_err(|e| e.to_string())?;
        inner.providers.keys().cloned().collect()
    };

    for provider_id in provider_ids {
        apply_provider_config(store, &provider_id, &settings, &defaults).await?;
    }
    Ok(())
}

pub async fn get_by_id(store: &InMemoryStore, id: &str) -> Result<Option<Provider>, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    Ok(inner.providers.get(id).cloned())
}

pub async fn create(store: &InMemoryStore, input: &CreateProvider) -> Result<Provider, String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    let id = input.name.to_lowercase().replace(' ', "-");
    let now = chrono::Utc::now().to_rfc3339();
    let provider = Provider {
        id: id.clone(),
        name: input.name.clone(),
        endpoints: input.endpoints.clone().unwrap_or_default(),
        api_key: input.api_key.clone(),
        enabled: input.enabled.unwrap_or(false),
        created_at: now.clone(),
        updated_at: now,
        privacy_policy_url: input.privacy_policy_url.clone(),
        terms_of_service_url: input.terms_of_service_url.clone(),
        status_page_url: input.status_page_url.clone(),
        headquarters: input.headquarters.clone(),
        datacenters: input.datacenters.clone(),
        api_keys: input.api_keys.clone().unwrap_or_default(),
        api: input.api.clone(),
        doc: input.doc.clone(),
        env: input.env.clone(),
        npm: input.npm.clone(),
        model_count: input.model_count.unwrap_or(0),
        catalog_models: Vec::new(),
    };
    inner.providers.insert(id, provider.clone());
    Ok(provider)
}

pub async fn update(
    store: &InMemoryStore,
    id: &str,
    input: &UpdateProvider,
) -> Result<Option<Provider>, String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    if let Some(p) = inner.providers.get_mut(id) {
        if let Some(ref name) = input.name {
            p.name = name.clone();
        }
        if let Some(ref endpoints) = input.endpoints {
            p.endpoints = endpoints.clone();
        }
        if let Some(ref api_key) = input.api_key {
            p.api_key = api_key.clone();
        }
        if let Some(ref enabled) = input.enabled {
            p.enabled = *enabled;
        }
        if let Some(ref key_configs) = input.api_keys {
            p.api_keys = key_configs.clone();
            p.api_key = select_preferred_api_key(key_configs).unwrap_or_default();
        }
        if let Some(ref api) = input.api {
            p.api = Some(api.clone());
        }
        if let Some(ref doc) = input.doc {
            p.doc = Some(doc.clone());
        }
        if let Some(ref env) = input.env {
            p.env = Some(env.clone());
        }
        if let Some(ref npm) = input.npm {
            p.npm = Some(npm.clone());
        }
        if let Some(model_count) = input.model_count {
            p.model_count = model_count;
        }
        p.updated_at = chrono::Utc::now().to_rfc3339();
        Ok(Some(p.clone()))
    } else {
        Ok(None)
    }
}

pub async fn delete(store: &InMemoryStore, id: &str) -> Result<bool, String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    Ok(inner.providers.remove(id).is_some())
}

fn apply_quota_reset_to_keys(
    api_keys: &mut [cab_core::types::ApiKeyConfig],
    key: &str,
    reset_at: Option<String>,
) -> bool {
    let mut changed = false;
    for entry in api_keys.iter_mut() {
        if entry.key == key {
            entry.quota_reset_at = reset_at.clone();
            changed = true;
        }
    }
    changed
}

/// Record when a subscription key's quota recovers after a 429.
pub async fn mark_api_key_quota_reset(
    store: &InMemoryStore,
    provider_id: &str,
    key: &str,
    reset_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), String> {
    let reset_at_str = reset_at.to_rfc3339();
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;

    if let Some(provider) = inner.providers.get_mut(provider_id) {
        apply_quota_reset_to_keys(&mut provider.api_keys, key, Some(reset_at_str.clone()));
        provider.api_key = select_preferred_api_key(&provider.api_keys).unwrap_or_default();
        provider.updated_at = chrono::Utc::now().to_rfc3339();
    }

    if let Some(user) = inner.settings.providers.get_mut(provider_id) {
        if let Some(api_keys) = &mut user.api_keys {
            apply_quota_reset_to_keys(api_keys, key, Some(reset_at_str));
        }
    }

    let settings = inner.settings.clone();
    drop(inner);
    crate::settings::save_to_disk(&settings)
}

/// Clear a recovered quota window after a successful upstream call.
pub async fn clear_api_key_quota_reset(
    store: &InMemoryStore,
    provider_id: &str,
    key: &str,
) -> Result<(), String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    let mut changed = false;

    if let Some(provider) = inner.providers.get_mut(provider_id) {
        if apply_quota_reset_to_keys(&mut provider.api_keys, key, None) {
            provider.api_key = select_preferred_api_key(&provider.api_keys).unwrap_or_default();
            provider.updated_at = chrono::Utc::now().to_rfc3339();
            changed = true;
        }
    }

    if let Some(user) = inner.settings.providers.get_mut(provider_id) {
        if let Some(api_keys) = &mut user.api_keys {
            changed |= apply_quota_reset_to_keys(api_keys, key, None);
        }
    }

    if !changed {
        return Ok(());
    }

    let settings = inner.settings.clone();
    drop(inner);
    crate::settings::save_to_disk(&settings)
}
