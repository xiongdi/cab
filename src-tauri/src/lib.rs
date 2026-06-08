use tauri::Manager;
use tracing_subscriber::EnvFilter;

struct PortState(std::sync::Mutex<u16>);

#[tauri::command]
fn get_gateway_port(state: tauri::State<'_, PortState>) -> u16 {
    *state.0.lock().unwrap()
}

/// Start the combined Axum server (gateway + management API) in a background task.
async fn start_server(port: u16, ready: tokio::sync::oneshot::Sender<()>) {
    // Load config
    let config = cab_core::CabConfig::load();

    // Initialize in-memory store
    let store = cab_db::init_store()
        .await
        .expect("Failed to initialize store");

    let store_for_sync = store.clone();
    tokio::spawn(async move {
        tracing::info!("Startup: Synchronizing models.dev provider/model catalog...");
        match cab_api::providers::sync_models_dev_catalog(&store_for_sync).await {
            Ok(count) => tracing::info!(
                "Startup: models.dev catalog synchronization finished. Synced {count} models."
            ),
            Err(e) => tracing::error!("Startup: models.dev catalog synchronization failed: {e:?}"),
        }
    });

    // Build combined router:
    //   /v1/*  → Gateway (proxy to LLM providers)
    //   /api/* → Management REST API
    let gateway_state = cab_gateway::GatewayState::new(store.clone());
    let gateway = cab_gateway::gateway_router(gateway_state);
    let api = cab_api::api_router(store);

    let build_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("build");

    let app = if build_dir.exists() {
        tracing::info!("Serving frontend assets from: {}", build_dir.display());
        let serve_dir = tower_http::services::ServeDir::new(&build_dir).fallback(
            tower_http::services::ServeFile::new(build_dir.join("index.html")),
        );
        gateway
            .merge(api)
            .nest_service(
                "/_app",
                tower_http::services::ServeDir::new(build_dir.join("_app")),
            )
            .fallback_service(serve_dir)
            .layer(tower_http::trace::TraceLayer::new_for_http())
    } else if cfg!(debug_assertions) {
        tracing::info!("Dev mode: API/gateway only (run `npm run build` or start Vite on :5173)");
        gateway
            .merge(api)
            .layer(tower_http::trace::TraceLayer::new_for_http())
    } else {
        tracing::warn!(
            "Frontend build directory not found at {}. Run `npm run build` first.",
            build_dir.display()
        );
        gateway
            .merge(api)
            .layer(tower_http::trace::TraceLayer::new_for_http())
    };

    let addr = format!("{}:{}", config.gateway.host, port);
    tracing::info!("CAB server starting on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind address");

    let _ = ready.send(());

    axum::serve(listener, app).await.expect("Server error");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,cab_gateway=debug,cab_api=debug")),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
            let config = cab_core::CabConfig::load();

            let port = tauri::async_runtime::block_on(async move {
                let store = cab_db::init_store().await.ok();
                if let Some(store) = store {
                    let settings = cab_db::settings::get(&store).await;
                    if let Ok(settings) = settings {
                        return settings.gateway_port as u16;
                    }
                }
                config.gateway.port
            });

            app.manage(PortState(std::sync::Mutex::new(port)));

            let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
            tauri::async_runtime::spawn(async move {
                start_server(port, ready_tx).await;
            });

            // Block until the gateway is listening so the UI can fetch /api immediately.
            tauri::async_runtime::block_on(async {
                if ready_rx.await.is_err() {
                    tracing::error!("Gateway server failed to start");
                }
            });

            let build_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("build");
            let use_built_ui = build_dir.exists();

            if let Some(window) = app.get_webview_window("main") {
                if use_built_ui {
                    let url = format!("http://127.0.0.1:{port}/");
                    if let Ok(parsed) = tauri::Url::parse(&url) {
                        let res = window.navigate(parsed);
                        if let Err(e) = res {
                            tracing::error!("Failed to navigate main window to {url}: {e}");
                        }
                    }
                } else if cfg!(debug_assertions) {
                    window.open_devtools();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_gateway_port])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
