//! Request-aware model routing for CAB gateway strategies.
//!
//! The five user-selectable strategies each store a positive semantic primary metric
//! and a positive semantic secondary metric on `RankedRouteCandidate.value` / `.capability`.
//! The comparator direction is per-strategy so each metric is human-readable as-is
//! (no `-cost` / `-time` tricks for display). Ties break on `model.name` (and then
//! `service_provider_id` for candidates).
//!
//! | Strategy (`RoutingStrategy`) | Primary key (`value`)                        | Secondary key (`capability`)         | Primary dir | Secondary dir |
//! |------------------------------|----------------------------------------------|--------------------------------------|-------------|----------------|
//! | `Agentic` (智能体策略)       | `agentic_index`                              | cost-performance                     | DESC        | DESC           |
//! | `Balanced` (平衡策略)        | cost-performance (`capability / cost`)        | 智能指数 (`overall_intelligence`)     | DESC        | DESC           |
//! | `Cheapest` / `price` (价格策略) | `effective_cost` (USD per Mtok)              | 智能指数 (`overall_intelligence`)     | ASC         | DESC           |
//! | `Intelligent` (代码能力策略) | `coding_index`                                | cost-performance                     | DESC        | DESC           |
//! | `Speed` (速度策略)            | AA total response time for 1000 tokens (s)   | `effective_cost`                     | ASC         | ASC            |
//! | `Auto`                        | same as `Balanced`                           | same as `Balanced`                   | DESC        | DESC           |
//!
//! `Speed` primary is the AA-style "Total Response Time for N Output Tokens"
//! metric: `time_to_first_token_secs + N / output_speed_tps`. It captures both
//! initial latency and steady-state decode throughput — the two things users
//! actually feel as "speed" for an interactive coding agent. `OUTPUT_TOKENS_FOR_SPEED_RANKING`
//! (=1000) gives a more realistic total response time for typical coding-agent outputs; smaller wins.
//!
//! Missing primary-key data sinks to the bottom regardless of direction (uses `+∞` for
//! ASC strategies, `-∞` for DESC strategies). Missing secondary data is always `-∞` so
//! the model never wins a tie-break on data it lacks. Models without `coding_index` are
//! still eligible for `Cheapest` / `Speed` / `Balanced` / `Agentic` — only `Intelligent`
//! requires `coding_index`; `Agentic` analogously requires `agentic_index`. `Speed`
//! falls back to `Cheapest` when *no* model in the pool has output-speed data.
//!
//! The `value` and `capability` fields are positive semantic numbers (no encoding tricks)
//! so the route explainer can display them directly with the right unit suffix.

use crate::types::Model;

/// Typical prompt:completion token ratio for coding agents (input-heavy).
pub const BALANCED_INPUT_OUTPUT_RATIO: f64 = 10.0;

/// Assumed prompt cache hit rate when `cache_read` pricing is available.
pub const INPUT_CACHE_HIT_RATE: f64 = 0.9;

/// Reference output length for the Speed strategy's "total response time" metric.
/// Uses AA-style "Total Response Time for N Output Tokens": `TTFT + N / tps`.
/// 1000 tokens gives a more realistic total time for typical coding-agent outputs.
pub const OUTPUT_TOKENS_FOR_SPEED_RANKING: f64 = 1000.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    Coding,
    Math,
    Agentic,
    General,
}

#[derive(Debug, Clone)]
pub struct RequestProfile {
    pub task: TaskKind,
    /// 0.0 trivial … 1.0 very demanding
    pub complexity: f64,
    pub estimated_input_tokens: u64,
    pub estimated_output_tokens: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingStrategy {
    Auto,
    Balanced,
    Cheapest,
    Intelligent,
    Speed,
    Agentic,
}

impl RoutingStrategy {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "auto" => Some(Self::Auto),
            "balanced" => Some(Self::Balanced),
            "cheapest" | "price" => Some(Self::Cheapest),
            "intelligent" => Some(Self::Intelligent),
            "speed" => Some(Self::Speed),
            "agentic" => Some(Self::Agentic),
            _ => None,
        }
    }
}

/// AA-style "Total Response Time for N Output Tokens": `TTFT + N/tps` in seconds.
/// Returns `None` when no speed data is available so the caller can mark the model
/// unroutable under the Speed strategy.
fn model_speed_score(model: &Model) -> Option<f64> {
    let tps = model.output_speed_tps.filter(|s| *s > 0.0)?;
    let ttft = model.time_to_first_token_secs.unwrap_or(0.0).max(0.0);
    Some(ttft + OUTPUT_TOKENS_FOR_SPEED_RANKING / tps)
}

/// Build a routing profile from an API JSON body and client agent id.
pub fn build_request_profile(body: &serde_json::Value, agent: &str) -> RequestProfile {
    let text = extract_request_text(body);
    let message_count = count_messages(body);
    let has_tools = body.get("tools").is_some() || body.get("functions").is_some();
    let estimated_output_tokens = estimate_output_tokens(body, message_count);
    classify_request(&text, agent, message_count, has_tools, estimated_output_tokens)
}

pub fn cache_read_cost_from_model(model: &Model) -> Option<f64> {
    model
        .pricing
        .as_ref()
        .and_then(|pricing| pricing.get("cache_read"))
        .and_then(|value| value.as_f64())
        .filter(|cost| *cost >= 0.0)
}

pub fn blended_input_cost(input: f64, cache_read: Option<f64>) -> f64 {
    let input = input.max(0.0);
    match cache_read {
        Some(cache_read) => {
            INPUT_CACHE_HIT_RATE * cache_read + (1.0 - INPUT_CACHE_HIT_RATE) * input
        }
        None => input,
    }
}

pub fn raw_effective_token_cost(
    input_cost: Option<f64>,
    output_cost: Option<f64>,
    cache_read_cost: Option<f64>,
) -> f64 {
    raw_effective_token_cost_with_ratio(
        input_cost,
        output_cost,
        cache_read_cost,
        BALANCED_INPUT_OUTPUT_RATIO,
    )
}

pub fn raw_effective_token_cost_with_ratio(
    input_cost: Option<f64>,
    output_cost: Option<f64>,
    cache_read_cost: Option<f64>,
    input_output_ratio: f64,
) -> f64 {
    let input = input_cost.unwrap_or(0.0);
    let output = output_cost.unwrap_or(0.0).max(0.0);
    let blended_input = blended_input_cost(input, cache_read_cost);
    blended_input * input_output_ratio.max(0.1) + output
}

/// Weighted per-1M-token cost used for cheapest / tie-breaks (no floor).
pub fn effective_token_cost(
    input_cost: Option<f64>,
    output_cost: Option<f64>,
    cache_read_cost: Option<f64>,
) -> f64 {
    raw_effective_token_cost(input_cost, output_cost, cache_read_cost)
}

pub fn raw_effective_token_cost_for_model(model: &Model) -> f64 {
    raw_effective_token_cost(
        model.input_cost,
        model.output_cost,
        cache_read_cost_from_model(model),
    )
}

pub fn effective_token_cost_for_model(model: &Model) -> f64 {
    raw_effective_token_cost_for_model(model)
}

