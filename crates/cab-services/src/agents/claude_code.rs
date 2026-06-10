use super::shared::backup_agent_config;
use super::{AgentConfigContext, AgentIntegration};
use serde_json::Value;
use std::fs;
use std::path::Path as StdPath;

pub struct Integration;

impl AgentIntegration for Integration {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    fn apply(&self, ctx: &AgentConfigContext<'_>) -> Result<(), std::io::Error> {
        let mode = ctx.mode;
        let cab_managed = ctx.cab_managed;
        let gateway_port = ctx.gateway_port;
        let gateway_key = ctx.gateway_key;
        let home = &ctx.home;

        let config_dir = StdPath::new(&home).join(".claude");
        let config_path = config_dir.join("settings.json");

        if config_path.exists()
            && let Ok(content) = fs::read_to_string(&config_path)
            && let Ok(mut json) = serde_json::from_str::<Value>(&content)
        {
            if cab_managed {
                let mut env_map = serde_json::Map::new();
                if let Some(existing_env) = json.get("env").and_then(|v| v.as_object()) {
                    env_map = existing_env.clone();
                }
                let gateway_ep = format!("http://localhost:{}", gateway_port);
                env_map.insert("ANTHROPIC_BASE_URL".to_string(), Value::String(gateway_ep));
                env_map.insert(
                    "ANTHROPIC_AUTH_TOKEN".to_string(),
                    Value::String(gateway_key.to_string()),
                );
                if mode == "manual" {
                    env_map.insert(
                        "CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY".to_string(),
                        Value::String("1".to_string()),
                    );
                } else {
                    env_map.remove("CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY");
                }
                // Manual/auto: do not pin a model — CC chooses, CAB routes in auto mode.
                env_map.remove("ANTHROPIC_MODEL");
                env_map.remove("ANTHROPIC_SMALL_FAST_MODEL");
                env_map.remove("ANTHROPIC_DEFAULT_SONNET_MODEL");
                env_map.remove("ANTHROPIC_DEFAULT_OPUS_MODEL");
                env_map.remove("ANTHROPIC_DEFAULT_HAIKU_MODEL");
                json["env"] = Value::Object(env_map);
                if let Some(obj) = json.as_object_mut() {
                    obj.remove("model");
                }
            } else if let Some(env) = json.get_mut("env").and_then(|v| v.as_object_mut()) {
                env.remove("ANTHROPIC_BASE_URL");
                env.remove("ANTHROPIC_AUTH_TOKEN");
                env.remove("CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY");
                env.remove("ANTHROPIC_MODEL");
                env.remove("ANTHROPIC_SMALL_FAST_MODEL");
                env.remove("ANTHROPIC_DEFAULT_SONNET_MODEL");
                env.remove("ANTHROPIC_DEFAULT_OPUS_MODEL");
                env.remove("ANTHROPIC_DEFAULT_HAIKU_MODEL");
            }

            if let Ok(pretty) = serde_json::to_string_pretty(&json) {
                backup_agent_config(&config_path);
                fs::write(&config_path, pretty)?;
                tracing::info!(
                    "Dynamic Config Switcher: Updated Claude Code settings.json at {} for mode {}",
                    config_path.display(),
                    mode
                );
            }
        }
        Ok(())
    }
}
