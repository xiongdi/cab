//! Request-aware model routing for CAB gateway strategies.
//!
//! ## Auto (`auto`)
//! 1. Parse request text → estimate complexity + task kind (coding / math / agentic / general).
//! 2. Score each enabled model with a task-weighted capability blend (AA indices on `Model`).
//! 3. Require minimum capability that rises with complexity (simple → cheap OK, hard → flagship).
//! 4. Rank by value = capability / effective_cost (or +∞ when catalog price is known free),
//!    then tie-break on capability, then cost. Missing catalog prices are excluded from value.
//!
//! ## Balanced (`balanced`)
//! Rank by task-primary capability / effective_cost (same 10:1 price weighting).
//!
//! ## Speed (`speed`)
//! Rank by AA median output speed (tokens/s), then TTFT, then cost. Models without speed
//! data sink to the bottom; if none have speed data, fall back to cheapest.

use std::collections::HashSet;

use crate::types::Model;

/// Typical prompt:completion token ratio for coding agents (input-heavy).
pub const BALANCED_INPUT_OUTPUT_RATIO: f64 = 10.0;

/// Assumed prompt cache hit rate when `cache_read` pricing is available.
pub const INPUT_CACHE_HIT_RATE: f64 = 0.9;

const MIN_COST_EPSILON: f64 = 0.001;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingStrategy {
    Auto,
    Balanced,
    Cheapest,
    Intelligent,
    Speed,
}

impl RoutingStrategy {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "auto" => Some(Self::Auto),
            "balanced" => Some(Self::Balanced),
            "cheapest" | "price" => Some(Self::Cheapest),
            "intelligent" => Some(Self::Intelligent),
            "speed" => Some(Self::Speed),
            _ => None,
        }
    }
}

fn model_output_speed(model: &Model) -> Option<f64> {
    model.output_speed_tps.filter(|speed| *speed > 0.0)
}

fn model_time_to_first_token(model: &Model) -> f64 {
    model.time_to_first_token_secs.unwrap_or(f64::MAX)
}

/// Build a routing profile from an API JSON body and client agent id.
pub fn build_request_profile(body: &serde_json::Value, agent: &str) -> RequestProfile {
    let text = extract_request_text(body);
    let message_count = count_messages(body);
    let has_tools = body.get("tools").is_some() || body.get("functions").is_some();
    classify_request(&text, agent, message_count, has_tools)
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
        Some(cache_read) => INPUT_CACHE_HIT_RATE * cache_read + (1.0 - INPUT_CACHE_HIT_RATE) * input,
        None => input,
    }
}

pub fn raw_effective_token_cost(
    input_cost: Option<f64>,
    output_cost: Option<f64>,
    cache_read_cost: Option<f64>,
) -> f64 {
    let input = input_cost.unwrap_or(0.0);
    let output = output_cost.unwrap_or(0.0).max(0.0);
    let blended_input = blended_input_cost(input, cache_read_cost);
    blended_input * BALANCED_INPUT_OUTPUT_RATIO + output
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
    let (Some(_), Some(_)) = (input_cost, output_cost) else {
        return f64::NEG_INFINITY;
    };
    let raw = raw_effective_token_cost(input_cost, output_cost, cache_read_cost);
    if raw <= 0.0 {
        f64::INFINITY
    } else {
        capability / raw
    }
}

pub fn effective_routing_cost(
    model: &Model,
    subscribed_provider_ids: Option<&HashSet<String>>,
) -> f64 {
    if subscribed_provider_ids
        .map(|ids| ids.contains(&model.provider_id))
        .unwrap_or(false)
    {
        MIN_COST_EPSILON
    } else {
        effective_token_cost_for_model(model)
    }
}

fn provider_is_subscribed(
    provider_id: &str,
    subscribed_provider_ids: Option<&HashSet<String>>,
) -> bool {
    subscribed_provider_ids
        .map(|ids| ids.contains(provider_id))
        .unwrap_or(false)
}