/// Balanced/auto value score: capability / cost, or +∞ when catalog price is known to be free.
pub fn capability_value_score(
    capability: f64,
    input_cost: Option<f64>,
    output_cost: Option<f64>,
    cache_read_cost: Option<f64>,
) -> f64 {
    capability_value_score_with_ratio(
        capability,
        input_cost,
        output_cost,
        cache_read_cost,
        BALANCED_INPUT_OUTPUT_RATIO,
    )
}

pub fn capability_value_score_with_ratio(
    capability: f64,
    input_cost: Option<f64>,
    output_cost: Option<f64>,
    cache_read_cost: Option<f64>,
    input_output_ratio: f64,
) -> f64 {
    let (Some(_), Some(_)) = (input_cost, output_cost) else {
        return f64::NEG_INFINITY;
    };
    let raw = raw_effective_token_cost_with_ratio(
        input_cost,
        output_cost,
        cache_read_cost,
        input_output_ratio,
    );
    if raw <= 0.0 {
        f64::INFINITY
    } else {
        capability / raw
    }
}

/// A routable (model, service-provider) pair with endpoint-specific pricing.
#[derive(Debug, Clone)]
pub struct RouteCandidate<'a> {
    pub model: &'a Model,
    pub service_provider_id: &'a str,
    pub input_cost: f64,
    pub output_cost: f64,
    pub cache_read_cost: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct RankedRouteCandidate<'a> {
    pub model: &'a Model,
    pub service_provider_id: &'a str,
    pub capability: f64,
    pub value: f64,
}

#[derive(Debug, Clone)]
pub struct RankedModelScore<'a> {
    pub model: &'a Model,
    pub capability: f64,
    pub value: f64,
}

struct ScoreParts {
    capability: f64,
    value: f64,
}

struct CostInputs {
    endpoint_cost: f64,
    input_cost: Option<f64>,
    output_cost: Option<f64>,
    cache_read_cost: Option<f64>,
    input_output_ratio: f64,
}

fn task_capability_available(model: &Model, task: TaskKind) -> bool {
    match task {
        TaskKind::Coding => model.coding_index.is_some() || model.overall_intelligence.is_some(),
        TaskKind::Math => model.math_index.is_some() || model.overall_intelligence.is_some(),
        TaskKind::Agentic => model.agentic_index.is_some() || model.overall_intelligence.is_some(),
        TaskKind::General => model.overall_intelligence.is_some(),
    }
}

/// Whether a model can participate in scoring for the given strategy and task.
pub fn model_routable_for_strategy(
    model: &Model,
    strategy: RoutingStrategy,
    task: TaskKind,
) -> bool {
    match strategy {
        RoutingStrategy::Cheapest => true,
        RoutingStrategy::Intelligent => model.coding_index.is_some(),
        RoutingStrategy::Agentic => model.agentic_index.is_some(),
        RoutingStrategy::Speed => model_speed_score(model).is_some(),
        RoutingStrategy::Balanced | RoutingStrategy::Auto => task_capability_available(model, task),
    }
}

/// Build the positive semantic sort keys for a single model under one strategy.
///
/// `value` carries the primary metric in its native units (positive); `capability`
/// carries the secondary metric. Comparator direction varies per strategy; missing
/// primary data sinks to the bottom in both directions (`+∞` for ASC strategies,
/// `-∞` for DESC strategies). Missing secondary data is always `-∞` so it never
/// wins a tie-break.
fn input_output_ratio_for_profile(profile: &RequestProfile) -> f64 {
    let input = profile.estimated_input_tokens.max(1) as f64;
    let output = profile.estimated_output_tokens.max(1) as f64;
    (input / output).clamp(0.5, 50.0)
}

fn score_parts(
    model: &Model,
    strategy: RoutingStrategy,
    task: TaskKind,
    costs: &CostInputs,
) -> ScoreParts {
    if !model_routable_for_strategy(model, strategy, task) {
        return ScoreParts {
            capability: f64::NEG_INFINITY,
            value: f64::NEG_INFINITY,
        };
    }

    let primary_missing = match strategy {
        RoutingStrategy::Cheapest | RoutingStrategy::Speed => f64::INFINITY,
        _ => f64::NEG_INFINITY,
    };
    let secondary_missing = f64::NEG_INFINITY;

    let primary_score = match strategy {
        RoutingStrategy::Balanced | RoutingStrategy::Auto => capability_value_score_with_ratio(
            primary_capability_loose(model, task),
            costs.input_cost,
            costs.output_cost,
            costs.cache_read_cost,
            costs.input_output_ratio,
        ),
        RoutingStrategy::Cheapest => costs.endpoint_cost,
        RoutingStrategy::Intelligent => model.coding_index.unwrap_or(primary_missing),
        RoutingStrategy::Agentic => model.agentic_index.unwrap_or(primary_missing),
        RoutingStrategy::Speed => model_speed_score(model).unwrap_or(primary_missing),
    };
    let secondary_score = match strategy {
        RoutingStrategy::Balanced | RoutingStrategy::Auto | RoutingStrategy::Cheapest => {
            model.overall_intelligence.unwrap_or(secondary_missing)
        }
        RoutingStrategy::Intelligent | RoutingStrategy::Agentic => capability_value_score_with_ratio(
            primary_capability_loose(model, task),
            costs.input_cost,
            costs.output_cost,
            costs.cache_read_cost,
            costs.input_output_ratio,
        ),
        RoutingStrategy::Speed => costs.endpoint_cost,
    };

    ScoreParts {
        value: primary_score,
        capability: secondary_score,
    }
}

fn primary_asc(strategy: RoutingStrategy) -> bool {
    matches!(strategy, RoutingStrategy::Cheapest | RoutingStrategy::Speed)
}

fn secondary_asc(strategy: RoutingStrategy) -> bool {
    matches!(strategy, RoutingStrategy::Speed)
}

/// Per-strategy comparator: primary key (direction depends on strategy) → secondary
/// key (direction depends on strategy). Both keys are positive semantic numbers; the
/// direction here decides which extreme (high or low) wins.
fn compare_ranked(
    strategy: RoutingStrategy,
    a_value: f64,
    b_value: f64,
    a_capability: f64,
    b_capability: f64,
) -> std::cmp::Ordering {
    let primary = if primary_asc(strategy) {
        a_value.partial_cmp(&b_value)
    } else {
        b_value.partial_cmp(&a_value)
    };
    let secondary = if secondary_asc(strategy) {
        a_capability.partial_cmp(&b_capability)
    } else {
        b_capability.partial_cmp(&a_capability)
    };
    primary
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| secondary.unwrap_or(std::cmp::Ordering::Equal))
}

fn score_parts_for_model(
    model: &Model,
    strategy: RoutingStrategy,
    task: TaskKind,
    endpoint_cost: f64,
    input_output_ratio: f64,
) -> ScoreParts {
    let costs = CostInputs {
        endpoint_cost,
        input_cost: model.input_cost,
        output_cost: model.output_cost,
        cache_read_cost: cache_read_cost_from_model(model),
        input_output_ratio,
    };
    score_parts(model, strategy, task, &costs)
}

