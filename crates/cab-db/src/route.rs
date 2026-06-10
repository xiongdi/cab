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
    drop(inner);
    crate::state::save_from_store(store)?;
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
        let updated = r.clone();
        drop(inner);
        crate::state::save_from_store(store)?;
        Ok(Some(updated))
    } else {
        Ok(None)
    }
}

fn p_name_update(r: &mut Route, name: &str) {
    r.name = name.to_string();
}

pub async fn delete(store: &InMemoryStore, id: &str) -> Result<bool, String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    let removed = inner.routes.remove(id).is_some();
    drop(inner);
    if removed {
        crate::state::save_from_store(store)?;
    }
    Ok(removed)
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

    fn route(name: &str, pattern: &str, priority: i32, enabled: Option<bool>) -> CreateRoute {
        CreateRoute {
            name: name.into(),
            agent_pattern: pattern.into(),
            model_id: format!("model-{name}"),
            fallback_ids: Some(vec!["fallback-1".into(), "fallback-2".into()]),
            priority: Some(priority),
            routing_strategy: Some("manual".into()),
            enabled,
        }
    }

    #[tokio::test]
    async fn route_crud_matching_priority_and_delete() {
        let _home = TestHome::new();
        let store = InMemoryStore::new();

        let low = create(&store, &route("Low Route", "codex", 1, None))
            .await
            .unwrap();
        assert_eq!(low.id, "low-route");
        assert!(low.enabled);
        let high = create(&store, &route("High Route", "cod*", 20, Some(true)))
            .await
            .unwrap();
        assert_eq!(high.fallback_ids, vec!["fallback-1", "fallback-2"]);
        create(&store, &route("Suffix Route", "*code", 10, Some(true)))
            .await
            .unwrap();
        create(&store, &route("Contains Route", "*ode*", 5, Some(true)))
            .await
            .unwrap();
        create(&store, &route("Disabled Route", "codex", 100, Some(false)))
            .await
            .unwrap();

        let all = list(&store).await.unwrap();
        assert_eq!(all.first().unwrap().id, "disabled-route");
        assert_eq!(
            get_by_id(&store, "low-route").await.unwrap().unwrap().name,
            "Low Route"
        );

        let matched = find_for_agent(&store, "codex").await.unwrap();
        assert_eq!(
            matched.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            vec!["high-route", "contains-route", "low-route"]
        );
        assert_eq!(
            find_for_agent(&store, "vscode").await.unwrap()[0].id,
            "suffix-route"
        );
        assert!(find_for_agent(&store, "unknown").await.unwrap().is_empty());

        let updated = update(
            &store,
            "low-route",
            &UpdateRoute {
                name: Some("Renamed".into()),
                agent_pattern: Some("agent-*".into()),
                model_id: Some("model-new".into()),
                fallback_ids: Some(vec!["fb-new".into()]),
                priority: Some(30),
                routing_strategy: Some("balanced".into()),
                enabled: Some(false),
            },
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(updated.name, "Renamed");
        assert_eq!(updated.agent_pattern, "agent-*");
        assert_eq!(updated.model_id, "model-new");
        assert_eq!(updated.fallback_ids, vec!["fb-new"]);
        assert_eq!(updated.routing_strategy, "balanced");
        assert!(!updated.enabled);
        assert!(
            update(
                &store,
                "missing",
                &UpdateRoute {
                    name: None,
                    agent_pattern: None,
                    model_id: None,
                    fallback_ids: None,
                    priority: None,
                    routing_strategy: None,
                    enabled: None,
                },
            )
            .await
            .unwrap()
            .is_none()
        );

        assert!(delete(&store, "low-route").await.unwrap());
        assert!(!delete(&store, "low-route").await.unwrap());
    }
}
