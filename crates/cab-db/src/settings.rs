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
