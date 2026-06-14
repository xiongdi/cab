/// Heuristic intelligence indices for models without Artificial Analysis records.
///
/// These are only used when no benchmark catalog is available; normal catalog sync
/// should rely on Artificial Analysis data and display missing values as empty.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelIntelligenceIndices {
    pub overall_intelligence: Option<f64>,
    pub coding_index: Option<f64>,
    pub agentic_index: Option<f64>,
    pub math_index: Option<f64>,
}

impl ModelIntelligenceIndices {
    pub fn missing() -> Self {
        Self {
            overall_intelligence: None,
            coding_index: None,
            agentic_index: None,
            math_index: None,
        }
    }

    pub fn is_missing(&self) -> bool {
        self.overall_intelligence.is_none()
            && self.coding_index.is_none()
            && self.agentic_index.is_none()
            && self.math_index.is_none()
    }

    fn present(overall: f64, coding: f64, agentic: f64, math: f64) -> Self {
        Self {
            overall_intelligence: Some(overall),
            coding_index: Some(coding),
            agentic_index: Some(agentic),
            math_index: Some(math),
        }
    }
}

pub fn infer_intelligence_indices(
    model_name: &str,
    context_length: i64,
    input_cost: Option<f64>,
    description: Option<&str>,
) -> ModelIntelligenceIndices {
    let name_lower = model_name.to_ascii_lowercase();
    let slug = name_lower.split('/').nth(1).unwrap_or(name_lower.as_str());
    let haystack = match description {
        Some(desc) => format!("{slug} {desc}").to_ascii_lowercase(),
        None => slug.to_string(),
    };

    let mut overall = 42.0;
    let mut coding = 38.0;
    let mut agentic = 40.0;

    // Reasoning / agentic flagship tiers
    if contains_any(
        &haystack,
        &[
            "deepseek-r1",
            "o1",
            "o3",
            "reasoner",
            "reasoning",
            "thinking",
        ],
    ) {
        overall += 28.0;
        agentic += 35.0;
        coding += 12.0;
    }

    // Top-tier general models
    if contains_any(
        &haystack,
        &[
            "opus",
            "gpt-4o",
            "gpt-4.1",
            "gpt-4",
            "claude-3.7",
            "claude-4",
            "gemini-2.5-pro",
            "gemini-3",
            "deepseek-v3.2",
            "deepseek-v3",
            "deepseek-v4",
            "deepseek-chat",
            "qwen3",
            "kimi-k2",
        ],
    ) {
        overall += 18.0;
        coding += 16.0;
        agentic += 14.0;
    }

    // Strong mid-tier
    if contains_any(
        &haystack,
        &[
            "sonnet",
            "gpt-4o-mini",
            "gemini-2.0",
            "gemini-2.5-flash",
            "mistral-large",
            "llama-3.3",
            "llama-4",
            "qwen2.5",
            "qwen-2.5",
        ],
    ) {
        overall += 10.0;
        coding += 10.0;
        agentic += 8.0;
    }

    // Coding specialists
    if contains_any(&haystack, &["coder", "codex", "code-", "-code", "devstral"]) {
        coding += 24.0;
        overall += 8.0;
        agentic += 10.0;
    }

    // Distilled / compact variants sit below full models
    if contains_any(
        &haystack,
        &[
            "distill",
            "distilled",
            "mini",
            "small",
            "lite",
            "flash",
            "nano",
            "haiku",
        ],
    ) {
        overall -= 10.0;
        coding -= 6.0;
        agentic -= 8.0;
    }

    // Explicit budget / fast SKUs
    if contains_any(&haystack, &["turbo", "fast", "instant"]) {
        overall -= 4.0;
        agentic -= 3.0;
    }

    // Provider-specific baseline: DeepSeek models are generally strong for price
    if name_lower.starts_with("deepseek/") && !contains_any(&haystack, &["distill"]) {
        overall += 6.0;
        coding += 8.0;
    }

    if context_length >= 256_000 {
        overall += 4.0;
        agentic += 4.0;
    } else if context_length >= 128_000 {
        overall += 2.0;
        agentic += 2.0;
    } else if context_length > 0 && context_length < 16_000 {
        overall -= 4.0;
        coding -= 2.0;
    }

    if let Some(cost) = input_cost {
        if cost >= 10.0 {
            overall += 8.0;
            coding += 5.0;
            agentic += 6.0;
        } else if cost >= 3.0 {
            overall += 5.0;
            coding += 3.0;
            agentic += 4.0;
        } else if cost >= 1.0 {
            overall += 2.0;
        } else if cost > 0.0 && cost <= 0.2 {
            overall -= 2.0;
        }
    }

    ModelIntelligenceIndices::present(
        clamp_score(overall),
        clamp_score(coding),
        clamp_score(agentic),
        clamp_score(overall * 0.88),
    )
}

