use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceScope {
    User,
    System,
}

impl ServiceScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::System => "system",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    pub scope: ServiceScope,
    pub cab_home: PathBuf,
    #[serde(default)]
    pub frontend_dir: Option<PathBuf>,
}

pub fn default_cab_home_for_scope(scope: ServiceScope) -> PathBuf {
    match scope {
        ServiceScope::User => cab_core::paths::default_user_cab_home(),
        ServiceScope::System => cab_core::paths::default_system_cab_home(),
    }
}

pub fn service_config_path_for_scope(scope: ServiceScope) -> PathBuf {
    default_cab_home_for_scope(scope).join("service.json")
}

/// Pointer written under the user cab home so CLI/GUI can discover system installs.
fn user_pointer_path() -> PathBuf {
    cab_core::paths::default_user_cab_home().join("service.json")
}

pub fn save_service_config(cfg: &ServiceConfig) -> Result<(), String> {
    let path = service_config_path_for_scope(cfg.scope);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| format!("Failed to write {}: {e}", path.display()))?;

    // Always keep a discoverable pointer for the installing user.
    let pointer = user_pointer_path();
    if pointer != path {
        if let Some(parent) = pointer.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(
            &pointer,
            serde_json::to_string_pretty(cfg).unwrap_or_default(),
        );
    }
    Ok(())
}

pub fn load_service_config() -> Option<ServiceConfig> {
    let candidates = [
        user_pointer_path(),
        service_config_path_for_scope(ServiceScope::User),
        service_config_path_for_scope(ServiceScope::System),
    ];
    for path in candidates {
        if let Ok(data) = fs::read_to_string(&path)
            && let Ok(cfg) = serde_json::from_str::<ServiceConfig>(&data)
        {
            return Some(cfg);
        }
    }
    None
}

pub fn clear_service_config(scope: ServiceScope) {
    // Read pointer before deleting system service.json (load falls back to system path).
    let pointer_was_system = load_service_config().is_some_and(|c| c.scope == ServiceScope::System);
    let _ = fs::remove_file(service_config_path_for_scope(scope));
    if scope == ServiceScope::System {
        if pointer_was_system {
            let _ = fs::remove_file(user_pointer_path());
        }
    } else {
        let _ = fs::remove_file(user_pointer_path());
    }
}

/// Apply `CAB_HOME` from an installed service.json so subsequent db_path() calls match the daemon.
pub fn apply_installed_cab_home() {
    if std::env::var_os("CAB_HOME").is_some() {
        return;
    }
    if let Some(cfg) = load_service_config() {
        // SAFETY: process-local override before any other threads spawn in cab-cli.
        unsafe {
            std::env::set_var("CAB_HOME", &cfg.cab_home);
        }
    }
}

pub fn looks_like_frontend(dir: &Path) -> bool {
    dir.is_dir() && (dir.join("index.html").exists() || dir.join("_app").exists())
}

pub fn resolve_frontend_dir_for_install(srv_exe: &Path) -> Option<PathBuf> {
    if let Some(exe_dir) = srv_exe.parent() {
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
    None
}

pub fn get_cab_srv_executable_path() -> Result<PathBuf, String> {
    let current_exe = std::env::current_exe()
        .map_err(|e| format!("Failed to determine current executable path: {e}"))?;
    let current_dir = current_exe
        .parent()
        .ok_or_else(|| "cab-cli has no parent directory".to_string())?;

    let srv_target_name = if cfg!(target_os = "windows") {
        "cab-srv.exe"
    } else {
        "cab-srv"
    };
    let srv_target_path = current_dir.join(srv_target_name);
    if srv_target_path.exists() {
        return Ok(srv_target_path);
    }

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Home directory not set".to_string())?;
    let fallback_bin = PathBuf::from(home)
        .join(".local")
        .join("bin")
        .join(srv_target_name);
    if fallback_bin.exists() {
        return Ok(fallback_bin);
    }

    #[cfg(target_os = "linux")]
    {
        let usr = PathBuf::from("/usr/bin").join(srv_target_name);
        if usr.exists() {
            return Ok(usr);
        }
    }

    Ok(fallback_bin)
}

pub fn get_working_dir(srv_exe: &Path) -> Result<String, String> {
    srv_exe
        .parent()
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| "cab-srv executable has no parent directory".to_string())
}

