use crate::InMemoryStore;
use cab_core::types::DashboardStats;
use std::collections::HashMap;

pub async fn get_stats(store: &InMemoryStore) -> Result<DashboardStats, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;

    let total_requests = inner.request_logs.len() as i64;
    let total_tokens: i64 = inner.request_logs.iter().map(|l| l.total_tokens).sum();

    // Active providers: enabled and has a configured API key
    let providers_count = inner
        .providers
        .values()
        .filter(|p| p.enabled && (!p.api_key.is_empty() || p.id == "provider-ollama"))
        .count() as i64;

    // Active models: enabled and its provider is active
    let active_provider_ids: std::collections::HashSet<String> = inner
        .providers
        .values()
        .filter(|p| p.enabled && (!p.api_key.is_empty() || p.id == "provider-ollama"))
        .map(|p| p.id.clone())
        .collect();
    let models_count = inner
        .models
        .values()
        .filter(|m| m.enabled && active_provider_ids.contains(&m.provider_id))
        .count() as i64;

    let mut recent_requests = inner.request_logs.clone();
    recent_requests.reverse();
    recent_requests.truncate(5);

    let mut requests_by_provider = HashMap::new();
    let mut requests_by_model = HashMap::new();

    for l in &inner.request_logs {
        *requests_by_provider.entry(l.provider.clone()).or_insert(0) += 1;
        *requests_by_model.entry(l.model.clone()).or_insert(0) += 1;
    }

    Ok(DashboardStats {
        total_requests,
        total_tokens,
        providers_count,
        models_count,
        recent_requests,
        requests_by_provider,
        requests_by_model,
    })
}
