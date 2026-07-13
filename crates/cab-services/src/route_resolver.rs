use std::collections::HashSet;

use cab_core::types::ApiKeyConfig;
use cab_core::types::{Model, Provider, ProviderEndpoint};
use cab_core::{
    RequestProfile, RouteCandidate, RoutingStrategy, build_request_profile,
    provider_has_available_key, rank_route_candidates, select_preferred_api_key,
};
use cab_db::catalog::RouteCatalog;

const DEFAULT_MAX_FALLBACK_ROUTES: usize = 3;

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
    native.sort_by_key(|endpoint| std::cmp::Reverse(endpoint.priority));

    let mut others: Vec<ProviderEndpoint> = provider
        .endpoints
        .iter()
        .filter(|e| e.protocol != protocol && e.enabled)
        .cloned()
        .collect();
    others.sort_by_key(|endpoint| std::cmp::Reverse(endpoint.priority));

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
    catalog: &impl RouteCatalog,
    agent: &str,
    requested_model: Option<&str>,
    request_body: Option<&serde_json::Value>,
) -> Result<ResolvedRoute, cab_core::CabError> {
    let request_profile = request_body
        .map(|body| build_request_profile(body, agent))
        .unwrap_or_else(|| build_request_profile(&serde_json::json!({}), agent));
    // Step 0: In auto mode, agent.model_id is a routing strategy managed by CAB.
    if let Ok(Some(agent_config)) = catalog.agent(agent).await
        && agent_config.mode == "auto"
        && let Some(ref configured_route_id) = agent_config.model_id
        && !configured_route_id.is_empty()
        && let Some(resolved) =
            resolve_route_by_id(catalog, agent, configured_route_id, &request_profile).await?
    {
        return Ok(resolved);
    }

    // Step 1: Check routes for this agent (glob matching)
    let routes = catalog.routes_for_agent(agent).await?;

    if let Some(route) = routes.first() {
        let strategy = route.routing_strategy.as_str();

        if matches!(
            strategy,
            "cheapest" | "intelligent" | "balanced" | "auto" | "speed"
        ) && let Some(resolved) =
            resolve_by_strategy(catalog, strategy, &request_profile).await?
        {
            return Ok(resolved);
        }

        // For unrecognized strategy strings: use the route's primary model
        if let Some(resolved) = resolve_model(catalog, &route.model_id).await? {
            let mut fallbacks = Vec::new();
            for fid in &route.fallback_ids {
                if let Some(fb) = resolve_model(catalog, fid).await? {
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
            resolve_route_by_id(catalog, agent, &model_name, &request_profile).await?
        {
            return Ok(resolved);
        }
    }

    // Step 3: If a specific model was requested, try to find it
    if let Some(model_name) = requested_model {
        let model_name = normalize_requested_model(model_name);
        if let Some(resolved) = resolve_model_by_name(catalog, &model_name).await? {
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

/// Select provider+model routes using the shared routing engine (see `cab_core::routing`).
async fn resolve_by_strategy(
    catalog: &impl RouteCatalog,
    strategy: &str,
    profile: &RequestProfile,
) -> Result<Option<ResolvedRoute>, cab_core::CabError> {
    let Some(parsed) = RoutingStrategy::parse(strategy) else {
        return Ok(None);
    };

    let entries = catalog.list_routable_entries().await?;
    if entries.is_empty() {
        return Ok(None);
    }

    let candidates: Vec<RouteCandidate<'_>> = entries
        .iter()
        .filter_map(|entry| {
            let input = entry.endpoint_input_cost?;
            let output = entry.endpoint_output_cost?;
            Some(RouteCandidate {
                model: &entry.model,
                service_provider_id: &entry.service_provider_id,
                input_cost: input,
                output_cost: output,
                cache_read_cost: entry.endpoint_cache_read_cost,
            })
        })
        .collect();

    let ranked = rank_route_candidates(&candidates, parsed, profile);
    let allow_same_provider_fallbacks = !matches!(parsed, RoutingStrategy::Auto);
    resolve_ranked_routes(
        catalog,
        ranked,
        DEFAULT_MAX_FALLBACK_ROUTES,
        allow_same_provider_fallbacks,
    )
    .await
}

async fn resolve_ranked_routes(
    catalog: &impl RouteCatalog,
    ranked: Vec<(&Model, &str)>,
    max_routes: usize,
    allow_same_provider_fallbacks: bool,
) -> Result<Option<ResolvedRoute>, cab_core::CabError> {
    let mut resolved_models = Vec::new();
    let mut seen_model_ids = HashSet::new();
    let mut providers_in_result = HashSet::new();

    for (model, provider_id) in &ranked {
        if resolved_models.len() >= max_routes {
            break;
        }
        if seen_model_ids.contains(&model.id) {
            continue;
        }
        if !allow_same_provider_fallbacks && providers_in_result.contains(*provider_id) {
            continue;
        }
        if let Some(resolved) = try_resolve_with_provider(catalog, model, provider_id).await? {
            seen_model_ids.insert(resolved.model.id.clone());
            providers_in_result.insert(provider_id.to_string());
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
    catalog: &impl RouteCatalog,
    _agent: &str,
    route_id: &str,
    profile: &RequestProfile,
) -> Result<Option<ResolvedRoute>, cab_core::CabError> {
    // Check built-in strategy names
    match route_id {
        "cheapest" | "price" => return resolve_by_strategy(catalog, "cheapest", profile).await,
        "intelligent" => return resolve_by_strategy(catalog, "intelligent", profile).await,
        "balanced" => return resolve_by_strategy(catalog, "balanced", profile).await,
        "auto" => return resolve_by_strategy(catalog, "auto", profile).await,
        "speed" => return resolve_by_strategy(catalog, "speed", profile).await,
        _ => {}
    }

    // Check custom route in store
    if let Some(route) = catalog.route_by_id(route_id).await? {
        let strategy = route.routing_strategy.as_str();
        if matches!(
            strategy,
            "cheapest" | "intelligent" | "balanced" | "auto" | "speed"
        ) && let Some(resolved) = resolve_by_strategy(catalog, strategy, profile).await?
        {
            return Ok(Some(resolved));
        }

        if let Some(resolved) = resolve_model(catalog, &route.model_id).await? {
            let mut fallbacks = Vec::new();
            for fid in &route.fallback_ids {
                if let Some(fb) = resolve_model(catalog, fid).await? {
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
    catalog: &impl RouteCatalog,
    model_id: &str,
) -> Result<Option<ResolvedModel>, cab_core::CabError> {
    let Some(model) = catalog.model_by_id(model_id).await? else {
        return Ok(None);
    };

    if !model.enabled {
        return Ok(None);
    }

    if let Some(resolved) = try_resolve_with_provider(catalog, &model, &model.provider_id).await? {
        return Ok(Some(resolved));
    }

    let tags = catalog.enabled_provider_tags_for_model(&model.name).await?;
    for tag in tags {
        if tag == model.provider_id {
            continue;
        }
        if let Some(resolved) = try_resolve_with_provider(catalog, &model, &tag).await? {
            return Ok(Some(resolved));
        }
    }

    Ok(None)
}

async fn try_resolve_with_provider(
    catalog: &impl RouteCatalog,
    model: &Model,
    provider_id: &str,
) -> Result<Option<ResolvedModel>, cab_core::CabError> {
    let Some(provider) = catalog.provider_by_id(provider_id).await? else {
        return Ok(None);
    };

    if !provider.enabled || !provider_has_available_key(&provider) {
        return Ok(None);
    }

    if !catalog.is_provider_healthy(provider_id) {
        tracing::debug!(
            provider_id,
            "skipping unhealthy provider during route resolution"
        );
        return Ok(None);
    }

    let provider_routing = catalog.enabled_provider_tags_for_model(&model.name).await?;
    Ok(Some(ResolvedModel {
        model_protocol: model.protocol.clone(),
        model: model.clone(),
        provider_id: provider.id.clone(),
        api_keys: provider.api_keys.clone(),
        endpoint_candidates: pick_endpoints_for_protocol(&provider, &model.protocol),
        provider_api_key: active_provider_api_key(&provider),
        provider_name: provider.name.clone(),
        provider_routing,
    }))
}

fn active_provider_api_key(provider: &Provider) -> String {
    select_preferred_api_key(&provider.api_keys).unwrap_or_else(|| provider.api_key.clone())
}

async fn resolve_model_by_name(
    catalog: &impl RouteCatalog,
    model_name: &str,
) -> Result<Option<ResolvedModel>, cab_core::CabError> {
    let Some(model) = catalog.model_by_name(model_name).await? else {
        return Ok(None);
    };

    if !model.enabled {
        return Ok(None);
    }

    if let Some(resolved) = try_resolve_with_provider(catalog, &model, &model.provider_id).await? {
        return Ok(Some(resolved));
    }

    let tags = catalog.enabled_provider_tags_for_model(&model.name).await?;
    for tag in tags {
        if tag == model.provider_id {
            continue;
        }
        if let Some(resolved) = try_resolve_with_provider(catalog, &model, &tag).await? {
            return Ok(Some(resolved));
        }
    }

    Ok(None)
}

/// Re-resolve a specific model on a specific provider, bypassing scoring.
///
/// Used by session affinity to keep a conversation pinned to the provider+model
/// it first resolved to (so the upstream prefix cache keeps hitting). Returns
/// `None` when the pinned target is no longer usable — model/provider disabled,
/// no available key, or marked unhealthy — so the caller falls back to normal
/// routing and re-pins.
pub async fn resolve_model_on_provider(
    catalog: &impl RouteCatalog,
    model_name: &str,
    provider_id: &str,
) -> Result<Option<ResolvedRoute>, cab_core::CabError> {
    let Some(model) = catalog.model_by_name(model_name).await? else {
        return Ok(None);
    };
    if !model.enabled {
        return Ok(None);
    }
    let Some(resolved) = try_resolve_with_provider(catalog, &model, provider_id).await? else {
        return Ok(None);
    };
    Ok(Some(ResolvedRoute {
        model: resolved.model,
        provider_id: resolved.provider_id,
        api_keys: resolved.api_keys,
        endpoint_candidates: resolved.endpoint_candidates,
        provider_api_key: resolved.provider_api_key,
        model_protocol: resolved.model_protocol,
        provider_name: resolved.provider_name,
        provider_routing: resolved.provider_routing,
        fallback_models: vec![],
    }))
}

// Removed hardcoded benchmarks; scores come from the synced catalog.

#[cfg(test)]
mod tests {
    use super::*;
    use cab_core::types::{Agent, ApiKeyConfig, Model, Provider, ProviderEndpoint, Route};

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
            logo: None,
            catalog_models: vec![],
        }
    }

    fn active_provider(id: &str, key: &str) -> Provider {
        Provider {
            id: id.into(),
            name: format!("Provider {id}"),
            endpoints: vec![
                ProviderEndpoint {
                    id: format!("{id}-chat"),
                    protocol: "openai-chat".into(),
                    url: format!("https://{id}.test/v1"),
                    label: None,
                    priority: 50,
                    enabled: true,
                },
                ProviderEndpoint {
                    id: format!("{id}-responses"),
                    protocol: "openai-responses".into(),
                    url: format!("https://{id}.test/v1"),
                    label: None,
                    priority: 40,
                    enabled: true,
                },
            ],
            api_key: key.into(),
            enabled: true,
            created_at: "now".into(),
            updated_at: "now".into(),
            privacy_policy_url: None,
            terms_of_service_url: None,
            status_page_url: None,
            headquarters: None,
            datacenters: None,
            api_keys: vec![
                ApiKeyConfig {
                    key: "sub-key".into(),
                    enabled: true,
                    quota_reset_at: None,
                },
                ApiKeyConfig {
                    key: key.into(),
                    enabled: true,
                    quota_reset_at: None,
                },
            ],
            api: None,
            doc: None,
            env: None,
            npm: None,
            model_count: 0,
            logo: None,
            catalog_models: vec![],
        }
    }

    fn model(id: &str, provider_id: &str, cost: f64, intelligence: f64, enabled: bool) -> Model {
        Model {
            id: id.into(),
            name: format!("{provider_id}/{id}"),
            display_name: format!("Model {id}"),
            provider_id: provider_id.into(),
            protocol: "openai-chat".into(),
            context_length: 128000,
            input_cost: Some(cost),
            output_cost: Some(cost * 2.0),
            enabled,
            overall_intelligence: Some(intelligence),
            coding_index: Some(intelligence),
            agentic_index: Some(intelligence),
            math_index: Some(intelligence),
            output_speed_tps: None,
            time_to_first_token_secs: None,
            created_at: "now".into(),
            updated_at: "now".into(),
            canonical_slug: Some(format!("{provider_id}/{id}")),
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
            links: Some(serde_json::json!({"native_model_id": format!("native-{id}")})),
        }
    }

    fn route(id: &str, agent_pattern: &str, model_id: &str, fallbacks: Vec<&str>) -> Route {
        Route {
            id: id.into(),
            name: id.into(),
            agent_pattern: agent_pattern.into(),
            model_id: model_id.into(),
            fallback_ids: fallbacks.into_iter().map(str::to_string).collect(),
            priority: 10,
            routing_strategy: "manual".into(),
            enabled: true,
            created_at: "now".into(),
            updated_at: "now".into(),
        }
    }

    fn model_endpoint(id: &str, model_name: &str) -> cab_db::endpoint::ModelEndpoint {
        cab_db::endpoint::ModelEndpoint {
            id: id.into(),
            model_id: model_name.into(),
            canonical_slug: model_name.into(),
            provider_name: "provider".into(),
            provider_tag: format!("tag/{id}"),
            native_model_id: model_name.into(),
            upstream_protocol: None,
            quantization: "unknown".into(),
            input_cost: Some(0.0),
            output_cost: Some(0.0),
            cache_read_cost: None,
            context_length: Some(128000),
            max_completion_tokens: None,
            status: 1,
            uptime_30m: None,
            uptime_5m: None,
            uptime_1d: None,
            supports_tools: true,
            supports_streaming: true,
            enabled: true,
            updated_at: "now".into(),
        }
    }

    fn seeded_store() -> cab_db::InMemoryStore {
        let store = cab_db::InMemoryStore::new();
        {
            let mut data = store.inner.write().unwrap();
            data.providers
                .insert("p1".into(), active_provider("p1", "payg-key"));
            data.providers
                .insert("p2".into(), active_provider("p2", "payg-key-2"));
            data.models
                .insert("cheap".into(), model("cheap", "p1", 0.1, 20.0, true));
            data.models
                .insert("smart".into(), model("smart", "p1", 5.0, 95.0, true));
            data.models
                .insert("backup".into(), model("backup", "p2", 1.0, 50.0, true));
            for key in ["cheap", "smart", "backup"] {
                let name = data.models[key].name.clone();
                data.model_endpoints
                    .insert(format!("{key}-ep"), model_endpoint(key, &name));
            }
        }
        store
    }

    #[tokio::test]
    async fn resolves_requested_model_name_and_claude_alias() {
        let store = seeded_store();

        let resolved = resolve_route(&store, "codex", Some("p1/smart"), None)
            .await
            .unwrap();
        assert_eq!(resolved.model.id, "smart");
        assert_eq!(resolved.provider_api_key, "sub-key");
        assert_eq!(resolved.endpoint_candidates[0].protocol, "openai-chat");
        assert_eq!(resolved.provider_routing, vec!["tag/smart"]);

        let alias = resolve_route(&store, "codex", Some("claude/cab/p1/cheap"), None)
            .await
            .unwrap();
        assert_eq!(alias.model.id, "cheap");
        assert!(alias.fallback_models.is_empty());
        assert_eq!(alias.as_primary_model().model.name, "p1/cheap");
    }

    #[tokio::test]
    async fn resolves_matching_route_with_available_fallbacks() {
        let store = seeded_store();
        {
            let mut data = store.inner.write().unwrap();
            data.routes.insert(
                "codex-route".into(),
                route("codex-route", "codex", "smart", vec!["missing", "backup"]),
            );
        }

        let resolved = resolve_route(
            &store,
            "codex",
            Some("p1/cheap"),
            Some(&serde_json::json!({"messages": [{"role": "user", "content": "hi"}]})),
        )
        .await
        .unwrap();

        assert_eq!(resolved.model.id, "smart");
        assert_eq!(resolved.fallback_models.len(), 1);
        assert_eq!(resolved.fallback_models[0].model.id, "backup");
        assert_eq!(resolved.fallback_models[0].provider_name, "Provider p2");
    }

    #[tokio::test]
    async fn auto_mode_resolves_when_legacy_api_key_empty_but_api_keys_configured() {
        let store = cab_db::InMemoryStore::new();
        {
            let mut data = store.inner.write().unwrap();
            let mut provider = active_provider("p1", "");
            provider.api_key.clear();
            provider.api_keys = vec![ApiKeyConfig {
                key: "sk-from-array".into(),
                enabled: true,
                quota_reset_at: None,
            }];
            data.providers.insert("p1".into(), provider);
            data.models
                .insert("m1".into(), model("m1", "p1", 1.0, 50.0, true));
            let name = data.models["m1"].name.clone();
            data.model_endpoints
                .insert("m1-ep".into(), model_endpoint("m1", &name));
            data.agents.insert(
                "claude-code".into(),
                Agent {
                    id: "claude-code".into(),
                    name: "Claude Code".into(),
                    mode: "auto".into(),
                    model_id: Some("auto".into()),
                    api_key: String::new(),
                    endpoint: String::new(),
                    updated_at: "now".into(),
                },
            );
        }

        let resolved = resolve_route(
            &store,
            "claude-code",
            Some("claude-opus-4-8"),
            Some(&serde_json::json!({"messages": [{"role": "user", "content": "hi"}]})),
        )
        .await
        .unwrap();

        assert_eq!(resolved.model.id, "m1");
        assert_eq!(resolved.provider_api_key, "sk-from-array");
    }

    #[tokio::test]
    async fn auto_agent_configured_route_id_overrides_requested_model() {
        let store = seeded_store();
        {
            let mut data = store.inner.write().unwrap();
            data.agents.insert(
                "codex".into(),
                Agent {
                    id: "codex".into(),
                    name: "Codex".into(),
                    mode: "auto".into(),
                    model_id: Some("custom".into()),
                    api_key: String::new(),
                    endpoint: String::new(),
                    updated_at: "now".into(),
                },
            );
            data.routes
                .insert("custom".into(), route("custom", "*", "backup", vec![]));
        }

        let resolved = resolve_route(&store, "codex", Some("p1/smart"), None)
            .await
            .unwrap();
        assert_eq!(resolved.model.id, "backup");
    }

    #[tokio::test]
    async fn auto_strategy_only_one_model_per_provider() {
        let store = seeded_store();
        {
            let mut data = store.inner.write().unwrap();
            data.models
                .insert("mid".into(), model("mid", "p1", 2.0, 75.0, true));
            let mid_name = data.models["mid"].name.clone();
            data.model_endpoints
                .insert("mid-ep".into(), model_endpoint("mid", &mid_name));
            data.models.get_mut("backup").unwrap().enabled = false;
            data.providers.get_mut("p2").unwrap().api_keys = vec![ApiKeyConfig {
                key: "payg-only".into(),
                enabled: true,
                quota_reset_at: None,
            }];
            data.agents.insert(
                "claude-code".into(),
                Agent {
                    id: "claude-code".into(),
                    name: "Claude Code".into(),
                    mode: "auto".into(),
                    model_id: Some("auto".into()),
                    api_key: String::new(),
                    endpoint: String::new(),
                    updated_at: "now".into(),
                },
            );
        }

        let resolved = resolve_route(&store, "claude-code", Some("auto"), None)
            .await
            .unwrap();

        let primary_id = resolved.provider_id.clone();
        assert!(
            !resolved
                .fallback_models
                .iter()
                .any(|fallback| fallback.provider_id == primary_id),
            "same provider should not appear again in fallbacks"
        );
    }

    #[tokio::test]
    async fn auto_strategy_primary_with_fallback_provider() {
        let store = seeded_store();
        {
            let mut data = store.inner.write().unwrap();
            data.models.get_mut("backup").unwrap().input_cost = Some(10.0);
            data.models.get_mut("backup").unwrap().output_cost = Some(20.0);
            data.models.get_mut("cheap").unwrap().enabled = false;
            data.providers.get_mut("p2").unwrap().api_keys = vec![ApiKeyConfig {
                key: "payg-only".into(),
                enabled: true,
                quota_reset_at: None,
            }];
            data.agents.insert(
                "claude-code".into(),
                Agent {
                    id: "claude-code".into(),
                    name: "Claude Code".into(),
                    mode: "auto".into(),
                    model_id: Some("auto".into()),
                    api_key: String::new(),
                    endpoint: String::new(),
                    updated_at: "now".into(),
                },
            );
        }

        let resolved = resolve_route(&store, "claude-code", Some("auto"), None)
            .await
            .unwrap();

        assert_eq!(resolved.provider_id, "p1");
        assert!(
            resolved
                .fallback_models
                .iter()
                .any(|fallback| fallback.provider_id == "p2"),
            "expected secondary provider in fallbacks"
        );
    }

    #[tokio::test]
    async fn auto_strategy_skips_rate_limited_key_for_next_provider() {
        let store = seeded_store();
        {
            let mut data = store.inner.write().unwrap();
            let reset_at = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
            data.providers.get_mut("p1").unwrap().api_keys = vec![ApiKeyConfig {
                key: "sub-key".into(),
                enabled: true,
                quota_reset_at: Some(reset_at),
            }];
            data.providers.get_mut("p2").unwrap().api_keys = vec![ApiKeyConfig {
                key: "payg-only".into(),
                enabled: true,
                quota_reset_at: None,
            }];
            data.agents.insert(
                "claude-code".into(),
                Agent {
                    id: "claude-code".into(),
                    name: "Claude Code".into(),
                    mode: "auto".into(),
                    model_id: Some("auto".into()),
                    api_key: String::new(),
                    endpoint: String::new(),
                    updated_at: "now".into(),
                },
            );
        }

        let resolved = resolve_route(&store, "claude-code", Some("auto"), None)
            .await
            .unwrap();

        assert_eq!(
            resolved.provider_id, "p2",
            "rate-limited key should defer to next provider"
        );
    }

    #[tokio::test]
    async fn built_in_balanced_strategy_ranks_by_value_for_pi() {
        let store = seeded_store();
        {
            let mut data = store.inner.write().unwrap();
            data.models
                .insert("mid".into(), model("mid", "p1", 2.0, 75.0, true));
            let name = data.models["mid"].name.clone();
            data.model_endpoints
                .insert("mid-ep".into(), model_endpoint("mid", &name));
        }

        let resolved = resolve_route(
            &store,
            "pi",
            Some("balanced"),
            Some(&serde_json::json!({
                "messages": [{"role": "user", "content": "Write a Rust web server"}]
            })),
        )
        .await
        .unwrap();

        // Value scoring picks cheap model first when providers are comparable.
        // cheap:  20 / (0.1*10 + 0.2) = 40
        // mid:    75 / (2.0*10 + 4.0) = 3.125
        // backup: 50 / (1.0*10 + 2.0) = 3.33
        // smart:  95 / (5.0*10 + 10.0) = 3.8
        assert!(
            !resolved.fallback_models.is_empty(),
            "balanced should return fallbacks"
        );
        assert_eq!(resolved.model.id, "cheap");
        assert!(
            resolved.fallback_models.len() >= 2,
            "balanced should return multiple fallbacks"
        );
    }

    #[tokio::test]
    async fn built_in_balanced_strategy_prefers_good_value_over_flagship_on_payg() {
        let store = seeded_store();
        {
            // Use a single pay-as-you-go key per provider for this scenario.
            let mut data = store.inner.write().unwrap();
            data.providers.get_mut("p1").unwrap().api_keys = vec![ApiKeyConfig {
                key: "payg-key".into(),
                enabled: true,
                quota_reset_at: None,
            }];
            data.providers.get_mut("p2").unwrap().api_keys = vec![ApiKeyConfig {
                key: "payg-key-2".into(),
                enabled: true,
                quota_reset_at: None,
            }];
            data.models
                .insert("mid".into(), model("mid", "p1", 2.0, 75.0, true));
            let name = data.models["mid"].name.clone();
            data.model_endpoints
                .insert("mid-ep".into(), model_endpoint("mid", &name));
        }

        // Simulate pi sending model="balanced" with PAYG providers
        let resolved = resolve_route(
            &store,
            "pi",
            Some("balanced"),
            Some(&serde_json::json!({
                "messages": [{"role": "user", "content": "Write a Rust web server"}]
            })),
        )
        .await
        .unwrap();

        // Balanced with PAYG: ranks by capability / effective_cost
        // cheap:  20 / (0.1*3 + 0.2) = 20/0.5  = 40   ← best value
        // backup: 50 / (1.0*3 + 2.0)  = 50/5    = 10
        // mid:    75 / (2.0*3 + 4.0)  = 75/10   = 7.5
        // smart:  95 / (5.0*3 + 10.0) = 95/25   = 3.8
        assert!(
            !resolved.fallback_models.is_empty(),
            "balanced should return fallbacks"
        );
        assert_eq!(resolved.model.name, "p1/cheap");
        assert_eq!(resolved.fallback_models[0].model.name, "p2/backup");
        assert_eq!(resolved.fallback_models[1].model.name, "p1/mid");
    }

    #[tokio::test]
    async fn built_in_cheapest_strategy_filters_and_ranks_models() {
        let store = seeded_store();
        {
            let mut data = store.inner.write().unwrap();
            data.models
                .insert("disabled".into(), model("disabled", "p1", -1.0, 99.0, true));
            data.providers.get_mut("p2").unwrap().enabled = false;
        }

        let resolved = resolve_route(
            &store,
            "codex",
            Some("cheapest"),
            Some(&serde_json::json!({"messages": [{"role": "user", "content": "simple"}]})),
        )
        .await
        .unwrap();

        assert_eq!(resolved.model.id, "cheap");
        assert_eq!(resolved.fallback_models.len(), 1);
        assert_eq!(resolved.fallback_models[0].model.id, "smart");
    }

    #[tokio::test]
    async fn returns_not_found_when_requested_model_is_unusable() {
        let store = seeded_store();
        {
            let mut data = store.inner.write().unwrap();
            data.models.get_mut("smart").unwrap().enabled = false;
            data.providers.get_mut("p1").unwrap().enabled = false;
        }

        let err = resolve_route(&store, "codex", Some("p1/smart"), None)
            .await
            .unwrap_err();
        assert!(matches!(err, cab_core::CabError::NotFound(_)));
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
                id: "responses".into(),
                protocol: "openai-responses".into(),
                url: "https://example.com/v1".into(),
                label: None,
                priority: 50,
                enabled: true,
            },
        ]);
        let picked = pick_endpoints_for_protocol(&provider, "openai-responses");
        assert_eq!(picked.len(), 2);
        assert_eq!(picked[0].id, "responses");
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
