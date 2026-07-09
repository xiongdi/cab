// ═══════════════════════════════════════════════════════════════
// GENERATED CODE — DO NOT EDIT MANUALLY
// Source: spec/src/content/docs/modules/openapi.yaml
// Run `npm run generate-types` to regenerate
// ═══════════════════════════════════════════════════════════════

export interface ApiKeyConfig {
  key: string;
  enabled: boolean;
  /** RFC3339 timestamp when a 429 quota window ends. */
  quota_reset_at?: string | null;
}

export interface ProviderEndpoint {
  id: string;
  protocol: 'openai-chat' | 'anthropic' | 'openai-responses';
  url: string;
  label?: string | null;
  priority: number;
  enabled: boolean;
}

export interface Provider {
  id: string;
  name: string;
  endpoints: Array<ProviderEndpoint>;
  api_key: string;
  enabled: boolean;
  created_at: string;
  updated_at: string;
  privacy_policy_url?: string;
  terms_of_service_url?: string;
  status_page_url?: string;
  headquarters?: string;
  datacenters?: Array<string>;
  api_keys: Array<ApiKeyConfig>;
  api?: string | null;
  doc?: string | null;
  env?: Array<string> | null;
  npm?: string | null;
  model_count: number;
  logo?: string | null;
  catalog_models?: Array<string>;
}

export interface UpdateProvider {
  name?: string;
  endpoints?: Array<ProviderEndpoint>;
  api_key?: string;
  enabled?: boolean;
  privacy_policy_url?: string;
  terms_of_service_url?: string;
  status_page_url?: string;
  headquarters?: string;
  datacenters?: Array<string>;
  api_keys?: Array<ApiKeyConfig>;
  api?: string | null;
  doc?: string | null;
  env?: Array<string> | null;
  npm?: string | null;
  model_count?: number;
  logo?: string | null;
}

export interface Model {
  id: string;
  name: string;
  display_name: string;
  provider_id: string;
  provider_name?: string;
  protocol: string;
  context_length: number;
  input_cost?: number | null;
  output_cost?: number | null;
  enabled: boolean;
  overall_intelligence?: number | null;
  coding_index?: number | null;
  agentic_index?: number | null;
  math_index?: number | null;
  output_speed_tps?: number | null;
  time_to_first_token_secs?: number | null;
  created_at: string;
  updated_at: string;
  canonical_slug?: string;
  hugging_face_id?: string | null;
  created?: number;
  description?: string;
  architecture?: Record<string, unknown>;
  pricing?: Record<string, unknown>;
  top_provider?: Record<string, unknown>;
  per_request_limits?: Record<string, unknown>;
  supported_parameters?: Array<string>;
  default_parameters?: Record<string, unknown>;
  supported_voices?: Array<string> | null;
  knowledge_cutoff?: string | null;
  expiration_date?: string | null;
  links?: Record<string, unknown>;
}

export interface RoutableModel {
  id: string;
  name: string;
  display_name: string;
  provider_id: string;
  provider_name?: string;
  protocol: string;
  context_length: number;
  input_cost?: number | null;
  output_cost?: number | null;
  enabled: boolean;
  overall_intelligence?: number | null;
  coding_index?: number | null;
  agentic_index?: number | null;
  math_index?: number | null;
  output_speed_tps?: number | null;
  time_to_first_token_secs?: number | null;
  created_at: string;
  updated_at: string;
  canonical_slug?: string;
  hugging_face_id?: string | null;
  created?: number;
  description?: string;
  architecture?: Record<string, unknown>;
  pricing?: Record<string, unknown>;
  top_provider?: Record<string, unknown>;
  per_request_limits?: Record<string, unknown>;
  supported_parameters?: Array<string>;
  default_parameters?: Record<string, unknown>;
  supported_voices?: Array<string> | null;
  knowledge_cutoff?: string | null;
  expiration_date?: string | null;
  links?: Record<string, unknown>;
  service_provider_id?: string;
  endpoint_input_cost?: number | null;
  endpoint_output_cost?: number | null;
  endpoint_cache_read_cost?: number | null;
}

