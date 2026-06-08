use std::collections::HashSet;

use cab_core::types::{Model, Provider, ProviderEndpoint};
use cab_core::types::ApiKeyConfig;
use cab_core::{
    RequestProfile, RoutingStrategy, build_request_profile, provider_has_subscribed_key,
    rank_models, select_preferred_api_key,
};

/// A resolved route target with provider details.
#[derive(Debug, Clone)]
pub struct ResolvedRoute {
    pub model: Model,
    pub provider_id: String,
    pub api_keys: Vec<ApiKeyConfig>,
    pub endpoint_candidates: Vec<ProviderEndpoint>,
    pub provider_api_key: String,
    pub model_protocol: String,
    pub provider_name: String,
    pub provider_routing: Vec<String>,
    pub fallback_models: Vec<ResolvedModel>,
}

#[derive(Debug, Clone)]
pub struct ResolvedModel {
    pub model: Model,
    pub provider_id: String,
    pub endpoint_candidates: Vec<ProviderEndpoint>,
    pub api_keys: Vec<ApiKeyConfig>,
    pub provider_api_key: String,
    pub model_protocol: String,
    pub provider_name: String,
    pub provider_routing: Vec<String>,
}

impl ResolvedRoute {
    pub fn as_primary_model(&self) -> ResolvedModel {
        ResolvedModel {
            model: self.model.clone(),
            provider_id: self.provider_id.clone(),
            api_keys: self.api_keys.clone(),
            endpoint_candidates: self.endpoint_candidates.clone(),
            provider_api_key: self.provider_api_key.clone(),
            model_protocol: self.model_protocol.clone(),
            provider_name: self.provider_name.clone(),
            provider_routing: self.provider_routing.clone(),
        }
    }
}

/// Filter and sort endpoints: native protocol first, then fall back to others for conversion.
/// Within each group, endpoints are sorted by priority descending.
pub fn pick_endpoints_for_protocol(provider: &Provider, protocol: &str) -> Vec<ProviderEndpoint> {
    let mut native: Vec<ProviderEndpoint> = provider
        .endpoints
        .iter()
        .filter(|e| e.protocol == protocol && e.enabled)
        .cloned()
        .collect();
    native.sort_by(|a, b| b.priority.cmp(&a.priority));

    let mut others: Vec<ProviderEndpoint> = provider
        .endpoints
        .iter()
        .filter(|e| e.protocol != protocol && e.enabled)
        .cloned()
        .collect();
    others.sort_by(|a, b| b.priority.cmp(&a.priority));

    if native.is_empty() && others.is_empty() {
        tracing::warn!(
            "No enabled endpoints found for provider {} (requested protocol {})",
            provider.id,
            protocol
        );
        return Vec::new();
    }

    if native.is_empty() {
        tracing::warn!(
            "No exact protocol match found for provider {} with protocol {}, falling back to alternate protocols",
            provider.id,
            protocol
        );
    }

    native.extend(others);
    native
}

