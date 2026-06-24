use super::shared::backup_agent_config;
use super::{AgentConfigContext, AgentIntegration};
use std::fs;
use std::path::Path as StdPath;

/// Codex 0.134+ requires a JWT-shaped `tokens.id_token` even when CAB injects the gateway key.
const CAB_CODEX_ID_TOKEN: &str =
    "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJzdWIiOiJjYWItZ2F0ZXdheSJ9.Y2Fi";

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
        let api_key = ctx.api_key;
        let gateway_key = ctx.gateway_key;

        let config_dir = StdPath::new(&home).join(".codex");
        let config_path = config_dir.join("config.toml");
        let auth_path = config_dir.join("auth.json");

        if !config_dir.exists() {
            let _ = fs::create_dir_all(&config_dir);
        }

        let mut toml_val: toml::Value = if config_path.exists() {
            fs::read_to_string(&config_path)
                .ok()
                .and_then(|c| c.parse::<toml::Value>().ok())
                .unwrap_or_else(|| toml::Value::Table(toml::Table::new()))
        } else {
            toml::Value::Table(toml::Table::new())
        };

        let key = if api_key.is_empty() {
            gateway_key.to_string()
        } else {
            api_key.to_string()
        };

        if cab_managed {
            if let Some(table) = toml_val.as_table_mut() {
                if mode == "auto" {
                    // Set model to a known default to prevent metadata warning and env key errors
                    table.insert(
                        "model".to_string(),
                        toml::Value::String("gpt-5.5".to_string()),
                    );
                } else if let Some(strat) = strategy {
                    // Manual mode with selected model - extract the suffix (e.g. gpt-5.5 from openai/gpt-5.5)
                    let model_name = if let Some(pos) = strat.find('/') {
                        &strat[pos + 1..]
                    } else {
                        strat
                    };
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
            let mut cab_provider = toml::Table::new();
            cab_provider.insert(
                "name".to_string(),
                toml::Value::String("CAB Gateway".to_string()),
            );
            cab_provider.insert("base_url".to_string(), toml::Value::String(ep));

            // Tell Codex this provider requires OpenAI OAuth authentication.
            // When requires_openai_auth is true, Codex reads tokens.access_token
            // from auth.json and sends it as the Authorization: Bearer <access_token> header.
            // This allows us to inject our gateway key into auth.json and avoid
            // needing the user to set a system environment variable.
            cab_provider.insert(
                "requires_openai_auth".to_string(),
                toml::Value::Boolean(true),
            );

            if let Some(table) = toml_val.as_table_mut() {
                let providers = table
                    .entry("model_providers".to_string())
                    .or_insert_with(|| toml::Value::Table(toml::Table::new()));
                if let Some(providers_table) = providers.as_table_mut() {
                    providers_table.insert("cab".to_string(), toml::Value::Table(cab_provider));
                }
            }

            // Write our gateway key into auth.json as the access_token.
            // We back up any existing ChatGPT tokens so we can restore them in native mode.
            let mut auth_json: serde_json::Value = if auth_path.exists() {
                fs::read_to_string(&auth_path)
                    .ok()
                    .and_then(|c| serde_json::from_str(&c).ok())
                    .unwrap_or_else(|| serde_json::json!({}))
            } else {
                serde_json::json!({})
            };

            if let Some(obj) = auth_json.as_object_mut() {
                // Back up existing ChatGPT tokens if they are not already our managed tokens
                let current_token = obj
                    .get("tokens")
                    .and_then(|t| t.get("access_token"))
                    .and_then(|v| v.as_str());

                let is_cab_token = current_token
                    .map(|t| t.starts_with("cab-token-"))
                    .unwrap_or(false);

                if let Some(toks) = obj.get("tokens").and_then(|t| t.as_object())
                    && !is_cab_token {
                        let acc = toks.get("access_token").cloned();
                        let ref_t = toks.get("refresh_token").cloned();
                        let id_t = toks.get("id_token").cloned();
                        let lr = obj.get("last_refresh").cloned();
                        let am = obj.get("auth_mode").cloned();

                        if let Some(a) = acc {
                            obj.insert("cab_backup_access_token".to_string(), a);
                        }
                        if let Some(r) = ref_t {
                            obj.insert("cab_backup_refresh_token".to_string(), r);
                        }
                        if let Some(i) = id_t {
                            obj.insert("cab_backup_id_token".to_string(), i);
                        }
                        if let Some(l) = lr {
                            obj.insert("cab_backup_last_refresh".to_string(), l);
                        }
                        if let Some(m) = am {
                            obj.insert("cab_backup_auth_mode".to_string(), m);
                        }
                    }

                let current_api_key = obj.get("OPENAI_API_KEY").and_then(|v| v.as_str());
                let is_cab_api_key = current_api_key
                    .map(|k| k.starts_with("cab-token-"))
                    .unwrap_or(false);
                if let Some(k) = obj.get("OPENAI_API_KEY")
                    && !is_cab_api_key {
                        obj.insert("cab_backup_openai_api_key".to_string(), k.clone());
                    }

                // Set our CAB managed ChatGPT auth
                obj.insert(
                    "auth_mode".to_string(),
                    serde_json::Value::String("chatgpt".to_string()),
                );
                obj.insert(
                    "last_refresh".to_string(),
                    serde_json::Value::String("2099-01-01T00:00:00Z".to_string()),
                );

                let tokens = serde_json::json!({
                    "access_token": key.clone(),
                    "refresh_token": "",
                    "id_token": CAB_CODEX_ID_TOKEN,
                });
                obj.insert("tokens".to_string(), tokens);

                // Also insert OPENAI_API_KEY just in case the user has some other tool or script checking it
                obj.insert(
                    "OPENAI_API_KEY".to_string(),
                    serde_json::Value::String(key.clone()),
                );
            }
            if let Ok(pretty) = serde_json::to_string_pretty(&auth_json) {
                let _ = fs::write(&auth_path, pretty);
            }
        } else {
            // Clean up config.toml
            if let Some(table) = toml_val.as_table_mut() {
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

            // Restore backup fields in auth.json if they exist
            if auth_path.exists()
                && let Ok(content) = fs::read_to_string(&auth_path)
                    && let Ok(mut auth_json) = serde_json::from_str::<serde_json::Value>(&content) {
                        let mut modified = false;
                        if let Some(obj) = auth_json.as_object_mut() {
                            if let Some(bak_acc) = obj.remove("cab_backup_access_token") {
                                let mut tokens = obj
                                    .get("tokens")
                                    .and_then(|t| t.as_object().cloned())
                                    .unwrap_or_default();
                                tokens.insert("access_token".to_string(), bak_acc);
                                if let Some(bak_ref) = obj.remove("cab_backup_refresh_token") {
                                    tokens.insert("refresh_token".to_string(), bak_ref);
                                }
                                if let Some(bak_id) = obj.remove("cab_backup_id_token") {
                                    tokens.insert("id_token".to_string(), bak_id);
                                }
                                obj.insert("tokens".to_string(), serde_json::Value::Object(tokens));
                                modified = true;
                            } else {
                                let current_token = obj
                                    .get("tokens")
                                    .and_then(|t| t.get("access_token"))
                                    .and_then(|v| v.as_str());
                                if current_token
                                    .map(|t| {
                                        t.starts_with("cab-token-")
                                            || t == gateway_key
                                            || t == api_key
                                    })
                                    .unwrap_or(false)
                                {
                                    obj.remove("tokens");
                                    modified = true;
                                }
                            }

                            if let Some(bak_lr) = obj.remove("cab_backup_last_refresh") {
                                obj.insert("last_refresh".to_string(), bak_lr);
                                modified = true;
                            } else if obj.get("last_refresh").and_then(|v| v.as_str())
                                == Some("2099-01-01T00:00:00Z")
                            {
                                obj.remove("last_refresh");
                                modified = true;
                            }

                            if let Some(bak_am) = obj.remove("cab_backup_auth_mode") {
                                obj.insert("auth_mode".to_string(), bak_am);
                                modified = true;
                            } else if obj.get("auth_mode").and_then(|v| v.as_str())
                                == Some("chatgpt")
                            {
                                obj.remove("auth_mode");
                                modified = true;
                            }

                            if let Some(bak_key) = obj.remove("cab_backup_openai_api_key") {
                                obj.insert("OPENAI_API_KEY".to_string(), bak_key);
                                modified = true;
                            } else {
                                let current_api_key =
                                    obj.get("OPENAI_API_KEY").and_then(|v| v.as_str());
                                if current_api_key
                                    .map(|k| {
                                        k.starts_with("cab-token-")
                                            || k == gateway_key
                                            || k == api_key
                                    })
                                    .unwrap_or(false)
                                {
                                    obj.remove("OPENAI_API_KEY");
                                    modified = true;
                                }
                            }
                        }
                        if modified
                            && let Ok(pretty) = serde_json::to_string_pretty(&auth_json) {
                                let _ = fs::write(&auth_path, pretty);
                            }
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
