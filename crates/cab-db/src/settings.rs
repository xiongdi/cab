use std::path::PathBuf;

use cab_core::types::{ModelUserSettings, ProviderUserSettings, Settings};

use crate::InMemoryStore;

pub fn settings_file_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".cab").join("settings.json")
}

pub fn default_settings() -> Settings {
    Settings {
        gateway_port: 3125,
        log_retention_days: 30,
        gateway_key: "cab-token-6a05e2d5-c0f5-48fa-8656-e91026bb4b2a".to_string(),
        artificial_analysis_api_key: None,
        providers: Default::default(),
        models: Default::default(),
    }
}

pub fn load_from_disk() -> Settings {
    let path = settings_file_path();
    if !path.exists() {
        return default_settings();
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<Settings>(&content) {
            Ok(settings) => settings,
            Err(e) => {
                tracing::warn!("Failed to parse {}: {e}, using defaults", path.display());
                default_settings()
            }
        },
        Err(e) => {
            tracing::warn!("Failed to read {}: {e}, using defaults", path.display());
            default_settings()
        }
    }
}

pub fn save_to_disk(settings: &Settings) -> Result<(), String> {
    let path = settings_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let content = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    tracing::info!("Saved settings to {}", path.display());
    Ok(())
}

pub async fn get(store: &InMemoryStore) -> Result<Settings, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    Ok(inner.settings.clone())
}

pub async fn update(store: &InMemoryStore, settings: &Settings) -> Result<Settings, String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    inner.settings = settings.clone();
    drop(inner);
    save_to_disk(settings)?;
    Ok(settings.clone())
}

pub async fn set_provider_override(
    store: &InMemoryStore,
    provider_id: &str,
    override_settings: ProviderUserSettings,
) -> Result<(), String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    inner
        .settings
        .providers
        .insert(provider_id.to_string(), override_settings);
    let settings = inner.settings.clone();
    drop(inner);
    save_to_disk(&settings)
}

pub async fn set_model_override(
    store: &InMemoryStore,
    model_name: &str,
    override_settings: ModelUserSettings,
) -> Result<(), String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    inner
        .settings
        .models
        .insert(model_name.to_string(), override_settings);
    let settings = inner.settings.clone();
    drop(inner);
    save_to_disk(&settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cab_core::types::{ApiKeyConfig, ProviderEndpoint};

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

    #[test]
    fn settings_path_defaults_load_and_save_roundtrip() {
        let _home = TestHome::new();
        let path = settings_file_path();
        assert!(path.ends_with(".cab/settings.json"));

        let defaults = default_settings();
        assert_eq!(defaults.gateway_port, 3125);
        assert_eq!(defaults.log_retention_days, 30);
        assert!(defaults.gateway_key.starts_with("cab-token-"));
        assert!(defaults.providers.is_empty());
        assert!(defaults.models.is_empty());
        assert_eq!(load_from_disk().gateway_port, 3125);

        let mut settings = defaults.clone();
        settings.gateway_port = 4567;
        settings.artificial_analysis_api_key = Some("aa-key".into());
        settings.providers.insert(
            "provider-1".into(),
            ProviderUserSettings {
                enabled: Some(true),
                api_key: Some("legacy-key".into()),
                api_keys: Some(vec![ApiKeyConfig {
                    key: "key-1".into(),
                    enabled: true,
                    subscribed: true,
                    quota_reset_at: None,
                }]),
                endpoints: Some(vec![ProviderEndpoint {
                    id: "ep-1".into(),
                    protocol: "openai-chat".into(),
                    url: "https://example.test/v1".into(),
                    label: Some("Example".into()),
                    priority: 1,
                    enabled: true,
                }]),
            },
        );
        settings.models.insert(
            "model-1".into(),
            ModelUserSettings {
                enabled: Some(false),
            },
        );

        save_to_disk(&settings).unwrap();
        let loaded = load_from_disk();
        assert_eq!(loaded.gateway_port, 4567);
        assert_eq!(
            loaded.artificial_analysis_api_key.as_deref(),
            Some("aa-key")
        );
        assert_eq!(
            loaded.providers["provider-1"].api_keys.as_ref().unwrap()[0].key,
            "key-1"
        );
        assert_eq!(loaded.models["model-1"].enabled, Some(false));
    }

    #[test]
    fn load_from_disk_falls_back_for_invalid_json() {
        let _home = TestHome::new();
        let path = settings_file_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{not-json").unwrap();

        assert_eq!(load_from_disk().gateway_port, 3125);
    }

    #[tokio::test]
    async fn store_get_update_and_overrides_persist() {
        let _home = TestHome::new();
        let store = InMemoryStore::new();

        let mut settings = get(&store).await.unwrap();
        settings.gateway_port = 9999;
        let updated = update(&store, &settings).await.unwrap();
        assert_eq!(updated.gateway_port, 9999);
        assert_eq!(load_from_disk().gateway_port, 9999);

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

        let loaded = load_from_disk();
        assert_eq!(
            loaded.providers["provider-1"].api_key.as_deref(),
            Some("key")
        );
        assert_eq!(loaded.models["provider/model"].enabled, Some(false));
    }
}
