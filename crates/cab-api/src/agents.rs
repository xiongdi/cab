use axum::Json;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use cab_core::CabError;
use cab_core::types::UpdateAgent;
use serde_json::Value;
use std::fs;
use std::path::Path as StdPath;

use crate::ApiState;

pub async fn list_agents(State(state): State<ApiState>) -> Result<impl IntoResponse, CabError> {
    let agents = cab_db::agent::list(&state.pool)
        .await
        .map_err(CabError::Database)?
        .into_iter()
        .map(normalize_agent_mode)
        .collect::<Vec<_>>();
    Ok(Json(agents))
}

pub async fn get_agent(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, CabError> {
    let agent = cab_db::agent::get_by_id(&state.pool, &id)
        .await
        .map_err(CabError::Database)?
        .ok_or_else(|| CabError::NotFound(format!("Agent {id} not found")))?;
    Ok(Json(normalize_agent_mode(agent)))
}

pub async fn update_agent(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(input): Json<UpdateAgent>,
) -> Result<impl IntoResponse, CabError> {
    let agent = cab_db::agent::update(&state.pool, &id, &input)
        .await
        .map_err(CabError::Database)?
        .ok_or_else(|| CabError::NotFound(format!("Agent {id} not found")))?;

    // Normalize legacy modes from older CAB versions.
    let agent = normalize_agent_mode(agent);

    let settings = cab_db::settings::get(&state.pool)
        .await
        .unwrap_or_else(|_| cab_db::settings::default_settings());

    // Automated Switcher Engine: Dynamically rewrite config files on disk
    if let Err(e) = apply_agent_config(
        &state.pool,
        &agent,
        settings.gateway_port,
        &settings.gateway_key,
    )
    .await
    {
        tracing::error!("Failed to write config file for agent {}: {}", agent.id, e);
    }

    Ok(Json(agent))
}

pub(crate) fn normalize_agent_mode(mut agent: cab_core::types::Agent) -> cab_core::types::Agent {
    agent.mode = match agent.mode.as_str() {
        "config" => "auto".to_string(),
        "proxy" => "native".to_string(),
        other => other.to_string(),
    };
    agent
}

fn backup_agent_config(path: &StdPath) {
    if !path.exists() {
        return;
    }
    let Some(parent) = path.parent() else {
        return;
    };
    let backup_dir = parent.join("backups");
    if fs::create_dir_all(&backup_dir).is_err() {
        return;
    }
    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "config".to_string());
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let backup_path = backup_dir.join(format!("{file_name}.cab-backup.{ts}"));
    if let Err(e) = fs::copy(path, &backup_path) {
        tracing::warn!("Failed to backup {}: {}", path.display(), e);
    }
}

