use super::shared::{backup_agent_config, cab_identifying_headers, opencode_model_config};
use super::{AgentConfigContext, AgentIntegration};
use serde_json::Value;
use std::fs;
use std::path::Path as StdPath;

pub struct Integration;

impl AgentIntegration for Integration {
    fn id(&self) -> &'static str {
        "opencode"
    }

    fn apply(&self, ctx: &AgentConfigContext<'_>) -> Result<(), std::io::Error> {
        let agent_id = &ctx.agent.id;
        let mode = ctx.mode;
        let api_key = ctx.api_key;
        let endpoint = ctx.endpoint;
        let cab_managed = ctx.cab_managed;
        let enabled_models = ctx.enabled_models;
        let gateway_port = ctx.gateway_port;
        let gateway_key = ctx.gateway_key;
        let home = &ctx.home;

        let app_name = if agent_id == "kilocode" {
            "Kilo Code"
        } else {
            "OpenCode"
        };
        let config_dir_name = if agent_id == "kilocode" {
            "kilo"
        } else {
            "opencode"
        };
        let config_dir = StdPath::new(&home).join(".config").join(config_dir_name);
        let config_path = config_dir.join("opencode.json");

        if !config_dir.exists() {
            let _ = fs::create_dir_all(&config_dir);
        }

        let mut json: Value = if config_path.exists() {
            fs::read_to_string(&config_path)
                .ok()
                .and_then(|c| serde_json::from_str(&c).ok())
                .unwrap_or_else(|| serde_json::json!({}))
        } else {
            serde_json::json!({})
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

            let mut options_obj = serde_json::Map::new();
            options_obj.insert("baseURL".to_string(), Value::String(ep));
            options_obj.insert("apiKey".to_string(), Value::String(key));
            options_obj.insert(
                "headers".to_string(),
                Value::Object(cab_identifying_headers(agent_id)),
            );

            let npm = if endpoint.contains("anthropic") {
                "@ai-sdk/anthropic"
            } else {
                "@ai-sdk/openai-compatible"
            };

            let mut models_obj = serde_json::Map::new();
            if mode == "auto" {
                for strategy_name in ["auto", "balanced", "intelligent", "price", "speed"] {
                    models_obj.insert(
                        strategy_name.to_string(),
                        opencode_model_config(&format!("CAB {strategy_name}"), agent_id),
                    );
                }
            } else {
                for model in enabled_models {
                    models_obj.insert(
                        model.name.clone(),
                        opencode_model_config(&model.display_name, agent_id),
                    );
                    if let Some(pos) = model.name.find('/') {
                        let suffix = &model.name[pos + 1..];
                        models_obj.entry(suffix.to_string()).or_insert_with(|| {
                            opencode_model_config(&model.display_name, agent_id)
                        });
                    }
                }
            }

            let mut cab_provider = serde_json::Map::new();
            cab_provider.insert("npm".to_string(), Value::String(npm.to_string()));
            cab_provider.insert("name".to_string(), Value::String("CAB Gateway".to_string()));
            cab_provider.insert("options".to_string(), Value::Object(options_obj));
            cab_provider.insert("models".to_string(), Value::Object(models_obj));

            let mut providers_map = serde_json::Map::new();
            if let Some(existing_providers) = json.get("provider").and_then(|p| p.as_object()) {
                providers_map = existing_providers.clone();
            }
            providers_map.remove("openai");
            providers_map.insert("cab".to_string(), Value::Object(cab_provider));
            json["provider"] = Value::Object(providers_map);
        } else if let Some(providers) = json.get_mut("provider").and_then(|p| p.as_object_mut()) {
            providers.remove("cab");
            providers.remove("openai");
            if providers.is_empty()
                && let Some(obj) = json.as_object_mut()
            {
                obj.remove("provider");
            }
        }

        if let Ok(pretty) = serde_json::to_string_pretty(&json) {
            backup_agent_config(&config_path);
            fs::write(&config_path, pretty)?;
            tracing::info!(
                "Dynamic Config Switcher: Updated {} opencode.json at {} for mode {}",
                app_name,
                config_path.display(),
                mode
            );
        }
        Ok(())
    }
}
