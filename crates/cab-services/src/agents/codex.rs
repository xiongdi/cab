use super::shared::backup_agent_config;
use super::{AgentConfigContext, AgentIntegration};
use std::fs;
use std::path::Path as StdPath;

pub struct Integration;

impl AgentIntegration for Integration {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn apply(&self, ctx: &AgentConfigContext<'_>) -> Result<(), std::io::Error> {
        let mode = ctx.mode;
        let endpoint = ctx.endpoint;
        let strategy = ctx.strategy;
        let cab_managed = ctx.cab_managed;
        let gateway_port = ctx.gateway_port;
        let home = &ctx.home;

        let config_dir = StdPath::new(&home).join(".codex");
        let config_path = config_dir.join("config.toml");

        if !config_dir.exists() {
            let _ = fs::create_dir_all(&config_dir);
        }

        let mut toml_val: toml::Value = if config_path.exists() {
            fs::read_to_string(&config_path)
                .ok()
                .and_then(|c| c.parse::<toml::Value>().ok())
                .unwrap_or_else(|| toml::Value::Table(toml::value::Table::new()))
        } else {
            toml::Value::Table(toml::value::Table::new())
        };

        if cab_managed {
            if let Some(table) = toml_val.as_table_mut() {
                if mode == "auto" {
                    let model_name = strategy.unwrap_or("auto");
                    table.insert(
                        "model".to_string(),
                        toml::Value::String(model_name.to_string()),
                    );
                } else {
                    table.remove("model");
                }
                table.insert(
                    "model_provider".to_string(),
                    toml::Value::String("cab".to_string()),
                );
            }

            let default_ep = format!("http://localhost:{}/v1", gateway_port);
            let ep = if endpoint.is_empty() {
                default_ep
            } else {
                endpoint.to_string()
            };
            let mut cab_provider = toml::value::Table::new();
            cab_provider.insert(
                "name".to_string(),
                toml::Value::String("CAB Gateway".to_string()),
            );
            cab_provider.insert("base_url".to_string(), toml::Value::String(ep));
            cab_provider.insert(
                "env_key".to_string(),
                toml::Value::String("OPENAI_API_KEY".to_string()),
            );

            if let Some(table) = toml_val.as_table_mut() {
                let providers = table
                    .entry("model_providers".to_string())
                    .or_insert_with(|| toml::Value::Table(toml::value::Table::new()));
                if let Some(providers_table) = providers.as_table_mut() {
                    providers_table.insert("cab".to_string(), toml::Value::Table(cab_provider));
                }
            }
        } else if let Some(table) = toml_val.as_table_mut() {
            table.remove("model_provider");
            if let Some(providers) = table
                .get_mut("model_providers")
                .and_then(|p| p.as_table_mut())
            {
                providers.remove("cab");
            }
            let is_empty = table
                .get("model_providers")
                .and_then(|p| p.as_table())
                .map(|p| p.is_empty())
                .unwrap_or(false);
            if is_empty {
                table.remove("model_providers");
            }
        }

        if let Ok(pretty) = toml::to_string_pretty(&toml_val) {
            backup_agent_config(&config_path);
            fs::write(&config_path, pretty)?;
            tracing::info!(
                "Dynamic Config Switcher: Updated Codex config.toml at {} for mode {}",
                config_path.display(),
                mode
            );
        }
        Ok(())
    }
}
