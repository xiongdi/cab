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