/// AA benchmark data is incomplete or unavailable for this model.
pub fn capability_indices_missing(model: &crate::types::Model) -> bool {
    model.overall_intelligence.is_none()
        || model.coding_index.is_none()
        || model.agentic_index.is_none()
        || model.math_index.is_none()
}

/// Legacy persisted rows used all-zero f64 fields as a missing sentinel.
pub fn normalize_legacy_missing_indices(model: &mut crate::types::Model) {
    if matches!(
        (
            model.overall_intelligence,
            model.coding_index,
            model.agentic_index,
            model.math_index
        ),
        (Some(0.0), Some(0.0), Some(0.0), Some(0.0))
    ) {
        model.overall_intelligence = None;
        model.coding_index = None;
        model.agentic_index = None;
        model.math_index = None;
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn clamp_score(value: f64) -> f64 {
    value.clamp(1.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepseek_models_are_not_all_identical() {
        let chat = infer_intelligence_indices("deepseek/deepseek-chat", 64_000, Some(0.14), None);
        let r1 = infer_intelligence_indices("deepseek/deepseek-r1", 64_000, Some(0.55), None);
        let coder = infer_intelligence_indices("deepseek/deepseek-coder", 64_000, Some(0.14), None);
        let flash =
            infer_intelligence_indices("deepseek/deepseek-v4-flash", 128_000, Some(0.10), None);

        assert!(r1.overall_intelligence > chat.overall_intelligence);
        assert!(coder.coding_index > chat.coding_index);
        assert!(chat.overall_intelligence > flash.overall_intelligence);
        assert_ne!(chat.overall_intelligence, Some(30.0));
        assert_ne!(r1.overall_intelligence, Some(30.0));
    }

    #[test]
    fn capability_indices_missing_detects_aa_absence() {
        use crate::types::Model;
        let missing = Model {
            id: "x".into(),
            name: "vendor/new".into(),
            display_name: "new".into(),
            provider_id: "vendor".into(),
            protocol: "openai-chat".into(),
            context_length: 128000,
            input_cost: Some(1.0),
            output_cost: Some(2.0),
            enabled: true,
            overall_intelligence: None,
            coding_index: None,
            agentic_index: None,
            math_index: None,
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
        };
        assert!(capability_indices_missing(&missing));
    }

    #[test]
    fn normalize_legacy_zero_sentinel() {
        use crate::types::Model;
        let mut legacy = Model {
            id: "x".into(),
            name: "vendor/new".into(),
            display_name: "new".into(),
            provider_id: "vendor".into(),
            protocol: "openai-chat".into(),
            context_length: 128000,
            input_cost: Some(1.0),
            output_cost: Some(2.0),
            enabled: true,
            overall_intelligence: Some(0.0),
            coding_index: Some(0.0),
            agentic_index: Some(0.0),
            math_index: Some(0.0),
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
        };
        normalize_legacy_missing_indices(&mut legacy);
        assert!(capability_indices_missing(&legacy));
    }
}
