use crate::InMemoryStore;
use cab_core::provider_defaults::{
    load_provider_defaults, resolve_provider_default_protocol, resolve_provider_endpoints,
};
use cab_core::types::{
    CreateProvider, Provider, ProviderEndpoint, Settings, UpdateProvider, select_preferred_api_key,
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
    logo: Option<&str>,
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
        if logo.is_some() {
            existing.logo = logo.map(|s| s.to_string());
        }
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
            logo: logo.map(|s| s.to_string()),
            catalog_models: catalog_models.to_vec(),
        };
        inner.providers.insert(id.to_string(), provider);
    }

    drop(inner);
    // Persist to SQLite
    if let Some(pool) = &store.pool {
        let conn = pool.get().map_err(|e| e.to_string())?;
        if let Some(provider) = store
            .inner
            .read()
            .map_err(|e| e.to_string())?
            .providers
            .get(id)
            .cloned()
        {
            crate::sqlite::upsert_catalog_provider(&conn, &provider)?;
        }
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
    let provider_clone = provider.clone();
    drop(inner);
    // Persist to SQLite
    if let Some(pool) = &store.pool {
        let conn = pool.get().map_err(|e| e.to_string())?;
        crate::sqlite::upsert_catalog_provider(&conn, &provider_clone)?;
    }
    Ok(())
}

/// Apply bundled defaults merged with user overrides from settings.
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
        if let Some(logo) = &user.logo {
            provider.logo = Some(logo.clone());
        }
    }

    provider.updated_at = chrono::Utc::now().to_rfc3339();
    let provider_clone = provider.clone();
    drop(inner);
    // Persist to SQLite
    if let Some(pool) = &store.pool {
        let conn = pool.get().map_err(|e| e.to_string())?;
        crate::sqlite::upsert_catalog_provider(&conn, &provider_clone)?;
    }
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
        logo: input.logo.clone(),
        catalog_models: Vec::new(),
    };
    inner.providers.insert(id, provider.clone());
    drop(inner);
    if let Some(pool) = &store.pool {
        let conn = pool.get().map_err(|e| e.to_string())?;
        crate::sqlite::upsert_catalog_provider(&conn, &provider)?;
    }
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
        if let Some(logo) = &input.logo {
            p.logo = logo.clone();
        }
        p.updated_at = chrono::Utc::now().to_rfc3339();
        let updated = p.clone();
        drop(inner);
        if let Some(pool) = &store.pool {
            let conn = pool.get().map_err(|e| e.to_string())?;
            crate::sqlite::upsert_catalog_provider(&conn, &updated)?;
        }
        Ok(Some(updated))
    } else {
        Ok(None)
    }
}

pub async fn delete(store: &InMemoryStore, id: &str) -> Result<bool, String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    let removed = inner.providers.remove(id).is_some();
    drop(inner);
    if removed && let Some(pool) = &store.pool {
        let conn = pool.get().map_err(|e| e.to_string())?;
        crate::sqlite::delete_catalog_provider(&conn, id)?;
    }
    Ok(removed)
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

    if let Some(user) = inner.settings.providers.get_mut(provider_id)
        && let Some(api_keys) = &mut user.api_keys
    {
        apply_quota_reset_to_keys(api_keys, key, Some(reset_at_str));
    }

    drop(inner);
    // Persist the modified settings to SQLite
    if let Some(pool) = &store.pool {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let s = store.inner.read().map_err(|e| e.to_string())?;
        crate::sqlite::save_settings(&conn, &s.settings)?;
    }
    Ok(())
}