/// Resolve which model + provider to use for a given agent and optional requested model.
///
/// Priority:
/// 1. If agent matches a route's agent_pattern → apply that route's routing_strategy
/// 2. If requested_model exists in our model DB → use it directly
/// 3. Return error
pub async fn resolve_route(
    pool: &cab_db::InMemoryStore,
    agent: &str,
    requested_model: Option<&str>,
    request_body: Option<&serde_json::Value>,
) -> Result<ResolvedRoute, cab_core::CabError> {
    let request_profile = request_body
        .map(|body| build_request_profile(body, agent))
        .unwrap_or_else(|| build_request_profile(&serde_json::json!({}), agent));
    // Step 0: In auto mode, agent.model_id is a routing strategy managed by CAB.
    if let Ok(Some(agent_config)) = cab_db::agent::get_by_id(pool, agent).await {
        if agent_config.mode == "auto" {
            if let Some(ref configured_route_id) = agent_config.model_id {
                if !configured_route_id.is_empty() {
                    if let Some(resolved) =
                        resolve_route_by_id(pool, agent, configured_route_id, &request_profile)
                            .await?
                    {
                        return Ok(resolved);
                    }
                }
            }
        }
    }

    // Step 1: Check routes for this agent (glob matching)
    let routes = cab_db::route::find_for_agent(pool, agent)
        .await
        .map_err(cab_core::CabError::Database)?;

    if let Some(route) = routes.first() {
        let strategy = route.routing_strategy.as_str();

        if matches!(strategy, "cheapest" | "intelligent" | "balanced" | "auto") {
            if let Some(resolved) = resolve_by_strategy(pool, strategy, &request_profile).await? {
                return Ok(resolved);
            }
        }

        // For unrecognized strategy strings: use the route's primary model
        if let Some(resolved) = resolve_model(pool, &route.model_id).await? {
            let mut fallbacks = Vec::new();
            for fid in &route.fallback_ids {
                if let Some(fb) = resolve_model(pool, fid).await? {
                    fallbacks.push(fb);
                }
            }
            return Ok(ResolvedRoute {
                model: resolved.model,
                provider_id: resolved.provider_id,
                api_keys: resolved.api_keys,
                endpoint_candidates: resolved.endpoint_candidates,
                provider_api_key: resolved.provider_api_key,
                model_protocol: resolved.model_protocol,
                provider_name: resolved.provider_name,
                provider_routing: resolved.provider_routing,
                fallback_models: fallbacks,
            });
        }
    }

    // Step 2: Built-in routing strategies passed as the requested model name
    if let Some(model_name) = requested_model {
        let model_name = normalize_requested_model(model_name);
        if let Some(resolved) =
            resolve_route_by_id(pool, agent, &model_name, &request_profile).await?
        {
            return Ok(resolved);
        }
    }

    // Step 3: If a specific model was requested, try to find it
    if let Some(model_name) = requested_model {
        let model_name = normalize_requested_model(model_name);
        if let Some(resolved) = resolve_model_by_name(pool, &model_name).await? {
            return Ok(ResolvedRoute {
                model: resolved.model,
                provider_id: resolved.provider_id,
                api_keys: resolved.api_keys,
                endpoint_candidates: resolved.endpoint_candidates,
                provider_api_key: resolved.provider_api_key,
                model_protocol: resolved.model_protocol,
                provider_name: resolved.provider_name,
                provider_routing: resolved.provider_routing,
                fallback_models: vec![],
            });
        }
    }

    Err(cab_core::CabError::NotFound(
        "No matching route or model found".to_string(),
    ))
}

fn normalize_requested_model(model_name: &str) -> String {
    model_name
        .strip_prefix("claude/cab/")
        .unwrap_or(model_name)
        .to_string()
}

/// Select models using the shared routing engine (see `cab_core::routing`).
async fn resolve_by_strategy(
    pool: &cab_db::InMemoryStore,
    strategy: &str,
    profile: &RequestProfile,
) -> Result<Option<ResolvedRoute>, cab_core::CabError> {
    let Some(parsed) = RoutingStrategy::parse(strategy) else {
        return Ok(None);
    };

    let enabled = enabled_routable_models(pool).await?;
    if enabled.is_empty() {
        return Ok(None);
    }

    let subscribed_provider_ids = subscribed_provider_ids(pool).await?;
    let ranked = rank_models(
        &enabled,
        parsed,
        profile,
        Some(&subscribed_provider_ids),
    );
    resolve_ranked_models(pool, ranked, 3).await
}

async fn subscribed_provider_ids(
    pool: &cab_db::InMemoryStore,
) -> Result<HashSet<String>, cab_core::CabError> {
    let all_providers = cab_db::provider::list_catalog(pool)
        .await
        .map_err(cab_core::CabError::Database)?;

    Ok(all_providers
        .into_iter()
        .filter(|p| provider_has_subscribed_key(&p.api_keys))
        .map(|p| p.id)
        .collect())
}

