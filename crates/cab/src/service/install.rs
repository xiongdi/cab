use super::scope::{
    ServiceConfig, ServiceScope, clear_service_config, default_cab_home_for_scope,
    get_cab_srv_executable_path, get_working_dir, require_admin_for_system,
    resolve_frontend_dir_for_install, run_cmd, save_service_config,
};
use std::fs;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::path::PathBuf;

/// Dedicated OS account for system-scoped cab-srv (least privilege).
#[cfg(any(target_os = "linux", target_os = "macos"))]
const SYSTEM_SERVICE_USER: &str = "cab";

pub fn install_service(scope: ServiceScope) -> Result<(), String> {
    require_admin_for_system(scope)?;

    let executable_path = get_cab_srv_executable_path()?;
    let working_dir = get_working_dir(&executable_path)?;
    let frontend_dir = resolve_frontend_dir_for_install(&executable_path);
    let cab_home = default_cab_home_for_scope(scope);
    fs::create_dir_all(&cab_home)
        .map_err(|e| format!("Failed to create data dir {}: {e}", cab_home.display()))?;

    let cfg = ServiceConfig {
        scope,
        cab_home: cab_home.clone(),
        frontend_dir: frontend_dir.clone(),
    };

    match scope {
        ServiceScope::User => install_user(&executable_path, &working_dir, &cfg)?,
        ServiceScope::System => install_system(&executable_path, &working_dir, &cfg)?,
    }

    save_service_config(&cfg)?;
    println!(
        "Installed cab-srv as {} service (data: {}).",
        scope.as_str(),
        cab_home.display()
    );

    // System install already runs elevated — start now so the installer needs only one UAC prompt.
    if scope == ServiceScope::System {
        match super::start_daemon() {
            Ok(()) => println!("System service started."),
            Err(e) => println!("Warning: installed but failed to start: {e}"),
        }
    } else {
        println!("Start with: cab-cli start");
    }
    Ok(())
}

pub fn uninstall_service() -> Result<(), String> {
    let scope = super::scope::load_service_config()
        .map(|c| c.scope)
        .unwrap_or(ServiceScope::User);
    require_admin_for_system(scope)?;

    match scope {
        ServiceScope::User => uninstall_user()?,
        ServiceScope::System => uninstall_system()?,
    }
    clear_service_config(scope);
    println!("Uninstalled cab-srv {} service.", scope.as_str());
    Ok(())
}

fn env_lines(cfg: &ServiceConfig) -> String {
    let mut lines = format!("Environment=CAB_HOME={}\n", cfg.cab_home.display());
    if let Some(fe) = &cfg.frontend_dir {
        lines.push_str(&format!("Environment=CAB_FRONTEND_DIR={}\n", fe.display()));
    }
    lines
}

#[cfg(any(test, target_os = "macos"))]
fn plist_env_block(cfg: &ServiceConfig) -> String {
    let mut entries = format!(
        "\t\t<key>CAB_HOME</key>\n\t\t<string>{}</string>\n",
        cfg.cab_home.display()
    );
    if let Some(fe) = &cfg.frontend_dir {
        entries.push_str(&format!(
            "\t\t<key>CAB_FRONTEND_DIR</key>\n\t\t<string>{}</string>\n",
            fe.display()
        ));
    }
    format!("\t<key>EnvironmentVariables</key>\n\t<dict>\n{entries}\t</dict>\n")
}

/// LaunchAgent / LaunchDaemon plist body (pure; unit-tested on all CI hosts).
#[cfg(any(test, target_os = "macos"))]
pub fn launchd_plist_content(
    executable: &str,
    working_dir: &str,
    cfg: &ServiceConfig,
    stdout_log: &str,
    stderr_log: &str,
    run_as: Option<&str>,
) -> String {
    let mut user_keys = String::new();
    if let Some(u) = run_as
        && u != "root"
    {
        user_keys = format!(
            "\t<key>UserName</key>\n\t<string>{u}</string>\n\
             \t<key>GroupName</key>\n\t<string>{u}</string>\n"
        );
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \t<key>Label</key>\n\
         \t<string>com.cab.cab-srv</string>\n\
         \t<key>ProgramArguments</key>\n\
         \t<array>\n\
         \t\t<string>{executable}</string>\n\
         \t</array>\n\
         \t<key>RunAtLoad</key>\n\
         \t<true/>\n\
         \t<key>KeepAlive</key>\n\
         \t<true/>\n\
         \t<key>ThrottleInterval</key>\n\
         \t<integer>5</integer>\n\
         \t<key>ProcessType</key>\n\
         \t<string>Background</string>\n\
         \t<key>WorkingDirectory</key>\n\
         \t<string>{working_dir}</string>\n\
         {user_keys}\
         {}\
         \t<key>StandardOutPath</key>\n\
         \t<string>{stdout_log}</string>\n\
         \t<key>StandardErrorPath</key>\n\
         \t<string>{stderr_log}</string>\n\
         </dict>\n\
         </plist>\n",
        plist_env_block(cfg)
    )
}