pub fn require_admin_for_system(scope: ServiceScope) -> Result<(), String> {
    if scope != ServiceScope::System {
        return Ok(());
    }
    if is_elevated() {
        return Ok(());
    }
    // Child processes launched via elevation set this so a failed elevation cannot loop.
    if std::env::var_os("CAB_ELEVATED").is_some() {
        return Err(
            "System-scope operation still lacks administrator/root privileges after elevation."
                .into(),
        );
    }
    println!("System-scope requires administrator/root privileges. Requesting elevation…");
    let code = relaunch_current_process_elevated()?;
    std::process::exit(code);
}

/// Re-exec this process with the same argv under admin/root and wait for it.
fn relaunch_current_process_elevated() -> Result<i32, String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let args: Vec<String> = std::env::args().skip(1).collect();

    #[cfg(target_os = "windows")]
    {
        // PowerShell Start-Process -Verb RunAs pops UAC, then waits.
        let exe_ps = exe.to_string_lossy().replace('\'', "''");
        let arg_list = args
            .iter()
            .map(|a| format!("'{}'", a.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        let ps = format!(
            "$p = Start-Process -FilePath '{exe_ps}' -ArgumentList @({arg_list}) \
             -Verb RunAs -Wait -PassThru; \
             if ($null -eq $p) {{ exit 1 }}; exit $p.ExitCode"
        );
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps])
            .status()
            .map_err(|e| format!("Failed to request UAC elevation: {e}"))?;
        return Ok(status.code().unwrap_or(1));
    }

    #[cfg(target_os = "linux")]
    {
        let mut cmd = std::process::Command::new("pkexec");
        cmd.env("CAB_ELEVATED", "1");
        cmd.arg(&exe);
        cmd.args(&args);
        match cmd.status() {
            Ok(status) => Ok(status.code().unwrap_or(1)),
            Err(_) => {
                let mut sudo = std::process::Command::new("sudo");
                sudo.env("CAB_ELEVATED", "1");
                sudo.arg(&exe);
                sudo.args(&args);
                let status = sudo
                    .status()
                    .map_err(|e| format!("Failed to elevate via pkexec/sudo: {e}"))?;
                Ok(status.code().unwrap_or(1))
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        // Quote for AppleScript /bin/sh -c
        let mut shell = format!("export CAB_ELEVATED=1; '{}'", exe.display());
        for a in &args {
            shell.push(' ');
            shell.push_str(&shell_single_quote(a));
        }
        let script = format!(
            r#"do shell script {} with administrator privileges"#,
            apple_script_string(&shell)
        );
        let status = std::process::Command::new("osascript")
            .args(["-e", &script])
            .status()
            .map_err(|e| format!("Failed to elevate via osascript: {e}"))?;
        Ok(status.code().unwrap_or(1))
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = (exe, args);
        Err("Cannot elevate on this OS".into())
    }
}

#[cfg(target_os = "macos")]
fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(target_os = "macos")]
fn apple_script_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn is_elevated() -> bool {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("net")
            .args(["session"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(unix)]
    {
        std::process::Command::new("id")
            .arg("-u")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim() == "0")
            .unwrap_or(false)
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        false
    }
}

pub fn run_cmd(cmd: &str, args: &[&str]) -> Result<(), String> {
    let status = std::process::Command::new(cmd)
        .args(args)
        .status()
        .map_err(|e| format!("Failed to execute command '{cmd}': {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Command '{cmd} {}' exited with non-zero status: {status}",
            args.join(" ")
        ))
    }
}