async fn enabled_routable_models(
    pool: &cab_db::InMemoryStore,
) -> Result<Vec<Model>, cab_core::CabError> {
    let all_models = cab_db::model::list(pool)
        .await
        .map_err(cab_core::CabError::Database)?;

    let all_providers = cab_db::provider::list(pool)
        .await
        .map_err(cab_core::CabError::Database)?;
    let active_provider_ids: std::collections::HashSet<String> = all_providers
        .into_iter()
        .filter(|p| p.enabled && (!p.api_key.is_empty() || p.id == "provider-ollama"))
        .map(|p| p.id)
        .collect();

    Ok(all_models
        .into_iter()
        .filter(|m| {
            m.enabled
                && active_provider_ids.contains(&m.provider_id)
                && m.input_cost.unwrap_or(0.0) >= 0.0
                && m.output_cost.unwrap_or(0.0) >= 0.0
        })
        .collect())
}

async fn resolve_ranked_models(
    pool: &cab_db::InMemoryStore,
    ranked: Vec<&Model>,
    max_models: usize,
) -> Result<Option<ResolvedRoute>, cab_core::CabError> {
    let mut resolved_models = Vec::new();
    for model in ranked.iter().take(max_models) {
        if let Some(resolved) = resolve_model(pool, &model.id).await? {
            resolved_models.push(resolved);
        }
    }

    let mut iter = resolved_models.into_iter();
    let Some(primary) = iter.next() else {
        return Ok(None);
    };
    let fallbacks = iter.collect();

    Ok(Some(ResolvedRoute {
        model: primary.model,
        provider_id: primary.provider_id,
        api_keys: primary.api_keys,
        endpoint_candidates: primary.endpoint_candidates,
        provider_api_key: primary.provider_api_key,
        model_protocol: primary.model_protocol,
        provider_name: primary.provider_name,
        provider_routing: primary.provider_routing,
        fallback_models: fallbacks,
    }))
}

async fn resolve_route_by_id(
    pool: &cab_db::InMemoryStore,
    _agent: &str,
    route_id: &str,
    profile: &RequestProfile,
) -> Result<Option<ResolvedRoute>, cab_core::CabError> {
    // Check built-in strategy names
    match route_id {
        "cheapest" | "price" => return resolve_by_strategy(pool, "cheapest", profile).await,
        "intelligent" => return resolve_by_strategy(pool, "intelligent", profile).await,
        "balanced" => return resolve_by_strategy(pool, "balanced", profile).await,
        "auto" => return resolve_by_strategy(pool, "auto", profile).await,
        _ => {}
    }

    // Check custom route in store
    if let Some(route) = cab_db::route::get_by_id(pool, route_id)
        .await
        .ok()
        .flatten()
    {
        let strategy = route.routing_strategy.as_str();
        if matches!(strategy, "cheapest" | "intelligent" | "balanced" | "auto") {
            if let Some(resolved) = resolve_by_strategy(pool, strategy, profile).await? {
                return Ok(Some(resolved));
            }
        }

        if let Some(resolved) = resolve_model(pool, &route.model_id).await? {
            let mut fallbacks = Vec::new();
            for fid in &route.fallback_ids {
                if let Some(fb) = resolve_model(pool, fid).await? {
                    fallbacks.push(fb);
                }
            }
            return Ok(Some(ResolvedRoute {
                model: resolved.model,
                provider_id: resolved.provider_id,
                api_keys: resolved.api_keys,
                endpoint_candidates: resolved.endpoint_candidates,
                provider_api_key: resolved.provider_api_key,
                model_protocol: resolved.model_protocol,
                provider_name: resolved.provider_name,
                provider_routing: resolved.provider_routing,
                fallback_models: fallbacks,
            }));
        }
    }

    Ok(None)
}

