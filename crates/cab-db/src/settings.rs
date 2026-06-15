use std::path::PathBuf;

use cab_core::types::{ModelUserSettings, ProviderUserSettings, Settings, UpdateSettings};

use crate::InMemoryStore;

pub fn settings_file_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".cab").join("settings.json")
}

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

pub fn settings_backup_path() -> PathBuf {
    settings_file_path().with_extension("json.bak")
}

fn provider_user_has_keys(settings: &ProviderUserSettings) -> bool {
    if settings
        .api_key
        .as_ref()
        .is_some_and(|key| !key.trim().is_empty())
    {
        return true;
    }
    settings.api_keys.as_ref().is_some_and(|keys| {
        keys.iter()
            .any(|entry| entry.enabled && !entry.key.trim().is_empty())
    })
}

fn configured_provider_count(
    providers: &std::collections::HashMap<String, ProviderUserSettings>,
) -> usize {
    providers
        .values()
        .filter(|provider| provider_user_has_keys(provider))
        .count()
}

/// Never drop provider keys or model toggles when a gateway-only save races or regresses.
fn preserve_user_overrides(incoming: &mut Settings, on_disk: &Settings) {
    let disk_keys = configured_provider_count(&on_disk.providers);
    let incoming_keys = configured_provider_count(&incoming.providers);

    if disk_keys > incoming_keys {
        for (provider_id, disk_provider) in &on_disk.providers {
            if !provider_user_has_keys(disk_provider) {
                continue;
            }
            let replace = match incoming.providers.get(provider_id) {
                None => true,
                Some(incoming_provider) => !provider_user_has_keys(incoming_provider),
            };
            if replace {
                incoming
                    .providers
                    .insert(provider_id.clone(), disk_provider.clone());
            }
        }
        tracing::warn!(
            "Prevented settings save from wiping {disk_keys} configured provider(s); restored from disk"
        );
    }

    if incoming.models.is_empty() && !on_disk.models.is_empty() {
        incoming.models = on_disk.models.clone();
        tracing::warn!(
            "Prevented settings save from wiping {} model override(s); restored from disk",
            on_disk.models.len()
        );
    }
}

fn read_settings_file(path: &PathBuf) -> Option<Settings> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn load_from_disk() -> Settings {
    let path = settings_file_path();
    if !path.exists() {
        let settings = default_settings();
        if let Err(e) = save_to_disk(&settings) {
            tracing::warn!("Failed to write initial settings: {e}");
        }
        return settings;
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<Settings>(&content) {
            Ok(settings) => settings,
            Err(e) => {
                tracing::warn!("Failed to parse {}: {e}", path.display());
                if let Some(backup) = read_settings_file(&settings_backup_path()) {
                    tracing::warn!(
                        "Loaded settings from backup {}",
                        settings_backup_path().display()
                    );
                    return backup;
                }
                default_settings()
            }
        },
        Err(e) => {
            tracing::warn!("Failed to read {}: {e}, using defaults", path.display());
            read_settings_file(&settings_backup_path()).unwrap_or_else(default_settings)
        }
    }
}

pub fn save_to_disk(settings: &Settings) -> Result<(), String> {
    let path = settings_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let mut to_write = settings.clone();
    if let Some(on_disk) = read_settings_file(&path) {
        preserve_user_overrides(&mut to_write, &on_disk);
    }

    let content = serde_json::to_string_pretty(&to_write).map_err(|e| e.to_string())?;

    if path.exists() {
        let backup = settings_backup_path();
        if let Err(e) = std::fs::copy(&path, &backup) {
            tracing::warn!("Failed to write settings backup {}: {e}", backup.display());
        }
    }

    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &content).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    tracing::info!("Saved settings to {}", path.display());
    Ok(())
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
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    inner.settings = settings.clone();
    drop(inner);
    save_to_disk(settings)?;
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
    use cab_core::types::{
        ApiKeyConfig, ModelUserSettings, ProviderEndpoint, ProviderUserSettings,
    };

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
        assert!(defaults.auth_enabled);
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
    fn save_to_disk_preserves_provider_keys_when_incoming_is_empty() {
        let _home = TestHome::new();
        let path = settings_file_path();

        let mut with_keys = default_settings();
        with_keys.providers.insert(
            "minimax".into(),
            ProviderUserSettings {
                enabled: Some(true),
                api_key: Some("secret".into()),
                api_keys: None,
                endpoints: None,
            },
        );
        with_keys.models.insert(
            "minimax/MiniMax-M3".into(),
            ModelUserSettings {
                enabled: Some(true),
            },
        );
        save_to_disk(&with_keys).unwrap();

        let mut wiped = default_settings();
        wiped.gateway_port = 4999;
        save_to_disk(&wiped).unwrap();

        let loaded = load_from_disk();
        assert_eq!(loaded.gateway_port, 4999);
        assert_eq!(
            loaded.providers["minimax"].api_key.as_deref(),
            Some("secret")
        );
        assert_eq!(loaded.models["minimax/MiniMax-M3"].enabled, Some(true));
        assert!(settings_backup_path().exists());
        assert!(path.exists());
    }

    #[test]
    fn load_from_disk_recovers_from_backup_after_corruption() {
        let _home = TestHome::new();
        let path = settings_file_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();

        let mut settings = default_settings();
        settings.gateway_port = 4888;
        settings.providers.insert(
            "minimax".into(),
            ProviderUserSettings {
                enabled: Some(true),
                api_key: Some("backup-secret".into()),
                api_keys: None,
                endpoints: None,
            },
        );
        save_to_disk(&settings).unwrap();
        // Second save creates settings.json.bak from the first successful write.
        save_to_disk(&settings).unwrap();
        std::fs::write(&path, "{not-json").unwrap();

        let loaded = load_from_disk();
        assert_eq!(loaded.gateway_port, 4888);
        assert_eq!(
            loaded.providers["minimax"].api_key.as_deref(),
            Some("backup-secret")
        );
    }

    #[test]
    fn load_from_disk_falls_back_for_invalid_json() {
        let _home = TestHome::new();
        let path = settings_file_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{not-json").unwrap();
        assert!(settings_backup_path().exists() == false);

        assert_eq!(load_from_disk().gateway_port, 3125);
    }

    #[test]
    fn merge_settings_preserves_provider_overrides() {
        let mut current = default_settings();
        current.providers.insert(
            "minimax".into(),
            ProviderUserSettings {
                enabled: Some(true),
                api_key: Some("secret".into()),
                api_keys: None,
                endpoints: None,
            },
        );
        let merged = merge_settings(
            &current,
            &UpdateSettings {
                gateway_port: Some(4567),
                ..Default::default()
            },
        );
        assert_eq!(merged.gateway_port, 4567);
        assert_eq!(
            merged.providers["minimax"].api_key.as_deref(),
            Some("secret")
        );
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
