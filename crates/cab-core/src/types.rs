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
    /// Subscription key: fixed cost already paid; routing treats marginal cost as near-zero.
    #[serde(default)]
    pub subscribed: bool,
    /// RFC3339 timestamp when a 429 quota window ends; key is skipped until then.
    #[serde(default)]
    pub quota_reset_at: Option<String>,
}

impl ApiKeyConfig {
    pub fn is_usable(&self) -> bool {
        self.enabled && !self.key.trim().is_empty()
    }
}

/// True when the provider has at least one enabled subscribed key not in quota recovery.
pub fn provider_has_subscribed_key(api_keys: &[ApiKeyConfig]) -> bool {
    api_keys.iter().any(|k| {
        k.is_usable() && k.subscribed && !crate::subscription_quota::is_key_rate_limited(k)
    })
}

/// Prefer subscribed keys, then any other enabled key; skip keys still rate-limited.
pub fn select_preferred_api_key(api_keys: &[ApiKeyConfig]) -> Option<String> {
    ordered_api_keys(api_keys).into_iter().next()
}

/// Keys to try in order: subscribed (available) → pay-as-you-go (available).
pub fn ordered_api_keys(api_keys: &[ApiKeyConfig]) -> Vec<String> {
    let mut keys = Vec::new();
    for key in api_keys {
        if key.is_usable() && key.subscribed && !crate::subscription_quota::is_key_rate_limited(key)
        {
            keys.push(key.key.clone());
        }
    }
    for key in api_keys {
        if key.is_usable()
            && !key.subscribed
            && !crate::subscription_quota::is_key_rate_limited(key)
        {
            keys.push(key.key.clone());
        }
    }
    keys
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub overall_intelligence: f64,
    pub coding_index: f64,
    pub agentic_index: f64,
    #[serde(default = "default_math_index")]
    pub math_index: f64,
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

fn default_math_index() -> f64 {
    30.0
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
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
    pub latency_ms: i64,
    #[serde(rename = "status_code")]
    pub status: i32,
    #[serde(rename = "error_message")]
    pub error: Option<String>,
    pub path: String,
    pub stream: bool,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelUserSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

fn default_auth_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub gateway_port: i64,
    pub log_retention_days: i64,
    pub gateway_key: String,
    #[serde(default = "default_auth_enabled")]
    pub auth_enabled: bool,
    /// Artificial Analysis API key for benchmark sync.
    #[serde(default)]
    pub artificial_analysis_api_key: Option<String>,
    #[serde(default)]
    pub providers: HashMap<String, ProviderUserSettings>,
    #[serde(default)]
    pub models: HashMap<String, ModelUserSettings>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub capability: f64,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteExplainResult {
    pub resolved: Option<ResolvedSummary>,
    pub decision_steps: Vec<DecisionStep>,
    pub ranked_candidates: Vec<RankedModelSummary>,
}
