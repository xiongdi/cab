use super::shared::{cab_identifying_headers, run_openclaw_config};
use super::{AgentConfigContext, AgentIntegration};
use serde_json::Value;

pub struct Integration;

impl AgentIntegration for Integration {
    fn id(&self) -> &'static str {
        "openclaw"
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

            let mut provider_models = Vec::new();
            let mut allowlist = serde_json::Map::new();
            let default_model_ref = if mode == "auto" {
                for strategy_name in ["auto", "balanced", "intelligent", "price"] {
                    provider_models.push(serde_json::json!({
                        "id": strategy_name,
                        "name": format!("CAB {}", strategy_name),
                    }));
                    allowlist.insert(
                        format!("cab/{strategy_name}"),
                        serde_json::json!({
                            "alias": format!("CAB {}", strategy_name),
                            "agentRuntime": { "id": "openclaw" },
                        }),
                    );
                }
                format!("cab/{}", strategy.unwrap_or("auto"))
            } else {
                for model in enabled_models {
                    provider_models.push(serde_json::json!({
                        "id": model.name.clone(),
                        "name": model.display_name.clone(),
                        "contextWindow": model.context_length,
                        "cost": {
                            "input": model.input_cost.unwrap_or(0.0),
                            "output": model.output_cost.unwrap_or(0.0),
                            "cacheRead": 0,
                            "cacheWrite": 0,
                        },
                    }));
                    allowlist.insert(
                        format!("cab/{}", model.name),
                        serde_json::json!({
                            "alias": model.display_name.clone(),
                            "agentRuntime": { "id": "openclaw" },
                        }),
                    );
                }
                enabled_models
                    .first()
                    .map(|model| format!("cab/{}", model.name))
                    .unwrap_or_else(|| "cab/auto".to_string())
            };

            if provider_models.is_empty() {
                provider_models.push(serde_json::json!({
                    "id": "auto",
                    "name": "CAB auto",
                }));
                allowlist.insert(
                    "cab/auto".to_string(),
                    serde_json::json!({
                        "alias": "CAB auto",
                        "agentRuntime": { "id": "openclaw" },
                    }),
                );
            }

            let cab_provider = serde_json::json!({
                "baseUrl": ep,
                "apiKey": key,
                "api": "openai-completions",
                "timeoutSeconds": 600,
                "headers": cab_identifying_headers("openclaw"),
                "models": provider_models,
            });
            run_openclaw_config(vec![
                "config".to_string(),
                "set".to_string(),
                "models.providers.cab".to_string(),
                cab_provider.to_string(),
                "--strict-json".to_string(),
                "--merge".to_string(),
            ])?;

            run_openclaw_config(vec![
                "config".to_string(),
                "set".to_string(),
                "agents.defaults.models".to_string(),
                Value::Object(allowlist).to_string(),
                "--strict-json".to_string(),
                "--merge".to_string(),
            ])?;

            run_openclaw_config(vec![
                "config".to_string(),
                "set".to_string(),
                "agents.defaults.model".to_string(),
                Value::String(default_model_ref).to_string(),
                "--strict-json".to_string(),
            ])?;

            tracing::info!(
                "Dynamic Config Switcher: Updated OpenClaw openclaw.json via `openclaw config` for mode {}",
                mode
            );
        } else {
            let _ = run_openclaw_config(vec![
                "config".to_string(),
                "unset".to_string(),
                "models.providers.cab".to_string(),
            ]);
            for strategy_name in ["auto", "balanced", "intelligent", "price"] {
                let _ = run_openclaw_config(vec![
                    "config".to_string(),
                    "unset".to_string(),
                    format!("agents.defaults.models[\"cab/{strategy_name}\"]"),
                ]);
            }
        }
        Ok(())
    }
}
