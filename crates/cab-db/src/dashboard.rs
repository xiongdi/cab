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

#[cfg(test)]
mod tests {
    use super::*;
    use cab_core::types::{Model, Provider, ProviderEndpoint, RequestLog};

    fn provider(id: &str, enabled: bool, key: &str) -> Provider {
        Provider {
            id: id.into(),
            name: id.into(),
            endpoints: vec![ProviderEndpoint {
                id: "ep".into(),
                protocol: "openai-chat".into(),
                url: "https://example.test/v1".into(),
                label: None,
                priority: 1,
                enabled: true,
            }],
            api_key: key.into(),
            enabled,
            created_at: "now".into(),
            updated_at: "now".into(),
            privacy_policy_url: None,
            terms_of_service_url: None,
            status_page_url: None,
            headquarters: None,
            datacenters: None,
            api_keys: vec![],
            api: None,
            doc: None,
            env: None,
            npm: None,
            model_count: 0,
            catalog_models: vec![],
        }
    }

    fn model(id: &str, provider_id: &str, enabled: bool) -> Model {
        Model {
            id: id.into(),
            name: id.into(),
            display_name: id.into(),
            provider_id: provider_id.into(),
            protocol: "openai-chat".into(),
            context_length: 1,
            input_cost: None,
            output_cost: None,
            enabled,
            overall_intelligence: Some(1.0),
            coding_index: Some(1.0),
            agentic_index: Some(1.0),
            math_index: Some(1.0),
            output_speed_tps: None,
            time_to_first_token_secs: None,
            created_at: "now".into(),
            updated_at: "now".into(),
            canonical_slug: None,
            hugging_face_id: None,
            created: None,
            description: None,
            architecture: None,
            pricing: None,
            top_provider: None,
            per_request_limits: None,
            supported_parameters: None,
            default_parameters: None,
            supported_voices: None,
            knowledge_cutoff: None,
            expiration_date: None,
            links: None,
        }
    }

    fn log(id: &str, provider: &str, model: &str, tokens: i64) -> RequestLog {
        RequestLog {
            id: id.into(),
            timestamp: id.into(),
            agent: "codex".into(),
            provider: provider.into(),
            model: model.into(),
            input_tokens: tokens,
            output_tokens: 0,
            total_tokens: tokens,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            latency_ms: 1,
            status: 200,
            error: None,
            path: "/v1/chat/completions".into(),
            stream: false,
            request_body: None,
            response_body: None,
        }
    }

    #[tokio::test]
    async fn dashboard_stats_counts_active_resources_recent_logs_and_groupings() {
        let store = InMemoryStore::new();
        {
            let mut data = store.inner.write().unwrap();
            data.providers
                .insert("p1".into(), provider("p1", true, "key"));
            data.providers.insert("p2".into(), provider("p2", true, ""));
            data.providers.insert(
                "provider-ollama".into(),
                provider("provider-ollama", true, ""),
            );
            data.models.insert("m1".into(), model("m1", "p1", true));
            data.models.insert("m2".into(), model("m2", "p2", true));
            data.models
                .insert("m3".into(), model("m3", "provider-ollama", true));
            data.models.insert("m4".into(), model("m4", "p1", false));
            for idx in 0..6 {
                data.request_logs
                    .push(log(&format!("log-{idx}"), "p1", "m1", idx));
            }
        }

        let stats = get_stats(&store).await.unwrap();
        assert_eq!(stats.total_requests, 6);
        assert_eq!(stats.total_tokens, 15);
        assert_eq!(stats.providers_count, 2);
        assert_eq!(stats.models_count, 2);
        assert_eq!(stats.recent_requests.len(), 5);
        assert_eq!(stats.recent_requests[0].id, "log-5");
        assert_eq!(stats.requests_by_provider["p1"], 6);
        assert_eq!(stats.requests_by_model["m1"], 6);
    }
}
