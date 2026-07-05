use clap::{Parser, Subcommand};

use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

#[derive(Parser)]
#[command(name = "cab", about = "Coding Agents Bridge CLI", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the cabd daemon service
    Start,
    /// Stop the cabd daemon service
    Stop,
    /// Restart the cabd daemon service
    Restart,
    /// Check the status of the cabd daemon and gateway
    Status,
    /// Show cabd daemon logs
    Logs {
        /// Follow log output
        #[arg(short, long)]
        follow: bool,
        /// Number of journal lines to show
        #[arg(short, long, default_value = "50")]
        lines: u32,
    },
    /// Manage coding agent configurations
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
    },
    /// Manage LLM providers
    Provider {
        #[command(subcommand)]
        command: ProviderCommands,
    },
    /// Manage systemd service installation
    Service {
        #[command(subcommand)]
        command: ServiceCommands,
    },
}

#[derive(Subcommand)]
enum AgentCommands {
    /// List all supported coding agents and their configurations
    List,
    /// Configure a specific coding agent
    Set {
        /// Agent ID (e.g. claude-code, codex, opencode)
        id: String,
        /// Agent mode (native, auto, manual)
        #[arg(long)]
        mode: Option<String>,
        /// Primary model strategy/ID (e.g. auto, cheapest, intelligent)
        #[arg(long)]
        model: Option<String>,
        /// API key override for the agent
        #[arg(long)]
        api_key: Option<String>,
        /// Endpoint override URL for the agent
        #[arg(long)]
        endpoint: Option<String>,
    },
}

#[derive(Subcommand)]
enum ProviderCommands {
    /// List all configured LLM providers and their API key status
    List,
    /// Enable an LLM provider
    Enable {
        /// Provider ID (e.g. openai, anthropic, deepseek)
        id: String,
    },
    /// Disable an LLM provider
    Disable {
        /// Provider ID (e.g. openai, anthropic, deepseek)
        id: String,
    },
    /// Set the API key for an LLM provider
    SetKey {
        /// Provider ID (e.g. openai, anthropic, deepseek)
        id: String,
        /// Upstream API key
        key: String,
    },
}

#[derive(Subcommand)]
enum ServiceCommands {
    /// Install the cabd systemd user service
    Install,
    /// Uninstall the cabd systemd user service
    Uninstall,
}

// Structs matching the API response for deserialization
#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct Agent {
    id: String,
    name: String,
    mode: String,
    model_id: Option<String>,
    endpoint: String,
}

#[derive(Deserialize, Debug)]
struct ProviderEndpoint {
    url: String,
}

#[derive(Deserialize, Debug)]
struct ApiKeyConfig {
    key: String,
    enabled: bool,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct Provider {
    id: String,
    name: String,
    enabled: bool,
    api_key: String,
    api_keys: Vec<ApiKeyConfig>,
    endpoints: Vec<ProviderEndpoint>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Start => start_daemon(),
        Commands::Stop => stop_daemon(),
        Commands::Restart => restart_daemon(),
        Commands::Status => show_status().await,
        Commands::Logs { follow, lines } => show_logs(follow, lines),
        Commands::Agent { command } => match command {
            AgentCommands::List => list_agents().await,
            AgentCommands::Set {
                id,
                mode,
                model,
                api_key,
                endpoint,
            } => set_agent(id, mode, model, api_key, endpoint).await,
        },
        Commands::Provider { command } => match command {
            ProviderCommands::List => list_providers().await,
            ProviderCommands::Enable { id } => enable_provider(id).await,
            ProviderCommands::Disable { id } => disable_provider(id).await,
            ProviderCommands::SetKey { id, key } => set_provider_key(id, key).await,
        },
        Commands::Service { command } => match command {
            ServiceCommands::Install => install_service(),
            ServiceCommands::Uninstall => uninstall_service(),
        },
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

// Load settings from SQLite db directly
fn load_settings_from_db() -> Result<cab_core::types::Settings, String> {
    let path = cab_db::sqlite::db_path();
    if !path.exists() {
        return Err(
            "CAB database does not exist. Please run 'cabd' or start the service to initialize."
                .to_string(),
        );
    }
    let conn = rusqlite::Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(|e| format!("Failed to open database: {e}"))?;

    let row: Result<String, _> =
        conn.query_row("SELECT data FROM settings WHERE id = 1", [], |row| {
            row.get(0)
        });
    match row {
        Ok(data) => {
            let settings = serde_json::from_str::<cab_core::types::Settings>(&data)
                .map_err(|e| format!("Failed to parse settings JSON: {e}"))?;
            Ok(settings)
        }
        Err(e) => Err(format!("Failed to load settings from DB: {e}")),
    }
}

fn api_client() -> Result<(reqwest::Client, String, u16), String> {
    let settings = load_settings_from_db()?;
    let client = reqwest::Client::new();
    Ok((client, settings.gateway_key, settings.gateway_port as u16))
}

fn start_daemon() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        println!("Starting cabd service...");
        run_cmd("systemctl", &["--user", "start", "cabd"])
    }
    #[cfg(target_os = "macos")]
    {
        println!("Starting cabd service...");
        let plist = get_launchd_plist_path()?;
        let plist_str = plist.to_string_lossy().to_string();
        run_cmd("launchctl", &["load", &plist_str])?;
        run_cmd("launchctl", &["start", "com.cab.cabd"])
    }
    #[cfg(target_os = "windows")]
    {
        println!("Starting cabd background process...");
        let executable_path = get_cabd_executable_path()?;
        let working_dir = get_working_dir()?;

        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x00000008;
        Command::new(&executable_path)
            .current_dir(&working_dir)
            .creation_flags(DETACHED_PROCESS)
            .spawn()
            .map_err(|e| format!("Failed to spawn cabd background process: {e}"))?;

        println!("cabd process started in the background.");
        Ok(())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Err("Unsupported operating system for daemon control".to_string())
    }
}

