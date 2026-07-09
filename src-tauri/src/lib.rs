use std::path::PathBuf;
use std::process::Command;
use tauri::Manager;
use tracing_subscriber::EnvFilter;

struct PortState(std::sync::Mutex<u16>);

/// Find the cab-cli binary bundled with the app, searching:
/// 1. Next to the current executable (Windows, AppImage)
/// 2. In the Tauri resource directory (macOS bundle)
/// 3. Falls back to the bare name (let OS search $PATH — Linux DEB)
fn find_cab_cli(app: &tauri::AppHandle) -> PathBuf {
    let bin_name = if cfg!(target_os = "windows") {
        "cab-cli.exe"
    } else {
        "cab-cli"
    };

    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidate = dir.join(bin_name);
        if candidate.exists() {
            return candidate;
        }
    }

    if let Ok(resource_dir) = app.path().resource_dir() {
        let candidate = resource_dir.join(bin_name);
        if candidate.exists() {
            return candidate;
        }
    }

    PathBuf::from(bin_name)
}

/// Auto-install and start the cab-srv daemon service on first launch.
/// Uses a marker file (~/.cab/.daemon_installed) to avoid running every time.
fn auto_install_daemon(app: &tauri::AppHandle) {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    let marker = PathBuf::from(&home).join(".cab").join(".daemon_installed");

    if marker.exists() {
        return;
    }

    let cab_cli = find_cab_cli(app);
    tracing::info!("First launch: installing daemon service via {:?}", cab_cli);

    match Command::new(&cab_cli)
        .arg("service")
        .arg("install")
        .output()
    {
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if out.status.success() {
                tracing::info!("Daemon service installed successfully");
                if let Some(parent) = marker.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::write(&marker, "installed");

                if let Err(e) = Command::new(&cab_cli).arg("start").output() {
                    tracing::warn!("Failed to start daemon: {e}");
                } else {
                    tracing::info!("Daemon service started");
                }
            } else {
                tracing::warn!("Daemon service install failed: {stderr}");
            }
        }
        Err(e) => tracing::warn!("Failed to run cab-cli service install: {e}"),
    }
}

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
        match cab_services::catalog::sync_on_startup(&store_for_sync).await {
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

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .expect("Server error");
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

            // Auto-install daemon service on first launch (fire-and-forget)
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                auto_install_daemon(&app_handle);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_gateway_port])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
