use std::collections::HashSet;

use cab_core::types::{
    DecisionStep, RankedModelSummary, ResolvedSummary, RouteExplainRequest, RouteExplainResult,
};
use cab_core::{
    RequestProfile, RouteCandidate, RoutingStrategy, build_request_profile,
    model_routable_for_strategy, provider_has_subscribed_key, rank_route_candidates_with_scores,
};
use cab_db::InMemoryStore;
use cab_db::catalog::RouteCatalog;

use crate::route_resolver::{ResolvedRoute, resolve_route};

fn push_step(steps: &mut Vec<DecisionStep>, step: &str, matched: bool, detail: impl Into<String>) {
    steps.push(DecisionStep {
        step: step.to_string(),
        matched,
        detail: detail.into(),
    });
}

async fn infer_strategy(
    agent: &str,
    requested_model: Option<&str>,
    pool: &InMemoryStore,
) -> RoutingStrategy {
    if let Ok(Some(agent_config)) = cab_db::agent::get_by_id(pool, agent).await
        && agent_config.mode == "auto"
        && let Some(ref configured) = agent_config.model_id
        && !configured.is_empty()
        && let Some(parsed) = RoutingStrategy::parse(configured)
    {
        return parsed;
    }

    if let Ok(routes) = pool.routes_for_agent(agent).await
        && let Some(route) = routes.first()
        && let Some(parsed) = RoutingStrategy::parse(route.routing_strategy.as_str())
    {
        return parsed;
    }

    if let Some(model) = requested_model
        && let Some(parsed) = RoutingStrategy::parse(model)
    {
        return parsed;
    }

    RoutingStrategy::Auto
}

async fn subscribed_provider_ids(pool: &InMemoryStore) -> HashSet<String> {
    cab_db::provider::list_catalog(pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|p| provider_has_subscribed_key(&p.api_keys))
        .map(|p| p.id)
        .collect()
}

async fn ranked_candidates(
    pool: &InMemoryStore,
    strategy: RoutingStrategy,
    profile: &RequestProfile,
) -> Vec<RankedModelSummary> {
    let entries = cab_db::routability::list_routable_model_entries(pool)
        .await
        .unwrap_or_default();
    if entries.is_empty() {
        return Vec::new();
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

    let subscribed = subscribed_provider_ids(pool).await;
    let mut ranked = Vec::new();
    for score in rank_route_candidates_with_scores(
        &candidates,
        strategy,
        profile,
        Some(&subscribed),
    ) {
        if ranked.len() >= 10 {
            break;
        }
        ranked.push(RankedModelSummary {
            model_id: score.model.name.clone(),
            provider_id: score.service_provider_id.to_string(),
            subscribed: subscribed.contains(score.service_provider_id),
            capability: if model_routable_for_strategy(score.model, strategy, profile.task)
                && score.capability.is_finite()
            {
                Some(score.capability)
            } else {
                None
            },
            value: if model_routable_for_strategy(score.model, strategy, profile.task)
                && score.value.is_finite()
            {
                Some(score.value)
            } else {
                None
            },
            value_unbounded: model_routable_for_strategy(score.model, strategy, profile.task)
                && score.value.is_infinite()
                && score.value.is_sign_positive(),
        });
    }

    ranked
}

fn resolved_summary(resolved: &ResolvedRoute, strategy: Option<String>) -> ResolvedSummary {
    ResolvedSummary {
        model_id: resolved.model.name.clone(),
        provider_id: resolved.provider_id.clone(),
        strategy,
    }
}

/// Explain how CAB would route a hypothetical gateway request.
pub async fn explain(pool: &InMemoryStore, request: &RouteExplainRequest) -> RouteExplainResult {
    let mut steps = Vec::new();
    let body = request.body.as_ref();
    let profile = body
        .map(|value| build_request_profile(value, &request.agent))
        .unwrap_or_else(|| build_request_profile(&serde_json::json!({}), &request.agent));

    push_step(
        &mut steps,
        "parse_request_profile",
        true,
        format!(
            "task={:?}, complexity={:.2}, estimated_input_tokens={}",
            profile.task, profile.complexity, profile.estimated_input_tokens
        ),
    );

    if let Ok(Some(agent_config)) = cab_db::agent::get_by_id(pool, &request.agent).await {
        push_step(
            &mut steps,
            "load_agent_config",
            true,
            format!(
                "mode={}, model_id={:?}",
                agent_config.mode, agent_config.model_id
            ),
        );
        if agent_config.mode == "auto"
            && let Some(ref route_id) = agent_config.model_id
            && !route_id.is_empty()
        {
            push_step(
                &mut steps,
                "agent_auto_route",
                true,
                format!("agent configured strategy/route: {route_id}"),
            );
        }
    } else {
        push_step(
            &mut steps,
            "load_agent_config",
            false,
            format!("agent {} not found", request.agent),
        );
    }

    let strategy = infer_strategy(&request.agent, request.model.as_deref(), pool).await;
    let strategy_name = match strategy {
        RoutingStrategy::Auto => "auto",
        RoutingStrategy::Balanced => "balanced",
        RoutingStrategy::Cheapest => "cheapest",
        RoutingStrategy::Intelligent => "intelligent",
        RoutingStrategy::Speed => "speed",
    };

    let ranked = ranked_candidates(pool, strategy, &profile).await;

    match resolve_route(pool, &request.agent, request.model.as_deref(), body).await {
        Ok(resolved) => {
            push_step(
                &mut steps,
                "resolve_route",
                true,
                format!(
                    "selected model={}, provider={}, strategy={}",
                    resolved.model.name, resolved.provider_id, strategy_name
                ),
            );
            RouteExplainResult {
                resolved: Some(resolved_summary(&resolved, Some(strategy_name.to_string()))),
                decision_steps: steps,
                ranked_candidates: ranked,
            }
        }
        Err(err) => {
            push_step(&mut steps, "resolve_route", false, err.to_string());
            RouteExplainResult {
                resolved: None,
                decision_steps: steps,
                ranked_candidates: ranked,
            }
        }
    }
}
