use super::shared::backup_agent_config;
use super::{AgentConfigContext, AgentIntegration};
use std::fs;
use std::path::Path as StdPath;

/// Grok Build integration (`grok` CLI / TUI).
///
/// Config lives at `~/.grok/config.toml` (or `$GROK_HOME/config.toml`).
///
/// **Auto mode** — injects `cab-*` model entries (OpenAI chat-completions) pointing
/// at the CAB gateway, sets `[models].default` to the active strategy, and writes
/// the gateway key inline (`api_key`) plus identifying `extra_headers`.
///
/// **Manual mode** — same CAB endpoint, with one model entry per enabled model.
///
/// **Native mode** — removes CAB-managed model entries and restores the previous
/// default model when it was backed up.
pub struct Integration;

const CAB_MODEL_PREFIX: &str = "cab-";
const BACKUP_DEFAULT_KEY: &str = "cab_backup_default_model";

impl AgentIntegration for Integration {
    fn id(&self) -> &'static str {
        "grok-build"
    }

    fn apply(&self, ctx: &AgentConfigContext<'_>) -> Result<(), std::io::Error> {
        let mode = ctx.mode;
        let endpoint = ctx.endpoint;
        let strategy = ctx.strategy;
        let cab_managed = ctx.cab_managed;
        let gateway_port = ctx.gateway_port;
        let api_key = ctx.api_key;
        let gateway_key = ctx.gateway_key;
        let enabled_models = ctx.enabled_models;

        let config_dir = grok_home(&ctx.home);
        let config_path = config_dir.join("config.toml");

        if !config_dir.exists() {
            let _ = fs::create_dir_all(&config_dir);
        }

        let mut toml_val: toml::Value = if config_path.exists() {
            fs::read_to_string(&config_path)
                .ok()
                .and_then(|c| toml::from_str(&c).ok())
                .unwrap_or_else(|| toml::Value::Table(toml::Table::new()))
        } else {
            toml::Value::Table(toml::Table::new())
        };

        let table = toml_val.as_table_mut().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "grok config.toml root is not a table",
            )
        })?;

        backup_default_model(table);

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

            remove_cab_models(table);

            let mut default_id = String::from("cab-auto");

            if mode == "auto" {
                for strategy_name in [
                    "auto",
                    "balanced",
                    "intelligent",
                    "agentic",
                    "price",
                    "speed",
                ] {
                    let model_id = format!("{CAB_MODEL_PREFIX}{strategy_name}");
                    insert_cab_model(
                        table,
                        &model_id,
                        strategy_name,
                        &format!("CAB {strategy_name}"),
                        &ep,
                        &key,
                    );
                }
                if let Some(strat) = strategy.filter(|s| !s.is_empty()) {
                    default_id = format!("{CAB_MODEL_PREFIX}{strat}");
                }
            } else {
                // manual
                if enabled_models.is_empty() {
                    insert_cab_model(table, "cab-auto", "auto", "CAB auto", &ep, &key);
                    default_id = "cab-auto".to_string();
                } else {
                    for (idx, model) in enabled_models.iter().enumerate() {
                        let model_id =
                            format!("{CAB_MODEL_PREFIX}{}", sanitize_model_key(&model.name));
                        insert_cab_model(
                            table,
                            &model_id,
                            &model.name,
                            &model.display_name,
                            &ep,
                            &key,
                        );
                        if idx == 0 {
                            default_id = model_id;
                        }
                    }
                    if let Some(strat) = strategy.filter(|s| !s.is_empty()) {
                        let preferred = format!("{CAB_MODEL_PREFIX}{}", sanitize_model_key(strat));
                        if table
                            .get("model")
                            .and_then(|m| m.as_table())
                            .is_some_and(|m| m.contains_key(&preferred))
                        {
                            default_id = preferred;
                        }
                    }
                }
            }

            let models = table
                .entry("models".to_string())
                .or_insert_with(|| toml::Value::Table(toml::Table::new()));
            if let Some(models_table) = models.as_table_mut() {
                models_table.insert("default".to_string(), toml::Value::String(default_id));
            }
        } else {
            remove_cab_models(table);
            restore_default_model(table);
        }

        if let Ok(pretty) = toml::to_string_pretty(&toml_val) {
            backup_agent_config(&config_path);
            fs::write(&config_path, pretty)?;
            tracing::info!(
                "Dynamic Config Switcher: Updated grok-build config.toml at {} for mode {}",
                config_path.display(),
                mode
            );
        }
        Ok(())
    }
}

