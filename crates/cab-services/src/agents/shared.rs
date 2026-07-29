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

pub fn cab_identifying_headers(agent_id: &str) -> serde_json::Map<String, Value> {
    let user_agent = match agent_id {
        "opencode" => "OpenCode/CAB",
        "grok-build" => "GrokBuild/CAB",
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

pub async fn collect_enabled_models(pool: &cab_db::InMemoryStore) -> Vec<cab_core::types::Model> {
    cab_db::routability::list_routable_models(pool)
        .await
        .unwrap_or_default()
}