/// Windows user-scope Task Scheduler XML (UTF-16 body without BOM).
///
/// Runs `wscript.exe` with a `.vbs` launcher so the console `cab-srv` does not
/// flash a visible CMD window (VBS `Run ..., 0` starts the `.cmd` hidden).
#[cfg(any(test, target_os = "windows"))]
pub fn windows_user_task_xml(vbs_launcher: &str) -> String {
    let args_xml = format!("\"{vbs_launcher}\"").replace('&', "&amp;");
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>CAB Coding Agents Bridge (user scope)</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
    <RestartOnFailure>
      <Interval>PT1M</Interval>
      <Count>5</Count>
    </RestartOnFailure>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>wscript.exe</Command>
      <Arguments>{args_xml}</Arguments>
    </Exec>
  </Actions>
</Task>
"#
    )
}

/// Build Linux systemd unit body (shared by user and system scopes).
#[cfg(target_os = "linux")]
pub fn linux_unit_content(
    executable: &str,
    working_dir: &str,
    cfg: &ServiceConfig,
    system: bool,
) -> String {
    let wanted = if system {
        "multi-user.target"
    } else {
        "default.target"
    };

    let mut hardening = if system {
        format!(
            "User={SYSTEM_SERVICE_USER}\n\
             Group={SYSTEM_SERVICE_USER}\n\
             StateDirectory=cab\n\
             UMask=0077\n\
             CapabilityBoundingSet=\n\
             AmbientCapabilities=\n\
             ProtectSystem=strict\n\
             ProtectHome=true\n\
             PrivateTmp=true\n\
             PrivateDevices=true\n\
             NoNewPrivileges=true\n\
             RestrictSUIDSGID=true\n\
             LockPersonality=true\n\
             RestrictRealtime=true\n\
             RestrictNamespaces=true\n\
             SystemCallArchitectures=native\n\
             RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6\n\
             ReadWritePaths={}\n",
            cfg.cab_home.display()
        )
    } else {
        String::new()
    };

    if system && let Some(fe) = &cfg.frontend_dir {
        hardening.push_str(&format!("ReadOnlyPaths={}\n", fe.display()));
    }

    format!(
        "[Unit]\n\
         Description=CAB (Coding Agents Bridge) Daemon\n\
         After=network-online.target\n\
         Wants=network-online.target\n\n\
         [Service]\n\
         Type=simple\n\
         ExecStart={executable}\n\
         Restart=always\n\
         RestartSec=5\n\
         WorkingDirectory={working_dir}\n\
         {hardening}\
         {}\
         StandardOutput=journal\n\
         StandardError=journal\n\n\
         [Install]\n\
         WantedBy={wanted}\n",
        env_lines(cfg)
    )
}

#[cfg(target_os = "linux")]
fn ensure_linux_system_user(cab_home: &std::path::Path) -> Result<(), String> {
    let exists = std::process::Command::new("id")
        .arg(SYSTEM_SERVICE_USER)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !exists {
        // Prefer useradd; fall back to adduser on some distros.
        let shell = if PathBuf::from("/usr/sbin/nologin").exists() {
            "/usr/sbin/nologin"
        } else {
            "/bin/false"
        };
        let result = run_cmd(
            "useradd",
            &[
                "--system",
                "--home-dir",
                &cab_home.display().to_string(),
                "--no-create-home",
                "--shell",
                shell,
                SYSTEM_SERVICE_USER,
            ],
        );
        if result.is_err() {
            run_cmd(
                "adduser",
                &[
                    "--system",
                    "--home",
                    &cab_home.display().to_string(),
                    "--no-create-home",
                    "--shell",
                    shell,
                    "--disabled-login",
                    SYSTEM_SERVICE_USER,
                ],
            )?;
        }
    }
    fs::create_dir_all(cab_home).map_err(|e| e.to_string())?;
    run_cmd(
        "chown",
        &[
            "-R",
            &format!("{SYSTEM_SERVICE_USER}:{SYSTEM_SERVICE_USER}"),
            &cab_home.display().to_string(),
        ],
    )?;
    Ok(())
}

