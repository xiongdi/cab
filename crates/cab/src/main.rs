use clap::{Parser, Subcommand};
use serde::Deserialize;

mod service;

use service::{
    ScopeArg, apply_installed_cab_home, install_service, is_service_active, show_logs,
    start_daemon, stop_daemon, uninstall_service,
};

#[derive(Parser)]
#[command(name = "cab-cli", about = "Coding Agents Bridge CLI", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the cab-srv daemon service
    Start,
    /// Stop the cab-srv daemon service
    Stop,
    /// Restart the cab-srv daemon service
    Restart,
    /// Check the status of the cab-srv daemon and gateway
    Status,
    /// Show cab-srv daemon logs
    Logs {
        #[arg(short, long)]
        follow: bool,
        #[arg(short, long, default_value = "50")]
        lines: u32,
    },
    Agent {
        #[command(subcommand)]
        command: AgentCommands,
    },
    Provider {
        #[command(subcommand)]
        command: ProviderCommands,
    },
    /// Manage cab-srv background service installation
    Service {
        #[command(subcommand)]
        command: ServiceCommands,
    },
}

#[derive(Subcommand)]
enum AgentCommands {
    List,
    Set {
        id: String,
        #[arg(long)]
        mode: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        api_key: Option<String>,
        #[arg(long)]
        endpoint: Option<String>,
    },
}

#[derive(Subcommand)]
enum ProviderCommands {
    List,
    Enable { id: String },
    Disable { id: String },
    SetKey { id: String, key: String },
}

#[derive(Subcommand)]
enum ServiceCommands {
    /// Install cab-srv as a user or system service
    Install {
        /// Service scope: user (default) or system (requires admin/root)
        #[arg(long, value_enum, default_value_t = ScopeArg::User)]
        scope: ScopeArg,
    },
    /// Uninstall the installed cab-srv service
    Uninstall,
}

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

fn restart_daemon() -> Result<(), String> {
    stop_daemon()?;
    start_daemon()
}

#[tokio::main]
async fn main() {
    apply_installed_cab_home();

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
            ServiceCommands::Install { scope } => install_service(scope.into()),
            ServiceCommands::Uninstall => uninstall_service(),
        },
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn load_settings_from_db() -> Result<cab_core::types::Settings, String> {
    let path = cab_db::sqlite::db_path();
    if !path.exists() {
        return Err(
            "CAB database does not exist. Please run 'cab-srv' or start the service to initialize."
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
        Ok(data) => serde_json::from_str::<cab_core::types::Settings>(&data)
            .map_err(|e| format!("Failed to parse settings JSON: {e}")),
        Err(e) => Err(format!("Failed to load settings from DB: {e}")),
    }
}

fn api_client() -> Result<(reqwest::Client, String, u16), String> {
    let settings = load_settings_from_db()?;
    Ok((
        reqwest::Client::new(),
        settings.gateway_key,
        settings.gateway_port as u16,
    ))
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
    let scope = service::load_service_config()
        .map(|c| c.scope.as_str().to_string())
        .unwrap_or_else(|| "user (default)".into());

    println!(
        "CAB Daemon (scope={scope}): {}",
        if service_active { "Active" } else { "Inactive" }
    );
    println!("HTTP Gateway Port: {}", port);
    println!(
        "Data directory (CAB_HOME): {}",
        cab_core::paths::cab_home().display()
    );
    println!("Gateway Key (Auth Token): {}", db_settings.gateway_key);
    println!("Auth Enabled: {}", db_settings.auth_enabled);

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
    } else {
        println!(
            "\n(HTTP API is currently unreachable. Start the daemon to inspect configurations.)"
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
