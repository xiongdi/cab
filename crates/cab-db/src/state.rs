use std::collections::HashMap;

use cab_core::types::{Agent, PersistedState};

use crate::InMemoryStore;

pub const STATE_VERSION: u32 = 1;

/// Persist agents + routes to SQLite. No-op if no pool is available.
pub fn save_from_store(store: &InMemoryStore) -> Result<(), String> {
    let (agents, routes) = {
        let inner = store.inner.read().map_err(|e| e.to_string())?;
        (inner.agents.clone(), inner.routes.clone())
    };

    if let Some(pool) = &store.pool {
        let conn = pool.get().map_err(|e| e.to_string())?;
        crate::sqlite::save_state(&conn, &agents, &routes)?;
    }
    Ok(())
}

pub fn merge_into_store(store: &InMemoryStore, state: PersistedState) {
    let mut inner = store.inner.write().expect("store lock poisoned");
    // Overlay persisted agents onto seeded defaults so newly-supported agents
    // remain visible and unsupported persisted agents are dropped.
    for (id, agent) in state.agents {
        if inner.agents.contains_key(&id) {
            inner.agents.insert(id, agent);
        }
    }
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
        "reasonix",
    ] {
        let name = match *id {
            "claude-code" => "Claude Code",
            "codex" => "Codex",
            "opencode" => "OpenCode",
            "hermes" => "Hermes Agent",
            "kilocode" => "Kilo Code",
            "openclaw" => "OpenClaw",
            "pi" => "Pi",
            "reasonix" => "Reasonix",
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
    use crate::sqlite;

    fn test_store() -> InMemoryStore {
        let pool = sqlite::test_pool().unwrap();
        let conn = pool.get().unwrap();
        sqlite::init_schema(&conn).unwrap();
        InMemoryStore::with_sqlite(pool)
    }

    #[tokio::test]
    async fn save_and_load_round_trip() {
        let store = test_store();
        {
            let mut inner = store.inner.write().unwrap();
            inner.agents.get_mut("codex").unwrap().mode = "auto".to_string();
        }
        save_from_store(&store).unwrap();

        // Verify by reading from SQLite
        let conn = store.pool.as_ref().unwrap().get().unwrap();
        let loaded = sqlite::load_state(&conn).unwrap();
        assert_eq!(loaded.agents["codex"].mode, "auto");
        assert_eq!(loaded.version, STATE_VERSION);
    }
}
