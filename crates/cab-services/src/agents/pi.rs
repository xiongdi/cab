use super::shared::{backup_agent_config, cab_identifying_headers};
use super::{AgentConfigContext, AgentIntegration};
use serde_json::Value;
use std::fs;
use std::path::Path as StdPath;

pub struct Integration;

impl AgentIntegration for Integration {
    fn id(&self) -> &'static str {
        "pi"
    }

    fn apply(&self, ctx: &AgentConfigContext<'_>) -> Result<(), std::io::Error> {
        let mode = ctx.mode;
        let api_key = ctx.api_key;
        let endpoint = ctx.endpoint;
        let strategy = ctx.strategy;
        let cab_managed = ctx.cab_managed;
        let enabled_models = ctx.enabled_models;
        let gateway_port = ctx.gateway_port;
        let gateway_key = ctx.gateway_key;
        let home = &ctx.home;

        let config_dir = StdPath::new(&home).join(".pi").join("agent");
        let models_path = config_dir.join("models.json");
        let settings_path = config_dir.join("settings.json");

        if !config_dir.exists() {
            let _ = fs::create_dir_all(&config_dir);
        }

        let mut models_json: Value = if models_path.exists() {
            fs::read_to_string(&models_path)
                .ok()
                .and_then(|c| serde_json::from_str(&c).ok())
                .unwrap_or_else(|| serde_json::json!({}))
        } else {
            serde_json::json!({})
        };
        let mut settings_json: Value = if settings_path.exists() {
            fs::read_to_string(&settings_path)
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

            let mut pi_models = Vec::new();
            let pi_headers = cab_identifying_headers("pi");
            let default_model = if mode == "auto" {
                for strategy_name in ["auto", "balanced", "intelligent", "price", "speed"] {
                    pi_models.push(serde_json::json!({
                        "id": strategy_name,
                        "name": format!("CAB {}", strategy_name),
                        "contextWindow": 200000,
                        "maxTokens": 8192,
                        "headers": pi_headers,
                    }));
                }
                strategy.unwrap_or("auto").to_string()
            } else {
                for model in enabled_models {
                    pi_models.push(serde_json::json!({
                        "id": model.name.clone(),
                        "name": model.display_name.clone(),
                        "contextWindow": model.context_length,
                        "maxTokens": model
                            .per_request_limits
                            .as_ref()
                            .and_then(|v| v.get("output_tokens"))
                            .and_then(|v| v.as_i64())
                            .unwrap_or(8192),
                        "cost": {
                            "input": model.input_cost.unwrap_or(0.0),
                            "output": model.output_cost.unwrap_or(0.0),
                            "cacheRead": 0,
                            "cacheWrite": 0,
                        },
                        "headers": pi_headers,
                    }));
                }
                enabled_models
                    .first()
                    .map(|model| model.name.clone())
                    .unwrap_or_else(|| "auto".to_string())
            };

            if pi_models.is_empty() {
                pi_models.push(serde_json::json!({
                    "id": "auto",
                    "name": "CAB auto",
                    "contextWindow": 200000,
                    "maxTokens": 8192,
                    "headers": pi_headers,
                }));
            }

            let cab_provider = serde_json::json!({
                "baseUrl": ep,
                "api": "openai-completions",
                "apiKey": key,
                "authHeader": true,
                "headers": pi_headers,
                "compat": {
                    "supportsDeveloperRole": false,
                    "supportsStore": false,
                    "supportsReasoningEffort": false
                },
                "models": pi_models,
            });

            let mut providers_map = models_json
                .get("providers")
                .and_then(|p| p.as_object())
                .cloned()
                .unwrap_or_default();
            providers_map.insert("cab".to_string(), cab_provider);
            models_json["providers"] = Value::Object(providers_map);

            if !settings_json.is_object() {
                settings_json = serde_json::json!({});
            }
            if let Some(settings_obj) = settings_json.as_object_mut() {
                settings_obj.insert(
                    "defaultProvider".to_string(),
                    Value::String("cab".to_string()),
                );
                settings_obj.insert("defaultModel".to_string(), Value::String(default_model));
                settings_obj.insert(
                    "enabledModels".to_string(),
                    Value::Array(vec![Value::String("cab/*".to_string())]),
                );
            }
        } else {
            if let Some(providers) = models_json
                .get_mut("providers")
                .and_then(|p| p.as_object_mut())
            {
                providers.remove("cab");
                if providers.is_empty()
                    && let Some(obj) = models_json.as_object_mut()
                {
                    obj.remove("providers");
                }
            }

            if let Some(settings_obj) = settings_json.as_object_mut() {
                if settings_obj
                    .get("defaultProvider")
                    .and_then(|v| v.as_str())
                    .is_some_and(|provider| provider == "cab")
                {
                    settings_obj.remove("defaultProvider");
                    settings_obj.remove("defaultModel");
                }
                if let Some(enabled) = settings_obj
                    .get_mut("enabledModels")
                    .and_then(|v| v.as_array_mut())
                {
                    enabled.retain(|value| {
                        value
                            .as_str()
                            .map(|pattern| !pattern.starts_with("cab/"))
                            .unwrap_or(true)
                    });
                }
            }
        }

        if let Ok(pretty) = serde_json::to_string_pretty(&models_json) {
            backup_agent_config(&models_path);
            fs::write(&models_path, pretty)?;
        }
        if let Ok(pretty) = serde_json::to_string_pretty(&settings_json) {
            backup_agent_config(&settings_path);
            fs::write(&settings_path, pretty)?;
        }
        tracing::info!(
            "Dynamic Config Switcher: Updated Pi models.json/settings.json at {} for mode {}",
            config_dir.display(),
            mode
        );
        Ok(())
    }
}
