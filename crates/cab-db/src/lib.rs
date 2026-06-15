pub mod agent;
pub mod auth;
pub mod catalog;
pub mod dashboard;
pub mod endpoint;
pub mod log;
pub mod log_store;
pub mod model;
pub mod provider;
pub mod routability;
pub mod route;
pub mod settings;
pub mod state;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use cab_core::types::{Agent, Model, Provider, RequestLog, Route, Settings};

use crate::endpoint::ModelEndpoint;

#[cfg(test)]
pub(crate) static TEST_HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Debug, Clone)]
pub struct InMemoryStore {
    pub inner: Arc<RwLock<StoreData>>,
}

#[derive(Debug)]
pub struct StoreData {
    pub providers: HashMap<String, Provider>,
    pub models: HashMap<String, Model>,
    pub routes: HashMap<String, Route>,
    pub agents: HashMap<String, Agent>,
    pub request_logs: Vec<RequestLog>,
    pub settings: Settings,
    pub model_endpoints: HashMap<String, ModelEndpoint>,
}

impl Default for InMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryStore {
    pub fn new() -> Self {
        let agents = state::seed_agents();

        Self {
            inner: Arc::new(RwLock::new(StoreData {
                providers: HashMap::new(),
                models: HashMap::new(),
                routes: HashMap::new(),
                agents,
                request_logs: Vec::new(),
                settings: settings::default_settings(),
                model_endpoints: HashMap::new(),
            })),
        }
    }
}

/// Initialize the in-memory store and load persisted settings from ~/.cab/settings.json.
pub async fn init_store() -> anyhow::Result<InMemoryStore> {
    let settings = settings::load_from_disk();
    let store = InMemoryStore::new();
    {
        let mut inner = store.inner.write().expect("store lock poisoned");
        inner.settings = settings;
    }

    if let Some(persisted) = state::load_from_disk() {
        state::merge_into_store(&store, persisted);
    } else if let Err(e) = state::save_from_store(&store) {
        tracing::warn!("Failed to write initial state.json: {e}");
    }

    let retention_days = store
        .inner
        .read()
        .expect("store lock poisoned")
        .settings
        .log_retention_days;
    if let Err(e) = log_store::enforce_retention(retention_days) {
        tracing::warn!("Failed to enforce log retention: {e}");
    }
    match log_store::load_into_store(&store) {
        Ok(count) if count > 0 => tracing::info!("Loaded {count} request logs from JSONL"),
        Ok(_) => {}
        Err(e) => tracing::warn!("Failed to load request logs: {e}"),
    }

    tracing::info!(
        "In-memory store initialized (settings: {}, state: {})",
        settings::settings_file_path().display(),
        state::state_file_path().display()
    );
    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHome {
        _dir: tempfile::TempDir,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl TestHome {
        fn new() -> Self {
            let lock = TEST_HOME_LOCK
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
    fn new_store_seeds_supported_agents_and_defaults() {
        let store = InMemoryStore::new();
        let data = store.inner.read().unwrap();
        assert_eq!(data.agents.len(), 7);
        assert!(data.agents.contains_key("claude-code"));
        assert_eq!(data.settings.gateway_port, 3125);
        assert!(data.providers.is_empty());
        assert!(data.models.is_empty());
        assert!(data.routes.is_empty());
        assert!(data.request_logs.is_empty());
        assert!(data.model_endpoints.is_empty());
    }

    #[tokio::test]
    async fn init_store_loads_settings_from_disk() {
        let _home = TestHome::new();
        let mut settings = settings::default_settings();
        settings.gateway_port = 4321;
        settings::save_to_disk(&settings).unwrap();

        let store = init_store().await.unwrap();
        assert_eq!(store.inner.read().unwrap().settings.gateway_port, 4321);
    }
}