fn subscribed_sort_key(
    provider_id: &str,
    subscribed_provider_ids: Option<&HashSet<String>>,
) -> u8 {
    if provider_is_subscribed(provider_id, subscribed_provider_ids) {
        0
    } else {
        1
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

pub fn effective_routing_cost_for_candidate(
    candidate: &RouteCandidate<'_>,
    subscribed_provider_ids: Option<&HashSet<String>>,
) -> f64 {
    if subscribed_provider_ids
        .map(|ids| ids.contains(candidate.service_provider_id))
        .unwrap_or(false)
    {
        MIN_COST_EPSILON
    } else {
        effective_token_cost(
            Some(candidate.input_cost),
            Some(candidate.output_cost),
            candidate.cache_read_cost,
        )
    }
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

fn all_composite_indices_present(model: &Model) -> bool {
    model.overall_intelligence.is_some()
        && model.coding_index.is_some()
        && model.agentic_index.is_some()
        && model.math_index.is_some()
}

fn task_capability_available(model: &Model, task: TaskKind) -> bool {
    match task {
        TaskKind::Coding => {
            model.coding_index.is_some() || model.overall_intelligence.is_some()
        }
        TaskKind::Math => model.math_index.is_some() || model.overall_intelligence.is_some(),
        TaskKind::Agentic => {
            model.agentic_index.is_some() || model.overall_intelligence.is_some()
        }
        TaskKind::General => model.overall_intelligence.is_some(),
    }
}

/// Whether a model can participate in scoring for the given strategy and task.
pub fn model_routable_for_strategy(model: &Model, strategy: RoutingStrategy, task: TaskKind) -> bool {
    match strategy {
        RoutingStrategy::Cheapest => true,
        RoutingStrategy::Intelligent => model.coding_index.is_some(),
        RoutingStrategy::Speed => model_output_speed(model).is_some(),
        RoutingStrategy::Balanced | RoutingStrategy::Auto => task_capability_available(model, task),
    }
}

fn score_parts(
    model: &Model,
    strategy: RoutingStrategy,
    task: TaskKind,
    endpoint_cost: f64,
    value_input_cost: Option<f64>,
    value_output_cost: Option<f64>,
    value_cache_read_cost: Option<f64>,
) -> ScoreParts {
    if !model_routable_for_strategy(model, strategy, task) {
        let value = match strategy {
            RoutingStrategy::Cheapest => -endpoint_cost,
            _ => f64::NEG_INFINITY,
        };
        return ScoreParts {
            capability: f64::NEG_INFINITY,
            value,
        };
    }

    let capability = match strategy {
        RoutingStrategy::Intelligent => model.coding_index.unwrap_or(0.0),
        RoutingStrategy::Speed => model_output_speed(model).unwrap_or(0.0),
        RoutingStrategy::Cheapest => 0.0,
        RoutingStrategy::Balanced => primary_capability_loose(model, task),
        RoutingStrategy::Auto => {
            if all_composite_indices_present(model) {
                composite_capability(model, task)
            } else {
                primary_capability_loose(model, task)
            }
        }
    };
    let value = match strategy {
        RoutingStrategy::Cheapest => -endpoint_cost,
        RoutingStrategy::Intelligent | RoutingStrategy::Speed => capability,
        RoutingStrategy::Balanced | RoutingStrategy::Auto => capability_value_score(
            capability,
            value_input_cost,
            value_output_cost,
            value_cache_read_cost,
        ),
    };
    ScoreParts { capability, value }
}

fn score_parts_for_model(
    model: &Model,
    strategy: RoutingStrategy,
    task: TaskKind,
    endpoint_cost: f64,
) -> ScoreParts {
    score_parts(
        model,
        strategy,
        task,
        endpoint_cost,
        model.input_cost,
        model.output_cost,
        cache_read_cost_from_model(model),
    )
}

fn score_parts_for_candidate(
    candidate: &RouteCandidate<'_>,
    strategy: RoutingStrategy,
    task: TaskKind,
    endpoint_cost: f64,
) -> ScoreParts {
    score_parts(
        candidate.model,
        strategy,
        task,
        endpoint_cost,
        Some(candidate.input_cost),
        Some(candidate.output_cost),
        candidate.cache_read_cost,
    )
}

fn score_models<'a>(
    models: &'a [Model],
    strategy: RoutingStrategy,
    profile: &RequestProfile,
    subscribed_provider_ids: Option<&HashSet<String>>,
) -> Vec<(&'a Model, f64, f64)> {
    if matches!(strategy, RoutingStrategy::Speed)
        && !models.iter().any(|model| model_output_speed(model).is_some())
    {
        tracing::warn!(
            "Speed strategy has no models with AA output speed data; falling back to cheapest"
        );
        return score_models(models, RoutingStrategy::Cheapest, profile, subscribed_provider_ids);
    }

    let mut scored: Vec<(&Model, f64, f64)> = models
        .iter()
        .map(|model| {
            let routing_cost = effective_token_cost_for_model(model);
            let parts = score_parts_for_model(model, strategy, profile.task, routing_cost);
            (model, parts.capability, parts.value)
        })
        .collect();

    if matches!(strategy, RoutingStrategy::Auto) {
        let min_required = min_required_capability(profile);
        scored.retain(|(_, capability, _)| *capability >= min_required);
        if scored.is_empty() {
            scored = models
                .iter()
                .map(|model| {
                    let routing_cost = effective_token_cost_for_model(model);
                    let parts = score_parts_for_model(model, strategy, profile.task, routing_cost);
                    (model, parts.capability, parts.value)
                })
                .collect();
        }
    }

    scored.sort_by(|(a_model, a_cap, a_val), (b_model, b_cap, b_val)| {
        subscribed_sort_key(&a_model.provider_id, subscribed_provider_ids)
            .cmp(&subscribed_sort_key(
                &b_model.provider_id,
                subscribed_provider_ids,
            ))
            .then_with(|| {
                b_val
                    .partial_cmp(a_val)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                b_cap
                    .partial_cmp(a_cap)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                if matches!(strategy, RoutingStrategy::Speed) {
                    model_time_to_first_token(a_model)
                        .partial_cmp(&model_time_to_first_token(b_model))
                        .unwrap_or(std::cmp::Ordering::Equal)
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .then_with(|| {
                effective_token_cost_for_model(a_model)
                    .partial_cmp(&effective_token_cost_for_model(b_model))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a_model.name.cmp(&b_model.name))
    });

    scored
}

fn score_route_candidates<'a>(
    candidates: &'a [RouteCandidate<'a>],
    strategy: RoutingStrategy,
    profile: &RequestProfile,
    subscribed_provider_ids: Option<&HashSet<String>>,
) -> Vec<(&'a Model, &'a str, f64, f64)> {
    if matches!(strategy, RoutingStrategy::Speed)
        && !candidates
            .iter()
            .any(|c| model_output_speed(c.model).is_some())
    {
        tracing::warn!(
            "Speed strategy has no models with AA output speed data; falling back to cheapest"
        );
        return score_route_candidates(
            candidates,
            RoutingStrategy::Cheapest,
            profile,
            subscribed_provider_ids,
        );
    }

    let mut scored: Vec<(&Model, &str, f64, f64, f64)> = candidates
        .iter()
        .map(|candidate| {
            let endpoint_cost = effective_token_cost(
                Some(candidate.input_cost),
                Some(candidate.output_cost),
                candidate.cache_read_cost,
            );
            let parts = score_parts_for_candidate(candidate, strategy, profile.task, endpoint_cost);
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
        scored.retain(|(_, _, capability, _, _)| *capability >= min_required);
        if scored.is_empty() {
            scored = candidates
                .iter()
                .map(|candidate| {
                    let routing_cost = effective_token_cost(
                        Some(candidate.input_cost),
                        Some(candidate.output_cost),
                        candidate.cache_read_cost,
                    );
                    let parts = score_parts_for_candidate(candidate, strategy, profile.task, routing_cost);
                    (
                        candidate.model,
                        candidate.service_provider_id,
                        parts.capability,
                        parts.value,
                        routing_cost,
                    )
                })
                .collect();
        }
    }

    scored.sort_by(
        |(a_model, a_provider, a_cap, a_val, a_cost), (b_model, b_provider, b_cap, b_val, b_cost)| {
            subscribed_sort_key(a_provider, subscribed_provider_ids)
                .cmp(&subscribed_sort_key(b_provider, subscribed_provider_ids))
                .then_with(|| {
                    b_val
                        .partial_cmp(a_val)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    b_cap
                        .partial_cmp(a_cap)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    if matches!(strategy, RoutingStrategy::Speed) {
                        model_time_to_first_token(a_model)
                            .partial_cmp(&model_time_to_first_token(b_model))
                            .unwrap_or(std::cmp::Ordering::Equal)
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
                .then_with(|| a_cost.partial_cmp(b_cost).unwrap_or(std::cmp::Ordering::Equal))
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
    subscribed_provider_ids: Option<&HashSet<String>>,
) -> Vec<RankedRouteCandidate<'a>> {
    score_route_candidates(candidates, strategy, profile, subscribed_provider_ids)
        .into_iter()
        .map(|(model, service_provider_id, capability, value)| RankedRouteCandidate {
            model,
            service_provider_id,
            capability,
            value,
        })
        .collect()
}

pub fn rank_route_candidates<'a>(
    candidates: &'a [RouteCandidate<'a>],
    strategy: RoutingStrategy,
    profile: &RequestProfile,
    subscribed_provider_ids: Option<&HashSet<String>>,
) -> Vec<(&'a Model, &'a str)> {
    score_route_candidates(candidates, strategy, profile, subscribed_provider_ids)
        .into_iter()
        .map(|(model, provider, _, _)| (model, provider))
        .collect()
}

pub fn rank_models_with_scores<'a>(
    models: &'a [Model],
    strategy: RoutingStrategy,
    profile: &RequestProfile,
    subscribed_provider_ids: Option<&HashSet<String>>,
) -> Vec<RankedModelScore<'a>> {
    score_models(models, strategy, profile, subscribed_provider_ids)
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
    subscribed_provider_ids: Option<&HashSet<String>>,
) -> Vec<&'a Model> {
    score_models(models, strategy, profile, subscribed_provider_ids)
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

fn primary_capability_loose(model: &Model, task: TaskKind) -> f64 {
    match task {
        TaskKind::Coding => model.coding_index.or(model.overall_intelligence),
        TaskKind::Math => model.math_index.or(model.overall_intelligence),
        TaskKind::Agentic => model.agentic_index.or(model.overall_intelligence),
        TaskKind::General => model.overall_intelligence,
    }
    .expect("task_capability_available should be checked before scoring")
}

fn composite_capability(model: &Model, task: TaskKind) -> f64 {
    let overall = model.overall_intelligence.expect("complete indices");
    let coding = model.coding_index.expect("complete indices");
    let agentic = model.agentic_index.expect("complete indices");
    let math = model.math_index.expect("complete indices");
    match task {
        TaskKind::Coding => weighted_score(&[
            (coding, 0.55),
            (overall, 0.22),
            (agentic, 0.13),
            (math, 0.10),
        ]),
        TaskKind::Math => weighted_score(&[
            (math, 0.58),
            (overall, 0.24),
            (coding, 0.10),
            (agentic, 0.08),
        ]),
        TaskKind::Agentic => weighted_score(&[
            (agentic, 0.42),
            (overall, 0.28),
            (coding, 0.22),
            (math, 0.08),
        ]),
        TaskKind::General => weighted_score(&[
            (overall, 0.45),
            (coding, 0.22),
            (math, 0.18),
            (agentic, 0.15),
        ]),
    }
}

fn weighted_score(parts: &[(f64, f64)]) -> f64 {
    parts.iter().map(|(score, weight)| score * weight).sum()
}

fn classify_request(
    text: &str,
    agent: &str,
    message_count: usize,
    has_tools: bool,
) -> RequestProfile {
    let lower = text.to_ascii_lowercase();
    let agent_lower = agent.to_ascii_lowercase();
    let estimated_input_tokens = (text.len().max(1) / 4) as u64;

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
            "代码",
            "函数",
            "重构",
            "报错",
            "编译",
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
    }
}

fn matches_agent_kind(agent: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| agent.contains(needle))
}

fn count_keyword_hits(haystack: &str, keywords: &[&str]) -> usize {
    keywords.iter().filter(|kw| haystack.contains(*kw)).count()
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
        assert!(
            (effective_token_cost(Some(1.0), Some(1.0), None) - 11.0).abs() < f64::EPSILON
        );
    }

    #[test]
    fn effective_token_cost_applies_cache_hit_rate() {
        // input 1.0, cache_read 0.1 → blended 0.19, + output 1.0 → 0.19*10+1 = 2.9
        assert!(
            (effective_token_cost(Some(1.0), Some(1.0), Some(0.1)) - 2.9).abs() < f64::EPSILON
        );
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
        };
        let models = [weak, strong];
        let ranked = rank_models(&models, RoutingStrategy::Balanced, &profile, None);
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
        let ranked = rank_route_candidates(&candidates, RoutingStrategy::Balanced, &profile, None);
        assert_eq!(ranked[0].1, "subscription");
        let scores = rank_route_candidates_with_scores(&candidates, RoutingStrategy::Balanced, &profile, None);
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
        };
        let models = [cheap, strong];
        let ranked = rank_models(&models, RoutingStrategy::Auto, &profile, None);
        assert_eq!(ranked[0].name, "strong");
    }

    #[test]
    fn auto_prefers_cheap_model_for_simple_prompt() {
        let cheap = sample_model("cheap", 0.1, 0.1, (42.0, 38.0, 30.0, 30.0));
        let strong = sample_model("strong", 5.0, 20.0, (90.0, 92.0, 80.0, 75.0));
        let profile = RequestProfile {
            task: TaskKind::General,
            complexity: 0.1,
            estimated_input_tokens: 120,
        };
        let models = [cheap, strong];
        let ranked = rank_models(&models, RoutingStrategy::Auto, &profile, None);
        assert_eq!(ranked[0].name, "cheap");
    }

    #[test]
    fn balanced_uses_weighted_cost_not_input_only() {
        let a = sample_model("a", 1.0, 10.0, (50.0, 70.0, 40.0, 40.0));
        let b = sample_model("b", 3.0, 1.0, (50.0, 70.0, 40.0, 40.0));
        let profile = RequestProfile {
            task: TaskKind::Coding,
            complexity: 0.5,
            estimated_input_tokens: 1000,
        };
        let models = [a, b];
        let ranked = rank_models(&models, RoutingStrategy::Balanced, &profile, None);
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
        };
        let models = [complete, partial];
        let ranked = rank_models(&models, RoutingStrategy::Intelligent, &profile, None);
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
        };
        let models = [missing, scored];
        let ranked = rank_models(&models, RoutingStrategy::Balanced, &profile, None);
        assert_eq!(ranked[0].name, "scored");
        assert_eq!(ranked[1].name, "no-aa");
    }

    #[test]
    fn subscribed_provider_beats_expensive_payg_model() {
        let mut cheap = sample_model("cheap-payg", 0.1, 0.1, (50.0, 50.0, 40.0, 40.0));
        cheap.provider_id = "payg".into();
        let mut subscribed =
            sample_model("subscribed-flagship", 5.0, 20.0, (50.0, 50.0, 40.0, 40.0));
        subscribed.provider_id = "subscribed-vendor".into();
        let profile = RequestProfile {
            task: TaskKind::General,
            complexity: 0.3,
            estimated_input_tokens: 500,
        };
        let models = [cheap, subscribed];
        let subscribed_ids = HashSet::from(["subscribed-vendor".to_string()]);
        let ranked = rank_models(
            &models,
            RoutingStrategy::Balanced,
            &profile,
            Some(&subscribed_ids),
        );
        assert_eq!(ranked[0].name, "subscribed-flagship");
    }

    #[test]
    fn subscribed_tier_ranks_before_higher_capability_payg_on_intelligent() {
        let mut payg = sample_model("payg-smart", 0.5, 0.5, (90.0, 90.0, 80.0, 80.0));
        payg.provider_id = "payg".into();
        let mut subscribed = sample_model("subscribed-basic", 2.0, 2.0, (40.0, 40.0, 35.0, 35.0));
        subscribed.provider_id = "subscribed-vendor".into();
        let profile = RequestProfile {
            task: TaskKind::Coding,
            complexity: 0.5,
            estimated_input_tokens: 1000,
        };
        let models = [payg, subscribed];
        let subscribed_ids = HashSet::from(["subscribed-vendor".to_string()]);
        let ranked = rank_models(
            &models,
            RoutingStrategy::Intelligent,
            &profile,
            Some(&subscribed_ids),
        );
        assert_eq!(ranked[0].name, "subscribed-basic");
    }

    #[test]
    fn speed_prefers_faster_model() {
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
        };
        let models = [slow, fast];
        let ranked = rank_models(&models, RoutingStrategy::Speed, &profile, None);
        assert_eq!(ranked[0].name, "fast");
    }

    #[test]
    fn speed_tiebreaks_on_lower_ttft() {
        let mut a = sample_model("a", 1.0, 1.0, (50.0, 50.0, 40.0, 40.0));
        a.output_speed_tps = Some(100.0);
        a.time_to_first_token_secs = Some(1.5);
        let mut b = sample_model("b", 1.0, 1.0, (50.0, 50.0, 40.0, 40.0));
        b.output_speed_tps = Some(100.0);
        b.time_to_first_token_secs = Some(0.8);
        let profile = RequestProfile {
            task: TaskKind::General,
            complexity: 0.2,
            estimated_input_tokens: 200,
        };
        let models = [a, b];
        let ranked = rank_models(&models, RoutingStrategy::Speed, &profile, None);
        assert_eq!(ranked[0].name, "b");
    }

    #[test]
    fn speed_without_data_falls_back_to_cheapest() {
        let cheap = sample_model("cheap", 0.1, 0.1, (50.0, 50.0, 40.0, 40.0));
        let pricey = sample_model("pricey", 5.0, 5.0, (90.0, 90.0, 80.0, 80.0));
        let profile = RequestProfile {
            task: TaskKind::General,
            complexity: 0.2,
            estimated_input_tokens: 200,
        };
        let models = [pricey, cheap];
        let ranked = rank_models(&models, RoutingStrategy::Speed, &profile, None);
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
