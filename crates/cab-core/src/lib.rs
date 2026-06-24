pub mod benchmark_catalog;
pub mod config;
pub mod error;
pub mod health;
pub mod model_scores;
pub mod provider_defaults;
pub mod routing;
pub mod subscription_quota;
pub mod types;

pub use benchmark_catalog::{
    AaModelMapFile, BenchmarkCatalog, BenchmarkCatalogFile, CatalogSourceStatus,
    ModelsDevCatalogFile, aa_model_map_path, aa_model_map_status,
    artificial_analysis_catalog_status, artificial_analysis_models_path, catalog_root_dir,
    embedded_aa_model_map, ensure_aa_model_map_file, load_aa_model_map,
    load_artificial_analysis_catalog, load_models_dev_catalog_file, models_dev_catalog_path,
    models_dev_catalog_status, models_dev_catalog_url, models_dev_lab_logo_url,
    models_dev_provider_logo_url, refresh_aa_model_map_exact_matches,
    resolve_artificial_analysis_api_key, resolve_intelligence_indices, resolve_performance_metrics,
    save_aa_model_map,
};
pub use config::CabConfig;
pub use error::CabError;
pub use health::HealthTracker;
pub use model_scores::{
    ModelIntelligenceIndices, capability_indices_missing, infer_intelligence_indices,
    normalize_legacy_missing_indices,
};
pub use provider_defaults::{
    ProviderDefaultsCatalog, load_provider_defaults, resolve_provider_default_protocol,
    resolve_provider_endpoints,
};
pub use routing::{
    BALANCED_INPUT_OUTPUT_RATIO, INPUT_CACHE_HIT_RATE, RankedModelScore, RankedRouteCandidate,
    RequestProfile, RouteCandidate, RoutingStrategy, TaskKind, blended_input_cost,
    build_request_profile, cache_read_cost_from_model, capability_value_score,
    effective_token_cost, effective_token_cost_for_model, model_routable_for_strategy, rank_models,
    rank_models_with_scores, rank_route_candidates, rank_route_candidates_with_scores,
    raw_effective_token_cost, raw_effective_token_cost_for_model,
};
pub use subscription_quota::{
    DEFAULT_QUOTA_RESET_SECS, extract_retry_after, is_key_rate_limited, resolve_quota_reset_at,
};
pub use types::{
    ordered_api_keys, provider_has_available_key, provider_has_configured_key,
    select_preferred_api_key,
};
