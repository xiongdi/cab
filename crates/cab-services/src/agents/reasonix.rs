use super::shared::backup_agent_config;
use super::{AgentConfigContext, AgentIntegration};
use std::fs;
use std::path::Path as StdPath;

/// Reasonix integration.
///
/// Config lives at `~/.reasonix/config.toml` (TOML).
/// Secrets live at `~/.reasonix/.env` (`KEY=VALUE` lines).
///
/// **Auto mode** — CAB injects a `[[providers]]` entry named `"cab"` pointing at
/// the CAB gateway, sets `default_model = "cab"`, and writes the gateway key to
/// `.env` as `CAB_API_KEY`.  Reasonix sends `model = <strategy>` and CAB routes
/// per the active strategy.
///
/// **Manual mode** — same CAB provider entry, but with a `models` list exposing
/// every enabled model so the user can pick one through Reasonix.
///
/// **Native mode** — the CAB provider is removed and `default_model` is restored
/// to its pre-CAB value; `CAB_API_KEY` is stripped from `.env`.
pub struct Integration;

impl AgentIntegration for Integration {
    fn id(&self) -> &'static str {
        "reasonix"
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
        let enabled_models = ctx.enabled_models;

        let config_dir = StdPath::new(&home).join(".reasonix");
        let config_path = config_dir.join("config.toml");
        let env_path = config_dir.join(".env");

        if !config_dir.exists() {
            let _ = fs::create_dir_all(&config_dir);
        }

        // ---------- 1. Parse / build TOML ----------
        let mut toml_val: toml::Value = if config_path.exists() {
            fs::read_to_string(&config_path)
                .ok()
                .and_then(|c| c.parse::<toml::Value>().ok())
                .unwrap_or_else(|| toml::Value::Table(toml::Table::new()))
        } else {
            toml::Value::Table(toml::Table::new())
        };