async fn resolve_model(
    pool: &cab_db::InMemoryStore,
    model_id: &str,
) -> Result<Option<ResolvedModel>, cab_core::CabError> {
    let model = cab_db::model::get_by_id(pool, model_id)
        .await
        .map_err(cab_core::CabError::Database)?;
    let Some(model) = model else {
        return Ok(None);
    };

    let provider = cab_db::provider::get_by_id(pool, &model.provider_id)
        .await
        .map_err(cab_core::CabError::Database)?;
    let Some(provider) = provider else {
        return Ok(None);
    };

    if !provider.enabled || provider.api_key.trim().is_empty() || !model.enabled {
        return Ok(None);
    }

    let provider_routing = cab_db::endpoint::enabled_provider_tags_for_model(pool, &model.name)
        .await
        .map_err(cab_core::CabError::Database)?;
    Ok(Some(ResolvedModel {
        model_protocol: model.protocol.clone(),
        model: model.clone(),
        provider_id: provider.id.clone(),
        api_keys: provider.api_keys.clone(),
        endpoint_candidates: pick_endpoints_for_protocol(&provider, &model.protocol),
        provider_api_key: active_provider_api_key(&provider),
        provider_name: provider.name,
        provider_routing,
    }))
}

fn active_provider_api_key(provider: &Provider) -> String {
    select_preferred_api_key(&provider.api_keys)
        .unwrap_or_else(|| provider.api_key.clone())
}

async fn resolve_model_by_name(
    pool: &cab_db::InMemoryStore,
    model_name: &str,
) -> Result<Option<ResolvedModel>, cab_core::CabError> {
    let model = cab_db::model::get_by_name(pool, model_name)
        .await
        .map_err(cab_core::CabError::Database)?;
    let Some(model) = model else {
        return Ok(None);
    };

    let provider = cab_db::provider::get_by_id(pool, &model.provider_id)
        .await
        .map_err(cab_core::CabError::Database)?;
    let Some(provider) = provider else {
        return Ok(None);
    };

    if !provider.enabled || provider.api_key.trim().is_empty() || !model.enabled {
        return Ok(None);
    }

    let provider_routing = cab_db::endpoint::enabled_provider_tags_for_model(pool, &model.name)
        .await
        .map_err(cab_core::CabError::Database)?;
    Ok(Some(ResolvedModel {
        model_protocol: model.protocol.clone(),
        model: model.clone(),
        provider_id: provider.id.clone(),
        api_keys: provider.api_keys.clone(),
        endpoint_candidates: pick_endpoints_for_protocol(&provider, &model.protocol),
        provider_api_key: active_provider_api_key(&provider),
        provider_name: provider.name,
        provider_routing,
    }))
}

// Removed hardcoded benchmarks; scores come from the synced catalog.

#[cfg(test)]
mod tests {
    use super::*;
    use cab_core::types::{Provider, ProviderEndpoint};

    fn make_provider(endpoints: Vec<ProviderEndpoint>) -> Provider {
        Provider {
            id: "p1".into(),
            name: "p1".into(),
            endpoints,
            api_key: "".into(),
            enabled: true,
            created_at: "".into(),
            updated_at: "".into(),
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

    #[test]
    fn picks_endpoints_matching_protocol() {
        let provider = make_provider(vec![
            ProviderEndpoint {
                id: "ep1".into(),
                protocol: "openai-chat".into(),
                url: "https://api.openai.com/v1".into(),
                label: None,
                priority: 50,
                enabled: true,
            },
            ProviderEndpoint {
                id: "ep2".into(),
                protocol: "anthropic".into(),
                url: "https://api.anthropic.com".into(),
                label: None,
                priority: 50,
                enabled: true,
            },
        ]);
        let picked = pick_endpoints_for_protocol(&provider, "anthropic");
        assert_eq!(picked.len(), 2);
        assert_eq!(picked[0].id, "ep2");
    }

    #[test]
    fn filters_disabled_endpoints() {
        let provider = make_provider(vec![
            ProviderEndpoint {
                id: "ep1".into(),
                protocol: "openai-chat".into(),
                url: "https://api.openai.com/v1".into(),
                label: None,
                priority: 50,
                enabled: false,
            },
            ProviderEndpoint {
                id: "ep2".into(),
                protocol: "openai-chat".into(),
                url: "https://backup.openai.com/v1".into(),
                label: None,
                priority: 40,
                enabled: true,
            },
        ]);
        let picked = pick_endpoints_for_protocol(&provider, "openai-chat");
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].id, "ep2");
    }

