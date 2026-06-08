/* ═══════════════════════════════════════════════════════════════
   CAB — TypeScript Types
   Mirrors the Rust backend API types exactly
   ═══════════════════════════════════════════════════════════════ */

export interface ApiKeyConfig {
  key: string;
  enabled: boolean;
  /** Subscription key: fixed cost already paid; routing favors near-zero marginal cost. */
  subscribed?: boolean;
  /** RFC3339 timestamp when a 429 quota window ends. */
  quota_reset_at?: string | null;
}

export interface ProviderEndpoint {
  id: string;
  protocol: 'openai-chat' | 'anthropic' | 'openai-responses' | 'gemini';
  url: string;
  label: string | null;
  priority: number;
  enabled: boolean;
}

// ── Provider ──────────────────────────────────────────────────
export interface Provider {
  id: string;
  name: string;
  endpoints: ProviderEndpoint[];
  api_key: string;
  enabled: boolean;
  created_at: string;
  updated_at: string;
  privacy_policy_url?: string;
  terms_of_service_url?: string;
  status_page_url?: string;
  headquarters?: string;
  datacenters?: string[];
  api_keys: ApiKeyConfig[];
  api?: string | null;
  doc?: string | null;
  env?: string[] | null;
  npm?: string | null;
  model_count: number;
  catalog_models?: string[];
}

export interface UpdateProvider {
  name?: string;
  endpoints?: ProviderEndpoint[];
  api_key?: string;
  enabled?: boolean;
  privacy_policy_url?: string;
  terms_of_service_url?: string;
  status_page_url?: string;
  headquarters?: string;
  datacenters?: string[];
  api_keys?: ApiKeyConfig[];
  api?: string | null;
  doc?: string | null;
  env?: string[] | null;
  npm?: string | null;
  model_count?: number;
}

// ── Model ─────────────────────────────────────────────────────
export interface Model {
  id: string;
  name: string;
  display_name: string;
  provider_id: string;
  provider_name?: string;
  protocol: string; // 'openai' or 'anthropic'
  context_length: number;
  input_cost?: number;
  output_cost?: number;
  enabled: boolean;
  overall_intelligence: number;
  coding_index: number;
  agentic_index: number;
  math_index: number;
  created_at: string;
  updated_at: string;
  // Catalog metadata
  canonical_slug?: string;
  hugging_face_id?: string | null;
  created?: number;
  description?: string;
  architecture?: any;
  pricing?: any;
  top_provider?: any;
  per_request_limits?: any;
  supported_parameters?: string[];
  default_parameters?: any;
  supported_voices?: string[] | null;
  knowledge_cutoff?: string | null;
  expiration_date?: string | null;
  links?: any;
}

export interface ModelEndpoint {
  id: string;
  model_id: string;
  canonical_slug: string;
  provider_name: string;
  provider_tag: string;
  native_model_id: string;
  quantization: string;
  input_cost: number;
  output_cost: number;
  cache_read_cost?: number;
  context_length: number;
  max_completion_tokens?: number;
  status: number; // 0 = ok, negative = degraded
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

// ── Route ─────────────────────────────────────────────────────
export interface Route {
  id: string;
  name: string;
  agent_pattern: string;
  primary_model_id: string;
  primary_model_name?: string;
  fallback_model_ids: string[];
  fallback_model_names?: string[];
  priority: number;
  /** one of: auto | cheapest | balanced | intelligent */
  routing_strategy: string;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface CreateRoute {
  name: string;
  agent_pattern: string;
  primary_model_id: string;
  fallback_model_ids?: string[];
  priority?: number;
  routing_strategy?: string;
  enabled?: boolean;
}

export interface UpdateRoute {
  name?: string;
  agent_pattern?: string;
  primary_model_id?: string;
  fallback_model_ids?: string[];
  priority?: number;
  routing_strategy?: string;
  enabled?: boolean;
}

// ── Request Log ───────────────────────────────────────────────
export interface RequestLog {
  id: string;
  timestamp: string;
  agent: string;
  provider: string;
  model: string;
  input_tokens: number;
  output_tokens: number;
  total_tokens: number;
  latency_ms: number;
  status_code: number;
  error_message?: string;
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
  data: RequestLog[];
  total: number;
  page: number;
  per_page: number;
  total_pages: number;
}

// ── Dashboard ─────────────────────────────────────────────────
export interface DashboardStats {
  total_requests: number;
  total_tokens: number;
  active_providers: number;
  active_models: number;
  requests_by_provider: Record<string, number>;
  requests_by_model: Record<string, number>;
  recent_requests: RequestLog[];
}

// ── Settings ──────────────────────────────────────────────────
export interface ProviderUserSettings {
  enabled?: boolean;
  api_key?: string;
  api_keys?: ApiKeyConfig[];
  endpoints?: ProviderEndpoint[];
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

export interface BenchmarkModelRecord {
  id: string;
  slug: string;
  name: string;
  creator_slug?: string | null;
  creator_name?: string | null;
  evaluations: BenchmarkEvaluations;
}

/** Three-source model view: models.dev + AA + settings.json */
export interface ModelCatalogEntry {
  id: string;
  catalog_id: string;
  enabled: boolean;
  models_dev: Record<string, unknown>;
  artificial_analysis: BenchmarkModelRecord | null;
  settings: ModelUserSettings;
}

export interface Settings {
  gateway_port: number;
  log_retention_days: number;
  gateway_status?: 'running' | 'stopped' | 'error';
  gateway_key: string;
  artificial_analysis_api_key?: string | null;
  providers?: Record<string, ProviderUserSettings>;
  models?: Record<string, ModelUserSettings>;
}

export interface UpdateSettings {
  gateway_port?: number;
  log_retention_days?: number;
  gateway_key?: string;
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
  synced_at: string | null;
  providers?: number | null;
  models?: number | null;
}

export interface CatalogStatusResponse {
  sources: CatalogSourceStatus[];
}

export interface SyncCatalogResponse {
  success: boolean;
  applied_models: number;
  providers: number;
  sources: CatalogSourceStatus[];
}

// ── Agent ─────────────────────────────────────────────────────
export interface Agent {
  id: string;
  name: string;
  mode: 'native' | 'auto' | 'manual' | 'config';
  model_id: string | null;
  model_name?: string; // Client-side mapped helper
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

// ── Column config for DataTable ───────────────────────────────
export interface Column<T = any> {
  key: string;
  label: string;
  sortable?: boolean;
  width?: string;
  align?: 'left' | 'center' | 'right';
  render?: (value: any, row: T) => string;
}

// ── Toast ─────────────────────────────────────────────────────
export type ToastType = 'success' | 'error' | 'warning' | 'info';

export interface Toast {
  id: string;
  type: ToastType;
  message: string;
  duration?: number;
}