fn stop_daemon() -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        println!("Stopping cabd service...");
        run_cmd("systemctl", &["--user", "stop", "cabd"])
    }
    #[cfg(target_os = "macos")]
    {
        println!("Stopping cabd service...");
        let plist = get_launchd_plist_path()?;
        let plist_str = plist.to_string_lossy().to_string();
        let _ = run_cmd("launchctl", &["stop", "com.cab.cabd"]);
        run_cmd("launchctl", &["unload", &plist_str])
    }
    #[cfg(target_os = "windows")]
    {
        println!("Stopping cabd process...");
        run_cmd("taskkill", &["/F", "/IM", "cabd.exe"])
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Err("Unsupported operating system for daemon control".to_string())
    }
}

fn restart_daemon() -> Result<(), String> {
    stop_daemon()?;
    start_daemon()
}

fn show_logs(follow: bool, lines: u32) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        let lines_str = lines.to_string();
        let mut args = vec!["--user", "-u", "cabd", "-n", &lines_str];
        if follow {
            args.push("-f");
        }
        let mut child = Command::new("journalctl")
            .args(&args)
            .spawn()
            .map_err(|e| format!("Failed to spawn journalctl: {e}"))?;
        let _ = child.wait();
        Ok(())
    }
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        let log_file = get_log_file_path()?;
        if !log_file.exists() {
            println!("No logs found at {}", log_file.display());
            return Ok(());
        }
        print_log_file(&log_file, lines, follow)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Err("Unsupported operating system for logs".to_string())
    }
}

#[allow(dead_code)]
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
                Ok(0) => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Ok(_) => {
                    print!("{}", line);
                }
                Err(e) => {
                    return Err(format!("Error reading logs: {e}"));
                }
            }
        }
    }
    Ok(())
}