fn score_parts_for_candidate(
    candidate: &RouteCandidate<'_>,
    strategy: RoutingStrategy,
    task: TaskKind,
    endpoint_cost: f64,
    input_output_ratio: f64,
) -> ScoreParts {
    let costs = CostInputs {
        endpoint_cost,
        input_cost: Some(candidate.input_cost),
        output_cost: Some(candidate.output_cost),
        cache_read_cost: candidate.cache_read_cost,
        input_output_ratio,
    };
    score_parts(candidate.model, strategy, task, &costs)
}

fn score_models<'a>(
    models: &'a [Model],
    strategy: RoutingStrategy,
    profile: &RequestProfile,
) -> Vec<(&'a Model, f64, f64)> {
    if matches!(strategy, RoutingStrategy::Speed)
        && !models
            .iter()
            .any(|model| model_speed_score(model).is_some())
    {
        tracing::warn!(
            "Speed strategy has no models with AA output speed data; falling back to cheapest"
        );
        return score_models(models, RoutingStrategy::Cheapest, profile);
    }

    let context_fitting: Vec<&Model> = models
        .iter()
        .filter(|m| m.context_length as u64 >= profile.estimated_input_tokens)
        .collect();

    let pool: Vec<&Model> = if context_fitting.is_empty() {
        tracing::warn!(
            input_tokens = profile.estimated_input_tokens,
            "no model fits estimated input size, falling back to all models"
        );
        models.iter().collect()
    } else {
        context_fitting
    };

    let io_ratio = input_output_ratio_for_profile(profile);

    let mut scored: Vec<(&Model, f64, f64)> = pool
        .clone()
        .into_iter()
        .map(|model| {
            let routing_cost = effective_token_cost_for_model(model);
            let parts = score_parts_for_model(model, strategy, profile.task, routing_cost, io_ratio);
            (model, parts.capability, parts.value)
        })
        .collect();

    if matches!(strategy, RoutingStrategy::Auto) {
        let min_required = min_required_capability(profile);
        let before = scored.len();
        scored.retain(|(model, _, _)| {
            task_capability_available(model, profile.task)
                && primary_capability_loose(model, profile.task) >= min_required
        });
        if let Some(max_cost) = auto_max_allowed_cost(profile)
            && !scored.is_empty()
        {
            let before_cost = scored.len();
            scored.retain(|(model, _, _)| {
                effective_token_cost_for_model(model) <= max_cost
            });
            if scored.is_empty() {
                tracing::debug!(
                    max_cost,
                    before_cost,
                    "auto cost ceiling emptied pool, reverting to capability-filtered set"
                );
                scored = pool
                    .clone()
                    .into_iter()
                    .filter(|model| {
                        task_capability_available(model, profile.task)
                            && primary_capability_loose(model, profile.task) >= min_required
                    })
                    .map(|model| {
                        let routing_cost = effective_token_cost_for_model(model);
                        let parts =
                            score_parts_for_model(model, strategy, profile.task, routing_cost, io_ratio);
                        (model, parts.capability, parts.value)
                    })
                    .collect();
            } else {
                tracing::debug!(
                    max_cost,
                    before_cost,
                    after = scored.len(),
                    "auto cost ceiling applied"
                );
            }
        }
        if scored.is_empty() {
            tracing::debug!(
                task = ?profile.task,
                complexity = profile.complexity,
                min_required,
                before,
                "auto capability filter emptied pool, falling back to all candidates"
            );
            scored = pool
                .clone()
                .into_iter()
                .map(|model| {
                    let routing_cost = effective_token_cost_for_model(model);
                    let parts = score_parts_for_model(model, strategy, profile.task, routing_cost, io_ratio);
                    (model, parts.capability, parts.value)
                })
                .collect();
        } else {
            tracing::debug!(
                task = ?profile.task,
                complexity = profile.complexity,
                min_required,
                before,
                after = scored.len(),
                "auto capability filter applied"
            );
        }
    }

    scored.sort_by(|(a_model, a_cap, a_val), (b_model, b_cap, b_val)| {
        compare_ranked(strategy, *a_val, *b_val, *a_cap, *b_cap)
            .then_with(|| a_model.name.cmp(&b_model.name))
    });

    scored
}

fn score_route_candidates<'a>(
    candidates: &'a [RouteCandidate<'a>],
    strategy: RoutingStrategy,
    profile: &RequestProfile,
) -> Vec<(&'a Model, &'a str, f64, f64)> {
    if matches!(strategy, RoutingStrategy::Speed)
        && !candidates
            .iter()
            .any(|c| model_speed_score(c.model).is_some())
    {
        tracing::warn!(
            "Speed strategy has no models with AA output speed data; falling back to cheapest"
        );
        return score_route_candidates(candidates, RoutingStrategy::Cheapest, profile);
    }

    let context_fitting: Vec<&RouteCandidate<'_>> = candidates
        .iter()
        .filter(|c| c.model.context_length as u64 >= profile.estimated_input_tokens)
        .collect();

    let pool: Vec<&RouteCandidate<'_>> = if context_fitting.is_empty() {
        tracing::warn!(
            input_tokens = profile.estimated_input_tokens,
            "no candidate fits estimated input size, falling back to all candidates"
        );
        candidates.iter().collect()
    } else {
        context_fitting
    };

    let io_ratio = input_output_ratio_for_profile(profile);

    let mut scored: Vec<(&Model, &str, f64, f64, f64)> = pool
        .clone()
        .into_iter()
        .map(|candidate| {
            let endpoint_cost = effective_token_cost(
                Some(candidate.input_cost),
                Some(candidate.output_cost),
                candidate.cache_read_cost,
            );
            let parts = score_parts_for_candidate(candidate, strategy, profile.task, endpoint_cost, io_ratio);
            (
                candidate.model,
                candidate.service_provider_id,
                parts.capability,
                parts.value,
                endpoint_cost,
            )
        })
        .collect();

    if matches!(strategy, RoutingStrategy::Auto) {
        let min_required = min_required_capability(profile);
        let before = scored.len();
        scored.retain(|(model, _, _, _, _)| {
            task_capability_available(model, profile.task)
                && primary_capability_loose(model, profile.task) >= min_required
        });
        if let Some(max_cost) = auto_max_allowed_cost(profile)
            && !scored.is_empty()
        {
            let before_cost = scored.len();
            scored.retain(|(_, _, _, _, cost)| *cost <= max_cost);
            if scored.is_empty() {
                tracing::debug!(
                    max_cost,
                    before_cost,
                    "auto cost ceiling emptied pool, reverting to capability-filtered set"
                );
                scored = pool
                    .clone()
                    .into_iter()
                    .filter(|candidate| {
                        task_capability_available(candidate.model, profile.task)
                            && primary_capability_loose(candidate.model, profile.task)
                                >= min_required
                    })
                    .map(|candidate| {
                        let routing_cost = effective_token_cost(
                            Some(candidate.input_cost),
                            Some(candidate.output_cost),
                            candidate.cache_read_cost,
                        );
                        let parts = score_parts_for_candidate(
                            candidate,
                            strategy,
                            profile.task,
                            routing_cost,
                            io_ratio,
                        );
                        (
                            candidate.model,
                            candidate.service_provider_id,
                            parts.capability,
                            parts.value,
                            routing_cost,
                        )
                    })
                    .collect();
            } else {
                tracing::debug!(
                    max_cost,
                    before_cost,
                    after = scored.len(),
                    "auto cost ceiling applied"
                );
            }
        }
        if scored.is_empty() {
            tracing::debug!(
                task = ?profile.task,
                complexity = profile.complexity,
                min_required,
                before,
                "auto capability filter emptied pool, falling back to all candidates"
            );
            scored = pool
                .clone()
                .into_iter()
                .map(|candidate| {
                    let routing_cost = effective_token_cost(
                        Some(candidate.input_cost),
                        Some(candidate.output_cost),
                        candidate.cache_read_cost,
                    );
                    let parts =
                        score_parts_for_candidate(candidate, strategy, profile.task, routing_cost, io_ratio);
                    (
                        candidate.model,
                        candidate.service_provider_id,
                        parts.capability,
                        parts.value,
                        routing_cost,
                    )
                })
                .collect();
        } else {
            tracing::debug!(
                task = ?profile.task,
                complexity = profile.complexity,
                min_required,
                before,
                after = scored.len(),
                "auto capability filter applied"
            );
        }
    }

    scored.sort_by(
        |(a_model, a_provider, a_cap, a_val, _a_cost),
         (b_model, b_provider, b_cap, b_val, _b_cost)| {
            compare_ranked(strategy, *a_val, *b_val, *a_cap, *b_cap)
                .then_with(|| a_model.name.cmp(&b_model.name))
                .then_with(|| a_provider.cmp(b_provider))
        },
    );

    scored
        .into_iter()
        .map(|(model, provider, capability, value, _)| (model, provider, capability, value))
        .collect()
}