    #[test]
    fn sorts_by_priority_desc() {
        let provider = make_provider(vec![
            ProviderEndpoint {
                id: "low".into(),
                protocol: "openai-chat".into(),
                url: "https://low-priority.com".into(),
                label: None,
                priority: 10,
                enabled: true,
            },
            ProviderEndpoint {
                id: "high".into(),
                protocol: "openai-chat".into(),
                url: "https://high-priority.com".into(),
                label: None,
                priority: 100,
                enabled: true,
            },
            ProviderEndpoint {
                id: "medium".into(),
                protocol: "openai-chat".into(),
                url: "https://medium-priority.com".into(),
                label: None,
                priority: 50,
                enabled: true,
            },
        ]);
        let picked = pick_endpoints_for_protocol(&provider, "openai-chat");
        assert_eq!(picked.len(), 3);
        assert_eq!(picked[0].id, "high");
        assert_eq!(picked[1].id, "medium");
        assert_eq!(picked[2].id, "low");
    }

    #[test]
    fn returns_fallback_endpoints_when_no_protocol_match() {
        let provider = make_provider(vec![ProviderEndpoint {
            id: "ep1".into(),
            protocol: "anthropic".into(),
            url: "https://api.anthropic.com".into(),
            label: None,
            priority: 50,
            enabled: true,
        }]);
        let picked = pick_endpoints_for_protocol(&provider, "openai-chat");
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].id, "ep1");
    }

    #[test]
    fn native_protocol_endpoints_precede_alternate_protocols() {
        let provider = make_provider(vec![
            ProviderEndpoint {
                id: "openai".into(),
                protocol: "openai-chat".into(),
                url: "https://example.com/v1".into(),
                label: None,
                priority: 100,
                enabled: true,
            },
            ProviderEndpoint {
                id: "gemini".into(),
                protocol: "gemini".into(),
                url: "https://generativelanguage.googleapis.com/v1beta".into(),
                label: None,
                priority: 50,
                enabled: true,
            },
        ]);
        let picked = pick_endpoints_for_protocol(&provider, "gemini");
        assert_eq!(picked.len(), 2);
        assert_eq!(picked[0].id, "gemini");
        assert_eq!(picked[1].id, "openai");
    }

    #[test]
    fn returns_empty_when_no_enabled_endpoints() {
        let provider = make_provider(vec![ProviderEndpoint {
            id: "ep1".into(),
            protocol: "anthropic".into(),
            url: "https://api.anthropic.com".into(),
            label: None,
            priority: 50,
            enabled: false,
        }]);
        let picked = pick_endpoints_for_protocol(&provider, "openai-chat");
        assert!(picked.is_empty());
    }

    #[test]
    fn minimax_international_and_china_both_picked() {
        // Two anthropic endpoints, both enabled
        let provider = make_provider(vec![
            ProviderEndpoint {
                id: "china".into(),
                protocol: "anthropic".into(),
                url: "https://api.minimax.cn/anthropic/v1".into(),
                label: Some("China".to_string()),
                priority: 90,
                enabled: true,
            },
            ProviderEndpoint {
                id: "international".into(),
                protocol: "anthropic".into(),
                url: "https://api.minimax.chat/anthropic/v1".into(),
                label: Some("International".to_string()),
                priority: 80,
                enabled: true,
            },
        ]);
        let picked = pick_endpoints_for_protocol(&provider, "anthropic");
        assert_eq!(picked.len(), 2);
        // Higher priority first
        assert_eq!(picked[0].id, "china");
        assert_eq!(picked[1].id, "international");
    }
}