fn run_cmd(cmd: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(cmd)
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

fn is_service_active() -> bool {
    #[cfg(target_os = "linux")]
    {
        let output = Command::new("systemctl")
            .args(["--user", "is-active", "cabd"])
            .output();
        match output {
            Ok(out) => {
                let status = String::from_utf8_lossy(&out.stdout).trim().to_string();
                status == "active"
            }
            Err(_) => false,
        }
    }
    #[cfg(target_os = "macos")]
    {
        let output = Command::new("launchctl").args(["list"]).output();
        match output {
            Ok(out) => {
                let list = String::from_utf8_lossy(&out.stdout);
                list.contains("com.cab.cabd")
            }
            Err(_) => false,
        }
    }
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("tasklist")
            .args(["/FI", "IMAGENAME eq cabd.exe"])
            .output();
        match output {
            Ok(out) => {
                let list = String::from_utf8_lossy(&out.stdout);
                list.contains("cabd.exe")
            }
            Err(_) => false,
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        false
    }
}

async fn check_api_alive(port: u16) -> bool {
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/api/settings", port);
    match client
        .get(&url)
        .timeout(std::time::Duration::from_millis(500))
        .send()
        .await
    {
        Ok(resp) => {
            resp.status().is_success() || resp.status() == reqwest::StatusCode::UNAUTHORIZED
        }
        Err(_) => false,
    }
}

async fn show_status() -> Result<(), String> {
    let db_settings = load_settings_from_db()?;
    let port = db_settings.gateway_port as u16;

    let service_active = is_service_active();
    let api_alive = check_api_alive(port).await;

    println!(
        "CAB Daemon (cabd.service): {}",
        if service_active { "Active" } else { "Inactive" }
    );
    println!("HTTP Gateway Port: {}", port);
    println!("Gateway Key (Auth Token): {}", db_settings.gateway_key);
    println!("Auth Enabled: {}", db_settings.auth_enabled);
    println!("Cache Affinity: {}", db_settings.cache_affinity_enabled);
    println!(
        "Cache Request Shaping: {}",
        db_settings.cache_request_shaping_enabled
    );

    if api_alive {
        println!("\n=== Agent Configs ===");
        let (client, key, _) = api_client()?;
        let agents_url = format!("http://127.0.0.1:{}/api/agents", port);
        let agents: Vec<Agent> = client
            .get(&agents_url)
            .header("Authorization", format!("Bearer {key}"))
            .send()
            .await
            .map_err(|e| format!("Failed to get agents: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse agents: {e}"))?;

        println!(
            "{:<15} {:<10} {:<20} {:<30}",
            "Agent ID", "Mode", "Model Strategy", "Endpoint"
        );
        println!("{}", "-".repeat(80));
        for agent in agents {
            println!(
                "{:<15} {:<10} {:<20} {:<30}",
                agent.id,
                agent.mode,
                agent.model_id.unwrap_or_else(|| "-".to_string()),
                agent.endpoint
            );
        }

        println!("\n=== Upstream Providers ===");
        let providers_url = format!("http://127.0.0.1:{}/api/providers", port);
        let providers: Vec<Provider> = client
            .get(&providers_url)
            .header("Authorization", format!("Bearer {key}"))
            .send()
            .await
            .map_err(|e| format!("Failed to get providers: {e}"))?
            .json()
            .await
            .map_err(|e| format!("Failed to parse providers: {e}"))?;

        println!(
            "{:<15} {:<10} {:<25} {:<30}",
            "Provider ID", "Status", "API Key Setup", "Primary Endpoint"
        );
        println!("{}", "-".repeat(80));
        for p in providers {
            let key_setup = if !p.api_key.trim().is_empty() {
                "Yes (legacy)"
            } else {
                let enabled_keys = p
                    .api_keys
                    .iter()
                    .filter(|k| k.enabled && !k.key.trim().is_empty())
                    .count();
                if enabled_keys > 0 {
                    &format!("Yes ({enabled_keys} keys)")
                } else {
                    "No"
                }
            };
            let endpoint_url = p.endpoints.first().map(|e| e.url.as_str()).unwrap_or("-");
            println!(
                "{:<15} {:<10} {:<25} {:<30}",
                p.id,
                if p.enabled { "Enabled" } else { "Disabled" },
                key_setup,
                endpoint_url
            );
        }
    } else {
        println!(
            "\n(HTTP API is currently unreachable. Start the daemon to inspect active configurations.)"
        );
    }

    Ok(())
}

async fn list_agents() -> Result<(), String> {
    let (client, key, port) = api_client()?;
    let url = format!("http://127.0.0.1:{}/api/agents", port);

    let agents: Vec<Agent> = client
        .get(&url)
        .header("Authorization", format!("Bearer {key}"))
        .send()
        .await
        .map_err(|e| format!("Failed to contact daemon: is it running? ({e})"))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse agents list: {e}"))?;

    println!(
        "{:<15} {:<10} {:<20} {:<30}",
        "Agent ID", "Mode", "Model Strategy", "Endpoint"
    );
    println!("{}", "-".repeat(80));
    for agent in agents {
        println!(
            "{:<15} {:<10} {:<20} {:<30}",
            agent.id,
            agent.mode,
            agent.model_id.unwrap_or_else(|| "-".to_string()),
            agent.endpoint
        );
    }
    Ok(())
}

async fn set_agent(
    id: String,
    mode: Option<String>,
    model: Option<String>,
    api_key: Option<String>,
    endpoint: Option<String>,
) -> Result<(), String> {
    let (client, key, port) = api_client()?;
    let url = format!("http://127.0.0.1:{}/api/agents/{}", port, id);

    let input = cab_core::types::UpdateAgent {
        mode,
        model_id: model.map(Some),
        api_key,
        endpoint,
    };

    let resp = client
        .put(&url)
        .header("Authorization", format!("Bearer {key}"))
        .json(&input)
        .send()
        .await
        .map_err(|e| format!("Failed to contact daemon: is it running? ({e})"))?;

    let status = resp.status();
    if status.is_success() {
        println!("Successfully updated configuration for agent '{}'.", id);
        Ok(())
    } else {
        let err_body = resp.text().await.unwrap_or_default();
        Err(format!(
            "Failed to update agent: HTTP {} - {}",
            status, err_body
        ))
    }
}

async fn list_providers() -> Result<(), String> {
    let (client, key, port) = api_client()?;
    let url = format!("http://127.0.0.1:{}/api/providers", port);

    let providers: Vec<Provider> = client
        .get(&url)
        .header("Authorization", format!("Bearer {key}"))
        .send()
        .await
        .map_err(|e| format!("Failed to contact daemon: is it running? ({e})"))?
        .json()
        .await
        .map_err(|e| format!("Failed to parse providers list: {e}"))?;

    println!(
        "{:<15} {:<10} {:<25} {:<30}",
        "Provider ID", "Status", "API Key Setup", "Primary Endpoint"
    );
    println!("{}", "-".repeat(80));
    for p in providers {
        let key_setup = if !p.api_key.trim().is_empty() {
            "Yes (legacy)"
        } else {
            let enabled_keys = p
                .api_keys
                .iter()
                .filter(|k| k.enabled && !k.key.trim().is_empty())
                .count();
            if enabled_keys > 0 {
                &format!("Yes ({enabled_keys} keys)")
            } else {
                "No"
            }
        };
        let endpoint_url = p.endpoints.first().map(|e| e.url.as_str()).unwrap_or("-");
        println!(
            "{:<15} {:<10} {:<25} {:<30}",
            p.id,
            if p.enabled { "Enabled" } else { "Disabled" },
            key_setup,
            endpoint_url
        );
    }
    Ok(())
}

async fn enable_provider(id: String) -> Result<(), String> {
    let (client, key, port) = api_client()?;
    let url = format!("http://127.0.0.1:{}/api/providers/{}", port, id);

    let input = cab_core::types::UpdateProvider {
        enabled: Some(true),
        ..Default::default()
    };

    let resp = client
        .put(&url)
        .header("Authorization", format!("Bearer {key}"))
        .json(&input)
        .send()
        .await
        .map_err(|e| format!("Failed to contact daemon: is it running? ({e})"))?;

    let status = resp.status();
    if status.is_success() {
        println!("Successfully enabled provider '{}'.", id);
        Ok(())
    } else {
        let err_body = resp.text().await.unwrap_or_default();
        Err(format!(
            "Failed to enable provider: HTTP {} - {}",
            status, err_body
        ))
    }
}

async fn disable_provider(id: String) -> Result<(), String> {
    let (client, key, port) = api_client()?;
    let url = format!("http://127.0.0.1:{}/api/providers/{}", port, id);

    let input = cab_core::types::UpdateProvider {
        enabled: Some(false),
        ..Default::default()
    };

    let resp = client
        .put(&url)
        .header("Authorization", format!("Bearer {key}"))
        .json(&input)
        .send()
        .await
        .map_err(|e| format!("Failed to contact daemon: is it running? ({e})"))?;

    let status = resp.status();
    if status.is_success() {
        println!("Successfully disabled provider '{}'.", id);
        Ok(())
    } else {
        let err_body = resp.text().await.unwrap_or_default();
        Err(format!(
            "Failed to disable provider: HTTP {} - {}",
            status, err_body
        ))
    }
}

async fn set_provider_key(id: String, key_value: String) -> Result<(), String> {
    let (client, key, port) = api_client()?;
    let url = format!("http://127.0.0.1:{}/api/providers/{}", port, id);

    // Build the ApiKeyConfig entry
    let api_keys = vec![cab_core::types::ApiKeyConfig {
        key: key_value.clone(),
        enabled: true,
        quota_reset_at: None,
    }];

    let input = cab_core::types::UpdateProvider {
        api_keys: Some(api_keys),
        // If enabling, also set enabled to true so they don't hit "Cannot enable a provider without configuring..." later
        enabled: Some(true),
        ..Default::default()
    };

    let resp = client
        .put(&url)
        .header("Authorization", format!("Bearer {key}"))
        .json(&input)
        .send()
        .await
        .map_err(|e| format!("Failed to contact daemon: is it running? ({e})"))?;

    let status = resp.status();
    if status.is_success() {
        println!("Successfully set API key and enabled provider '{}'.", id);
        Ok(())
    } else {
        let err_body = resp.text().await.unwrap_or_default();
        Err(format!(
            "Failed to configure API key: HTTP {} - {}",
            status, err_body
        ))
    }
}

// Service install/uninstall logic
#[cfg(target_os = "linux")]
fn get_systemd_service_path() -> Result<PathBuf, String> {
    let home =
        std::env::var("HOME").map_err(|_| "HOME environment variable not set".to_string())?;
    let path = PathBuf::from(home)
        .join(".config")
        .join("systemd")
        .join("user");
    Ok(path)
}

#[cfg(target_os = "macos")]
fn get_launchd_plist_path() -> Result<PathBuf, String> {
    let home =
        std::env::var("HOME").map_err(|_| "HOME environment variable not set".to_string())?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("LaunchAgents")
        .join("com.cab.cabd.plist"))
}