pub fn rank_route_candidates_with_scores<'a>(
    candidates: &'a [RouteCandidate<'a>],
    strategy: RoutingStrategy,
    profile: &RequestProfile,
) -> Vec<RankedRouteCandidate<'a>> {
    score_route_candidates(candidates, strategy, profile)
        .into_iter()
        .map(
            |(model, service_provider_id, capability, value)| RankedRouteCandidate {
                model,
                service_provider_id,
                capability,
                value,
            },
        )
        .collect()
}

pub fn rank_route_candidates<'a>(
    candidates: &'a [RouteCandidate<'a>],
    strategy: RoutingStrategy,
    profile: &RequestProfile,
) -> Vec<(&'a Model, &'a str)> {
    score_route_candidates(candidates, strategy, profile)
        .into_iter()
        .map(|(model, provider, _, _)| (model, provider))
        .collect()
}

pub fn rank_models_with_scores<'a>(
    models: &'a [Model],
    strategy: RoutingStrategy,
    profile: &RequestProfile,
) -> Vec<RankedModelScore<'a>> {
    score_models(models, strategy, profile)
        .into_iter()
        .map(|(model, capability, value)| RankedModelScore {
            model,
            capability,
            value,
        })
        .collect()
}

pub fn rank_models<'a>(
    models: &'a [Model],
    strategy: RoutingStrategy,
    profile: &RequestProfile,
) -> Vec<&'a Model> {
    score_models(models, strategy, profile)
        .into_iter()
        .map(|(model, _, _)| model)
        .collect()
}

fn min_required_capability(profile: &RequestProfile) -> f64 {
    let floor = match profile.task {
        TaskKind::Coding => 32.0,
        TaskKind::Math => 38.0,
        TaskKind::Agentic => 42.0,
        TaskKind::General => 24.0,
    };
    let ceiling = match profile.task {
        TaskKind::Coding => 88.0,
        TaskKind::Math => 92.0,
        TaskKind::Agentic => 95.0,
        TaskKind::General => 78.0,
    };
    floor + profile.complexity * (ceiling - floor)
}

/// Auto-only cost ceiling: trivial requests (low complexity) should not land on
/// flagship-priced models. Returns `None` when complexity is high enough that no
/// cost cap should apply. The ceiling is in USD per 1M effective tokens.
fn auto_max_allowed_cost(profile: &RequestProfile) -> Option<f64> {
    if profile.complexity >= 0.6 {
        return None;
    }
    let base = match profile.task {
        TaskKind::General => 2.0,
        TaskKind::Coding => 5.0,
        TaskKind::Math => 4.0,
        TaskKind::Agentic => 8.0,
    };
    Some(base * (0.5 + profile.complexity))
}

fn primary_capability_loose(model: &Model, task: TaskKind) -> f64 {
    match task {
        TaskKind::Coding => model.coding_index.or(model.overall_intelligence),
        TaskKind::Math => model.math_index.or(model.overall_intelligence),
        TaskKind::Agentic => model.agentic_index.or(model.overall_intelligence),
        TaskKind::General => model.overall_intelligence,
    }
    .expect("task_capability_available should be checked before scoring")
}

fn classify_request(
    text: &str,
    agent: &str,
    message_count: usize,
    has_tools: bool,
    estimated_output_tokens: u64,
) -> RequestProfile {
    let lower = text.to_ascii_lowercase();
    let agent_lower = agent.to_ascii_lowercase();
    let estimated_input_tokens = estimate_tokens(text);

    let coding_hits = count_keyword_hits(
        &lower,
        &[
            "```",
            "function",
            "class ",
            "import ",
            "def ",
            "struct ",
            "impl ",
            "console.log",
            "typescript",
            "javascript",
            "python",
            "rust",
            "golang",
            "refactor",
            "debug",
            "stack trace",
            "unit test",
            "lint",
            "compiler",
            "git ",
            "npm ",
            "cargo ",
            "algorithm",
            "optimize",
            "performance",
            "bug",
            "error",
            "api",
            "database",
            "query",
            "sql",
            "regex",
            "async",
            "await",
            "promise",
            "callback",
            "pointer",
            "memory",
            "segmentation fault",
            "nullpointer",
            "exception",
            "serialization",
            "deserialization",
            "endpoint",
            "handler",
            "middleware",
            "schema",
            "migration",
            "deployment",
            "docker",
            "kubernetes",
            "ci/cd",
            "代码",
            "函数",
            "重构",
            "报错",
            "编译",
            "算法",
            "优化",
            "性能",
            "接口",
            "数据库",
            "部署",
        ],
    );
    let math_hits = count_keyword_hits(
        &lower,
        &[
            "equation",
            "integral",
            "derivative",
            "matrix",
            "probability",
            "theorem",
            "proof",
            "calculate",
            "solve for",
            "log(",
            "sin(",
            "cos(",
            "sqrt(",
            "∫",
            "sum of",
            "series",
            "limit",
            "vector",
            "eigen",
            "polynomial",
            "combinator",
            "permutation",
            "binomial",
            "方差",
            "均值",
            "统计",
            "几何",
            "代数",
            "方程",
            "证明",
            "微积分",
            "概率",
            "线性代数",
        ],
    );
    let agentic_hits = count_keyword_hits(
        &lower,
        &[
            "step by step",
            "plan",
            "workflow",
            "orchestrat",
            "multi-step",
            "tool call",
            "agent",
            "reasoning",
            "analyze deeply",
            "investigate",
            "break down",
            "chain of thought",
            "multi-agent",
            "autonomous",
            "coordinate",
            "delegate",
            "subtask",
            "pipeline",
            "end to end",
            "逐步",
            "规划",
            "编排",
            "多步",
            "自主",
            "协调",
        ],
    );

    let agent_is_coding = matches_agent_kind(
        &agent_lower,
        &[
            "claude", "codex", "copilot", "aider", "cline", "continue", "hermes", "kilo",
            "openclaw", "claw", "pi", "code",
        ],
    );

    let mut task = TaskKind::General;
    let mut task_score = 0.0f64;

    let coding_score = coding_hits as f64 * 1.4 + if agent_is_coding { 2.5 } else { 0.0 };
    let math_score = math_hits as f64 * 1.8;
    let agentic_score = agentic_hits as f64 * 1.3 + if has_tools { 2.0 } else { 0.0 };

    if coding_score >= math_score && coding_score >= agentic_score && coding_score >= 1.0 {
        task = TaskKind::Coding;
        task_score = coding_score;
    } else if math_score >= agentic_score && math_score >= 1.0 {
        task = TaskKind::Math;
        task_score = math_score;
    } else if agentic_score >= 1.5 || (has_tools && message_count > 4) {
        task = TaskKind::Agentic;
        task_score = agentic_score;
    } else if agent_is_coding {
        task = TaskKind::Coding;
        task_score = coding_score.max(1.0);
    }

    let length_factor = (estimated_input_tokens as f64 / 6000.0).clamp(0.0, 1.0);
    let message_factor = ((message_count.saturating_sub(1) as f64) / 12.0).clamp(0.0, 1.0);
    let code_block_factor = (text.matches("```").count() as f64 / 4.0).clamp(0.0, 1.0);
    let task_factor = (task_score / 6.0).clamp(0.0, 1.0);

    let complexity = (length_factor * 0.35
        + message_factor * 0.20
        + code_block_factor * 0.25
        + task_factor * 0.20)
        .clamp(0.0, 1.0);

    RequestProfile {
        task,
        complexity,
        estimated_input_tokens,
        estimated_output_tokens,
    }
}

