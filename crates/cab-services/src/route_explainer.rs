use cab_core::types::{
    DecisionStep, RankedModelSummary, ResolvedSummary, RouteExplainRequest, RouteExplainResult,
    StrategyBoardRequest, StrategyBoardResult, StrategyBoardStrategy,
};
use cab_core::{
    RankedRouteCandidate, RequestProfile, RouteCandidate, RoutingStrategy, TaskKind,
    build_request_profile, model_routable_for_strategy, rank_route_candidates_with_scores,
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

fn routable_route_candidates<'a>(
    entries: &'a [cab_db::routability::RoutableModelEntry],
) -> Vec<RouteCandidate<'a>> {
    entries
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
        .collect()
}

fn ranked_model_summary(
    score: &RankedRouteCandidate<'_>,
    strategy: RoutingStrategy,
    task: TaskKind,
) -> RankedModelSummary {
    let routable = model_routable_for_strategy(score.model, strategy, task);
    RankedModelSummary {
        model_id: score.model.name.clone(),
        provider_id: score.service_provider_id.to_string(),
        capability: if routable && score.capability.is_finite() {
            Some(score.capability)
        } else {
            None
        },
        value: if routable && score.value.is_finite() {
            Some(score.value)
        } else {
            None
        },
        value_unbounded: routable && score.value.is_infinite() && score.value.is_sign_positive(),
    }
}

async fn rank_all_candidates(
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

    let candidates = routable_route_candidates(&entries);
    rank_route_candidates_with_scores(&candidates, strategy, profile)
        .iter()
        .map(|score| ranked_model_summary(score, strategy, profile.task))
        .collect()
}

async fn ranked_candidates(
    pool: &InMemoryStore,
    strategy: RoutingStrategy,
    profile: &RequestProfile,
) -> Vec<RankedModelSummary> {
    let mut ranked = rank_all_candidates(pool, strategy, profile).await;
    ranked.truncate(10);
    ranked
}

fn strategy_id(strategy: RoutingStrategy) -> &'static str {
    match strategy {
        RoutingStrategy::Auto => "auto",
        RoutingStrategy::Balanced => "balanced",
        RoutingStrategy::Cheapest => "cheapest",
        RoutingStrategy::Intelligent => "intelligent",
        RoutingStrategy::Speed => "speed",
    }
}

fn display_strategy(strategy: RoutingStrategy, candidates: &[RouteCandidate<'_>]) -> RoutingStrategy {
    if matches!(strategy, RoutingStrategy::Speed)
        && !candidates
            .iter()
            .any(|candidate| candidate.model.output_speed_tps.filter(|speed| *speed > 0.0).is_some())
    {
        RoutingStrategy::Cheapest
    } else {
        strategy
    }
}

/// Rank all routable models for each built-in strategy (routes page strategy board).
pub async fn strategy_board(
    pool: &InMemoryStore,
    request: &StrategyBoardRequest,
) -> StrategyBoardResult {
    let profile = request
        .body
        .as_ref()
        .map(|value| build_request_profile(value, &request.agent))
        .unwrap_or_else(|| build_request_profile(&serde_json::json!({}), &request.agent));

    let entries = cab_db::routability::list_routable_model_entries(pool)
        .await
        .unwrap_or_default();
    let route_candidates = routable_route_candidates(&entries);

    let strategies = [
        RoutingStrategy::Auto,
        RoutingStrategy::Balanced,
        RoutingStrategy::Cheapest,
        RoutingStrategy::Intelligent,
        RoutingStrategy::Speed,
    ]
    .into_iter()
    .map(|strategy| {
        let effective = display_strategy(strategy, &route_candidates);
        let candidates = rank_route_candidates_with_scores(&route_candidates, effective, &profile)
            .iter()
            .map(|score| ranked_model_summary(score, effective, profile.task))
            .collect();
        StrategyBoardStrategy {
            id: strategy_id(strategy).to_string(),
            display_strategy: strategy_id(effective).to_string(),
            task: format!("{:?}", profile.task),
            complexity: profile.complexity,
            candidates,
        }
    })
    .collect();

    StrategyBoardResult { strategies }
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
