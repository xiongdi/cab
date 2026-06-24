use cab_core::types::{ModelUserSettings, ProviderUserSettings, Settings, UpdateSettings};

use crate::InMemoryStore;

pub fn generate_gateway_key() -> String {
    format!("cab-token-{}", uuid::Uuid::new_v4())
}

pub fn default_settings() -> Settings {
    Settings {
        gateway_port: 3125,
        log_retention_days: 30,
        gateway_key: generate_gateway_key(),
        auth_enabled: true,
        artificial_analysis_api_key: None,
        providers: Default::default(),
        models: Default::default(),
    }
}

pub async fn get(store: &InMemoryStore) -> Result<Settings, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    Ok(inner.settings.clone())
}

pub fn merge_settings(current: &Settings, update: &UpdateSettings) -> Settings {
    let mut merged = current.clone();
    if let Some(gateway_port) = update.gateway_port {
        merged.gateway_port = gateway_port;
    }
    if let Some(log_retention_days) = update.log_retention_days {
        merged.log_retention_days = log_retention_days;
    }
    if let Some(gateway_key) = &update.gateway_key {
        merged.gateway_key = gateway_key.clone();
    }
    if let Some(auth_enabled) = update.auth_enabled {
        merged.auth_enabled = auth_enabled;
    }
    if let Some(aa_key) = &update.artificial_analysis_api_key {
        merged.artificial_analysis_api_key = aa_key.clone();
    }
    merged
}

pub async fn update(store: &InMemoryStore, settings: &Settings) -> Result<Settings, String> {
    {
        let mut inner = store.inner.write().map_err(|e| e.to_string())?;
        inner.settings = settings.clone();
    }
    if let Some(pool) = &store.pool {
        let conn = pool.get().map_err(|e| e.to_string())?;
        crate::sqlite::save_settings(&conn, settings)?;
    }
    Ok(settings.clone())
}

pub async fn apply_update(
    store: &InMemoryStore,
    patch: &UpdateSettings,
) -> Result<Settings, String> {
    let current = get(store).await?;
    let merged = merge_settings(&current, patch);
    update(store, &merged).await
}

pub async fn set_provider_override(
    store: &InMemoryStore,
    provider_id: &str,
    override_settings: ProviderUserSettings,
) -> Result<(), String> {
    let settings = {
        let mut inner = store.inner.write().map_err(|e| e.to_string())?;
        inner
            .settings
            .providers
            .insert(provider_id.to_string(), override_settings);
        inner.settings.clone()
    };
    if let Some(pool) = &store.pool {
        let conn = pool.get().map_err(|e| e.to_string())?;
        crate::sqlite::save_settings(&conn, &settings)?;
    }
    Ok(())
}

pub async fn set_model_override(
    store: &InMemoryStore,
    model_name: &str,
    override_settings: ModelUserSettings,
) -> Result<(), String> {
    let settings = {
        let mut inner = store.inner.write().map_err(|e| e.to_string())?;
        inner
            .settings
            .models
            .insert(model_name.to_string(), override_settings);
        inner.settings.clone()
    };
    if let Some(pool) = &store.pool {
        let conn = pool.get().map_err(|e| e.to_string())?;
        crate::sqlite::save_settings(&conn, &settings)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite;

    /// Create an in-memory SQLite-backed store for testing.
    async fn test_store() -> InMemoryStore {
        let pool = sqlite::test_pool().unwrap();
        let conn = pool.get().unwrap();
        sqlite::init_schema(&conn).unwrap();
        let store = InMemoryStore::with_sqlite(pool);
        store
    }

    #[test]
    fn generate_gateway_key_is_valid() {
        let key = generate_gateway_key();
        assert!(key.starts_with("cab-token-"));
    }

    #[tokio::test]
    async fn store_get_update_and_overrides_persist() {
        let store = test_store().await;

        let mut settings = get(&store).await.unwrap();
        settings.gateway_port = 9999;
        let updated = update(&store, &settings).await.unwrap();
        assert_eq!(updated.gateway_port, 9999);

        // Verify persisted in SQLite
        let conn = store.pool.as_ref().unwrap().get().unwrap();
        let loaded = sqlite::load_settings(&conn).unwrap().unwrap();
        assert_eq!(loaded.gateway_port, 9999);

        set_provider_override(
            &store,
            "provider-1",
            ProviderUserSettings {
                enabled: Some(true),
                api_key: Some("key".into()),
                api_keys: None,
                endpoints: None,
            },
        )
        .await
        .unwrap();
        set_model_override(
            &store,
            "provider/model",
            ModelUserSettings {
                enabled: Some(false),
            },
        )
        .await
        .unwrap();

        let stored = get(&store).await.unwrap();
        assert_eq!(stored.providers["provider-1"].enabled, Some(true));
        assert_eq!(stored.models["provider/model"].enabled, Some(false));
    }
}
