#![allow(clippy::all, dead_code)]
pub mod agents;
pub mod benchmarks;
pub mod catalog_provider_urls;
pub mod dashboard;
pub mod logs;
pub mod models;
pub mod providers;
pub mod routes;
pub mod settings;
pub mod traffic_hook;

use axum::Router;
use axum::routing::{delete, get, post, put};
use cab_db::InMemoryStore;
use tower_http::cors::{Any, CorsLayer};

/// Shared state for all management API handlers.
#[derive(Clone)]
pub struct ApiState {
    pub pool: InMemoryStore,
}

/// Build the complete management API router mounted at `/api`.
pub fn api_router(pool: InMemoryStore) -> Router {
    let state = ApiState { pool };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // Providers
        .route("/api/providers", get(providers::list_providers))
        .route("/api/providers", post(providers::create_provider))
        .route(
            "/api/providers/sync",
            post(providers::sync_models_dev_providers),
        )
        .route(
            "/api/providers/endpoint-summary",
            get(providers::list_endpoint_provider_summary),
        )
        .route(
            "/api/providers/endpoint-summary/{provider_name}",
            put(providers::update_endpoint_provider_status),
        )
        .route("/api/providers/{id}", get(providers::get_provider))
        .route("/api/providers/{id}", put(providers::update_provider))
        .route("/api/providers/{id}", delete(providers::delete_provider))
        .route(
            "/api/providers/{id}/sync",
            post(providers::sync_provider_models),
        )
        .route(
            "/api/providers/{id}/balance",
            get(providers::get_provider_balance),
        )
        // Models
        .route("/api/models", get(models::list_models))
        .route("/api/models/catalog", get(models::list_model_catalog))
        .route("/api/models", post(models::create_model))
        .route("/api/models/{id}", get(models::get_model))
        .route("/api/models/{id}", put(models::update_model))
        .route("/api/models/{id}", delete(models::delete_model))
        .route(
            "/api/models/{id}/endpoints",
            get(models::list_model_endpoints),
        )
        .route("/api/model-endpoints", put(models::update_model_endpoint))
        // Routes
        .route("/api/routes", get(routes::list_routes))
        .route("/api/routes", post(routes::create_route))
        .route("/api/routes/{id}", get(routes::get_route))
        .route("/api/routes/{id}", put(routes::update_route))
        .route("/api/routes/{id}", delete(routes::delete_route))
        // Logs
        .route("/api/logs", get(logs::query_logs))
        // Coding Agents
        .route("/api/agents", get(agents::list_agents))
        .route(
            "/api/agents/hijack-claude",
            post(agents::hijack_claude_desktop),
        )
        .route(
            "/api/agents/{id}/install-proxy",
            post(agents::install_agent_proxy),
        )
        .route("/api/agents/{id}", get(agents::get_agent))
        .route("/api/agents/{id}", put(agents::update_agent))
        // Dashboard
        .route("/api/dashboard/stats", get(dashboard::get_stats))
        // Settings
        .route("/api/settings", get(settings::get_settings))
        .route("/api/settings", put(settings::update_settings))
        .route(
            "/api/settings/catalog-status",
            get(settings::get_catalog_status),
        )
        .route("/api/settings/sync-catalog", post(settings::sync_catalog))
        .layer(cors)
        .with_state(state)
}