export interface ModelEndpoint {
  id: string;
  model_id: string;
  canonical_slug: string;
  provider_name: string;
  provider_tag: string;
  native_model_id: string;
  quantization: string;
  input_cost?: number | null;
  output_cost?: number | null;
  cache_read_cost?: number | null;
  context_length?: number | null;
  max_completion_tokens?: number;
  /** 0 = ok, negative = degraded */
  status: number;
  uptime_30m?: number;
  uptime_5m?: number;
  uptime_1d?: number;
  supports_tools: boolean;
  supports_streaming: boolean;
  enabled: boolean;
  updated_at: string;
}

export interface UpdateModel {
  enabled?: boolean;
}

export interface Route {
  id: string;
  name: string;
  agent_pattern: string;
  primary_model_id: string;
  primary_model_name?: string;
  fallback_model_ids: Array<string>;
  fallback_model_names?: Array<string>;
  priority: number;
  /** one of: auto | cheapest | balanced | intelligent | speed | agentic */
  routing_strategy: string;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface CreateRoute {
  name: string;
  agent_pattern: string;
  primary_model_id: string;
  fallback_model_ids?: Array<string>;
  priority?: number;
  routing_strategy?: string;
  enabled?: boolean;
}

export interface UpdateRoute {
  name?: string;
  agent_pattern?: string;
  primary_model_id?: string;
  fallback_model_ids?: Array<string>;
  priority?: number;
  routing_strategy?: string;
  enabled?: boolean;
}

export interface RequestLog {
  id: string;
  timestamp: string;
  agent: string;
  provider: string;
  model: string;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  cache_read_tokens?: number;
  cache_creation_tokens?: number;
  latency_ms: number;
  status_code: number;
  error_message?: string;
  request_body?: string;
  response_body?: string;
}

export interface LogFilter {
  agent?: string;
  provider?: string;
  model?: string;
  status?: string;
  search?: string;
  page?: number;
  per_page?: number;
}

export interface PaginatedLogs {
  data: Array<RequestLog>;
  total: number;
  page: number;
  per_page: number;
  total_pages: number;
}

export interface DashboardStats {
  total_requests: number;
  total_tokens: number;
  active_providers: number;
  active_models: number;
  requests_by_provider: Record<string, number>;
  requests_by_model: Record<string, number>;
  recent_requests: Array<RequestLog>;
}

export interface ProviderUserSettings {
  enabled?: boolean;
  api_key?: string;
  api_keys?: Array<ApiKeyConfig>;
  endpoints?: Array<ProviderEndpoint>;
  logo?: string | null;
}

export interface ModelUserSettings {
  enabled?: boolean;
}

export interface BenchmarkEvaluations {
  artificial_analysis_intelligence_index?: number | null;
  artificial_analysis_coding_index?: number | null;
  artificial_analysis_math_index?: number | null;
  tau2?: number | null;
  terminalbench_hard?: number | null;
  livecodebench?: number | null;
  scicode?: number | null;
  gpqa?: number | null;
  hle?: number | null;
}

export interface BenchmarkPerformance {
  median_output_tokens_per_second?: number | null;
  median_time_to_first_token_seconds?: number | null;
  median_time_to_first_answer_token?: number | null;
}

export interface BenchmarkModelRecord {
  id: string;
  slug: string;
  name: string;
  creator_slug?: string | null;
  creator_name?: string | null;
  evaluations: BenchmarkEvaluations;
  performance?: BenchmarkPerformance;
}

export interface ModelCatalogEntry {
  id: string;
  catalog_id: string;
  enabled: boolean;
  models_dev: Record<string, unknown>;
  artificial_analysis: BenchmarkModelRecord;
  settings: ModelUserSettings;
}

export interface ToolSchemaCost {
  name: string;
  tokens: number;
}

export interface ToolWeightSnapshot {
  agent: string;
  captured_at_ms: number;
  total_tokens: number;
  tool_count: number;
  tools: ToolSchemaCost[];
}

export interface Settings {
  gateway_port: number;
  log_retention_days: number;
  gateway_status?: 'running' | 'stopped' | 'error';
  gateway_key: string;
  auth_enabled?: boolean;
  cache_affinity_enabled?: boolean;
  cache_request_shaping_enabled?: boolean;
  artificial_analysis_api_key?: string | null;
  providers?: Record<string, ProviderUserSettings>;
  models?: Record<string, ModelUserSettings>;
}

export interface UpdateSettings {
  gateway_port?: number;
  log_retention_days?: number;
  gateway_key?: string;
  auth_enabled?: boolean;
  cache_affinity_enabled?: boolean;
  cache_request_shaping_enabled?: boolean;
  artificial_analysis_api_key?: string | null;
  providers?: Record<string, ProviderUserSettings>;
  models?: Record<string, ModelUserSettings>;
}

export interface CatalogSourceStatus {
  id: string;
  name: string;
  url: string;
  cache_path: string;
  available: boolean;
  synced_at?: string | null;
  providers?: number | null;
  models?: number | null;
}

export interface CatalogStatusResponse {
  sources: Array<CatalogSourceStatus>;
}

export interface SyncCatalogResponse {
  success: boolean;
  applied_models: number;
  providers: number;
  sources: Array<CatalogSourceStatus>;
}

export interface Agent {
  id: string;
  name: string;
  mode: 'native' | 'auto' | 'manual' | 'config';
  model_id: string | null;
  /** Client-side mapped helper */
  model_name?: string;
  api_key: string;
  endpoint: string;
  updated_at: string;
}

export interface UpdateAgent {
  mode?: 'native' | 'auto' | 'manual' | 'config';
  model_id?: string | null;
  api_key?: string;
  endpoint?: string;
}

export interface RouteExplainRequest {
  agent: string;
  model?: string | null;
  body?: Record<string, unknown> | null;
}

export interface DecisionStep {
  step: string;
  matched: boolean;
  detail: string;
}

export interface ResolvedSummary {
  model_id: string;
  provider_id: string;
  strategy?: string | null;
}

export interface RankedModelSummary {
  model_id: string;
  provider_id: string;
  /** Absent when AA benchmark data is not available (not the same as 0). */
  capability?: number | null;
  value?: number | null;
  /** True when value is +infinity (known $0 catalog price). */
  value_unbounded?: boolean;
}

export interface RouteExplainResult {
  resolved?: ResolvedSummary;
  decision_steps: Array<DecisionStep>;
  ranked_candidates: Array<RankedModelSummary>;
}

export interface StrategyBoardRequest {
  agent: string;
  body?: Record<string, unknown> | null;
}

export interface StrategyBoardStrategy {
  id: string;
  display_strategy: string;
  task: string;
  complexity: number;
  candidates: Array<RankedModelSummary>;
}

export interface StrategyBoardResult {
  strategies: Array<StrategyBoardStrategy>;
}

export interface UsageRecord {
  id: string;
  timestamp: string;
  provider_id: string;
  model_id: string;
  service_provider_id: string;
  agent_id: string;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_creation_tokens: number;
  cost_usd: number;
  subscription: boolean;
  request_id?: string | null;
}

export interface ProviderUsageSummary {
  requests: number;
  input_tokens: number;
  output_tokens: number;
  cost_usd: number;
}

export interface ModelUsageSummary {
  requests: number;
  input_tokens: number;
  output_tokens: number;
  cost_usd: number;
}

export interface AgentUsageSummary {
  requests: number;
  input_tokens: number;
  output_tokens: number;
  cost_usd: number;
}

export interface UsageSummary {
  total_requests: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cost_usd: number;
  by_provider: Record<string, ProviderUsageSummary>;
  by_model: Record<string, ModelUsageSummary>;
  by_agent: Record<string, AgentUsageSummary>;
}

export interface CheckUpdateResponse {
  available: boolean;
  current_version: string;
  latest_version: string;
  release_notes: string;
  download_url: string | null;
  published_at: string | null;
}

export interface InstallUpdateResponse {
  success: boolean;
  message: string;
}
