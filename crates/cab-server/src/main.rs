use cab_server::build_combined_router;
use tracing_subscriber::EnvFilter;

async fn log_request(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    tracing::info!("Incoming request: {} {}", method, uri);
    next.run(req).await
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,cab_gateway=debug,cab_api=debug")),
        )
        .init();

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

    let build_dir = std::env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("build");

    let core = build_combined_router(store.clone());

    let app = if build_dir.exists() {
        tracing::info!("Serving frontend assets from: {}", build_dir.display());
        let serve_dir = tower_http::services::ServeDir::new(&build_dir).fallback(
            tower_http::services::ServeFile::new(build_dir.join("index.html")),
        );
        core.nest_service(
            "/_app",
            tower_http::services::ServeDir::new(build_dir.join("_app")),
        )
        .fallback_service(serve_dir)
    } else {
        tracing::warn!(
            "Frontend build directory not found at {}. Run `npm run build` to build frontend.",
            build_dir.display()
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
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
