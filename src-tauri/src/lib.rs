use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
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
        let candidate = resource_dir.join("bin").join(bin_name);
        if candidate.exists() {
            return candidate;
        }
    }

    PathBuf::from(bin_name)
}

fn service_config_exists() -> bool {
    let user = cab_core::paths::default_user_cab_home().join("service.json");
    if user.exists() {
        return true;
    }
    cab_core::paths::default_system_cab_home()
        .join("service.json")
        .exists()
}

fn apply_cab_home_from_service_json() {
    if std::env::var_os("CAB_HOME").is_some() {
        return;
    }
    let path = cab_core::paths::default_user_cab_home().join("service.json");
    let Ok(data) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&data) else {
        return;
    };
    if let Some(home) = v.get("cab_home").and_then(|x| x.as_str()) {
        unsafe {
            std::env::set_var("CAB_HOME", home);
        }
    }
}

fn read_gateway_port() -> u16 {
    apply_cab_home_from_service_json();
    let config = cab_core::CabConfig::load();
    let path = cab_db::sqlite::db_path();
    if !path.exists() {
        return config.gateway.port;
    }

    let Ok(conn) = rusqlite::Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    ) else {
        return config.gateway.port;
    };

    let Ok(data) = conn.query_row("SELECT data FROM settings WHERE id = 1", [], |row| {
        row.get::<_, String>(0)
    }) else {
        return config.gateway.port;
    };

    serde_json::from_str::<cab_core::types::Settings>(&data)
        .map(|s| s.gateway_port as u16)
        .unwrap_or(config.gateway.port)
}

fn gateway_alive(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{port}/api/settings");
    let Ok(client) = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
    else {
        return false;
    };
    match client.get(&url).send() {
        Ok(resp) => resp.status().is_success() || resp.status().as_u16() == 401,
        Err(_) => false,
    }
}

/// Prompt for user vs system scope. Returns `"user"` or `"system"`.
/// Defaults to user if the dialog cannot be shown.
fn prompt_service_scope() -> &'static str {
    let title = "CAB service scope";
    let body = "Install for all users (system)? This will ask for administrator/root. Choose No for current user only (default).";

    #[cfg(target_os = "linux")]
    {
        if Command::new("zenity")
            .args([
                "--question",
                "--title",
                title,
                "--text",
                body,
                "--ok-label",
                "System",
                "--cancel-label",
                "Current user",
            ])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            "system"
        } else {
            "user"
        }
    }

    #[cfg(target_os = "macos")]
    {
        let script = format!(
            r#"display dialog "{body}" with title "{title}" buttons {{"Current user", "System"}} default button "Current user""#
        );
        let out = Command::new("osascript")
            .args(["-e", &script])
            .output()
            .ok();
        if let Some(o) = out {
            let s = String::from_utf8_lossy(&o.stdout);
            if s.contains("System") {
                return "system";
            }
        }
        "user"
    }

    #[cfg(target_os = "windows")]
    {
        let ps = format!(
            "Add-Type -AssemblyName PresentationFramework; \
             $r = [System.Windows.MessageBox]::Show('{body}','{title}','YesNo','Question'); \
             if ($r -eq 'Yes') {{ exit 0 }} else {{ exit 1 }}"
        );
        if Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
        {
            "system"
        } else {
            "user"
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = (title, body);
        "user"
    }
}

fn run_cab_cli_install(cab_cli: &PathBuf, scope: &str) -> Result<(), String> {
    // cab-cli self-elevates (UAC / pkexec / osascript) when --scope system.
    let out = Command::new(cab_cli)
        .args(["service", "install", "--scope", scope])
        .output()
        .map_err(|e| format!("Failed to run cab-cli service install: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        Err(format!(
            "cab-cli service install --scope {scope} failed: {stderr}{stdout}"
        ))
    }
}

/// Ensure cab-srv is running: install service if needed (with scope choice), start, wait for HTTP.
fn ensure_cab_srv(app: &tauri::AppHandle, port: u16) -> Result<(), String> {
    if gateway_alive(port) {
        tracing::info!("cab-srv already reachable on port {port}");
        return Ok(());
    }

    let cab_cli = find_cab_cli(app);
    tracing::info!("cab-srv not reachable; ensuring via {:?}", cab_cli);

    if !service_config_exists() {
        let scope = prompt_service_scope();
        tracing::info!("Installing cab-srv with scope={scope}");
        match run_cab_cli_install(&cab_cli, scope) {
            Ok(()) => tracing::info!("cab-cli service install succeeded"),
            Err(e) => return Err(e),
        }
    }

    match Command::new(&cab_cli).arg("start").output() {
        Ok(out) if out.status.success() => tracing::info!("cab-cli start succeeded"),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            tracing::warn!("cab-cli start returned non-zero: {stderr}");
        }
        Err(e) => return Err(format!("Failed to run cab-cli start: {e}")),
    }

    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if gateway_alive(port) {
            tracing::info!("cab-srv is ready on port {port}");
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }

    Err(format!(
        "cab-srv did not become ready on http://127.0.0.1:{port}/ within 30s"
    ))
}

fn navigate_to_gateway(app: &tauri::AppHandle, port: u16) {
    let url = format!("http://127.0.0.1:{port}/");
    if let Some(window) = app.get_webview_window("main") {
        match tauri::Url::parse(&url) {
            Ok(parsed) => {
                if let Err(e) = window.navigate(parsed) {
                    tracing::error!("Failed to navigate main window to {url}: {e}");
                }
            }
            Err(e) => tracing::error!("Invalid gateway URL {url}: {e}"),
        }
    }
}

fn navigate_to_error(app: &tauri::AppHandle, message: &str) {
    let escaped = message
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;");
    let html = format!(
        "data:text/html;charset=utf-8,<!DOCTYPE html><html><body style=\"font-family:system-ui;padding:2rem\">\
         <h1>CAB gateway unavailable</h1><p>{escaped}</p>\
         <p>Try: <code>cab-cli start</code> or ensure port is free.</p></body></html>"
    );
    if let Some(window) = app.get_webview_window("main")
        && let Ok(parsed) = tauri::Url::parse(&html)
    {
        let _ = window.navigate(parsed);
    }
}

#[tauri::command]
fn get_gateway_port(state: tauri::State<'_, PortState>) -> u16 {
    *state.0.lock().unwrap()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .setup(|app| {
            let port = read_gateway_port();
            app.manage(PortState(std::sync::Mutex::new(port)));

            match ensure_cab_srv(app.handle(), port) {
                Ok(()) => navigate_to_gateway(app.handle(), port),
                Err(e) => {
                    tracing::error!("{e}");
                    navigate_to_error(app.handle(), &e);
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![get_gateway_port])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