fn get_cabd_executable_path() -> Result<PathBuf, String> {
    let current_exe = std::env::current_exe()
        .map_err(|e| format!("Failed to determine current executable path: {e}"))?;
    let current_dir = current_exe.parent().unwrap();

    let cabd_target_name = if cfg!(target_os = "windows") {
        "cabd.exe"
    } else {
        "cabd"
    };
    let cabd_target_path = current_dir.join(cabd_target_name);
    if cabd_target_path.exists() {
        return Ok(cabd_target_path);
    }

    let server_target_name = if cfg!(target_os = "windows") {
        "cab-server.exe"
    } else {
        "cab-server"
    };
    let server_target_path = current_dir.join(server_target_name);
    if server_target_path.exists() {
        return Ok(server_target_path);
    }

    // Default fallback
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Home directory not set".to_string())?;
    let fallback_bin = PathBuf::from(home)
        .join(".local")
        .join("bin")
        .join(cabd_target_name);
    Ok(fallback_bin)
}

fn get_working_dir() -> Result<String, String> {
    let workspace = "/home/xiongdi/workspace/cab";
    if std::path::Path::new(workspace).exists() {
        return Ok(workspace.to_string());
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Home directory not set".to_string())?;
    Ok(home)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn get_log_file_path() -> Result<PathBuf, String> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Home directory not set".to_string())?;
    Ok(PathBuf::from(home)
        .join(".cab")
        .join("logs")
        .join("cabd.stdout.log"))
}

fn install_service() -> Result<(), String> {
    let executable_path = get_cabd_executable_path()?;
    let working_dir = get_working_dir()?;
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Home directory not set".to_string())?;
    let _ = home;

    #[cfg(target_os = "linux")]
    {
        let service_dir = get_systemd_service_path()?;
        std::fs::create_dir_all(&service_dir)
            .map_err(|e| format!("Failed to create service dir: {e}"))?;
        let service_file = service_dir.join("cabd.service");

        let service_content = format!(
            "[Unit]\n\
             Description=CAB (Coding Agents Bridge) Daemon\n\
             After=network.target\n\n\
             [Service]\n\
             Type=simple\n\
             ExecStart={}\n\
             Restart=always\n\
             RestartSec=5\n\
             WorkingDirectory={}\n\
             StandardOutput=journal\n\
             StandardError=journal\n\n\
             [Install]\n\
             WantedBy=default.target\n",
            executable_path.display(),
            working_dir
        );
        std::fs::write(&service_file, service_content)
            .map_err(|e| format!("Failed to write service file: {e}"))?;
        println!("Wrote systemd service unit to {}", service_file.display());
        run_cmd("systemctl", &["--user", "daemon-reload"])?;
        run_cmd("systemctl", &["--user", "enable", "cabd"])?;
        println!("cabd service installed and enabled successfully.");
        println!("To start it: cab start");
    }
    #[cfg(target_os = "macos")]
    {
        let plist_path = get_launchd_plist_path()?;
        let plist_dir = plist_path.parent().unwrap();
        std::fs::create_dir_all(plist_dir)
            .map_err(|e| format!("Failed to create launchd agent directory: {e}"))?;

        let log_dir = PathBuf::from(&home).join(".cab").join("logs");
        std::fs::create_dir_all(&log_dir).map_err(|e| format!("Failed to create log dir: {e}"))?;
        let stdout_log = log_dir.join("cabd.stdout.log");
        let stderr_log = log_dir.join("cabd.stderr.log");

        let plist_content = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
             <plist version=\"1.0\">\n\
             <dict>\n\
             \t<key>Label</key>\n\
             \t<string>com.cab.cabd</string>\n\
             \t<key>ProgramArguments</key>\n\
             \t<array>\n\
             \t\t<string>{}</string>\n\
             \t</array>\n\
             \t<key>RunAtLoad</key>\n\
             \t<true/>\n\
             \t<key>KeepAlive</key>\n\
             \t<true/>\n\
             \t<key>WorkingDirectory</key>\n\
             \t<string>{}</string>\n\
             \t<key>StandardOutPath</key>\n\
             \t<string>{}</string>\n\
             \t<key>StandardErrorPath</key>\n\
             \t<string>{}</string>\n\
             </dict>\n\
             </plist>\n",
            executable_path.display(),
            working_dir,
            stdout_log.display(),
            stderr_log.display()
        );
        std::fs::write(&plist_path, plist_content)
            .map_err(|e| format!("Failed to write plist file: {e}"))?;
        println!("Wrote launchd agent plist to {}", plist_path.display());
        let plist_str = plist_path.to_string_lossy().to_string();
        run_cmd("launchctl", &["load", &plist_str])?;
        println!("cabd launchd agent installed and loaded successfully.");
        println!("To start it: cab start");
    }
    #[cfg(target_os = "windows")]
    {
        let startup_dir = PathBuf::from(&home)
            .join("AppData")
            .join("Roaming")
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Startup");

        if !startup_dir.exists() {
            return Err(format!(
                "Windows Startup directory not found at {}",
                startup_dir.display()
            ));
        }

        let startup_bat = startup_dir.join("cabd_startup.bat");
        let bat_content = format!(
            "@echo off\n\
             cd /d \"{}\"\n\
             start /b \"cabd\" \"{}\" > NUL 2>&1\n",
            working_dir,
            executable_path.display()
        );
        std::fs::write(&startup_bat, bat_content)
            .map_err(|e| format!("Failed to write startup batch script: {e}"))?;
        println!("Wrote startup script to {}", startup_bat.display());
        println!("cabd shortcut added to startup folder successfully.");
        println!("To start it now: cab start");
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        return Err("Service installation is not supported on this OS".to_string());
    }
    Ok(())
}

