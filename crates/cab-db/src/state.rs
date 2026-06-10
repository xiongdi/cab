use std::collections::HashMap;
use std::path::PathBuf;

use cab_core::types::{Agent, PersistedState};

use crate::InMemoryStore;

pub const STATE_VERSION: u32 = 1;

pub fn state_file_path() -> PathBuf {
    crate::settings::settings_file_path()
        .parent()
        .map(|p| p.join("state.json"))
        .unwrap_or_else(|| PathBuf::from("state.json"))
}

pub fn save_from_store(store: &InMemoryStore) -> Result<(), String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    let state = PersistedState {
        version: STATE_VERSION,
        agents: inner.agents.clone(),
        routes: inner.routes.clone(),
    };
    drop(inner);
    save_to_disk(&state)
}

pub fn save_to_disk(state: &PersistedState) -> Result<(), String> {
    let path = state_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let content = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &content).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    tracing::info!("Saved state to {}", path.display());
    Ok(())
}

pub fn load_from_disk() -> Option<PersistedState> {
    let path = state_file_path();
    if !path.exists() {
        return None;
    }

    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<PersistedState>(&content) {
            Ok(state) => Some(state),
            Err(e) => {
                tracing::warn!("Failed to parse {}: {e}", path.display());
                None
            }
        },
        Err(e) => {
            tracing::warn!("Failed to read {}: {e}", path.display());
            None
        }
    }
}

pub fn merge_into_store(store: &InMemoryStore, state: PersistedState) {
    let mut inner = store.inner.write().expect("store lock poisoned");
    inner.agents = state.agents;
    inner.routes = state.routes;
}

pub fn seed_agents() -> HashMap<String, Agent> {
    let mut agents = HashMap::new();
    for id in &[
        "claude-code",
        "codex",
        "opencode",
        "hermes",
        "kilocode",
        "openclaw",
        "pi",
    ] {
        let name = match *id {
            "claude-code" => "Claude Code",
            "codex" => "Codex",
            "opencode" => "OpenCode",
            "hermes" => "Hermes Agent",
            "kilocode" => "Kilo Code",
            "openclaw" => "OpenClaw",
            "pi" => "Pi",
            _ => "",
        };
        agents.insert(
            id.to_string(),
            Agent {
                id: id.to_string(),
                name: name.to_string(),
                mode: "native".to_string(),
                model_id: None,
                api_key: "".to_string(),
                endpoint: "".to_string(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            },
        );
    }
    agents
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TEST_HOME_LOCK;

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
    fn save_and_load_round_trip() {
        let _home = TestHome::new();
        let store = InMemoryStore::new();
        {
            let mut inner = store.inner.write().unwrap();
            inner.agents.get_mut("codex").unwrap().mode = "auto".to_string();
        }
        save_from_store(&store).unwrap();
        let loaded = load_from_disk().expect("state file");
        assert_eq!(loaded.agents["codex"].mode, "auto");
        assert_eq!(loaded.version, STATE_VERSION);
    }

    #[test]
    fn atomic_write_uses_tmp_file() {
        let _home = TestHome::new();
        let state = PersistedState {
            version: STATE_VERSION,
            agents: seed_agents(),
            routes: HashMap::new(),
        };
        save_to_disk(&state).unwrap();
        assert!(state_file_path().exists());
        assert!(!state_file_path().with_extension("json.tmp").exists());
    }
}
