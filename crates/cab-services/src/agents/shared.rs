use serde_json::Value;
use std::fs;
use std::path::Path as StdPath;

pub fn backup_agent_config(path: &StdPath) {
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

pub fn yaml_quote(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

/// Hermes only supports custom request headers on the OpenAI-compatible wire
/// (`api_mode: chat_completions`). Anthropic-only upstream models are reached via
/// CAB gateway protocol translation, not by switching Hermes to `anthropic_messages`.
pub fn cab_identifying_headers(agent_id: &str) -> serde_json::Map<String, Value> {
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

pub fn opencode_model_config(display_name: &str, agent_id: &str) -> Value {
    let mut model = serde_json::Map::new();
    model.insert("name".to_string(), Value::String(display_name.to_string()));
    model.insert(
        "headers".to_string(),
        Value::Object(cab_identifying_headers(agent_id)),
    );
    Value::Object(model)
}

pub fn build_hermes_model_block(model_name: &str, endpoint: &str, api_key: &str) -> String {
    format!(
        "model:\n  provider: custom\n  default: {}\n  model: {}\n  base_url: {}\n  api_key: {}\n  api_mode: chat_completions\n  default_headers:\n    User-Agent: {}\n    X-CAB-Agent: \"hermes\"",
        yaml_quote(model_name),
        yaml_quote(model_name),
        yaml_quote(endpoint),
        yaml_quote(api_key),
        yaml_quote("HermesAgent/CAB"),
    )
}

pub fn replace_top_level_yaml_block(content: &str, key: &str, replacement: &str) -> String {
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

pub fn run_openclaw_config(args: Vec<String>) -> Result<(), std::io::Error> {
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
        return Err(std::io::Error::other(format!(
            "`openclaw {}` failed: {}{}",
            args.join(" "),
            stderr.trim(),
            if stdout.trim().is_empty() {
                String::new()
            } else {
                format!(" {}", stdout.trim())
            }
        )));
    }

    Ok(())
}

pub async fn collect_enabled_models(pool: &cab_db::InMemoryStore) -> Vec<cab_core::types::Model> {
    cab_db::routability::list_routable_models(pool)
        .await
        .unwrap_or_default()
}