fn grok_home(user_home: &str) -> std::path::PathBuf {
    if let Ok(override_home) = std::env::var("GROK_HOME")
        && !override_home.trim().is_empty()
    {
        return StdPath::new(&override_home).to_path_buf();
    }
    StdPath::new(user_home).join(".grok")
}

fn backup_default_model(table: &mut toml::Table) {
    // Prefer an existing backup so re-entering auto mode doesn't overwrite it.
    if table.contains_key(BACKUP_DEFAULT_KEY) {
        return;
    }
    let current = table
        .get("models")
        .and_then(|m| m.as_table())
        .and_then(|m| m.get("default"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    if let Some(def) = current
        && !def.starts_with(CAB_MODEL_PREFIX)
    {
        table.insert(BACKUP_DEFAULT_KEY.to_string(), toml::Value::String(def));
    }
}

fn restore_default_model(table: &mut toml::Table) {
    let backup = table
        .remove(BACKUP_DEFAULT_KEY)
        .and_then(|v| v.as_str().map(|s| s.to_string()));
    let Some(models) = table
        .entry("models".to_string())
        .or_insert_with(|| toml::Value::Table(toml::Table::new()))
        .as_table_mut()
    else {
        return;
    };
    match backup {
        Some(def) => {
            models.insert("default".to_string(), toml::Value::String(def));
        }
        None => {
            if models
                .get("default")
                .and_then(|v| v.as_str())
                .is_some_and(|d| d.starts_with(CAB_MODEL_PREFIX))
            {
                models.remove("default");
            }
        }
    }
}

fn remove_cab_models(table: &mut toml::Table) {
    let Some(model) = table.get_mut("model").and_then(|m| m.as_table_mut()) else {
        return;
    };
    model.retain(|k, _| !k.starts_with(CAB_MODEL_PREFIX));
    if model.is_empty() {
        table.remove("model");
    }
}

fn insert_cab_model(
    table: &mut toml::Table,
    model_id: &str,
    api_model: &str,
    display_name: &str,
    endpoint: &str,
    api_key: &str,
) {
    let mut entry = toml::Table::new();
    entry.insert(
        "model".to_string(),
        toml::Value::String(api_model.to_string()),
    );
    entry.insert(
        "base_url".to_string(),
        toml::Value::String(endpoint.to_string()),
    );
    entry.insert(
        "name".to_string(),
        toml::Value::String(display_name.to_string()),
    );
    entry.insert(
        "api_key".to_string(),
        toml::Value::String(api_key.to_string()),
    );
    entry.insert(
        "api_backend".to_string(),
        toml::Value::String("chat_completions".to_string()),
    );

    let mut headers = toml::Table::new();
    headers.insert(
        "X-CAB-Agent".to_string(),
        toml::Value::String("grok-build".to_string()),
    );
    headers.insert(
        "User-Agent".to_string(),
        toml::Value::String("GrokBuild/CAB".to_string()),
    );
    entry.insert("extra_headers".to_string(), toml::Value::Table(headers));

    let model = table
        .entry("model".to_string())
        .or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if let Some(model_table) = model.as_table_mut() {
        model_table.insert(model_id.to_string(), toml::Value::Table(entry));
    }
}

fn sanitize_model_key(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_replaces_slashes() {
        assert_eq!(sanitize_model_key("openai/gpt-5"), "openai-gpt-5");
    }

    #[test]
    fn backup_reads_existing_default() {
        let raw = r#"
[models]
default = "grok-build"

[model.grok-build]
model = "grok-4.5"
name = "Grok 4.5"
"#;
        let mut val: toml::Value = toml::from_str(raw).expect("parse");
        let table = val.as_table_mut().unwrap();
        backup_default_model(table);
        assert_eq!(
            table.get(BACKUP_DEFAULT_KEY).and_then(|v| v.as_str()),
            Some("grok-build")
        );
    }
}