        let table = toml_val.as_table_mut().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "reasonix config.toml root is not a table",
            )
        })?;

        // Back up the original default_model so we can restore it in native mode.
        let prev_default = table
            .get("default_model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if prev_default.as_deref() != Some("cab") {
            // Only back up when it's a user-set value.
            if let Some(ref def) = prev_default {
                table.insert(
                    "cab_backup_default_model".to_string(),
                    toml::Value::String(def.clone()),
                );
            } else {
                table.remove("cab_backup_default_model");
            }
        }

        // ---------- 2. CAB-managed path (auto / manual) ----------
        if cab_managed {
            // Remove any previous CAB provider from the [[providers]] array.
            retain_providers(table, |name| name != "cab");

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

            // Build the CAB provider entry.
            let mut cab_provider = toml::Table::new();
            cab_provider.insert("name".to_string(), toml::Value::String("cab".to_string()));
            cab_provider.insert(
                "kind".to_string(),
                toml::Value::String("openai".to_string()),
            );
            cab_provider.insert("base_url".to_string(), toml::Value::String(ep));

            if mode == "auto" {
                if let Some(strat) = strategy {
                    cab_provider
                        .insert("model".to_string(), toml::Value::String(strat.to_string()));
                } else {
                    cab_provider
                        .insert("model".to_string(), toml::Value::String("auto".to_string()));
                }
            } else {
                // manual mode: list all enabled models
                if !enabled_models.is_empty() {
                    let model_names: Vec<String> =
                        enabled_models.iter().map(|m| m.name.clone()).collect();
                    let default_model = model_names.first().cloned().unwrap_or_default();
                    cab_provider.insert(
                        "models".to_string(),
                        toml::Value::Array(
                            model_names
                                .iter()
                                .map(|n| toml::Value::String(n.clone()))
                                .collect(),
                        ),
                    );
                    if !default_model.is_empty() {
                        cab_provider
                            .insert("default".to_string(), toml::Value::String(default_model));
                    }
                } else if let Some(strat) = strategy {
                    cab_provider
                        .insert("model".to_string(), toml::Value::String(strat.to_string()));
                } else {
                    cab_provider
                        .insert("model".to_string(), toml::Value::String("auto".to_string()));
                }
            }

            cab_provider.insert(
                "api_key_env".to_string(),
                toml::Value::String("CAB_API_KEY".to_string()),
            );

            // Push the CAB provider into the [[providers]] array.
            push_provider(table, cab_provider);

            // Set default_model to "cab".
            table.insert(
                "default_model".to_string(),
                toml::Value::String("cab".to_string()),
            );

            // Write the gateway key to .env.
            write_env_key(&env_path, "CAB_API_KEY", &key)?;
        } else {
            // ---------- 3. Native mode ----------
            retain_providers(table, |name| name != "cab");

            // Restore original default_model.
            if let Some(def) = table
                .remove("cab_backup_default_model")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
            {
                table.insert("default_model".to_string(), toml::Value::String(def));
            } else {
                // If there is no backup and the current default_model is "cab", just remove it
                // so Reasonix falls back to its own defaults.
                if table.get("default_model").and_then(|v| v.as_str()) == Some("cab") {
                    table.remove("default_model");
                }
            }

            // Remove CAB_API_KEY from .env.
            remove_env_key(&env_path, "CAB_API_KEY")?;
        }

        // ---------- 4. Write config ----------
        // Clean up the backup key from the persisted file (it's an internal marker).
        table.remove("cab_backup_default_model");

        if let Ok(pretty) = toml::to_string_pretty(&toml_val) {
            backup_agent_config(&config_path);
            fs::write(&config_path, pretty)?;
            tracing::info!(
                "Dynamic Config Switcher: Updated reasonix config.toml at {} for mode {}",
                config_path.display(),
                mode
            );
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Remove providers whose `name` field matches `predicate`.
fn retain_providers(table: &mut toml::Table, predicate: impl Fn(&str) -> bool) {
    let Some(providers_val) = table.get_mut("providers") else {
        return;
    };
    let Some(arr) = providers_val.as_array_mut() else {
        return;
    };
    arr.retain(|v| {
        v.as_table()
            .and_then(|t| t.get("name"))
            .and_then(|n| n.as_str())
            .map(&predicate)
            .unwrap_or(true)
    });
}

/// Append a provider table to the `[[providers]]` array.
fn push_provider(table: &mut toml::Table, provider: toml::Table) {
    let providers = table
        .entry("providers".to_string())
        .or_insert_with(|| toml::Value::Array(Vec::new()));
    if let Some(arr) = providers.as_array_mut() {
        arr.push(toml::Value::Table(provider));
    }
}

/// Write (or replace) a `KEY=VALUE` line into a dotenv-style file.
/// Other lines are preserved.
fn write_env_key(path: &StdPath, key: &str, value: &str) -> Result<(), std::io::Error> {
    let mut lines: Vec<String> = if path.exists() {
        fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(|l| l.to_string())
            .collect()
    } else {
        Vec::new()
    };

    let prefix = format!("{}=", key);
    let replacement = format!("{}={}", key, value);

    if let Some(pos) = lines
        .iter()
        .position(|l| l.trim_start().starts_with(&prefix))
    {
        lines[pos] = replacement;
    } else {
        // Ensure a trailing newline anchor before appending.
        if lines.last().map(|l| l.is_empty()) != Some(true) {
            lines.push(String::new());
        }
        lines.push(replacement);
    }

    // Trim trailing blank lines (keep at most one).
    while lines.len() > 1 && lines.last().map(|l| l.is_empty()) == Some(true) {
        lines.pop();
    }
    if lines.last().map(|l| l.is_empty()) != Some(true) {
        lines.push(String::new());
    }

    fs::write(path, lines.join("\n"))?;
    Ok(())
}

/// Remove a `KEY=...` line from a dotenv-style file.
fn remove_env_key(path: &StdPath, key: &str) -> Result<(), std::io::Error> {
    if !path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(path).unwrap_or_default();
    let prefix = format!("{}=", key);
    let lines: Vec<&str> = content
        .lines()
        .filter(|l| !l.trim_start().starts_with(&prefix))
        .collect();

    // Trim trailing blank lines.
    let mut end = lines.len();
    while end > 0 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }

    if end == 0 {
        // File is effectively empty — remove it so Reasonix doesn't see a stale empty .env.
        let _ = fs::remove_file(path);
    } else {
        fs::write(path, lines[..end].join("\n") + "\n")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn retain_providers_removes_matching() {
        let mut table = toml::Table::new();
        let mut p1 = toml::Table::new();
        p1.insert("name".into(), toml::Value::String("deepseek".into()));
        let mut p2 = toml::Table::new();
        p2.insert("name".into(), toml::Value::String("cab".into()));
        table.insert(
            "providers".into(),
            toml::Value::Array(vec![toml::Value::Table(p1), toml::Value::Table(p2)]),
        );
        retain_providers(&mut table, |n| n != "cab");
        let arr = table["providers"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"].as_str().unwrap(), "deepseek");
    }

    #[test]
    fn push_provider_adds_to_array() {
        let mut table = toml::Table::new();
        let mut p = toml::Table::new();
        p.insert("name".into(), toml::Value::String("cab".into()));
        push_provider(&mut table, p);
        let arr = table["providers"].as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["name"].as_str().unwrap(), "cab");
    }

    #[test]
    fn env_write_and_remove() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".env");

        write_env_key(&path, "CAB_API_KEY", "sk-test").unwrap();
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("CAB_API_KEY=sk-test"));

        remove_env_key(&path, "CAB_API_KEY").unwrap();
        assert!(
            !path.exists()
                || fs::read_to_string(&path)
                    .unwrap_or_default()
                    .trim()
                    .is_empty()
        );
    }
}
