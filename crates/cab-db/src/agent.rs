use crate::InMemoryStore;
use cab_core::types::{Agent, UpdateAgent};

pub async fn list(store: &InMemoryStore) -> Result<Vec<Agent>, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    let mut list: Vec<Agent> = inner.agents.values().cloned().collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(list)
}

pub async fn get_by_id(store: &InMemoryStore, id: &str) -> Result<Option<Agent>, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    Ok(inner.agents.get(id).cloned())
}

pub async fn update(
    store: &InMemoryStore,
    id: &str,
    input: &UpdateAgent,
) -> Result<Option<Agent>, String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    if let Some(agent) = inner.agents.get_mut(id) {
        if let Some(ref mode) = input.mode {
            agent.mode = match mode.as_str() {
                "config" => "auto".to_string(),
                "proxy" => "native".to_string(),
                other => other.to_string(),
            };
        }
        if let Some(ref model_id_opt) = input.model_id {
            agent.model_id = match model_id_opt {
                Some(s) if !s.trim().is_empty() => Some(s.clone()),
                _ => None,
            };
        }
        if let Some(ref api_key) = input.api_key {
            agent.api_key = api_key.clone();
        }
        if let Some(ref endpoint) = input.endpoint {
            agent.endpoint = endpoint.clone();
        }
        agent.updated_at = chrono::Utc::now().to_rfc3339();
        Ok(Some(agent.clone()))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InMemoryStore;

    const SUPPORTED_AGENT_IDS: &[&str] = &[
        "claude-code",
        "codex",
        "opencode",
        "hermes",
        "kilocode",
        "openclaw",
        "pi",
    ];

    #[tokio::test]
    async fn default_store_lists_seven_supported_agents() {
        let store = InMemoryStore::new();
        let agents = list(&store).await.expect("list agents");
        assert_eq!(agents.len(), 7);
        for id in SUPPORTED_AGENT_IDS {
            assert!(agents.iter().any(|a| a.id == *id), "missing agent {id}");
        }
        assert!(!agents.iter().any(|a| a.id == "cursor"));
        assert!(!agents.iter().any(|a| a.id == "antigravity"));
    }

    #[tokio::test]
    async fn removed_agents_are_not_found() {
        let store = InMemoryStore::new();
        for id in ["cursor", "antigravity"] {
            let agent = get_by_id(&store, id).await.expect("get_by_id");
            assert!(agent.is_none(), "{id} should not exist");
        }
    }

    #[tokio::test]
    async fn update_normalizes_legacy_proxy_mode_to_native() {
        let store = InMemoryStore::new();
        {
            let mut inner = store.inner.write().expect("lock");
            inner.agents.get_mut("codex").unwrap().mode = "proxy".to_string();
        }
        let updated = update(
            &store,
            "codex",
            &UpdateAgent {
                mode: Some("proxy".to_string()),
                model_id: None,
                api_key: None,
                endpoint: None,
            },
        )
        .await
        .expect("update")
        .expect("codex exists");
        assert_eq!(updated.mode, "native");
    }
}