fn yaml_quote(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// Hermes only supports custom request headers on the OpenAI-compatible wire
/// (`api_mode: chat_completions`). Anthropic-only upstream models are reached via
/// CAB gateway protocol translation, not by switching Hermes to `anthropic_messages`.
fn cab_identifying_headers(agent_id: &str) -> serde_json::Map<String, Value> {
    let user_agent = match agent_id {
        "opencode" => "OpenCode/CAB",
        "kilocode" => "KiloCode/CAB",
        "openclaw" => "OpenClaw/CAB",
        "pi" => "pi-coding-agent/CAB",
        "hermes" => "HermesAgent/CAB",
        _ => "CAB",
    };
    let mut headers = serde_json::Map::new();
    headers.insert(
        "X-CAB-Agent".to_string(),
        Value::String(agent_id.to_string()),
    );
    headers.insert(
        "User-Agent".to_string(),
        Value::String(user_agent.to_string()),
    );
    headers
}

fn opencode_model_config(display_name: &str, agent_id: &str) -> Value {
    let mut model = serde_json::Map::new();
    model.insert("name".to_string(), Value::String(display_name.to_string()));
    model.insert(
        "headers".to_string(),
        Value::Object(cab_identifying_headers(agent_id)),
    );
    Value::Object(model)
}

fn build_hermes_model_block(model_name: &str, endpoint: &str, api_key: &str) -> String {
    format!(
        "model:\n  provider: custom\n  default: {}\n  model: {}\n  base_url: {}\n  api_key: {}\n  api_mode: chat_completions\n  default_headers:\n    User-Agent: {}\n    X-CAB-Agent: \"hermes\"",
        yaml_quote(model_name),
        yaml_quote(model_name),
        yaml_quote(endpoint),
        yaml_quote(api_key),
        yaml_quote("HermesAgent/CAB"),
    )
}

fn replace_top_level_yaml_block(content: &str, key: &str, replacement: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let Some(start) = lines.iter().position(|line| {
        let trimmed = line.trim_start();
        line.len() == trimmed.len()
            && (trimmed == format!("{key}:") || trimmed.starts_with(&format!("{key}: ")))
    }) else {
        let mut out = content.trim_end().to_string();
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(replacement.trim_end());
        out.push('\n');
        return out;
    };

    let end = lines
        .iter()
        .enumerate()
        .skip(start + 1)
        .find_map(|(idx, line)| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            let unindented = line.trim_start();
            if line.len() == unindented.len() && !trimmed.starts_with('#') {
                Some(idx)
            } else {
                None
            }
        })
        .unwrap_or(lines.len());

    let mut out = String::new();
    for line in &lines[..start] {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(replacement.trim_end());
    out.push('\n');
    for line in &lines[end..] {
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn run_openclaw_config(args: Vec<String>) -> Result<(), std::io::Error> {
    let output = std::process::Command::new("openclaw")
        .args(&args)
        .output()
        .map_err(|e| {
            std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to run `openclaw {}`. Ensure OpenClaw is installed and on PATH: {e}",
                    args.join(" ")
                ),
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!(
                "`openclaw {}` failed: {}{}",
                args.join(" "),
                stderr.trim(),
                if stdout.trim().is_empty() {
                    String::new()
                } else {
                    format!(" {}", stdout.trim())
                }
            ),
        ));
    }

    Ok(())
}

async fn collect_enabled_models(pool: &cab_db::InMemoryStore) -> Vec<cab_core::types::Model> {
    let Ok(all_models) = cab_db::model::list(pool).await else {
        return Vec::new();
    };
    let active_providers = match cab_db::provider::list(pool).await {
        Ok(providers) => providers
            .into_iter()
            .filter(|p| p.enabled && (!p.api_key.is_empty() || p.id == "provider-ollama"))
            .map(|p| p.id)
            .collect::<std::collections::HashSet<_>>(),
        Err(_) => std::collections::HashSet::new(),
    };
    all_models
        .into_iter()
        .filter(|m| m.enabled && active_providers.contains(&m.provider_id))
        .collect()
}

/// Dynamic Configuration Switcher Engine (inspired by cc-switch)
///
/// Writes agent config files for CAB-managed modes:
/// - auto: gateway + routing strategy aliases (CAB picks provider/model per request)
/// - manual: gateway + all enabled models exposed to the agent
async fn apply_agent_config(
    pool: &cab_db::InMemoryStore,
    agent: &cab_core::types::Agent,
    gateway_port: i64,
    gateway_key: &str,
) -> Result<(), std::io::Error> {
    let agent_id = &agent.id;
    let mode = agent.mode.as_str();
    let api_key = &agent.api_key;
    let endpoint = &agent.endpoint;
    let strategy = agent.model_id.as_deref().filter(|s| !s.is_empty());
    let cab_managed = mode == "auto" || mode == "manual";
    let enabled_models = if mode == "manual" {
        collect_enabled_models(pool).await
    } else {
        Vec::new()
    };

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "User home directory could not be resolved (neither HOME nor USERPROFILE env var is set)",
            )
        })?;

    match agent_id.as_str() {
        "claude-code" => {
            let config_dir = StdPath::new(&home).join(".claude");
            let config_path = config_dir.join("settings.json");

            if config_path.exists() {
                if let Ok(content) = fs::read_to_string(&config_path) {
                    if let Ok(mut json) = serde_json::from_str::<Value>(&content) {
                        if cab_managed {
                            let mut env_map = serde_json::Map::new();
                            if let Some(existing_env) = json.get("env").and_then(|v| v.as_object())
                            {
                                env_map = existing_env.clone();
                            }
                            let gateway_ep = format!("http://localhost:{}", gateway_port);
                            env_map.insert(
                                "ANTHROPIC_BASE_URL".to_string(),
                                Value::String(gateway_ep),
                            );
                            env_map.insert(
                                "ANTHROPIC_AUTH_TOKEN".to_string(),
                                Value::String(gateway_key.to_string()),
                            );
                            env_map.insert(
                                "CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY".to_string(),
                                Value::String("1".to_string()),
                            );
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
                        } else if let Some(env) =
                            json.get_mut("env").and_then(|v| v.as_object_mut())
                        {
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
                }
            }
        }
        "codex" => {
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
        }
        "opencode" | "kilocode" => {
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
                    for strategy_name in ["auto", "balanced", "intelligent", "price"] {
                        models_obj.insert(
                            strategy_name.to_string(),
                            opencode_model_config(&format!("CAB {strategy_name}"), agent_id),
                        );
                    }
                } else {
                    for model in &enabled_models {
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
            } else if let Some(providers) = json.get_mut("provider").and_then(|p| p.as_object_mut())
            {
                providers.remove("cab");
                providers.remove("openai");
                if providers.is_empty() {
                    if let Some(obj) = json.as_object_mut() {
                        obj.remove("provider");
                    }
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
        }
        "hermes" => {
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
        }
        "openclaw" => {
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
                    for model in &enabled_models {
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
        }
        "pi" => {
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
                    for strategy_name in ["auto", "balanced", "intelligent", "price"] {
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
                    for model in &enabled_models {
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
                    if providers.is_empty() {
                        if let Some(obj) = models_json.as_object_mut() {
                            obj.remove("providers");
                        }
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
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cab_core::types::{Agent, ApiKeyConfig, Model, Provider, ProviderEndpoint};
    use std::path::PathBuf;

    struct TestHome {
        dir: tempfile::TempDir,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl TestHome {
        fn new() -> Self {
            let lock = crate::TEST_HOME_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let dir = tempfile::tempdir().unwrap();
            unsafe {
                std::env::set_var("HOME", dir.path());
                std::env::remove_var("USERPROFILE");
            }
            Self { dir, _lock: lock }
        }

        fn path(&self, path: &str) -> PathBuf {
            self.dir.path().join(path)
        }
    }

    fn sample_agent(mode: &str) -> Agent {
        Agent {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            mode: mode.to_string(),
            model_id: None,
            api_key: String::new(),
            endpoint: String::new(),
            updated_at: String::new(),
        }
    }

    fn agent(id: &str, mode: &str) -> Agent {
        Agent {
            id: id.to_string(),
            name: id.to_string(),
            mode: mode.to_string(),
            model_id: Some("balanced".to_string()),
            api_key: String::new(),
            endpoint: String::new(),
            updated_at: String::new(),
        }
    }

    fn pool_with_models() -> cab_db::InMemoryStore {
        let pool = cab_db::InMemoryStore::new();
        {
            let mut data = pool.inner.write().unwrap();
            data.providers.insert(
                "provider-1".into(),
                Provider {
                    id: "provider-1".into(),
                    name: "Provider One".into(),
                    endpoints: vec![ProviderEndpoint {
                        id: "chat".into(),
                        protocol: "openai-chat".into(),
                        url: "https://provider.test/v1".into(),
                        label: None,
                        priority: 50,
                        enabled: true,
                    }],
                    api_key: "key".into(),
                    enabled: true,
                    created_at: "now".into(),
                    updated_at: "now".into(),
                    privacy_policy_url: None,
                    terms_of_service_url: None,
                    status_page_url: None,
                    headquarters: None,
                    datacenters: None,
                    api_keys: vec![ApiKeyConfig {
                        key: "key".into(),
                        enabled: true,
                        subscribed: false,
                        quota_reset_at: None,
                    }],
                    api: None,
                    doc: None,
                    env: None,
                    npm: None,
                    model_count: 0,
                    catalog_models: vec![],
                },
            );
            data.models.insert(
                "model-1".into(),
                Model {
                    id: "model-1".into(),
                    name: "provider/model-1".into(),
                    display_name: "Model One".into(),
                    provider_id: "provider-1".into(),
                    protocol: "openai-chat".into(),
                    context_length: 64000,
                    input_cost: Some(1.0),
                    output_cost: Some(2.0),
                    enabled: true,
                    overall_intelligence: 80.0,
                    coding_index: 80.0,
                    agentic_index: 80.0,
                    math_index: 80.0,
                    created_at: "now".into(),
                    updated_at: "now".into(),
                    canonical_slug: None,
                    hugging_face_id: None,
                    created: None,
                    description: None,
                    architecture: None,
                    pricing: None,
                    top_provider: None,
                    per_request_limits: Some(serde_json::json!({"output_tokens": 1234})),
                    supported_parameters: None,
                    default_parameters: None,
                    supported_voices: None,
                    knowledge_cutoff: None,
                    expiration_date: None,
                    links: None,
                },
            );
        }
        pool
    }

    #[test]
    fn normalize_maps_config_to_auto() {
        let agent = normalize_agent_mode(sample_agent("config"));
        assert_eq!(agent.mode, "auto");
    }

    #[test]
    fn normalize_maps_proxy_to_native() {
        let agent = normalize_agent_mode(sample_agent("proxy"));
        assert_eq!(agent.mode, "native");
    }

    #[test]
    fn normalize_preserves_supported_modes() {
        for mode in ["native", "auto", "manual"] {
            let agent = normalize_agent_mode(sample_agent(mode));
            assert_eq!(agent.mode, mode);
        }
    }

    #[test]
    fn opencode_model_config_includes_identifying_headers() {
        let model = opencode_model_config("CAB auto", "kilocode");
        let headers = model
            .get("headers")
            .and_then(|v| v.as_object())
            .expect("headers");
        assert_eq!(
            headers.get("X-CAB-Agent").and_then(|v| v.as_str()),
            Some("kilocode")
        );
        assert_eq!(
            headers.get("User-Agent").and_then(|v| v.as_str()),
            Some("KiloCode/CAB")
        );
    }

    #[test]
    fn hermes_model_block_uses_openai_wire_and_identifying_headers() {
        let block =
            build_hermes_model_block("balanced", "http://localhost:3125/v1", "cab-local-key");
        assert!(block.contains("api_mode: chat_completions"));
        assert!(block.contains("default_headers:"));
        assert!(block.contains("User-Agent: \"HermesAgent/CAB\""));
        assert!(block.contains("X-CAB-Agent: \"hermes\""));
        assert!(!block.contains("anthropic_messages"));
    }

    #[test]
    fn yaml_block_replacement_appends_replaces_and_preserves_following_keys() {
        let appended = replace_top_level_yaml_block("other: true\n", "model", "model: cab");
        assert!(appended.contains("other: true"));
        assert!(appended.contains("model: cab"));

        let replaced = replace_top_level_yaml_block(
            "before: 1\nmodel:\n  old: true\nnext: 2\n",
            "model",
            "model:\n  new: true",
        );
        assert!(replaced.contains("before: 1"));
        assert!(replaced.contains("model:\n  new: true"));
        assert!(replaced.contains("next: 2"));
        assert!(!replaced.contains("old: true"));
    }

    #[test]
    fn backup_agent_config_creates_timestamped_copy_when_file_exists() {
        let home = TestHome::new();
        let path = home.path("config.json");
        fs::write(&path, "{\"old\":true}").unwrap();

        backup_agent_config(&path);

        let backups = fs::read_dir(home.path("backups")).unwrap().count();
        assert_eq!(backups, 1);
    }

    #[tokio::test]
    async fn codex_auto_manual_and_native_modes_update_toml_config() {
        let home = TestHome::new();
        let pool = pool_with_models();
        let config_path = home.path(".codex/config.toml");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(
            &config_path,
            "model_provider = \"old\"\n[model_providers.old]\nname = \"Old\"\n",
        )
        .unwrap();

        apply_agent_config(&pool, &agent("codex", "auto"), 4567, "gw-key")
            .await
            .unwrap();
        let auto = fs::read_to_string(&config_path).unwrap();
        assert!(auto.contains("model = \"balanced\""));
        assert!(auto.contains("model_provider = \"cab\""));
        assert!(auto.contains("base_url = \"http://localhost:4567/v1\""));

        apply_agent_config(&pool, &agent("codex", "manual"), 4567, "gw-key")
            .await
            .unwrap();
        let manual = fs::read_to_string(&config_path).unwrap();
        assert!(!manual.contains("model = \"balanced\""));
        assert!(manual.contains("model_provider = \"cab\""));

        apply_agent_config(&pool, &agent("codex", "native"), 4567, "gw-key")
            .await
            .unwrap();
        let native = fs::read_to_string(&config_path).unwrap();
        assert!(!native.contains("model_provider = \"cab\""));
        assert!(!native.contains("[model_providers.cab]"));
    }

    #[tokio::test]
    async fn claude_code_managed_mode_rewrites_existing_settings_env() {
        let home = TestHome::new();
        let pool = pool_with_models();
        let config_path = home.path(".claude/settings.json");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(
            &config_path,
            serde_json::json!({
                "model": "old",
                "env": {
                    "KEEP": "yes",
                    "ANTHROPIC_MODEL": "old-model"
                }
            })
            .to_string(),
        )
        .unwrap();

        apply_agent_config(&pool, &agent("claude-code", "auto"), 3125, "gw-key")
            .await
            .unwrap();
        let json: Value = serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
        assert!(json.get("model").is_none());
        assert_eq!(json["env"]["KEEP"], "yes");
        assert_eq!(json["env"]["ANTHROPIC_BASE_URL"], "http://localhost:3125");
        assert_eq!(json["env"]["ANTHROPIC_AUTH_TOKEN"], "gw-key");
        assert_eq!(
            json["env"]["CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY"],
            "1"
        );
        assert!(json["env"].get("ANTHROPIC_MODEL").is_none());

        apply_agent_config(&pool, &agent("claude-code", "native"), 3125, "gw-key")
            .await
            .unwrap();
        let json: Value = serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
        assert!(json["env"].get("ANTHROPIC_BASE_URL").is_none());
        assert_eq!(json["env"]["KEEP"], "yes");
    }

    #[tokio::test]
    async fn opencode_and_kilocode_write_auto_manual_and_native_provider_config() {
        let home = TestHome::new();
        let pool = pool_with_models();

        apply_agent_config(&pool, &agent("opencode", "auto"), 3125, "gw-key")
            .await
            .unwrap();
        let opencode_path = home.path(".config/opencode/opencode.json");
        let json: Value =
            serde_json::from_str(&fs::read_to_string(&opencode_path).unwrap()).unwrap();
        assert_eq!(
            json["provider"]["cab"]["models"]["balanced"]["name"],
            "CAB balanced"
        );
        assert_eq!(
            json["provider"]["cab"]["options"]["headers"]["X-CAB-Agent"],
            "opencode"
        );

        apply_agent_config(&pool, &agent("kilocode", "manual"), 3125, "gw-key")
            .await
            .unwrap();
        let kilo_path = home.path(".config/kilo/opencode.json");
        let json: Value = serde_json::from_str(&fs::read_to_string(&kilo_path).unwrap()).unwrap();
        assert_eq!(
            json["provider"]["cab"]["models"]["provider/model-1"]["name"],
            "Model One"
        );
        assert_eq!(
            json["provider"]["cab"]["models"]["model-1"]["headers"]["User-Agent"],
            "KiloCode/CAB"
        );

        apply_agent_config(&pool, &agent("kilocode", "native"), 3125, "gw-key")
            .await
            .unwrap();
        let json: Value = serde_json::from_str(&fs::read_to_string(&kilo_path).unwrap()).unwrap();
        assert!(json.get("provider").is_none());
    }

    #[tokio::test]
    async fn hermes_and_pi_configs_cover_managed_and_native_modes() {
        let home = TestHome::new();
        let pool = pool_with_models();

        apply_agent_config(&pool, &agent("hermes", "auto"), 3125, "gw-key")
            .await
            .unwrap();
        let hermes_path = home.path(".hermes/config.yaml");
        let hermes = fs::read_to_string(&hermes_path).unwrap();
        assert!(hermes.contains("model: \"balanced\""));
        assert!(hermes.contains("api_key: \"gw-key\""));

        apply_agent_config(&pool, &agent("hermes", "native"), 3125, "gw-key")
            .await
            .unwrap();
        assert!(
            fs::read_to_string(&hermes_path)
                .unwrap()
                .contains("model: \"\"")
        );

        apply_agent_config(&pool, &agent("pi", "manual"), 3125, "gw-key")
            .await
            .unwrap();
        let models_path = home.path(".pi/agent/models.json");
        let settings_path = home.path(".pi/agent/settings.json");
        let models: Value =
            serde_json::from_str(&fs::read_to_string(&models_path).unwrap()).unwrap();
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert_eq!(
            models["providers"]["cab"]["models"][0]["id"],
            "provider/model-1"
        );
        assert_eq!(models["providers"]["cab"]["models"][0]["maxTokens"], 1234);
        assert_eq!(settings["defaultProvider"], "cab");
        assert_eq!(settings["defaultModel"], "provider/model-1");

        apply_agent_config(&pool, &agent("pi", "native"), 3125, "gw-key")
            .await
            .unwrap();
        let models: Value =
            serde_json::from_str(&fs::read_to_string(&models_path).unwrap()).unwrap();
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert!(models.get("providers").is_none());
        assert!(settings.get("defaultProvider").is_none());
    }

    #[tokio::test]
    async fn openclaw_branch_reports_missing_cli_for_managed_mode() {
        let home = TestHome::new();
        let pool = pool_with_models();
        let old_path = std::env::var("PATH").unwrap_or_default();
        unsafe {
            std::env::set_var("PATH", home.path("empty-bin"));
        }

        let err = apply_agent_config(&pool, &agent("openclaw", "auto"), 3125, "gw-key")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Failed to run `openclaw"));

        unsafe {
            std::env::set_var("PATH", old_path);
        }
    }
}

pub async fn sync_all_agent_configs(pool: &cab_db::InMemoryStore) -> Result<(), CabError> {
    let settings = cab_db::settings::get(pool)
        .await
        .map_err(CabError::Database)?;
    let agents = cab_db::agent::list(pool)
        .await
        .map_err(CabError::Database)?;
    for agent in agents {
        let agent = normalize_agent_mode(agent);
        if let Err(e) =
            apply_agent_config(pool, &agent, settings.gateway_port, &settings.gateway_key).await
        {
            tracing::error!("Failed to sync config file for agent {}: {}", agent.id, e);
        }
    }
    Ok(())
}
