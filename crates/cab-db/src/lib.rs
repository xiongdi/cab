pub mod agent;
pub mod auth;
pub mod catalog;
pub mod dashboard;
pub mod endpoint;
pub mod log;
pub mod model;
pub mod provider;
pub mod routability;
pub mod route;
pub mod settings;
pub mod sqlite;
pub mod state;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use cab_core::types::{Agent, Model, Provider, RequestLog, Route, Settings};

use crate::endpoint::ModelEndpoint;

#[cfg(test)]
pub(crate) static TEST_HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Clone)]
pub struct InMemoryStore {
    pub inner: Arc<RwLock<StoreData>>,
    pub health: Arc<cab_core::HealthTracker>,
    pub pool: Option<r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>>,
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
            health: Arc::new(cab_core::HealthTracker::new()),
            pool: None,
        }
    }

    pub fn with_sqlite(
        sqlite_pool: r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>,
    ) -> Self {
        let store = Self::new();
        Self {
            pool: Some(sqlite_pool),
            ..store
        }
    }

    pub fn sqlite(&self) -> Option<&r2d2::Pool<r2d2_sqlite::SqliteConnectionManager>> {
        self.pool.as_ref()
    }
}

pub async fn init_store() -> anyhow::Result<InMemoryStore> {
    let sqlite_pool = sqlite::init().map_err(|e| anyhow::anyhow!(e))?;
    let conn = sqlite_pool
        .get()
        .map_err(|e| anyhow::anyhow!("Pool get failed: {e}"))?;

    // Load settings from SQLite, or insert defaults if first run
    let settings = sqlite::load_settings(&conn)
        .map_err(|e| anyhow::anyhow!(e))?
        .unwrap_or_else(|| {
            let s = settings::default_settings();
            if let Err(e) = sqlite::save_settings(&conn, &s) {
                tracing::warn!("Failed to seed initial settings into SQLite: {e}");
            }
            s
        });

    // Load agents + routes from SQLite
    let persisted = sqlite::load_state(&conn).map_err(|e| anyhow::anyhow!(e))?;
    let has_persisted = !persisted.agents.is_empty() || !persisted.routes.is_empty();

    let store = InMemoryStore::with_sqlite(sqlite_pool);
    {
        let mut inner = store.inner.write().expect("store lock poisoned");
        inner.settings = settings;
        if has_persisted {
            inner.agents = persisted.agents;
            inner.routes = persisted.routes;
        }
    }

    // Persist initial seed agents/routes if state was empty
    if !has_persisted
        && let Err(e) = state::save_from_store(&store)
    {
        tracing::warn!("Failed to write initial state: {e}");
    }

    // Load catalog providers, models, and endpoints from SQLite
    match sqlite::load_catalog_providers(&conn) {
        Ok(providers) if !providers.is_empty() => {
            let count = providers.len();
            let mut inner = store.inner.write().expect("store lock poisoned");
            inner.providers = providers;
            tracing::info!("Loaded {count} catalog providers from SQLite");
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("Failed to load catalog providers from SQLite: {e}"),
    }
    match sqlite::load_catalog_models(&conn) {
        Ok(models) if !models.is_empty() => {
            let count = models.len();
            let mut inner = store.inner.write().expect("store lock poisoned");
            inner.models = models;
            tracing::info!("Loaded {count} catalog models from SQLite");
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("Failed to load catalog models from SQLite: {e}"),
    }
    match sqlite::load_model_endpoints(&conn) {
        Ok(endpoints) if !endpoints.is_empty() => {
            let count = endpoints.len();
            let mut inner = store.inner.write().expect("store lock poisoned");
            inner.model_endpoints = endpoints;
            tracing::info!("Loaded {count} model endpoints from SQLite");
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("Failed to load model endpoints from SQLite: {e}"),
    }

    // Enforce log retention
    let retention_days = store
        .inner
        .read()
        .expect("store lock poisoned")
        .settings
        .log_retention_days;
    if let Err(e) = sqlite::enforce_log_retention(&conn, retention_days) {
        tracing::warn!("Failed to enforce log retention: {e}");
    }

    // Load recent logs from SQLite into memory
    match sqlite::load_logs(&conn, 500) {
        Ok(logs) if !logs.is_empty() => {
            let count = logs.len();
            let mut inner = store.inner.write().expect("store lock poisoned");
            inner.request_logs = logs;
            tracing::info!("Loaded {count} request logs from SQLite");
        }
        Ok(_) => {}
        Err(e) => tracing::warn!("Failed to load request logs from SQLite: {e}"),
    }

    drop(conn);

    tracing::info!(
        "Store initialized with SQLite at {}",
        sqlite::db_path().display()
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
    async fn init_store_creates_sqlite_and_loads_settings() {
        let _home = TestHome::new();
        let store = init_store().await.unwrap();
        assert_eq!(store.inner.read().unwrap().settings.gateway_port, 3125);
        assert!(store.pool.is_some());
    }
}