fn uninstall_service() -> Result<(), String> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| "Home not set".to_string())?;
    let _ = home;

    #[cfg(target_os = "linux")]
    {
        let service_dir = get_systemd_service_path()?;
        let service_file = service_dir.join("cabd.service");
        if !service_file.exists() {
            println!("Service is not installed.");
            return Ok(());
        }
        println!("Disabling and stopping cabd service...");
        let _ = run_cmd("systemctl", &["--user", "disable", "cabd"]);
        let _ = run_cmd("systemctl", &["--user", "stop", "cabd"]);
        std::fs::remove_file(&service_file)
            .map_err(|e| format!("Failed to remove service file: {e}"))?;
        run_cmd("systemctl", &["--user", "daemon-reload"])?;
    }
    #[cfg(target_os = "macos")]
    {
        let plist_path = get_launchd_plist_path()?;
        if !plist_path.exists() {
            println!("Service is not installed.");
            return Ok(());
        }
        println!("Unloading and stopping cabd service...");
        let plist_str = plist_path.to_string_lossy().to_string();
        let _ = run_cmd("launchctl", &["unload", &plist_str]);
        std::fs::remove_file(&plist_path)
            .map_err(|e| format!("Failed to remove plist file: {e}"))?;
    }
    #[cfg(target_os = "windows")]
    {
        let startup_dir = PathBuf::from(&home)
            .join("AppData")
            .join("Roaming")
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Startup");
        let startup_bat = startup_dir.join("cabd_startup.bat");
        if !startup_bat.exists() {
            println!("Service shortcut is not installed.");
            return Ok(());
        }
        println!("Stopping cabd process...");
        let _ = run_cmd("taskkill", &["/F", "/IM", "cabd.exe"]);
        std::fs::remove_file(&startup_bat)
            .map_err(|e| format!("Failed to remove startup script: {e}"))?;
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        return Err("Service uninstallation is not supported on this OS".to_string());
    }
    println!("cabd service uninstalled successfully.");
    Ok(())
}
