pub mod agents;
pub mod benchmarks;
pub mod catalog_provider_urls;
pub mod dashboard;
pub mod diagnostics;
pub mod logs;
pub mod models;
pub mod providers;
pub mod routes;
pub mod routing;
pub mod settings;
pub mod update;
pub mod usage;
use axum::Router;
use axum::extract::{ConnectInfo, Request};
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use cab_db::InMemoryStore;
use std::net::SocketAddr;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

/// Origins that belong to the local dashboard (browser dev server, the bundled
/// UI served on the gateway port, and the Tauri shell).
fn is_trusted_local_origin(val: &str) -> bool {
    val.starts_with("http://localhost:")
        || val.starts_with("http://127.0.0.1:")
        || val.starts_with("http://[::1]:")
        || val.starts_with("tauri://")
        || val.starts_with("http://tauri.")
}

/// Whether the connection itself originates from the loopback interface.
///
/// `ConnectInfo` is injected by `into_make_service_with_connect_info`. When it
/// is absent (e.g. some test harnesses) we fail closed and require a token.
fn peer_is_loopback(request: &Request) -> bool {
    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip().is_loopback())
        .unwrap_or(false)
}

async fn api_auth_middleware(
    axum::extract::State(state): axum::extract::State<ApiState>,
    request: Request,
    next: Next,
) -> Response {
    let origin = request
        .headers()
        .get("origin")
        .and_then(|v| v.to_str().ok());
    let referer = request
        .headers()
        .get("referer")
        .and_then(|v| v.to_str().ok());

    let has_trusted_origin = origin.map(is_trusted_local_origin).unwrap_or(false)
        || referer.map(is_trusted_local_origin).unwrap_or(false);

    // The dashboard runs in a browser and cannot attach the gateway key, so we
    // allow it through on a trusted same-host origin. `Origin`/`Referer` are
    // trivially spoofable by non-browser clients, so the bypass is additionally
    // gated on the TCP peer being loopback. This keeps a `host = "0.0.0.0"` /
    // LAN deployment from exposing the management API (and its secrets)
    // unauthenticated to remote callers.
    let bypass = has_trusted_origin && peer_is_loopback(&request);

    if !bypass
        && let Err(err) = cab_db::auth::verify(
            &state.pool,
            request
                .headers()
                .get("authorization")
                .and_then(|v| v.to_str().ok()),
        )
        .await
    {
        return err.into_response();
    }
    next.run(request).await
}

#[cfg(test)]
pub(crate) static TEST_HOME_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
pub(crate) struct TestHome {
    _dir: tempfile::TempDir,
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

#[cfg(test)]
impl TestHome {
    pub(crate) async fn new() -> Self {
        let lock = TEST_HOME_LOCK.lock().await;
        let dir = tempfile::tempdir().expect("tempdir");
        unsafe {
            std::env::set_var("HOME", dir.path());
            std::env::remove_var("USERPROFILE");
        }
        Self {
            _dir: dir,
            _lock: lock,
        }
    }
}

/// Shared state for all management API handlers.
#[derive(Clone)]
pub struct ApiState {
    pub pool: InMemoryStore,
}

/// Build the complete management API router mounted at `/api`.
pub fn api_router(pool: InMemoryStore) -> Router {
    let state = ApiState { pool };

    // Only reflect trusted local dashboard origins instead of `*`, so a hostile
    // web page cannot read management API responses cross-origin from a browser.
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin: &HeaderValue, _| {
            origin
                .to_str()
                .map(is_trusted_local_origin)
                .unwrap_or(false)
        }))
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
        .route("/api/models/routable", get(models::list_routable_models))
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
        .route("/api/routing/explain", post(routing::explain_routing))
        .route("/api/routing/strategy-board", post(routing::strategy_board))
        .route(
            "/api/diagnostics/tool-weights",
            get(diagnostics::tool_weights),
        )
        // Logs
        .route("/api/logs", get(logs::query_logs).delete(logs::delete_logs))
        // Coding Agents
        .route("/api/agents", get(agents::list_agents))
        .route("/api/agents/{id}", get(agents::get_agent))
        .route("/api/agents/{id}", put(agents::update_agent))
        // Dashboard
        .route("/api/dashboard/stats", get(dashboard::get_stats))
        // Usage
        .route("/api/usage/summary", get(usage::get_summary))
        .route("/api/usage/records", get(usage::get_records))
        // Settings
        .route("/api/settings", get(settings::get_settings))
        .route("/api/settings", put(settings::update_settings))
        .route("/api/logos/{*path}", get(settings::get_logo_svg))
        .route(
            "/api/settings/catalog-status",
            get(settings::get_catalog_status),
        )
        .route("/api/settings/sync-catalog", post(settings::sync_catalog))
        // Updates
        .route("/api/update/check", get(update::check_update))
        .route("/api/update/install", post(update::install_update))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            api_auth_middleware,
        ))
        .layer(cors)
        .with_state(state)
}