fn matches_agent_kind(agent: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| agent.contains(needle))
}

fn count_keyword_hits(haystack: &str, keywords: &[&str]) -> usize {
    keywords.iter().filter(|kw| haystack.contains(*kw)).count()
}

fn estimate_tokens(text: &str) -> u64 {
    let ascii = text.chars().filter(|c| c.is_ascii()).count() as f64;
    let non_ascii = text.chars().filter(|c| !c.is_ascii()).count() as f64;
    ((ascii * 0.25 + non_ascii * 0.5).max(1.0)) as u64
}

fn estimate_output_tokens(body: &serde_json::Value, message_count: usize) -> u64 {
    if let Some(max_tokens) = body
        .get("max_tokens")
        .and_then(|v| v.as_u64())
        .or_else(|| body.get("max_completion_tokens").and_then(|v| v.as_u64()))
    {
        return max_tokens;
    }
    let task_output_hint = body
        .get("messages")
        .and_then(|v| v.as_array())
        .and_then(|msgs| msgs.last())
        .and_then(|last| last.get("content"))
        .map(|content| estimate_tokens(content_str(content).as_str()))
        .unwrap_or(500);
    let min_floor = if message_count > 4 { 1024 } else { 512 };
    task_output_hint.clamp(min_floor, 8192)
}

fn content_str(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(items) => {
            let mut parts = Vec::new();
            for item in items {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    parts.push(text.to_string());
                } else if let Some(s) = item.as_str() {
                    parts.push(s.to_string());
                }
            }
            parts.join(" ")
        }
        _ => String::new(),
    }
}

fn count_messages(body: &serde_json::Value) -> usize {
    body.get("messages")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or_else(|| {
            body.get("contents")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(1)
        })
}

fn extract_request_text(body: &serde_json::Value) -> String {
    let mut parts = Vec::new();

    if let Some(system) = body.get("system") {
        push_text_part(&mut parts, system);
    }
    if let Some(instructions) = body.get("instructions") {
        push_text_part(&mut parts, instructions);
    }
    if let Some(input) = body.get("input") {
        push_text_part(&mut parts, input);
    }

    if let Some(messages) = body.get("messages").and_then(|v| v.as_array()) {
        for message in messages {
            if let Some(content) = message.get("content") {
                push_text_part(&mut parts, content);
            }
        }
    }

    if let Some(contents) = body.get("contents").and_then(|v| v.as_array()) {
        for content in contents {
            if let Some(parts_arr) = content.get("parts").and_then(|v| v.as_array()) {
                for part in parts_arr {
                    if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                        parts.push(text.to_string());
                    }
                }
            }
        }
    }

    if let Some(system_instruction) = body.get("systemInstruction") {
        push_text_part(&mut parts, system_instruction);
    }

    parts.join("\n")
}

