use super::shared::{backup_agent_config, build_hermes_model_block, replace_top_level_yaml_block};
use super::{AgentConfigContext, AgentIntegration};
use std::fs;
use std::path::Path as StdPath;

pub struct Integration;

impl AgentIntegration for Integration {
    fn id(&self) -> &'static str {
        "hermes"
    }

    fn apply(&self, ctx: &AgentConfigContext<'_>) -> Result<(), std::io::Error> {
        let mode = ctx.mode;
        let api_key = ctx.api_key;
        let endpoint = ctx.endpoint;
        let strategy = ctx.strategy;
        let cab_managed = ctx.cab_managed;
        let gateway_port = ctx.gateway_port;
        let gateway_key = ctx.gateway_key;
        let home = &ctx.home;

        let config_dir = StdPath::new(&home).join(".hermes");
        let config_path = config_dir.join("config.yaml");

        if !config_dir.exists() {
            let _ = fs::create_dir_all(&config_dir);
        }

        let mut content = if config_path.exists() {
            fs::read_to_string(&config_path).unwrap_or_default()
        } else {
            String::new()
        };

        if cab_managed {
            let default_ep = format!("http://localhost:{}/v1", gateway_port);
            let ep = if endpoint.is_empty() {
                default_ep
            } else {
                endpoint.to_string()
            };
            let key = if api_key.is_empty() {
                gateway_key.to_string()
            } else {
                api_key.to_string()
            };
            let model_name = strategy.unwrap_or("auto");
            let model_block = build_hermes_model_block(model_name, &ep, &key);
            content = replace_top_level_yaml_block(&content, "model", &model_block);
        } else {
            let native_block = "model: \"\"";
            content = replace_top_level_yaml_block(&content, "model", native_block);
        }

        backup_agent_config(&config_path);
        fs::write(&config_path, content)?;
        tracing::info!(
            "Dynamic Config Switcher: Updated Hermes config.yaml at {} for mode {}",
            config_path.display(),
            mode
        );
        Ok(())
    }
}