/// Ensure the `cab` service account can traverse to and execute `cab-srv` (and read UI assets).
#[cfg(target_os = "linux")]
fn ensure_linux_service_can_run(
    executable_path: &std::path::Path,
    cfg: &ServiceConfig,
) -> Result<(), String> {
    grant_linux_user_access(SYSTEM_SERVICE_USER, executable_path, false)?;
    // Parent directories need execute (traverse) only.
    let mut parent = executable_path.parent();
    while let Some(dir) = parent {
        if dir.as_os_str().is_empty() || dir == std::path::Path::new("/") {
            break;
        }
        grant_linux_user_access(SYSTEM_SERVICE_USER, dir, true)?;
        parent = dir.parent();
    }
    if let Some(fe) = &cfg.frontend_dir {
        grant_linux_user_access(SYSTEM_SERVICE_USER, fe, false)?;
        let mut p = fe.parent();
        while let Some(dir) = p {
            if dir.as_os_str().is_empty() || dir == std::path::Path::new("/") {
                break;
            }
            grant_linux_user_access(SYSTEM_SERVICE_USER, dir, true)?;
            p = dir.parent();
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn grant_linux_user_access(
    user: &str,
    path: &std::path::Path,
    traverse_only: bool,
) -> Result<(), String> {
    let path_s = path.display().to_string();
    let acl = if traverse_only {
        format!("u:{user}:--x")
    } else {
        format!("u:{user}:r-x")
    };
    if run_cmd("setfacl", &["-m", &acl, &path_s]).is_ok() {
        return Ok(());
    }
    // Fallback when ACL tools are absent: world execute/read (acceptable for shipped binaries).
    if traverse_only {
        let _ = run_cmd("chmod", &["a+x", &path_s]);
    } else {
        let _ = run_cmd("chmod", &["a+rx", &path_s]);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn install_user(
    executable_path: &std::path::Path,
    working_dir: &str,
    cfg: &ServiceConfig,
) -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    let service_dir = PathBuf::from(&home)
        .join(".config")
        .join("systemd")
        .join("user");
    fs::create_dir_all(&service_dir).map_err(|e| e.to_string())?;
    let service_file = service_dir.join("cab-srv.service");
    let content = linux_unit_content(
        &executable_path.display().to_string(),
        working_dir,
        cfg,
        false,
    );
    fs::write(&service_file, content).map_err(|e| e.to_string())?;
    run_cmd("systemctl", &["--user", "daemon-reload"])?;
    run_cmd("systemctl", &["--user", "enable", "cab-srv"])?;
    let _ = run_cmd("loginctl", &["enable-linger", &whoami_user()?]);
    Ok(())
}

#[cfg(target_os = "linux")]
fn install_system(
    executable_path: &std::path::Path,
    working_dir: &str,
    cfg: &ServiceConfig,
) -> Result<(), String> {
    ensure_linux_system_user(&cfg.cab_home)?;
    ensure_linux_service_can_run(executable_path, cfg)?;
    let service_file = PathBuf::from("/etc/systemd/system/cab-srv.service");
    let content = linux_unit_content(
        &executable_path.display().to_string(),
        working_dir,
        cfg,
        true,
    );
    fs::write(&service_file, content).map_err(|e| {
        format!(
            "Failed to write {}: {e} (need root?)",
            service_file.display()
        )
    })?;
    run_cmd("systemctl", &["daemon-reload"])?;
    run_cmd("systemctl", &["enable", "cab-srv"])?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn uninstall_user() -> Result<(), String> {
    let _ = run_cmd("systemctl", &["--user", "disable", "--now", "cab-srv"]);
    if let Ok(home) = std::env::var("HOME") {
        let path = PathBuf::from(home).join(".config/systemd/user/cab-srv.service");
        let _ = fs::remove_file(path);
        let _ = run_cmd("systemctl", &["--user", "daemon-reload"]);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn uninstall_system() -> Result<(), String> {
    let _ = run_cmd("systemctl", &["disable", "--now", "cab-srv"]);
    let _ = fs::remove_file("/etc/systemd/system/cab-srv.service");
    let _ = run_cmd("systemctl", &["daemon-reload"]);
    // Leave the `cab` system user and /var/lib/cab data; operator may want to keep or purge.
    Ok(())
}

#[cfg(target_os = "linux")]
fn whoami_user() -> Result<String, String> {
    let out = std::process::Command::new("whoami")
        .output()
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(target_os = "macos")]
fn install_user(
    executable_path: &std::path::Path,
    working_dir: &str,
    cfg: &ServiceConfig,
) -> Result<(), String> {
    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    let plist_path = PathBuf::from(&home).join("Library/LaunchAgents/com.cab.cab-srv.plist");
    write_launchd_plist(&plist_path, executable_path, working_dir, cfg, false, None)?;
    let plist_str = plist_path.to_string_lossy().to_string();
    macos_bootstrap_user(&plist_str)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn install_system(
    executable_path: &std::path::Path,
    working_dir: &str,
    cfg: &ServiceConfig,
) -> Result<(), String> {
    let run_as = ensure_macos_daemon_user(&cfg.cab_home)?;
    ensure_macos_daemon_can_run(executable_path, cfg, &run_as)?;
    let plist_path = PathBuf::from("/Library/LaunchDaemons/com.cab.cab-srv.plist");
    write_launchd_plist(
        &plist_path,
        executable_path,
        working_dir,
        cfg,
        true,
        Some(&run_as),
    )?;
    // Log files created as root during plist write — hand to daemon user.
    if run_as != "root" {
        let _ = run_cmd(
            "chown",
            &[
                "-R",
                &format!("{run_as}:{run_as}"),
                &cfg.cab_home.display().to_string(),
            ],
        );
    }
    let plist_str = plist_path.to_string_lossy().to_string();
    macos_bootstrap_system(&plist_str)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_current_uid() -> Result<String, String> {
    let out = std::process::Command::new("id")
        .arg("-u")
        .output()
        .map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(target_os = "macos")]
fn macos_bootstrap_user(plist: &str) -> Result<(), String> {
    let uid = macos_current_uid()?;
    let domain = format!("gui/{uid}");
    let label = format!("{domain}/com.cab.cab-srv");
    let _ = run_cmd("launchctl", &["bootout", &domain, plist]);
    let _ = run_cmd("launchctl", &["unload", plist]);
    if run_cmd("launchctl", &["bootstrap", &domain, plist]).is_ok() {
        let _ = run_cmd("launchctl", &["enable", &label]);
        let _ = run_cmd("launchctl", &["kickstart", "-k", &label]);
        return Ok(());
    }
    run_cmd("launchctl", &["load", "-w", plist])
}

#[cfg(target_os = "macos")]
fn macos_bootstrap_system(plist: &str) -> Result<(), String> {
    let _ = run_cmd("launchctl", &["bootout", "system", plist]);
    let _ = run_cmd("launchctl", &["unload", plist]);
    if run_cmd("launchctl", &["bootstrap", "system", plist]).is_ok() {
        let _ = run_cmd("launchctl", &["enable", "system/com.cab.cab-srv"]);
        let _ = run_cmd("launchctl", &["kickstart", "-k", "system/com.cab.cab-srv"]);
        return Ok(());
    }
    run_cmd("launchctl", &["load", "-w", plist])
}

#[cfg(target_os = "macos")]
fn macos_uid_or_gid_in_use(id: u32) -> bool {
    let id_s = id.to_string();
    // `id <uid>` succeeds if a user owns that UID.
    if std::process::Command::new("id")
        .arg(&id_s)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return true;
    }
    // Also scan dscl UniqueID / PrimaryGroupID lists.
    for (path, key) in [("/Users", "UniqueID"), ("/Groups", "PrimaryGroupID")] {
        if let Ok(out) = std::process::Command::new("dscl")
            .args([".", "-list", path, key])
            .output()
        {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if line.split_whitespace().any(|t| t == id_s) {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(target_os = "macos")]
fn pick_macos_daemon_id() -> u32 {
    // Prefer classic daemon range; skip collisions.
    for id in 289u32..400 {
        if !macos_uid_or_gid_in_use(id) {
            return id;
        }
    }
    for id in 400u32..500 {
        if !macos_uid_or_gid_in_use(id) {
            return id;
        }
    }
    289 // last resort; create may still fail and we fall back to root
}

/// Create `_cab` daemon account when possible; returns the account name to run as.
#[cfg(target_os = "macos")]
fn ensure_macos_daemon_user(cab_home: &std::path::Path) -> Result<String, String> {
    let user = format!("_{SYSTEM_SERVICE_USER}");
    let exists = std::process::Command::new("dscl")
        .args([".", "-read", &format!("/Users/{user}")])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if exists {
        if !macos_daemon_user_looks_sane(&user) {
            eprintln!(
                "Warning: existing daemon user {user} has unexpected UniqueID ownership; \
                 LaunchDaemon will run as root."
            );
            return Ok("root".into());
        }
    } else {
        let id = pick_macos_daemon_id();
        let id_s = id.to_string();
        let _ = run_cmd("dscl", &[".", "-create", &format!("/Groups/{user}")]);
        let _ = run_cmd(
            "dscl",
            &[
                ".",
                "-create",
                &format!("/Groups/{user}"),
                "PrimaryGroupID",
                &id_s,
            ],
        );
        let _ = run_cmd("dscl", &[".", "-create", &format!("/Users/{user}")]);
        let _ = run_cmd(
            "dscl",
            &[".", "-create", &format!("/Users/{user}"), "UniqueID", &id_s],
        );
        let _ = run_cmd(
            "dscl",
            &[
                ".",
                "-create",
                &format!("/Users/{user}"),
                "PrimaryGroupID",
                &id_s,
            ],
        );
        let _ = run_cmd(
            "dscl",
            &[
                ".",
                "-create",
                &format!("/Users/{user}"),
                "UserShell",
                "/usr/bin/false",
            ],
        );
        let _ = run_cmd(
            "dscl",
            &[
                ".",
                "-create",
                &format!("/Users/{user}"),
                "NFSHomeDirectory",
                "/var/empty",
            ],
        );
        let _ = run_cmd(
            "dscl",
            &[
                ".",
                "-create",
                &format!("/Users/{user}"),
                "RealName",
                "CAB Gateway Daemon",
            ],
        );
    }

    fs::create_dir_all(cab_home).map_err(|e| e.to_string())?;
    let _ = run_cmd(
        "chown",
        &[
            "-R",
            &format!("{user}:{user}"),
            &cab_home.display().to_string(),
        ],
    );

    let ok = std::process::Command::new("dscl")
        .args([".", "-read", &format!("/Users/{user}"), "UniqueID"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok && macos_daemon_user_looks_sane(&user) {
        Ok(user)
    } else {
        eprintln!(
            "Warning: could not create/validate daemon user {user}; LaunchDaemon will run as root."
        );
        Ok("root".into())
    }
}

/// True when `id -un <UniqueID>` resolves back to `user` (no UID hijack / collision).
#[cfg(target_os = "macos")]
fn macos_daemon_user_looks_sane(user: &str) -> bool {
    let out = std::process::Command::new("dscl")
        .args([".", "-read", &format!("/Users/{user}"), "UniqueID"])
        .output()
        .ok();
    let Some(out) = out else {
        return false;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    // Typical: "UniqueID: 289"
    let Some(uid) = text
        .split_whitespace()
        .filter(|t| t.chars().all(|c| c.is_ascii_digit()))
        .next_back()
    else {
        return false;
    };
    let name = std::process::Command::new("id")
        .args(["-un", uid])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string());
    name.as_deref() == Some(user)
}

#[cfg(target_os = "macos")]
fn ensure_macos_daemon_can_run(
    executable_path: &std::path::Path,
    cfg: &ServiceConfig,
    run_as: &str,
) -> Result<(), String> {
    if run_as == "root" {
        return Ok(());
    }
    // World rx is the portable macOS approach without breaking SIP-protected trees.
    let _ = run_cmd("chmod", &["a+rx", &executable_path.display().to_string()]);
    if let Some(fe) = &cfg.frontend_dir {
        let _ = run_cmd("chmod", &["-R", "a+rX", &fe.display().to_string()]);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn write_launchd_plist(
    plist_path: &std::path::Path,
    executable_path: &std::path::Path,
    working_dir: &str,
    cfg: &ServiceConfig,
    system: bool,
    run_as: Option<&str>,
) -> Result<(), String> {
    if let Some(parent) = plist_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let log_dir = cfg.cab_home.join("logs");
    fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
    let stdout_log = log_dir.join("cab-srv.stdout.log");
    let stderr_log = log_dir.join("cab-srv.stderr.log");
    let content = launchd_plist_content(
        &executable_path.display().to_string(),
        working_dir,
        cfg,
        &stdout_log.display().to_string(),
        &stderr_log.display().to_string(),
        if system { run_as } else { None },
    );
    fs::write(plist_path, content)
        .map_err(|e| format!("Failed to write {}: {e}", plist_path.display()))?;
    if system {
        let _ = run_cmd("chown", &["root:wheel", &plist_path.display().to_string()]);
        let _ = run_cmd("chmod", &["644", &plist_path.display().to_string()]);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn uninstall_user() -> Result<(), String> {
    if let Ok(home) = std::env::var("HOME") {
        let plist = PathBuf::from(home).join("Library/LaunchAgents/com.cab.cab-srv.plist");
        let s = plist.to_string_lossy().to_string();
        let _ = run_cmd("launchctl", &["unload", &s]);
        let _ = fs::remove_file(plist);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn uninstall_system() -> Result<(), String> {
    let plist = PathBuf::from("/Library/LaunchDaemons/com.cab.cab-srv.plist");
    let s = plist.to_string_lossy().to_string();
    let _ = run_cmd("launchctl", &["bootout", "system", &s]);
    let _ = run_cmd("launchctl", &["unload", &s]);
    let _ = fs::remove_file(plist);
    Ok(())
}

#[cfg(target_os = "windows")]
fn install_user(
    executable_path: &std::path::Path,
    _working_dir: &str,
    cfg: &ServiceConfig,
) -> Result<(), String> {
    // Task Scheduler ONLOGON via hidden VBS → .cmd: no visible console for cab-srv.
    let launcher = cfg.cab_home.join("start-cab-srv.cmd");
    let vbs = cfg.cab_home.join("start-cab-srv.vbs");
    let mut bat = format!(
        "@echo off\r\nset \"CAB_HOME={}\"\r\n",
        cfg.cab_home.display()
    );
    if let Some(fe) = &cfg.frontend_dir {
        bat.push_str(&format!(
            "set \"CAB_FRONTEND_DIR={}\"\r\n",
            fe.display()
        ));
    }
    bat.push_str(&format!(
        "\"{}\" >> \"%CAB_HOME%\\logs\\cab-srv.stdout.log\" 2>> \"%CAB_HOME%\\logs\\cab-srv.stderr.log\"\r\n",
        executable_path.display()
    ));
    fs::create_dir_all(cfg.cab_home.join("logs")).map_err(|e| e.to_string())?;
    fs::write(&launcher, bat).map_err(|e| e.to_string())?;

    // WindowStyle 0 = hidden; keeps console cab-srv attached to an invisible cmd.
    let vbs_body = format!(
        "Set sh = CreateObject(\"WScript.Shell\")\r\n\
         sh.Run \"cmd /c \"\"{}\"\"\", 0, False\r\n",
        launcher.display()
    );
    fs::write(&vbs, vbs_body).map_err(|e| e.to_string())?;

    let xml_path = cfg.cab_home.join("cab-srv.task.xml");
    let xml = windows_user_task_xml(&vbs.display().to_string());
    // Task Scheduler expects UTF-16 LE XML when /XML is used with encoding declaration.
    let utf16: Vec<u8> = {
        let mut bytes = Vec::with_capacity(2 + xml.len() * 2);
        bytes.extend_from_slice(&[0xFF, 0xFE]); // BOM
        for u in xml.encode_utf16() {
            bytes.extend_from_slice(&u.to_le_bytes());
        }
        bytes
    };
    fs::write(&xml_path, utf16).map_err(|e| e.to_string())?;

    let xml_err = match run_cmd(
        "schtasks",
        &[
            "/Create",
            "/TN",
            "CAB\\cab-srv",
            "/XML",
            &xml_path.display().to_string(),
            "/F",
        ],
    ) {
        Ok(()) => return Ok(()),
        Err(e) => e,
    };

    // Fallback when /XML is denied (e.g. prior elevated task ACLs): plain /TR create.
    let tr = format!("wscript.exe \"{}\"", vbs.display());
    run_cmd(
        "schtasks",
        &[
            "/Create",
            "/TN",
            "CAB\\cab-srv",
            "/TR",
            &tr,
            "/SC",
            "ONLOGON",
            "/RL",
            "LIMITED",
            "/F",
        ],
    )
    .map_err(|e| format!("Failed to register scheduled task (XML: {xml_err}; /TR: {e})"))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn install_system(
    executable_path: &std::path::Path,
    _working_dir: &str,
    cfg: &ServiceConfig,
) -> Result<(), String> {
    // Windows Service via sc.exe; cab-srv.exe --service handles SCM.
    let bin_path = format!("\"{}\" --service", executable_path.display());
    let _ = run_cmd("sc", &["stop", "cab-srv"]);
    let _ = run_cmd("sc", &["delete", "cab-srv"]);

    run_cmd(
        "sc",
        &[
            "create",
            "cab-srv",
            &format!("binPath= {bin_path}"),
            "start= auto",
            "DisplayName= CAB Coding Agents Bridge",
            // Least-privilege service account (not LocalSystem).
            "obj= NT AUTHORITY\\LocalService",
            "password= ",
        ],
    )?;

    // Failure recovery: restart up to 3 times with 5s delay; reset counter daily.
    let _ = run_cmd(
        "sc",
        &[
            "failure",
            "cab-srv",
            "reset= 86400",
            "actions= restart/5000/restart/5000/restart/5000",
        ],
    );
    let _ = run_cmd("sc", &["failureflag", "cab-srv", "1"]);

    // Service-scoped environment (NOT machine-wide setx /M).
    set_windows_service_environment(cfg)?;

    // Grant LocalService rights on data dir (and ensure it exists).
    fs::create_dir_all(&cfg.cab_home).map_err(|e| e.to_string())?;
    let _ = run_cmd(
        "icacls",
        &[
            &cfg.cab_home.display().to_string(),
            "/grant",
            "NT AUTHORITY\\LocalService:(OI)(CI)M",
            "/T",
        ],
    );
    // Narrow ACL: executable file only (not the whole install tree).
    let _ = run_cmd(
        "icacls",
        &[
            &executable_path.display().to_string(),
            "/grant",
            "NT AUTHORITY\\LocalService:RX",
        ],
    );
    if let Some(fe) = &cfg.frontend_dir {
        let _ = run_cmd(
            "icacls",
            &[
                &fe.display().to_string(),
                "/grant",
                "NT AUTHORITY\\LocalService:(OI)(CI)RX",
                "/T",
            ],
        );
    }
    Ok(())
}

/// Write REG_MULTI_SZ `Environment` under the service key (service-only, not global).
#[cfg(target_os = "windows")]
fn set_windows_service_environment(cfg: &ServiceConfig) -> Result<(), String> {
    let mut values = vec![format!("CAB_HOME={}", cfg.cab_home.display())];
    if let Some(fe) = &cfg.frontend_dir {
        values.push(format!("CAB_FRONTEND_DIR={}", fe.display()));
    }
    // Build PowerShell MultiString assignment safely.
    let ps_array = values
        .iter()
        .map(|v| format!("'{}'", v.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",");
    let ps = format!(
        "$path = 'HKLM:\\SYSTEM\\CurrentControlSet\\Services\\cab-srv'; \
         if (-not (Test-Path $path)) {{ throw 'Service registry key missing' }}; \
         New-ItemProperty -Path $path -Name Environment -PropertyType MultiString \
           -Value @({ps_array}) -Force | Out-Null"
    );
    run_cmd("powershell", &["-NoProfile", "-Command", &ps])
}

#[cfg(target_os = "windows")]
fn uninstall_user() -> Result<(), String> {
    let _ = run_cmd("schtasks", &["/Delete", "/TN", "CAB\\cab-srv", "/F"]);
    Ok(())
}

#[cfg(target_os = "windows")]
fn uninstall_system() -> Result<(), String> {
    let cfg = super::scope::load_service_config();
    let exe = get_cab_srv_executable_path().ok();

    // Drop service-scoped Environment before deleting the service key.
    let _ = run_cmd(
        "powershell",
        &[
            "-NoProfile",
            "-Command",
            "Remove-ItemProperty -Path 'HKLM:\\SYSTEM\\CurrentControlSet\\Services\\cab-srv' \
             -Name Environment -ErrorAction SilentlyContinue",
        ],
    );
    let _ = run_cmd("sc", &["stop", "cab-srv"]);
    let _ = run_cmd("sc", &["delete", "cab-srv"]);

    // Revoke LocalService ACLs granted at install (best-effort).
    if let Some(cfg) = cfg.as_ref() {
        revoke_windows_localservice_acl(&cfg.cab_home.display().to_string());
        if let Some(fe) = &cfg.frontend_dir {
            revoke_windows_localservice_acl(&fe.display().to_string());
        }
    }
    if let Some(exe) = exe.as_ref() {
        revoke_windows_localservice_acl(&exe.display().to_string());
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn revoke_windows_localservice_acl(path: &str) {
    let _ = run_cmd("icacls", &[path, "/remove", "NT AUTHORITY\\LocalService"]);
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn install_user(_: &std::path::Path, _: &str, _: &ServiceConfig) -> Result<(), String> {
    Err("Unsupported OS".into())
}
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn install_system(_: &std::path::Path, _: &str, _: &ServiceConfig) -> Result<(), String> {
    Err("Unsupported OS".into())
}
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn uninstall_user() -> Result<(), String> {
    Err("Unsupported OS".into())
}
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn uninstall_system() -> Result<(), String> {
    Err("Unsupported OS".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_user_unit_includes_cab_home_and_default_target() {
        let cfg = ServiceConfig {
            scope: ServiceScope::User,
            cab_home: PathBuf::from("/home/me/.cab"),
            frontend_dir: Some(PathBuf::from("/usr/share/cab/ui")),
        };
        let unit = linux_unit_content("/usr/bin/cab-srv", "/usr/bin", &cfg, false);
        assert!(unit.contains("Environment=CAB_HOME=/home/me/.cab"));
        assert!(unit.contains("Environment=CAB_FRONTEND_DIR=/usr/share/cab/ui"));
        assert!(unit.contains("WantedBy=default.target"));
        assert!(unit.contains("ExecStart=/usr/bin/cab-srv"));
        assert!(!unit.contains("User=cab"));
        assert!(!unit.contains("ProtectSystem=strict"));
        assert!(!unit.contains("UMask=0077"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_system_unit_uses_hardening_and_cab_user() {
        let cfg = ServiceConfig {
            scope: ServiceScope::System,
            cab_home: PathBuf::from("/var/lib/cab"),
            frontend_dir: Some(PathBuf::from("/usr/share/cab/ui")),
        };
        let unit = linux_unit_content("/usr/bin/cab-srv", "/usr/bin", &cfg, true);
        assert!(unit.contains("WantedBy=multi-user.target"));
        assert!(unit.contains("Environment=CAB_HOME=/var/lib/cab"));
        assert!(unit.contains("User=cab"));
        assert!(unit.contains("Group=cab"));
        assert!(unit.contains("UMask=0077"));
        assert!(unit.contains("CapabilityBoundingSet="));
        assert!(unit.contains("AmbientCapabilities="));
        assert!(unit.contains("ProtectSystem=strict"));
        assert!(unit.contains("ProtectHome=true"));
        assert!(unit.contains("NoNewPrivileges=true"));
        assert!(unit.contains("ReadWritePaths=/var/lib/cab"));
        assert!(unit.contains("ReadOnlyPaths=/usr/share/cab/ui"));
        assert!(unit.contains("Environment=CAB_FRONTEND_DIR=/usr/share/cab/ui"));
    }

    #[test]
    fn launchd_system_plist_includes_daemon_user_and_env() {
        let cfg = ServiceConfig {
            scope: ServiceScope::System,
            cab_home: PathBuf::from("/Library/Application Support/cab"),
            frontend_dir: Some(PathBuf::from("/usr/local/share/cab/ui")),
        };
        let plist = launchd_plist_content(
            "/usr/local/bin/cab-srv",
            "/usr/local/bin",
            &cfg,
            "/Library/Application Support/cab/logs/cab-srv.stdout.log",
            "/Library/Application Support/cab/logs/cab-srv.stderr.log",
            Some("_cab"),
        );
        assert!(plist.contains("<string>com.cab.cab-srv</string>"));
        assert!(plist.contains("<string>/usr/local/bin/cab-srv</string>"));
        assert!(plist.contains("<key>UserName</key>"));
        assert!(plist.contains("<string>_cab</string>"));
        assert!(plist.contains("<key>KeepAlive</key>"));
        assert!(plist.contains("<key>ThrottleInterval</key>"));
        assert!(plist.contains("<string>/Library/Application Support/cab</string>"));
        assert!(plist.contains("<string>/usr/local/share/cab/ui</string>"));
    }

    #[test]
    fn windows_task_xml_has_logon_trigger_and_restart() {
        let xml = windows_user_task_xml(r"C:\Users\me\.cab\start-cab-srv.vbs");
        assert!(xml.contains("<LogonTrigger>"));
        assert!(xml.contains("<RunLevel>LeastPrivilege</RunLevel>"));
        assert!(xml.contains("<RestartOnFailure>"));
        assert!(xml.contains("<Interval>PT1M</Interval>"));
        assert!(xml.contains("<Count>5</Count>"));
        assert!(xml.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
        assert!(xml.contains("<Command>wscript.exe</Command>"));
        assert!(xml.contains(r#"<Arguments>"C:\Users\me\.cab\start-cab-srv.vbs"</Arguments>"#));
        assert!(xml.contains("&amp;") || !xml.contains(" & "));
    }

    #[test]
    fn windows_task_xml_escapes_ampersand_in_path() {
        let xml = windows_user_task_xml(r"C:\foo&bar\start.vbs");
        assert!(xml.contains(r#"<Arguments>"C:\foo&amp;bar\start.vbs"</Arguments>"#));
        assert!(!xml.contains(r"C:\foo&bar\start.vbs"));
    }
}
