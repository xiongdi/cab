//! CAB combined HTTP application (gateway + management API).

use std::path::PathBuf;

use axum::Router;
use cab_api::api_router;
use cab_db::InMemoryStore;
use cab_gateway::{GatewayState, gateway_router};

#[cfg(windows)]
pub mod windows_service;

/// Build the same router stack used in production (without static frontend assets).
pub fn build_combined_router(store: InMemoryStore) -> Router {
    let gateway = gateway_router(GatewayState::new(store.clone()));
    let api = api_router(store);
    gateway.merge(api)
}

async fn log_request(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    tracing::info!("Incoming request: {} {}", method, uri);
    next.run(req).await
}

/// Bind and serve the gateway + API (+ optional static UI) until stopped.
pub async fn run_server() -> Result<(), Box<dyn std::error::Error>> {
    let config = cab_core::CabConfig::load();

    let store = cab_db::init_store()
        .await
        .expect("Failed to initialize store");

    let store_clone = store.clone();
    tokio::spawn(async move {
        tracing::info!("Startup: Synchronizing models.dev provider/catalog...");
        match cab_services::catalog::sync_on_startup(&store_clone).await {
            Ok(count) => {
                tracing::info!(
                    "Startup: models.dev catalog synchronization finished. Synced {} models.",
                    count
                );
            }
            Err(e) => {
                tracing::error!(
                    "Startup: models.dev catalog synchronization failed: {:?}",
                    e
                );
            }
        }
    });

    let core = build_combined_router(store.clone());

    let app = if let Some(frontend_dir) = resolve_frontend_dir() {
        tracing::info!(
            "Serving frontend assets from: {}",
            frontend_dir.display()
        );
        let serve_dir = tower_http::services::ServeDir::new(&frontend_dir).fallback(
            tower_http::services::ServeFile::new(frontend_dir.join("index.html")),
        );
        core.nest_service(
            "/_app",
            tower_http::services::ServeDir::new(frontend_dir.join("_app")),
        )
        .fallback_service(serve_dir)
    } else {
        tracing::warn!(
            "Frontend assets not found (set CAB_FRONTEND_DIR, place ui/ next to cab-srv, or run from repo with build/). Gateway/API only."
        );
        core
    }
    .layer(axum::middleware::from_fn(log_request))
    .layer(tower_http::trace::TraceLayer::new_for_http());

    let settings = cab_db::settings::get(&store).await.unwrap_or_else(|_| {
        let mut settings = cab_db::settings::default_settings();
        settings.gateway_port = config.gateway.port as i64;
        settings
    });
    let gateway_port = settings.gateway_port as u16;

    let port_source = if settings.gateway_port == config.gateway.port as i64 {
        "cab.toml (default)"
    } else {
        "user override (via API)"
    };
    tracing::info!(
        host = %config.gateway.host,
        port = gateway_port,
        port_source,
        "Gateway bind config (host from cab.toml, port from settings)"
    );

    let http_addr = format!("{}:{}", config.gateway.host, gateway_port);
    let http_addr_parsed: std::net::SocketAddr =
        http_addr.parse().expect("Failed to parse HTTP address");

    tracing::info!("CAB HTTP Gateway running at http://{}", http_addr);
    axum_server::bind(http_addr_parsed)
        .serve(app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .await?;

    Ok(())
}

fn looks_like_frontend(dir: &std::path::Path) -> bool {
    dir.is_dir() && (dir.join("index.html").exists() || dir.join("_app").exists())
}

/// Resolve the directory that contains the built Svelte UI (`index.html` / `_app`).
///
/// Priority:
/// 1. `CAB_FRONTEND_DIR`
/// 2. `{exe_dir}/ui`
/// 3. `{exe_dir}/../ui` (Tauri Resources layout: `bin/` + sibling `ui/`)
/// 4. `/usr/share/cab/ui` (Linux .deb)
/// 5. `{cwd}/build` (dev)
pub fn resolve_frontend_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("CAB_FRONTEND_DIR") {
        let path = PathBuf::from(dir);
        if looks_like_frontend(&path) {
            return Some(path);
        }
        tracing::warn!(
            path = %path.display(),
            "CAB_FRONTEND_DIR set but does not look like a frontend build"
        );
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        for candidate in [exe_dir.join("ui"), exe_dir.join("../ui")] {
            if looks_like_frontend(&candidate) {
                return Some(candidate.canonicalize().unwrap_or(candidate));
            }
        }
    }

    let deb_ui = PathBuf::from("/usr/share/cab/ui");
    if looks_like_frontend(&deb_ui) {
        return Some(deb_ui);
    }

    if let Ok(cwd) = std::env::current_dir() {
        let build = cwd.join("build");
        if looks_like_frontend(&build) {
            return Some(build);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn resolve_frontend_dir_respects_env() {
        let tmp = tempfile::tempdir().unwrap();
        let ui = tmp.path().join("ui");
        fs::create_dir_all(&ui).unwrap();
        fs::write(ui.join("index.html"), "<html></html>").unwrap();
        // SAFETY: test-only env mutation; not parallel with other env tests that touch this key.
        unsafe {
            std::env::set_var("CAB_FRONTEND_DIR", &ui);
        }
        let resolved = resolve_frontend_dir();
        unsafe {
            std::env::remove_var("CAB_FRONTEND_DIR");
        }
        assert_eq!(resolved.as_deref(), Some(ui.as_path()));
    }
}
