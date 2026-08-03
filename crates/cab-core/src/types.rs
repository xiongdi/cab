use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ──────────────────────────── Provider ────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderEndpoint {
    pub id: String,
    pub protocol: String,
    pub url: String,
    pub label: Option<String>,
    pub priority: i32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyConfig {
    pub key: String,
    pub enabled: bool,
    /// RFC3339 timestamp when a 429 quota window ends; key is skipped until then.
    #[serde(default)]
    pub quota_reset_at: Option<String>,
}

impl ApiKeyConfig {
    pub fn is_usable(&self) -> bool {
        self.enabled && !self.key.trim().is_empty()
    }
}

/// True when the provider has at least one enabled key configured (ignores quota cooldown).
pub fn provider_has_configured_key(provider: &Provider) -> bool {
    provider.api_keys.iter().any(|k| k.is_usable()) || !provider.api_key.trim().is_empty()
}

/// True when the provider can accept a request right now (skips quota-cooled keys).
pub fn provider_has_available_key(provider: &Provider) -> bool {
    if !provider.enabled {
        return false;
    }
    !ordered_api_keys(&provider.api_keys).is_empty() || !provider.api_key.trim().is_empty()
}

/// First enabled key not in quota recovery.
pub fn select_preferred_api_key(api_keys: &[ApiKeyConfig]) -> Option<String> {
    ordered_api_keys(api_keys).into_iter().next()
}

/// Keys to try in configuration order; skip keys still rate-limited.
pub fn ordered_api_keys(api_keys: &[ApiKeyConfig]) -> Vec<String> {
    api_keys
        .iter()
        .filter(|key| key.is_usable() && !crate::subscription_quota::is_key_rate_limited(key))
        .map(|key| key.key.clone())
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub endpoints: Vec<ProviderEndpoint>,
    pub api_key: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub privacy_policy_url: Option<String>,
    pub terms_of_service_url: Option<String>,
    pub status_page_url: Option<String>,
    pub headquarters: Option<String>,
    pub datacenters: Option<Vec<String>>,
    pub api_keys: Vec<ApiKeyConfig>,
    pub api: Option<String>,
    pub doc: Option<String>,
    pub env: Option<Vec<String>>,
    pub npm: Option<String>,
    pub model_count: usize,
    #[serde(default)]
    pub logo: Option<String>,
    #[serde(default)]
    pub catalog_models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateProvider {
    pub name: String,
    pub endpoints: Option<Vec<ProviderEndpoint>>,
    pub api_key: String,
    pub enabled: Option<bool>,
    pub privacy_policy_url: Option<String>,
    pub terms_of_service_url: Option<String>,
    pub status_page_url: Option<String>,
    pub headquarters: Option<String>,
    pub datacenters: Option<Vec<String>>,
    pub api_keys: Option<Vec<ApiKeyConfig>>,
    pub api: Option<String>,
    pub doc: Option<String>,
    pub env: Option<Vec<String>>,
    pub npm: Option<String>,
    pub model_count: Option<usize>,
    #[serde(default)]
    pub logo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateProvider {
    pub name: Option<String>,
    pub endpoints: Option<Vec<ProviderEndpoint>>,
    pub api_key: Option<String>,
    pub enabled: Option<bool>,
    pub privacy_policy_url: Option<String>,
    pub terms_of_service_url: Option<String>,
    pub status_page_url: Option<String>,
    pub headquarters: Option<String>,
    pub datacenters: Option<Vec<String>>,
    pub api_keys: Option<Vec<ApiKeyConfig>>,
    pub api: Option<String>,
    pub doc: Option<String>,
    pub env: Option<Vec<String>>,
    pub npm: Option<String>,
    pub model_count: Option<usize>,
    #[serde(default)]
    pub logo: Option<Option<String>>,
}

// ──────────────────────────── Model ────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub provider_id: String,
    pub protocol: String, // "openai" or "anthropic"
    pub context_length: i64,
    pub input_cost: Option<f64>,
    pub output_cost: Option<f64>,
    pub enabled: bool,
    /// Absent when AA benchmark data is unavailable (distinct from a score of 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overall_intelligence: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coding_index: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agentic_index: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub math_index: Option<f64>,
    #[serde(default)]
    pub output_speed_tps: Option<f64>,
    #[serde(default)]
    pub time_to_first_token_secs: Option<f64>,
    pub created_at: String,
    pub updated_at: String,
    // Catalog metadata
    pub canonical_slug: Option<String>,
    pub hugging_face_id: Option<String>,
    pub created: Option<i64>,
    pub description: Option<String>,
    pub architecture: Option<serde_json::Value>,
    pub pricing: Option<serde_json::Value>,
    pub top_provider: Option<serde_json::Value>,
    pub per_request_limits: Option<serde_json::Value>,
    pub supported_parameters: Option<serde_json::Value>,
    pub default_parameters: Option<serde_json::Value>,
    pub supported_voices: Option<serde_json::Value>,
    pub knowledge_cutoff: Option<String>,
    pub expiration_date: Option<String>,
    pub links: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateModel {
    pub name: String,
    pub display_name: String,
    pub provider_id: String,
    pub protocol: String, // "openai" or "anthropic"
    pub context_length: i64,
    pub input_cost: Option<f64>,
    pub output_cost: Option<f64>,
    pub enabled: Option<bool>,
    pub overall_intelligence: Option<f64>,
    pub coding_index: Option<f64>,
    pub agentic_index: Option<f64>,
    pub math_index: Option<f64>,
    pub output_speed_tps: Option<f64>,
    pub time_to_first_token_secs: Option<f64>,
    // Catalog metadata
    pub canonical_slug: Option<String>,
    pub hugging_face_id: Option<String>,
    pub created: Option<i64>,
    pub description: Option<String>,
    pub architecture: Option<serde_json::Value>,
    pub pricing: Option<serde_json::Value>,
    pub top_provider: Option<serde_json::Value>,
    pub per_request_limits: Option<serde_json::Value>,
    pub supported_parameters: Option<serde_json::Value>,
    pub default_parameters: Option<serde_json::Value>,
    pub supported_voices: Option<serde_json::Value>,
    pub knowledge_cutoff: Option<String>,
    pub expiration_date: Option<String>,
    pub links: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateModel {
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub provider_id: Option<String>,
    pub protocol: Option<String>,
    pub context_length: Option<i64>,
    /// Outer `None` = leave unchanged; inner `None` = clear.
    pub input_cost: Option<Option<f64>>,
    pub output_cost: Option<Option<f64>>,
    pub enabled: Option<bool>,
    /// Outer `None` = leave unchanged; inner `None` = clear benchmark score.
    pub overall_intelligence: Option<Option<f64>>,
    pub coding_index: Option<Option<f64>>,
    pub agentic_index: Option<Option<f64>>,
    pub math_index: Option<Option<f64>>,
    pub output_speed_tps: Option<Option<f64>>,
    pub time_to_first_token_secs: Option<Option<f64>>,
    // Catalog metadata
    pub canonical_slug: Option<String>,
    pub hugging_face_id: Option<String>,
    pub created: Option<i64>,
    pub description: Option<String>,
    pub architecture: Option<serde_json::Value>,
    pub pricing: Option<serde_json::Value>,
    pub top_provider: Option<serde_json::Value>,
    pub per_request_limits: Option<serde_json::Value>,
    pub supported_parameters: Option<serde_json::Value>,
    pub default_parameters: Option<serde_json::Value>,
    pub supported_voices: Option<serde_json::Value>,
    pub knowledge_cutoff: Option<String>,
    pub expiration_date: Option<String>,
    pub links: Option<serde_json::Value>,
}

// ──────────────────────────── Route ────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub id: String,
    pub name: String,
    pub agent_pattern: String,
    #[serde(rename = "primary_model_id")]
    pub model_id: String,
    #[serde(rename = "fallback_model_ids")]
    pub fallback_ids: Vec<String>,
    pub priority: i32,
    /// One of: auto | cheapest | balanced | intelligent
    pub routing_strategy: String,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRoute {
    pub name: String,
    pub agent_pattern: String,
    #[serde(rename = "primary_model_id")]
    pub model_id: String,
    #[serde(rename = "fallback_model_ids")]
    pub fallback_ids: Option<Vec<String>>,
    pub priority: Option<i32>,
    pub routing_strategy: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRoute {
    pub name: Option<String>,
    pub agent_pattern: Option<String>,
    #[serde(rename = "primary_model_id")]
    pub model_id: Option<String>,
    #[serde(rename = "fallback_model_ids")]
    pub fallback_ids: Option<Vec<String>>,
    pub priority: Option<i32>,
    pub routing_strategy: Option<String>,
    pub enabled: Option<bool>,
}

// ──────────────────────────── Request Log ────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestLog {
    pub id: String,
    pub timestamp: String,
    pub agent: String,
    pub provider: String,
    pub model: String,
    /// Non-cached-read prompt tokens only (excludes cache **read**).
    /// On Anthropic this also excludes cache **write**; on OpenAI write usually
    /// still sits inside this count as a billing overlay.
    pub input_tokens: i64,
    /// Completion tokens only (no cache component).
    pub output_tokens: i64,
    /// `input + cache_read + cache_creation + output`.
    pub total_tokens: i64,
    /// Input tokens served from the upstream prefix cache (cache **hit** / read).
    #[serde(default)]
    pub cache_read_tokens: i64,
    /// Tokens written into the upstream prefix cache this turn (cache **write**).
    /// Billing leg — on OpenAI this usually overlays the non-read portion of
    /// `prompt_tokens`; on Anthropic it is disjoint from `input_tokens`.
    #[serde(default)]
    pub cache_creation_tokens: i64,
    pub latency_ms: i64,
    #[serde(rename = "status_code")]
    pub status: i32,
    #[serde(rename = "error_message")]
    pub error: Option<String>,
    pub path: String,
    pub stream: bool,
    #[serde(default)]
    pub request_body: Option<String>,
    #[serde(default)]
    pub response_body: Option<String>,
}

// ──────────────────────────── Usage Tracking ────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: String,
    pub timestamp: String,
    pub provider_id: String,
    pub model_id: String,
    pub service_provider_id: String,
    pub agent_id: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub cost_usd: f64,
    pub subscription: bool,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UsageSummary {
    pub total_requests: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cost_usd: f64,
    pub by_provider: std::collections::HashMap<String, ProviderUsageSummary>,
    pub by_model: std::collections::HashMap<String, ModelUsageSummary>,
    pub by_agent: std::collections::HashMap<String, AgentUsageSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderUsageSummary {
    pub requests: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelUsageSummary {
    pub requests: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentUsageSummary {
    pub requests: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct UsageQuery {
    pub range: Option<String>,
    pub group_by: Option<String>,
    pub per_page: Option<i64>,
    pub page: Option<i64>,
}

// ──────────────────────────── Dashboard ────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardStats {
    pub total_requests: i64,
    #[serde(rename = "total_tokens")]
    pub total_tokens: i64,
    #[serde(rename = "active_providers")]
    pub providers_count: i64,
    #[serde(rename = "active_models")]
    pub models_count: i64,
    pub recent_requests: Vec<RequestLog>,
    pub requests_by_provider: std::collections::HashMap<String, i64>,
    pub requests_by_model: std::collections::HashMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountByName {
    pub name: String,
    pub count: i64,
}

// ──────────────────────────── Log Query ────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogQuery {
    pub agent: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub status: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaginatedLogs {
    pub data: Vec<RequestLog>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
}

// ──────────────────────────── Settings ────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderUserSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_keys: Option<Vec<ApiKeyConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoints: Option<Vec<ProviderEndpoint>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelUserSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

fn default_auth_enabled() -> bool {
    true
}

fn default_cache_affinity_enabled() -> bool {
    true
}

fn default_cache_request_shaping_enabled() -> bool {
    true
}

/// **User runtime config** stored in the SQLite `settings` table, editable via `PUT /api/settings`.
///
/// This is the runtime counterpart to `cab.toml` (system bootstrap). See `cab_core::config::CabConfig`
/// for the full priority chain. In short: `gateway_port` here is the runtime value used at bind time;
/// `cab.toml [gateway] port` is only the first-install default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Runtime gateway port (editable via API). On first install this is seeded from `cab.toml`.
    pub gateway_port: i64,
    pub log_retention_days: i64,
    pub gateway_key: String,
    #[serde(default = "default_auth_enabled")]
    pub auth_enabled: bool,
    /// Pin a conversation to the provider+model it first resolved to, so the
    /// upstream prefix cache keeps hitting across turns instead of cold-starting
    /// whenever re-scoring or a rate-limit would otherwise switch providers.
    #[serde(default = "default_cache_affinity_enabled")]
    pub cache_affinity_enabled: bool,
    /// Rewrite outgoing requests for upstream prefix-cache friendliness:
    /// deterministically order tool schemas and inject Anthropic `cache_control`
    /// breakpoints (only when the client did not already set them).
    #[serde(default = "default_cache_request_shaping_enabled")]
    pub cache_request_shaping_enabled: bool,
    /// Artificial Analysis API key for benchmark sync.
    #[serde(default)]
    pub artificial_analysis_api_key: Option<String>,
    #[serde(default)]
    pub providers: HashMap<String, ProviderUserSettings>,
    #[serde(default)]
    pub models: HashMap<String, ModelUserSettings>,
}

/// Partial settings update — gateway fields only. Provider/model overrides use dedicated APIs.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateSettings {
    pub gateway_port: Option<i64>,
    pub log_retention_days: Option<i64>,
    pub gateway_key: Option<String>,
    pub auth_enabled: Option<bool>,
    pub cache_affinity_enabled: Option<bool>,
    pub cache_request_shaping_enabled: Option<bool>,
    /// Outer None = field omitted; inner None = clear the key.
    pub artificial_analysis_api_key: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedState {
    pub version: u32,
    pub agents: HashMap<String, Agent>,
    pub routes: HashMap<String, Route>,
}

// ──────────────────────────── Agent ────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub mode: String, // "native", "auto", "manual" (legacy: "config")
    pub model_id: Option<String>,
    pub api_key: String,
    pub endpoint: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateAgent {
    pub mode: Option<String>,
    pub model_id: Option<Option<String>>,
    pub api_key: Option<String>,
    pub endpoint: Option<String>,
}

// ──────────────────────────── Route Explain ────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteExplainRequest {
    pub agent: String,
    pub model: Option<String>,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionStep {
    pub step: String,
    pub matched: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedSummary {
    pub model_id: String,
    pub provider_id: String,
    pub strategy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedModelSummary {
    pub model_id: String,
    pub provider_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    /// True when value is unbounded (+∞) because catalog price is known to be free.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub value_unbounded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteExplainResult {
    pub resolved: Option<ResolvedSummary>,
    pub decision_steps: Vec<DecisionStep>,
    pub ranked_candidates: Vec<RankedModelSummary>,
}

/// Reference profile used when ranking strategy boards on the routes page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyBoardRequest {
    pub agent: String,
    #[serde(default)]
    pub body: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyBoardStrategy {
    pub id: String,
    /// Actual strategy used after fallbacks (e.g. speed → cheapest).
    pub display_strategy: String,
    pub task: String,
    pub complexity: f64,
    pub candidates: Vec<RankedModelSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyBoardResult {
    pub strategies: Vec<StrategyBoardStrategy>,
}

/// Normalize protocol usage into CAB's storage convention.
///
/// Fields:
/// - `input` = prompt tokens **not read from cache**
/// - `output` = completion tokens
/// - `cache_read` = prefix-cache **hit** (read from cache)
/// - `cache_creation` = prefix-cache **write** (written into cache this turn)
///
/// Important: cache **write** is a billing leg, not always a disjoint prompt slice.
/// - Anthropic (`reported_input_includes_cache = false`): `input`, `cache_read`,
///   and `cache_creation` partition the prompt →
///   `total = input + cache_read + cache_creation + output`.
/// - OpenAI Chat/Responses (`true`): wire `prompt`/`input` already includes
///   cache reads; `cache_write_tokens` are usually an overlay on the non-read
///   portion (still inside `prompt`). Only subtract `cache_read` →
///   `total = input + cache_read + output` (== wire prompt + output).
pub fn normalize_stored_tokens(
    reported_input: i64,
    reported_output: i64,
    cache_read: i64,
    cache_creation: i64,
    reported_input_includes_cache: bool,
) -> NormalizedTokens {
    let cache_read = cache_read.max(0);
    let cache_creation = cache_creation.max(0);
    let output_tokens = reported_output.max(0);
    let reported_input = reported_input.max(0);

    if reported_input_includes_cache {
        let input_tokens = (reported_input - cache_read).max(0);
        NormalizedTokens {
            input_tokens,
            output_tokens,
            cache_read_tokens: cache_read,
            cache_creation_tokens: cache_creation,
            total_tokens: input_tokens + cache_read + output_tokens,
        }
    } else {
        let input_tokens = reported_input;
        NormalizedTokens {
            input_tokens,
            output_tokens,
            cache_read_tokens: cache_read,
            cache_creation_tokens: cache_creation,
            total_tokens: input_tokens + cache_read + cache_creation + output_tokens,
        }
    }
}

/// Detect Anthropic-format `usage` whose `input_tokens` already includes the
/// cache-hit portion.
///
/// Spec-compliant Anthropic providers report `input`, `cache_read` and
/// `cache_creation` as disjoint legs. Some relays (e.g. OpenAI-style backends
/// behind `/v1/messages`) violate this and report the total prompt as
/// `input_tokens` while also emitting `cache_read_input_tokens` — summing them
/// then double-counts the cache read.
///
/// The signature of the inclusive layout is: cache legs non-zero and `input`
/// at least as large as them. When `input < cache_read` the input cannot
/// contain the whole cache read, so the layout must already be disjoint.
pub fn anthropic_input_includes_cache(
    reported_input: i64,
    cache_read: i64,
    cache_creation: i64,
) -> bool {
    if cache_read <= 0 && cache_creation <= 0 {
        return false;
    }
    reported_input >= cache_read + cache_creation
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NormalizedTokens {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub total_tokens: i64,
}

#[cfg(test)]
mod token_normalize_tests {
    use super::{anthropic_input_includes_cache, normalize_stored_tokens};

    #[test]
    fn openai_only_strips_cache_read_from_prompt() {
        // prompt=100 includes read=40; write=10 is overlay inside the other 60.
        let n = normalize_stored_tokens(100, 5, 40, 10, true);
        assert_eq!(n.input_tokens, 60);
        assert_eq!(n.cache_read_tokens, 40);
        assert_eq!(n.cache_creation_tokens, 10);
        assert_eq!(n.output_tokens, 5);
        assert_eq!(n.total_tokens, 105); // prompt + output; write not added again
    }

    #[test]
    fn anthropic_exclusive_parts_sum_into_total() {
        let n = normalize_stored_tokens(25, 12, 42, 9, false);
        assert_eq!(n.input_tokens, 25);
        assert_eq!(n.cache_read_tokens, 42);
        assert_eq!(n.cache_creation_tokens, 9);
        assert_eq!(n.total_tokens, 25 + 42 + 9 + 12);
    }

    #[test]
    fn anthropic_inclusive_input_is_recognized() {
        // Relay reports total prompt as input while cache read is separate.
        assert!(anthropic_input_includes_cache(29047, 28928, 0));
        assert!(anthropic_input_includes_cache(100, 40, 10));
    }

    #[test]
    fn disjoint_anthropic_input_is_not_inclusive() {
        // LongCat-style: input is only the non-cached leg.
        assert!(!anthropic_input_includes_cache(1217, 79232, 0));
        // No cache legs at all.
        assert!(!anthropic_input_includes_cache(100, 0, 0));
    }

    #[test]
    fn inclusive_anthropic_input_normalizes_without_double_count() {
        let n = normalize_stored_tokens(29047, 64, 28928, 0, true);
        assert_eq!(n.input_tokens, 119);
        assert_eq!(n.cache_read_tokens, 28928);
        assert_eq!(n.total_tokens, 29111); // 119 + 28928 + 64
    }
}