fn push_text_part(out: &mut Vec<String>, value: &serde_json::Value) {
    match value {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Array(items) => {
            for item in items {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    out.push(text.to_string());
                } else if let Some(s) = item.as_str() {
                    out.push(s.to_string());
                }
            }
        }
        serde_json::Value::Object(obj) => {
            if let Some(parts) = obj.get("parts") {
                push_text_part(out, parts);
            } else if let Some(text) = obj.get("text") {
                push_text_part(out, text);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_model(name: &str, input: f64, output: f64, scores: (f64, f64, f64, f64)) -> Model {
        Model {
            id: name.into(),
            name: name.into(),
            display_name: name.into(),
            provider_id: "p1".into(),
            protocol: "openai-chat".into(),
            context_length: 128_000,
            input_cost: Some(input),
            output_cost: Some(output),
            enabled: true,
            overall_intelligence: Some(scores.0),
            coding_index: Some(scores.1),
            agentic_index: Some(scores.2),
            math_index: Some(scores.3),
            output_speed_tps: None,
            time_to_first_token_secs: None,
            created_at: String::new(),
            updated_at: String::new(),
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

    #[test]
    fn effective_token_cost_weights_input_ten_to_one() {
        assert!((effective_token_cost(Some(1.0), Some(1.0), None) - 11.0).abs() < f64::EPSILON);
    }

    #[test]
    fn effective_token_cost_applies_cache_hit_rate() {
        // input 1.0, cache_read 0.1 → blended 0.19, + output 1.0 → 0.19*10+1 = 2.9
        assert!((effective_token_cost(Some(1.0), Some(1.0), Some(0.1)) - 2.9).abs() < f64::EPSILON);
    }

    #[test]
    fn effective_token_cost_is_zero_for_known_free_price() {
        assert_eq!(effective_token_cost(Some(0.0), Some(0.0), None), 0.0);
    }

    #[test]
    fn capability_value_score_is_infinity_for_known_free_price() {
        let score = capability_value_score(42.0, Some(0.0), Some(0.0), None);
        assert!(score.is_infinite() && score.is_sign_positive());
    }

    #[test]
    fn capability_value_score_excludes_missing_prices() {
        let score = capability_value_score(42.0, None, Some(0.0), None);
        assert!(score.is_infinite() && score.is_sign_negative());
    }

    #[test]
    fn free_models_tie_break_on_capability_after_infinite_value() {
        let weak = sample_model("free-weak", 0.0, 0.0, (40.0, 35.0, 30.0, 30.0));
        let strong = sample_model("free-strong", 0.0, 0.0, (70.0, 65.0, 55.0, 50.0));
        let profile = RequestProfile {
            task: TaskKind::Coding,
            complexity: 0.2,
            estimated_input_tokens: 500,
            estimated_output_tokens: 1024,
        };
        let models = [weak, strong];
        let ranked = rank_models(&models, RoutingStrategy::Balanced, &profile);
        assert_eq!(ranked[0].name, "free-strong");
        assert_eq!(ranked[1].name, "free-weak");
    }

    #[test]
    fn free_endpoint_outranks_paid_catalog_on_balanced_value() {
        let model = sample_model("paid-catalog", 0.3, 1.2, (43.0, 41.0, 35.0, 30.0));
        let profile = RequestProfile {
            task: TaskKind::Coding,
            complexity: 0.2,
            estimated_input_tokens: 500,
            estimated_output_tokens: 1024,
        };
        let paid = RouteCandidate {
            model: &model,
            service_provider_id: "payg",
            input_cost: 0.3,
            output_cost: 1.2,
            cache_read_cost: None,
        };
        let free = RouteCandidate {
            model: &model,
            service_provider_id: "subscription",
            input_cost: 0.0,
            output_cost: 0.0,
            cache_read_cost: None,
        };
        let candidates = [paid, free];
        let ranked = rank_route_candidates(&candidates, RoutingStrategy::Balanced, &profile);
        assert_eq!(ranked[0].1, "subscription");
        let scores =
            rank_route_candidates_with_scores(&candidates, RoutingStrategy::Balanced, &profile);
        assert!(scores[0].value.is_infinite());
        assert!(scores[1].value.is_finite());
    }

    #[test]
    fn auto_prefers_capable_model_for_complex_code() {
        let cheap = sample_model("cheap", 0.1, 0.1, (40.0, 35.0, 30.0, 30.0));
        let strong = sample_model("strong", 3.0, 15.0, (82.0, 88.0, 75.0, 70.0));
        let profile = RequestProfile {
            task: TaskKind::Coding,
            complexity: 0.85,
            estimated_input_tokens: 8000,
            estimated_output_tokens: 1024,
        };
        let models = [cheap, strong];
        let ranked = rank_models(&models, RoutingStrategy::Auto, &profile);
        assert_eq!(ranked[0].name, "strong");
    }

    #[test]
    fn auto_balanced_ranks_by_value_with_intelligence_secondary() {
        // Primary key: cost-performance (capability / effective_cost).
        // best-value: coding=70, cost=2.0 → value=35
        // cheap-dumb: coding=35, cost=2.0 → value=17.5
        // expensive-genius: coding=92, cost=70 → value≈1.31
        let best_value = sample_model("best-value", 0.1, 0.1, (50.0, 70.0, 40.0, 40.0));
        let expensive_genius = sample_model("expensive-genius", 5.0, 20.0, (50.0, 92.0, 40.0, 40.0));
        let cheap_dumb = sample_model("cheap-dumb", 0.1, 0.1, (50.0, 35.0, 40.0, 40.0));
        let profile = RequestProfile {
            task: TaskKind::Coding,
            complexity: 0.5,
            estimated_input_tokens: 1000,
            estimated_output_tokens: 1024,
        };
        let models = [cheap_dumb, expensive_genius, best_value];
        let ranked = rank_models(&models, RoutingStrategy::Balanced, &profile);
        assert_eq!(ranked[0].name, "best-value");
        assert_eq!(ranked[1].name, "cheap-dumb");
        assert_eq!(ranked[2].name, "expensive-genius");
    }

    #[test]
    fn balanced_breaks_value_ties_on_intelligence_secondary() {
        // Identical cost + capability → primary key (value) ties; secondary (overall
        // intelligence) must break the tie in DESC order.
        let smart = sample_model("smart", 1.0, 1.0, (80.0, 50.0, 40.0, 40.0));
        let dim = sample_model("dim", 1.0, 1.0, (40.0, 50.0, 40.0, 40.0));
        let profile = RequestProfile {
            task: TaskKind::Coding,
            complexity: 0.5,
            estimated_input_tokens: 1000,
            estimated_output_tokens: 1024,
        };
        let models = [dim, smart];
        let ranked = rank_models(&models, RoutingStrategy::Balanced, &profile);
        assert_eq!(ranked[0].name, "smart");
        assert_eq!(ranked[1].name, "dim");
    }

    #[test]
    fn cheapest_ranks_by_cost_then_intelligence() {
        // Primary key: effective_cost ASC. cheap-payg wins on cost.
        // Tie: pricey-cheap vs pricey-smart → secondary is intelligence, smart wins.
        let cheap_payg = sample_model("cheap-payg", 0.1, 0.1, (50.0, 50.0, 40.0, 40.0));
        let pricey_cheap = sample_model("pricey-cheap", 5.0, 5.0, (50.0, 30.0, 40.0, 40.0));
        let pricey_smart = sample_model("pricey-smart", 5.0, 5.0, (80.0, 50.0, 40.0, 40.0));
        let profile = RequestProfile {
            task: TaskKind::General,
            complexity: 0.3,
            estimated_input_tokens: 500,
            estimated_output_tokens: 1024,
        };
        let models = [pricey_smart, pricey_cheap, cheap_payg];
        let ranked = rank_models(&models, RoutingStrategy::Cheapest, &profile);
        assert_eq!(ranked[0].name, "cheap-payg");
        assert_eq!(ranked[1].name, "pricey-smart");
        assert_eq!(ranked[2].name, "pricey-cheap");
    }

    #[test]
    fn speed_scores_are_positive_semantic_values() {
        // Regression: previously `value` carried `-total_response_time` so the dashboard
        // surfaced "−1.94s" as the speed. After the positive-semantic refactor, `value`
        // must be the raw total_response_time (positive seconds) and `capability` the
        // raw effective_cost (positive). Smaller `value` still wins (ASC).
        let mut fast = sample_model("fast", 1.0, 1.0, (50.0, 50.0, 40.0, 40.0));
        fast.output_speed_tps = Some(200.0);
        fast.time_to_first_token_secs = Some(0.5);
        let profile = RequestProfile {
            task: TaskKind::Coding,
            complexity: 0.5,
            estimated_input_tokens: 1000,
            estimated_output_tokens: 1024,
        };
        let models = [fast];
        let scored = rank_models_with_scores(&models, RoutingStrategy::Speed, &profile);
        let s = &scored[0];
        assert!(s.value > 0.0, "speed.value must be positive seconds, got {}", s.value);
        assert!(s.capability > 0.0, "speed.capability must be positive cost, got {}", s.capability);
        // fast: 0.5 + 1000/200 = 5.5s
        assert!((s.value - 5.5).abs() < 1e-6, "expected 5.5s, got {}", s.value);
    }

    #[test]
    fn cheapest_scores_are_positive_semantic_values() {
        // Regression: previously `value` carried `-effective_cost`. After the
        // positive-semantic refactor, `value` is the raw effective_cost (positive USD/Mtok)
        // and smaller still wins (ASC).
        let cheap = sample_model("cheap", 0.1, 0.1, (50.0, 50.0, 40.0, 40.0));
        let pricey = sample_model("pricey", 5.0, 5.0, (50.0, 50.0, 40.0, 40.0));
        let profile = RequestProfile {
            task: TaskKind::General,
            complexity: 0.3,
            estimated_input_tokens: 500,
            estimated_output_tokens: 1024,
        };
        let models = [cheap, pricey];
        let scored = rank_models_with_scores(&models, RoutingStrategy::Cheapest, &profile);
        for s in &scored {
            assert!(s.value > 0.0, "cheapest.value must be positive cost, got {}", s.value);
            assert!(
                s.capability > 0.0 || s.capability == f64::NEG_INFINITY,
                "cheapest.capability must be positive intelligence or -∞, got {}",
                s.capability
            );
        }
    }

    #[test]
    fn intelligent_ranks_by_coding_index_then_value() {
        // Primary key: coding_index DESC. best-coder wins.
        // Tie on coding: high-coding-cheap vs high-coding-pricey → secondary is
        // cost-performance (capability/cost), cheap wins.
        let best_coder = sample_model("best-coder", 1.0, 1.0, (50.0, 95.0, 40.0, 40.0));
        let high_coding_cheap = sample_model("high-coding-cheap", 0.1, 0.1, (50.0, 90.0, 40.0, 40.0));
        let high_coding_pricey = sample_model("high-coding-pricey", 5.0, 20.0, (50.0, 90.0, 40.0, 40.0));
        let profile = RequestProfile {
            task: TaskKind::Coding,
            complexity: 0.5,
            estimated_input_tokens: 1000,
            estimated_output_tokens: 1024,
        };
        let models = [high_coding_pricey, high_coding_cheap, best_coder];
        let ranked = rank_models(&models, RoutingStrategy::Intelligent, &profile);
        assert_eq!(ranked[0].name, "best-coder");
        assert_eq!(ranked[1].name, "high-coding-cheap");
        assert_eq!(ranked[2].name, "high-coding-pricey");
    }

    #[test]
    fn agentic_ranks_by_agentic_index_then_value() {
        // Primary key: agentic_index DESC. best-agent wins.
        // Tie on agentic: high-agentic-cheap vs high-agentic-pricey → secondary is
        // cost-performance (capability/cost), cheap wins.
        let best_agent = sample_model("best-agent", 1.0, 1.0, (50.0, 40.0, 95.0, 40.0));
        let high_agentic_cheap = sample_model("high-agentic-cheap", 0.1, 0.1, (50.0, 40.0, 90.0, 40.0));
        let high_agentic_pricey = sample_model("high-agentic-pricey", 5.0, 20.0, (50.0, 40.0, 90.0, 40.0));
        let profile = RequestProfile {
            task: TaskKind::Coding,
            complexity: 0.5,
            estimated_input_tokens: 1000,
            estimated_output_tokens: 1024,
        };
        let models = [high_agentic_pricey, high_agentic_cheap, best_agent];
        let ranked = rank_models(&models, RoutingStrategy::Agentic, &profile);
        assert_eq!(ranked[0].name, "best-agent");
        assert_eq!(ranked[1].name, "high-agentic-cheap");
        assert_eq!(ranked[2].name, "high-agentic-pricey");
    }

    #[test]
    fn agentic_skips_models_without_agentic_index() {
        // Without agentic_index, a model is unroutable under the Agentic strategy and
        // sinks to the bottom; the remaining models still rank by agentic_index DESC.
        let mut no_agentic = sample_model("no-agentic", 1.0, 1.0, (50.0, 40.0, 0.0, 40.0));
        no_agentic.agentic_index = None;
        let good = sample_model("good", 1.0, 1.0, (50.0, 40.0, 80.0, 40.0));
        let profile = RequestProfile {
            task: TaskKind::Coding,
            complexity: 0.5,
            estimated_input_tokens: 200,
            estimated_output_tokens: 1024,
        };
        let models = [no_agentic, good];
        let ranked = rank_models(&models, RoutingStrategy::Agentic, &profile);
        assert_eq!(ranked[0].name, "good");
        assert_eq!(ranked[1].name, "no-agentic");
    }

    #[test]
    fn speed_ranks_by_total_response_time_then_cost() {
        // Primary key: total response time = TTFT + 1000/tps, encoded as -time DESC.
        // fast: ttft=0.5 + 100/200 = 1.0s
        // slow-cheap: ttft=1.0 + 100/100 = 2.0s
        // slow-pricey: ttft=1.0 + 100/100 = 2.0s
        // fast wins primary. slow-cheap and slow-pricey tie → secondary is -effective_cost,
        // slow-cheap wins.
        let mut fast = sample_model("fast", 1.0, 1.0, (50.0, 50.0, 40.0, 40.0));
        fast.output_speed_tps = Some(200.0);
        fast.time_to_first_token_secs = Some(0.5);
        let mut slow_cheap = sample_model("slow-cheap", 0.1, 0.1, (50.0, 50.0, 40.0, 40.0));
        slow_cheap.output_speed_tps = Some(100.0);
        slow_cheap.time_to_first_token_secs = Some(1.0);
        let mut slow_pricey = sample_model("slow-pricey", 5.0, 5.0, (50.0, 50.0, 40.0, 40.0));
        slow_pricey.output_speed_tps = Some(100.0);
        slow_pricey.time_to_first_token_secs = Some(1.0);
        let profile = RequestProfile {
            task: TaskKind::General,
            complexity: 0.2,
            estimated_input_tokens: 200,
            estimated_output_tokens: 1024,
        };
        let models = [slow_pricey, slow_cheap, fast];
        let ranked = rank_models(&models, RoutingStrategy::Speed, &profile);
        assert_eq!(ranked[0].name, "fast");
        assert_eq!(ranked[1].name, "slow-cheap");
        assert_eq!(ranked[2].name, "slow-pricey");
    }

    #[test]
    fn speed_primary_blends_ttft_and_throughput() {
        // Two models with the same output_speed_tps but different TTFTs — primary key
        // must reflect that the lower-TTFT model finishes faster even when decode rate
        // is identical. fast-ttft: 0.1 + 1000/100 = 10.1s. slow-ttft: 2.0 + 1000/100 = 12.0s.
        let mut fast_ttft = sample_model("fast-ttft", 1.0, 1.0, (50.0, 50.0, 40.0, 40.0));
        fast_ttft.output_speed_tps = Some(100.0);
        fast_ttft.time_to_first_token_secs = Some(0.1);
        let mut slow_ttft = sample_model("slow-ttft", 1.0, 1.0, (50.0, 50.0, 40.0, 40.0));
        slow_ttft.output_speed_tps = Some(100.0);
        slow_ttft.time_to_first_token_secs = Some(2.0);
        let profile = RequestProfile {
            task: TaskKind::General,
            complexity: 0.2,
            estimated_input_tokens: 200,
            estimated_output_tokens: 1024,
        };
        let models = [slow_ttft, fast_ttft];
        let ranked = rank_models(&models, RoutingStrategy::Speed, &profile);
        assert_eq!(ranked[0].name, "fast-ttft");
        assert_eq!(ranked[1].name, "slow-ttft");
    }

    #[test]
    fn cheapest_route_candidate_uses_endpoint_cost_as_primary() {
        // Same model offered at two different prices on two service providers.
        // Primary is endpoint effective_cost → cheap provider wins regardless of model
        // capability (capability is per-model, identical here, so secondary intelligence
        // also ties — provider name becomes the final tiebreak).
        let model = sample_model("m", 1.0, 1.0, (50.0, 50.0, 40.0, 40.0));
        let profile = RequestProfile {
            task: TaskKind::Coding,
            complexity: 0.2,
            estimated_input_tokens: 200,
            estimated_output_tokens: 1024,
        };
        let cheap = RouteCandidate {
            model: &model,
            service_provider_id: "cheap-provider",
            input_cost: 0.1,
            output_cost: 0.1,
            cache_read_cost: None,
        };
        let pricey = RouteCandidate {
            model: &model,
            service_provider_id: "pricey-provider",
            input_cost: 5.0,
            output_cost: 5.0,
            cache_read_cost: None,
        };
        let candidates = [pricey, cheap];
        let ranked = rank_route_candidates(&candidates, RoutingStrategy::Cheapest, &profile);
        assert_eq!(ranked[0].1, "cheap-provider");
        assert_eq!(ranked[1].1, "pricey-provider");
    }

    #[test]
    fn balanced_uses_weighted_cost_not_input_only() {
        let a = sample_model("a", 1.0, 10.0, (50.0, 70.0, 40.0, 40.0));
        let b = sample_model("b", 3.0, 1.0, (50.0, 70.0, 40.0, 40.0));
        let profile = RequestProfile {
            task: TaskKind::Coding,
            complexity: 0.5,
            estimated_input_tokens: 1000,
            estimated_output_tokens: 100,
        };
        let models = [a, b];
        let ranked = rank_models(&models, RoutingStrategy::Balanced, &profile);
        assert_eq!(ranked[0].name, "a");
    }

    #[test]
    fn intelligent_ranks_by_coding_when_math_index_missing() {
        let mut partial = sample_model("partial-aa", 1.0, 1.0, (50.0, 47.5, 40.0, 0.0));
        partial.math_index = None;
        let complete = sample_model("complete-aa", 1.0, 1.0, (39.0, 32.8, 54.0, 82.0));
        let profile = RequestProfile {
            task: TaskKind::Coding,
            complexity: 0.2,
            estimated_input_tokens: 200,
            estimated_output_tokens: 1024,
        };
        let models = [complete, partial];
        let ranked = rank_models(&models, RoutingStrategy::Intelligent, &profile);
        assert_eq!(ranked[0].name, "partial-aa");
        assert_eq!(ranked[1].name, "complete-aa");
    }

    #[test]
    fn balanced_sorts_missing_aa_indices_last() {
        let mut missing = sample_model("no-aa", 0.5, 0.5, (0.0, 0.0, 0.0, 0.0));
        missing.overall_intelligence = None;
        missing.coding_index = None;
        missing.agentic_index = None;
        missing.math_index = None;
        let scored = sample_model("scored", 2.0, 2.0, (60.0, 65.0, 55.0, 50.0));
        let profile = RequestProfile {
            task: TaskKind::Coding,
            complexity: 0.5,
            estimated_input_tokens: 1000,
            estimated_output_tokens: 1024,
        };
        let models = [missing, scored];
        let ranked = rank_models(&models, RoutingStrategy::Balanced, &profile);
        assert_eq!(ranked[0].name, "scored");
        assert_eq!(ranked[1].name, "no-aa");
    }

    #[test]
    fn cheap_endpoint_outranks_expensive_on_balanced() {
        let mut cheap = sample_model("cheap-payg", 0.1, 0.1, (50.0, 50.0, 40.0, 40.0));
        cheap.provider_id = "payg".into();
        let mut pricey = sample_model("pricey", 5.0, 20.0, (50.0, 50.0, 40.0, 40.0));
        pricey.provider_id = "premium".into();
        let profile = RequestProfile {
            task: TaskKind::General,
            complexity: 0.3,
            estimated_input_tokens: 500,
            estimated_output_tokens: 1024,
        };
        let models = [pricey, cheap];
        let ranked = rank_models(&models, RoutingStrategy::Balanced, &profile);
        assert_eq!(ranked[0].name, "cheap-payg");
    }

    #[test]
    fn speed_prefers_faster_model() {
        // fast: 0.5 + 100/200 = 1.0s. slow: 2.0 + 100/40 = 4.5s. fast wins on primary.
        let mut fast = sample_model("fast", 1.0, 1.0, (50.0, 50.0, 40.0, 40.0));
        fast.output_speed_tps = Some(200.0);
        fast.time_to_first_token_secs = Some(0.5);
        let mut slow = sample_model("slow", 0.1, 0.1, (90.0, 90.0, 80.0, 80.0));
        slow.output_speed_tps = Some(40.0);
        slow.time_to_first_token_secs = Some(2.0);
        let profile = RequestProfile {
            task: TaskKind::Coding,
            complexity: 0.5,
            estimated_input_tokens: 1000,
            estimated_output_tokens: 1024,
        };
        let models = [slow, fast];
        let ranked = rank_models(&models, RoutingStrategy::Speed, &profile);
        assert_eq!(ranked[0].name, "fast");
    }

    #[test]
    fn speed_tiebreaks_on_lower_cost_when_total_time_equal() {
        // Identical ttft + tps → primary key ties. Secondary is -effective_cost DESC;
        // cheap wins the tie-break regardless of provider-tier label.
        let mut cheap = sample_model("cheap", 0.1, 0.1, (50.0, 50.0, 40.0, 40.0));
        cheap.output_speed_tps = Some(100.0);
        cheap.time_to_first_token_secs = Some(1.0);
        let mut pricey = sample_model("pricey", 5.0, 5.0, (50.0, 50.0, 40.0, 40.0));
        pricey.output_speed_tps = Some(100.0);
        pricey.time_to_first_token_secs = Some(1.0);
        let profile = RequestProfile {
            task: TaskKind::General,
            complexity: 0.2,
            estimated_input_tokens: 200,
            estimated_output_tokens: 1024,
        };
        let models = [pricey, cheap];
        let ranked = rank_models(&models, RoutingStrategy::Speed, &profile);
        assert_eq!(ranked[0].name, "cheap");
    }

    #[test]
    fn speed_without_data_falls_back_to_cheapest() {
        let cheap = sample_model("cheap", 0.1, 0.1, (50.0, 50.0, 40.0, 40.0));
        let pricey = sample_model("pricey", 5.0, 5.0, (90.0, 90.0, 80.0, 80.0));
        let profile = RequestProfile {
            task: TaskKind::General,
            complexity: 0.2,
            estimated_input_tokens: 200,
            estimated_output_tokens: 1024,
        };
        let models = [pricey, cheap];
        let ranked = rank_models(&models, RoutingStrategy::Speed, &profile);
        assert_eq!(ranked[0].name, "cheap");
    }

    #[test]
    fn detects_math_task_from_prompt() {
        let body = serde_json::json!({
            "messages": [{"role": "user", "content": "Prove the integral transforms and solve the matrix equation."}]
        });
        let profile = build_request_profile(&body, "unknown");
        assert_eq!(profile.task, TaskKind::Math);
    }
}
