//! Request-aware model routing for CAB gateway strategies.
//!
//! ## Auto (`auto`)
//! 1. Parse request text → estimate complexity + task kind (coding / math / agentic / general).
//! 2. Score each enabled model with a task-weighted capability blend (AA indices on `Model`).
//! 3. Require minimum capability that rises with complexity (simple → cheap OK, hard → flagship).
//! 4. Rank by value = capability / effective_cost, where effective_cost uses a 3:1 input:output ratio.
//!    Providers with a subscribed API key use near-zero marginal cost (prepaid quota).
//!
//! ## Balanced (`balanced`)
//! Rank by task-primary capability / effective_cost (same 3:1 price weighting).

use std::collections::HashSet;

use crate::types::Model;

/// Typical prompt:completion token ratio for coding agents (input-heavy).
pub const BALANCED_INPUT_OUTPUT_RATIO: f64 = 3.0;

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
}

impl RoutingStrategy {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "auto" => Some(Self::Auto),
            "balanced" => Some(Self::Balanced),
            "cheapest" | "price" => Some(Self::Cheapest),
            "intelligent" => Some(Self::Intelligent),
            _ => None,
        }
    }
}

/// Build a routing profile from an API JSON body and client agent id.
pub fn build_request_profile(body: &serde_json::Value, agent: &str) -> RequestProfile {
    let text = extract_request_text(body);
    let message_count = count_messages(body);
    let has_tools = body.get("tools").is_some() || body.get("functions").is_some();
    classify_request(&text, agent, message_count, has_tools)
}

pub fn effective_token_cost(input_cost: Option<f64>, output_cost: Option<f64>) -> f64 {
    let input = input_cost.unwrap_or(0.0).max(0.0);
    let output = output_cost.unwrap_or(0.0).max(0.0);
    (input * BALANCED_INPUT_OUTPUT_RATIO + output).max(MIN_COST_EPSILON)
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
        effective_token_cost(model.input_cost, model.output_cost)
    }
}

pub fn rank_models<'a>(
    models: &'a [Model],
    strategy: RoutingStrategy,
    profile: &RequestProfile,
    subscribed_provider_ids: Option<&HashSet<String>>,
) -> Vec<&'a Model> {
    let mut scored: Vec<(&Model, f64, f64)> = models
        .iter()
        .map(|model| {
            let routing_cost = effective_routing_cost(model, subscribed_provider_ids);
            let capability = match strategy {
                RoutingStrategy::Intelligent => model.coding_index,
                RoutingStrategy::Cheapest => 0.0,
                RoutingStrategy::Balanced => primary_capability(model, profile.task),
                RoutingStrategy::Auto => composite_capability(model, profile.task),
            };
            let value = match strategy {
                RoutingStrategy::Cheapest => -routing_cost,
                RoutingStrategy::Intelligent => capability,
                RoutingStrategy::Balanced | RoutingStrategy::Auto => capability / routing_cost,
            };
            (model, capability, value)
        })
        .collect();

    if matches!(strategy, RoutingStrategy::Auto) {
        let min_required = min_required_capability(profile);
        scored.retain(|(_, capability, _)| *capability >= min_required);
        if scored.is_empty() {
            scored = models
                .iter()
                .map(|model| {
                    let routing_cost = effective_routing_cost(model, subscribed_provider_ids);
                    let capability = composite_capability(model, profile.task);
                    let value = capability / routing_cost;
                    (model, capability, value)
                })
                .collect();
        }
    }

    scored.sort_by(|(a_model, a_cap, a_val), (b_model, b_cap, b_val)| {
        b_val
            .partial_cmp(a_val)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b_cap
                    .partial_cmp(a_cap)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                effective_routing_cost(a_model, subscribed_provider_ids)
                    .partial_cmp(&effective_routing_cost(b_model, subscribed_provider_ids))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a_model.name.cmp(&b_model.name))
    });

    scored.into_iter().map(|(model, _, _)| model).collect()
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

fn primary_capability(model: &Model, task: TaskKind) -> f64 {
    match task {
        TaskKind::Coding => model.coding_index,
        TaskKind::Math => model.math_index,
        TaskKind::Agentic => model.agentic_index,
        TaskKind::General => model.overall_intelligence,
    }
}

fn composite_capability(model: &Model, task: TaskKind) -> f64 {
    match task {
        TaskKind::Coding => weighted_score(&[
            (model.coding_index, 0.55),
            (model.overall_intelligence, 0.22),
            (model.agentic_index, 0.13),
            (model.math_index, 0.10),
        ]),
        TaskKind::Math => weighted_score(&[
            (model.math_index, 0.58),
            (model.overall_intelligence, 0.24),
            (model.coding_index, 0.10),
            (model.agentic_index, 0.08),
        ]),
        TaskKind::Agentic => weighted_score(&[
            (model.agentic_index, 0.42),
            (model.overall_intelligence, 0.28),
            (model.coding_index, 0.22),
            (model.math_index, 0.08),
        ]),
        TaskKind::General => weighted_score(&[
            (model.overall_intelligence, 0.45),
            (model.coding_index, 0.22),
            (model.math_index, 0.18),
            (model.agentic_index, 0.15),
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
            "claude", "codex", "copilot", "aider", "cline", "continue", "gemini", "hermes", "kilo",
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
            overall_intelligence: scores.0,
            coding_index: scores.1,
            agentic_index: scores.2,
            math_index: scores.3,
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
    fn effective_token_cost_weights_input_three_to_one() {
        assert!((effective_token_cost(Some(1.0), Some(1.0)) - 4.0).abs() < f64::EPSILON);
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
        assert_eq!(ranked[0].name, "b");
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
    fn detects_math_task_from_prompt() {
        let body = serde_json::json!({
            "messages": [{"role": "user", "content": "Prove the integral transforms and solve the matrix equation."}]
        });
        let profile = build_request_profile(&body, "unknown");
        assert_eq!(profile.task, TaskKind::Math);
    }
}
