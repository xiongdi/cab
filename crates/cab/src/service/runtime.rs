use super::scope::{ServiceScope, load_service_config, require_admin_for_system, run_cmd};

#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::path::PathBuf;

fn current_scope() -> ServiceScope {
    load_service_config()
        .map(|c| c.scope)
        .unwrap_or(ServiceScope::User)
}

pub fn start_daemon() -> Result<(), String> {
    let scope = current_scope();
    require_admin_for_system(scope)?;
    match scope {
        ServiceScope::User => start_user(),
        ServiceScope::System => start_system(),
    }
}

pub fn stop_daemon() -> Result<(), String> {
    let scope = current_scope();
    require_admin_for_system(scope)?;
    match scope {
        ServiceScope::User => stop_user(),
        ServiceScope::System => stop_system(),
    }
}

pub fn is_service_active() -> bool {
    match current_scope() {
        ServiceScope::User => is_active_user(),
        ServiceScope::System => is_active_system(),
    }
}

pub fn show_logs(follow: bool, lines: u32) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let lines_str = lines.to_string();
        let scope = current_scope();
        let mut args: Vec<&str> = match scope {
            ServiceScope::User => vec!["--user", "-u", "cab-srv", "-n", &lines_str],
            ServiceScope::System => vec!["-u", "cab-srv", "-n", &lines_str],
        };
        if follow {
            args.push("-f");
        }
        let mut child = std::process::Command::new("journalctl")
            .args(&args)
            .spawn()
            .map_err(|e| format!("Failed to spawn journalctl: {e}"))?;
        let _ = child.wait();
        Ok(())
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let log_file = log_file_path()?;
        if !log_file.exists() {
            println!("No logs found at {}", log_file.display());
            return Ok(());
        }
        print_log_file(&log_file, lines, follow)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = (follow, lines);
        Err("Unsupported OS".into())
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn log_file_path() -> Result<PathBuf, String> {
    let home = if let Some(cfg) = load_service_config() {
        cfg.cab_home
    } else {
        cab_core::paths::cab_home()
    };
    Ok(home.join("logs").join("cab-srv.stdout.log"))
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn print_log_file(log_file: &std::path::Path, lines: u32, follow: bool) -> Result<(), String> {
    use std::fs::File;
    use std::io::{BufRead, BufReader, Seek, SeekFrom};

    let file = File::open(log_file).map_err(|e| format!("Failed to open log file: {e}"))?;
    let reader = BufReader::new(file);
    let all_lines: Vec<String> = reader.lines().map_while(Result::ok).collect();
    let start_idx = all_lines.len().saturating_sub(lines as usize);
    for line in &all_lines[start_idx..] {
        println!("{}", line);
    }
    if follow {
        println!("\n--- Following logs (Press Ctrl+C to stop) ---");
        let mut file =
            File::open(log_file).map_err(|e| format!("Failed to open log file for follow: {e}"))?;
        let _ = file.seek(SeekFrom::End(0));
        let mut reader = BufReader::new(file);
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => std::thread::sleep(std::time::Duration::from_millis(100)),
                Ok(_) => print!("{}", line),
                Err(e) => return Err(format!("Error reading logs: {e}")),
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn start_user() -> Result<(), String> {
    println!("Starting cab-srv (user systemd)...");
    run_cmd("systemctl", &["--user", "start", "cab-srv"])
}
#[cfg(target_os = "linux")]
fn start_system() -> Result<(), String> {
    println!("Starting cab-srv (system systemd)...");
    run_cmd("systemctl", &["start", "cab-srv"])
}
#[cfg(target_os = "linux")]
fn stop_user() -> Result<(), String> {
    run_cmd("systemctl", &["--user", "stop", "cab-srv"])
}
#[cfg(target_os = "linux")]
fn stop_system() -> Result<(), String> {
    run_cmd("systemctl", &["stop", "cab-srv"])
}
#[cfg(target_os = "linux")]
fn is_active_user() -> bool {
    std::process::Command::new("systemctl")
        .args(["--user", "is-active", "cab-srv"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false)
}
#[cfg(target_os = "linux")]
fn is_active_system() -> bool {
    std::process::Command::new("systemctl")
        .args(["is-active", "cab-srv"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "active")
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn start_user() -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    let plist = PathBuf::from(home).join("Library/LaunchAgents/com.cab.cab-srv.plist");
    let s = plist.to_string_lossy().to_string();
    let uid = std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "501".into());
    let domain = format!("gui/{uid}");
    let label = format!("{domain}/com.cab.cab-srv");
    if run_cmd("launchctl", &["bootstrap", &domain, &s]).is_err() {
        let _ = run_cmd("launchctl", &["load", "-w", &s]);
    }
    let _ = run_cmd("launchctl", &["enable", &label]);
    if run_cmd("launchctl", &["kickstart", "-k", &label]).is_err() {
        run_cmd("launchctl", &["start", "com.cab.cab-srv"])?;
    }
    Ok(())
}
#[cfg(target_os = "macos")]
fn start_system() -> Result<(), String> {
    let plist = "/Library/LaunchDaemons/com.cab.cab-srv.plist";
    if run_cmd("launchctl", &["bootstrap", "system", plist]).is_err() {
        let _ = run_cmd("launchctl", &["load", "-w", plist]);
    }
    let _ = run_cmd("launchctl", &["enable", "system/com.cab.cab-srv"]);
    if run_cmd("launchctl", &["kickstart", "-k", "system/com.cab.cab-srv"]).is_err() {
        run_cmd("launchctl", &["start", "com.cab.cab-srv"])?;
    }
    Ok(())
}
#[cfg(target_os = "macos")]
fn stop_user() -> Result<(), String> {
    let _ = run_cmd("launchctl", &["stop", "com.cab.cab-srv"]);
    if let Ok(home) = std::env::var("HOME") {
        let plist = PathBuf::from(home).join("Library/LaunchAgents/com.cab.cab-srv.plist");
        let s = plist.to_string_lossy().to_string();
        let uid = std::process::Command::new("id")
            .arg("-u")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|u| u.trim().to_string())
            .unwrap_or_else(|| "501".into());
        let domain = format!("gui/{uid}");
        let _ = run_cmd("launchctl", &["bootout", &domain, &s]);
        let _ = run_cmd("launchctl", &["unload", &s]);
    }
    Ok(())
}
#[cfg(target_os = "macos")]
fn stop_system() -> Result<(), String> {
    let plist = "/Library/LaunchDaemons/com.cab.cab-srv.plist";
    let _ = run_cmd("launchctl", &["kill", "SIGTERM", "system/com.cab.cab-srv"]);
    let _ = run_cmd("launchctl", &["bootout", "system", plist]);
    let _ = run_cmd("launchctl", &["unload", plist]);
    Ok(())
}
#[cfg(target_os = "macos")]
fn is_active_user() -> bool {
    let uid = std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "501".into());
    let label = format!("gui/{uid}/com.cab.cab-srv");
    if std::process::Command::new("launchctl")
        .args(["print", &label])
        .output()
        .ok()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return true;
    }
    std::process::Command::new("launchctl")
        .args(["list"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("com.cab.cab-srv"))
        .unwrap_or(false)
}
#[cfg(target_os = "macos")]
fn is_active_system() -> bool {
    if std::process::Command::new("launchctl")
        .args(["print", "system/com.cab.cab-srv"])
        .output()
        .ok()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return true;
    }
    is_active_user()
}

#[cfg(target_os = "windows")]
fn start_user() -> Result<(), String> {
    println!("Starting cab-srv scheduled task...");
    run_cmd("schtasks", &["/Run", "/TN", "CAB\\cab-srv"])
}
#[cfg(target_os = "windows")]
fn start_system() -> Result<(), String> {
    println!("Starting cab-srv Windows service...");
    run_cmd("sc", &["start", "cab-srv"])
}
#[cfg(target_os = "windows")]
fn stop_user() -> Result<(), String> {
    let _ = run_cmd("schtasks", &["/End", "/TN", "CAB\\cab-srv"]);
    // Graceful first (WM_CLOSE equivalent for console apps), then force.
    let soft = std::process::Command::new("taskkill")
        .args(["/IM", "cab-srv.exe"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    if soft.map(|s| s.success()).unwrap_or(false) {
        std::thread::sleep(std::time::Duration::from_millis(800));
    }
    if is_active_user() {
        let _ = run_cmd("taskkill", &["/F", "/IM", "cab-srv.exe"]);
    }
    Ok(())
}
#[cfg(target_os = "windows")]
fn stop_system() -> Result<(), String> {
    run_cmd("sc", &["stop", "cab-srv"])
}
#[cfg(target_os = "windows")]
fn is_active_user() -> bool {
    std::process::Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq cab-srv.exe"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("cab-srv.exe"))
        .unwrap_or(false)
}
#[cfg(target_os = "windows")]
fn is_active_system() -> bool {
    std::process::Command::new("sc")
        .args(["query", "cab-srv"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("RUNNING"))
        .unwrap_or(false)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn start_user() -> Result<(), String> {
    Err("Unsupported OS".into())
}
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn start_system() -> Result<(), String> {
    Err("Unsupported OS".into())
}
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn stop_user() -> Result<(), String> {
    Err("Unsupported OS".into())
}
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn stop_system() -> Result<(), String> {
    Err("Unsupported OS".into())
}
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn is_active_user() -> bool {
    false
}
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn is_active_system() -> bool {
    false
}
