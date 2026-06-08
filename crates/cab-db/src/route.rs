use crate::InMemoryStore;
use cab_core::types::{CreateRoute, Route, UpdateRoute};

pub async fn list(store: &InMemoryStore) -> Result<Vec<Route>, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    let mut list: Vec<Route> = inner.routes.values().cloned().collect();
    list.sort_by_key(|b| std::cmp::Reverse(b.priority));
    Ok(list)
}

pub async fn get_by_id(store: &InMemoryStore, id: &str) -> Result<Option<Route>, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    Ok(inner.routes.get(id).cloned())
}

fn matches_pattern(agent: &str, pattern: &str) -> bool {
    let pat = pattern.replace('*', "");
    if pattern.starts_with('*') && pattern.ends_with('*') {
        agent.contains(&pat)
    } else if pattern.starts_with('*') {
        agent.ends_with(&pat)
    } else if pattern.ends_with('*') {
        agent.starts_with(&pat)
    } else {
        agent == pattern
    }
}

pub async fn find_for_agent(store: &InMemoryStore, agent: &str) -> Result<Vec<Route>, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    let mut matched: Vec<Route> = inner
        .routes
        .values()
        .filter(|r| r.enabled && matches_pattern(agent, &r.agent_pattern))
        .cloned()
        .collect();
    matched.sort_by_key(|b| std::cmp::Reverse(b.priority));
    Ok(matched)
}

pub async fn create(store: &InMemoryStore, input: &CreateRoute) -> Result<Route, String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    let id = input.name.to_lowercase().replace(' ', "-");
    let now = chrono::Utc::now().to_rfc3339();
    let route = Route {
        id: id.clone(),
        name: input.name.clone(),
        agent_pattern: input.agent_pattern.clone(),
        model_id: input.model_id.clone(),
        fallback_ids: input.fallback_ids.clone().unwrap_or_default(),
        priority: input.priority.unwrap_or(0),
        routing_strategy: input
            .routing_strategy
            .clone()
            .unwrap_or_else(|| "auto".to_string()),
        enabled: input.enabled.unwrap_or(true),
        created_at: now.clone(),
        updated_at: now,
    };
    inner.routes.insert(id, route.clone());
    Ok(route)
}

pub async fn update(
    store: &InMemoryStore,
    id: &str,
    input: &UpdateRoute,
) -> Result<Option<Route>, String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    if let Some(r) = inner.routes.get_mut(id) {
        if let Some(ref name) = input.name {
            p_name_update(r, name);
        }
        if let Some(ref agent_pattern) = input.agent_pattern {
            r.agent_pattern = agent_pattern.clone();
        }
        if let Some(ref model_id) = input.model_id {
            r.model_id = model_id.clone();
        }
        if let Some(ref fallback_ids) = input.fallback_ids {
            r.fallback_ids = fallback_ids.clone();
        }
        if let Some(ref priority) = input.priority {
            r.priority = *priority;
        }
        if let Some(ref routing_strategy) = input.routing_strategy {
            r.routing_strategy = routing_strategy.clone();
        }
        if let Some(ref enabled) = input.enabled {
            r.enabled = *enabled;
        }
        r.updated_at = chrono::Utc::now().to_rfc3339();
        Ok(Some(r.clone()))
    } else {
        Ok(None)
    }
}

fn p_name_update(r: &mut Route, name: &str) {
    r.name = name.to_string();
}

pub async fn delete(store: &InMemoryStore, id: &str) -> Result<bool, String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    Ok(inner.routes.remove(id).is_some())
}