/// Clear a recovered quota window after a successful upstream call.
pub async fn clear_api_key_quota_reset(
    store: &InMemoryStore,
    provider_id: &str,
    key: &str,
) -> Result<(), String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    let mut changed = false;

    if let Some(provider) = inner.providers.get_mut(provider_id)
        && apply_quota_reset_to_keys(&mut provider.api_keys, key, None)
    {
        provider.api_key = select_preferred_api_key(&provider.api_keys).unwrap_or_default();
        provider.updated_at = chrono::Utc::now().to_rfc3339();
        changed = true;
    }

    if let Some(user) = inner.settings.providers.get_mut(provider_id)
        && let Some(api_keys) = &mut user.api_keys
    {
        changed |= apply_quota_reset_to_keys(api_keys, key, None);
    }

    if !changed {
        return Ok(());
    }

    drop(inner);
    // Persist the modified settings to SQLite
    if let Some(pool) = &store.pool {
        let conn = pool.get().map_err(|e| e.to_string())?;
        let s = store.inner.read().map_err(|e| e.to_string())?;
        crate::sqlite::save_settings(&conn, &s.settings)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cab_core::types::{ApiKeyConfig, ProviderUserSettings};

    struct TestHome {
        _dir: tempfile::TempDir,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl TestHome {
        fn new() -> Self {
            let lock = crate::TEST_HOME_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let dir = tempfile::tempdir().unwrap();
            unsafe {
                std::env::set_var("HOME", dir.path());
                std::env::remove_var("USERPROFILE");
            }
            Self {
                _dir: dir,
                _lock: lock,
            }
        }
    }

    fn endpoint(id: &str, protocol: &str, url: &str) -> ProviderEndpoint {
        ProviderEndpoint {
            id: id.into(),
            protocol: protocol.into(),
            url: url.into(),
            label: Some(id.into()),
            priority: 50,
            enabled: true,
        }
    }

    fn api_key(key: &str) -> ApiKeyConfig {
        ApiKeyConfig {
            key: key.into(),
            enabled: true,
            quota_reset_at: None,
        }
    }

    fn create_provider() -> CreateProvider {
        CreateProvider {
            name: "Provider One".into(),
            endpoints: Some(vec![endpoint("ep-1", "openai-chat", "https://one.test/v1")]),
            api_key: "legacy-key".into(),
            enabled: Some(true),
            privacy_policy_url: Some("https://privacy.test".into()),
            terms_of_service_url: Some("https://terms.test".into()),
            status_page_url: Some("https://status.test".into()),
            headquarters: Some("Earth".into()),
            datacenters: Some(vec!["iad".into()]),
            api_keys: Some(vec![api_key("sub-key"), api_key("payg-key")]),
            api: Some("https://api.test".into()),
            doc: Some("https://docs.test".into()),
            env: Some(vec!["PROVIDER_KEY".into()]),
            npm: Some("@provider/sdk".into()),
            model_count: Some(2),
            logo: None,
        }
    }

    #[tokio::test]
    async fn provider_crud_catalog_and_config_paths() {
        let _home = TestHome::new();
        let store = InMemoryStore::new();

        let created = create(&store, &create_provider()).await.unwrap();
        assert_eq!(created.id, "provider-one");
        assert_eq!(created.api_key, "legacy-key");
        assert!(created.enabled);
        assert_eq!(created.api_keys[0].key, "sub-key");

        upsert_catalog_provider(
            &store,
            "catalog-b",
            "Catalog B",
            Some(("openai-chat", "https://catalog-b.test/v1")),
            Some("privacy"),
            Some("terms"),
            Some("status"),
            Some("HQ"),
            Some(&["iad".into(), "sfo".into()]),
            Some("api"),
            Some("doc"),
            Some(&["ENV".into()]),
            Some("npm"),
            3,
            None,
            &["model-a".into()],
        )
        .await
        .unwrap();
        upsert_catalog_provider(
            &store,
            "catalog-a",
            "Catalog A",
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            0,
            None,
            &[],
        )
        .await
        .unwrap();
        upsert_catalog_provider(
            &store,
            "catalog-b",
            "Catalog B Updated",
            None,
            None,
            None,
            None,
            None,
            None,
            Some("api2"),
            Some("doc2"),
            Some(&["ENV2".into()]),
            Some("npm2"),
            4,
            None,
            &["model-b".into()],
        )
        .await
        .unwrap();

        let catalog = list_catalog(&store).await.unwrap();
        assert_eq!(
            catalog
                .iter()
                .map(|provider| provider.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Catalog A", "Catalog B Updated", "Provider One"]
        );
        let active = list(&store).await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, "provider-one");

        ensure_extra_endpoints(
            &store,
            "provider-one",
            &[
                ("openai-chat", "https://one.test/v1", Some("dup"), 1),
                (
                    "anthropic",
                    "https://one.test/anthropic",
                    Some("Anthropic"),
                    2,
                ),
            ],
        )
        .await
        .unwrap();
        ensure_extra_endpoints(&store, "missing", &[("x", "https://x.test", None, 1)])
            .await
            .unwrap();
        assert_eq!(
            get_by_id(&store, "provider-one")
                .await
                .unwrap()
                .unwrap()
                .endpoints
                .len(),
            2
        );

        let updated = update(
            &store,
            "provider-one",
            &UpdateProvider {
                name: Some("Provider Updated".into()),
                endpoints: Some(vec![endpoint("ep-2", "anthropic", "https://two.test")]),
                api_key: Some("new-legacy".into()),
                enabled: Some(false),
                privacy_policy_url: None,
                terms_of_service_url: None,
                status_page_url: None,
                headquarters: None,
                datacenters: None,
                api_keys: Some(vec![api_key("new-sub")]),
                api: Some("api".into()),
                doc: Some("doc".into()),
                env: Some(vec!["ENV".into()]),
                npm: Some("npm".into()),
                model_count: Some(9),
                logo: None,
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(updated.name, "Provider Updated");
        assert_eq!(updated.api_key, "new-sub");
        assert!(!updated.enabled);
        assert_eq!(updated.endpoints[0].protocol, "anthropic");
        assert_eq!(updated.model_count, 9);
        assert!(
            update(
                &store,
                "missing",
                &UpdateProvider {
                    name: None,
                    endpoints: None,
                    api_key: None,
                    enabled: None,
                    privacy_policy_url: None,
                    terms_of_service_url: None,
                    status_page_url: None,
                    headquarters: None,
                    datacenters: None,
                    api_keys: None,
                    api: None,
                    doc: None,
                    env: None,
                    npm: None,
                    model_count: None,
                    logo: None,
                },
            )
            .await
            .unwrap()
            .is_none()
        );

        assert!(delete(&store, "catalog-a").await.unwrap());
        assert!(!delete(&store, "catalog-a").await.unwrap());
    }

    #[tokio::test]
    async fn provider_settings_and_quota_reset_paths() {
        let _home = TestHome::new();
        let store = InMemoryStore::new();
        create(&store, &create_provider()).await.unwrap();

        {
            let mut inner = store.inner.write().unwrap();
            inner.settings.providers.insert(
                "provider-one".into(),
                ProviderUserSettings {
                    enabled: Some(true),
                    api_key: Some("settings-legacy".into()),
                    api_keys: Some(vec![api_key("sub-key"), api_key("payg-key")]),
                    endpoints: Some(vec![endpoint(
                        "settings-ep",
                        "openai-responses",
                        "https://settings.test/v1",
                    )]),
                    logo: None,
                },
            );
        }

        let defaults = cab_core::ProviderDefaultsCatalog {
            providers: Default::default(),
        };
        let settings = store.inner.read().unwrap().settings.clone();
        apply_provider_config(&store, "provider-one", &settings, &defaults)
            .await
            .unwrap();
        apply_provider_config(
            &store,
            "missing",
            &crate::settings::default_settings(),
            &defaults,
        )
        .await
        .unwrap();
        let configured = get_by_id(&store, "provider-one").await.unwrap().unwrap();
        assert!(configured.enabled);
        assert_eq!(configured.api_key, "sub-key");
        assert_eq!(configured.endpoints[0].protocol, "openai-responses");

        let reset_at = chrono::Utc::now() + chrono::Duration::seconds(60);
        mark_api_key_quota_reset(&store, "provider-one", "sub-key", reset_at)
            .await
            .unwrap();
        let after_reset = get_by_id(&store, "provider-one").await.unwrap().unwrap();
        assert!(after_reset.api_keys[0].quota_reset_at.is_some());
        assert_eq!(after_reset.api_key, "payg-key");

        clear_api_key_quota_reset(&store, "provider-one", "sub-key")
            .await
            .unwrap();
        let cleared = get_by_id(&store, "provider-one").await.unwrap().unwrap();
        assert!(cleared.api_keys[0].quota_reset_at.is_none());
        assert_eq!(cleared.api_key, "sub-key");
        clear_api_key_quota_reset(&store, "provider-one", "missing")
            .await
            .unwrap();
        clear_api_key_quota_reset(&store, "missing", "sub-key")
            .await
            .unwrap();

        let settings = crate::settings::default_settings();
        assert_eq!(
            default_protocol_for_provider("unknown", &settings, &defaults),
            "openai-chat"
        );
    }
}
