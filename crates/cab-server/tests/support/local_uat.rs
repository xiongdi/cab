//! Local UAT: real ~/.cab/settings.json keys, real upstream LLM calls, ephemeral TCP port.
//!
//! Enable with `CAB_RUN_UAT=1`. CI skips UAT unless explicitly opted in.

use std::path::PathBuf;

use cab_db::InMemoryStore;

use super::{TestServer, get_json, put_json, spawn_test_server};

pub const ENV_ENABLE: &str = "CAB_RUN_UAT";

pub fn enabled() -> bool {
    std::env::var(ENV_ENABLE).ok().as_deref() == Some("1")
}

/// Returns `true` when the test should return early (UAT disabled).
pub fn skip_unless_enabled() -> bool {
    if enabled() {
        return false;
    }
    eprintln!(
        "skip UAT: set {ENV_ENABLE}=1 to run local acceptance tests with ~/.cab/settings.json keys"
    );
    true
}

pub fn settings_path() -> PathBuf {
    cab_db::settings::settings_file_path()
}

/// Mirror production startup: load ~/.cab/settings.json and sync bundled catalog.
pub async fn init_local_store() -> InMemoryStore {
    let store = cab_db::init_store().await.expect("init store from ~/.cab");
    cab_api::providers::sync_models_dev_catalog(&store)
        .await
        .expect("sync models.dev catalog");
    store
}

pub async fn spawn_local_server() -> TestServer {
    let store = init_local_store().await;
    spawn_test_server(store).await
}

/// First enabled provider (with non-empty key) + enabled model from the user's config.
pub async fn first_routable_model(server: &TestServer) -> (String, String) {
    let providers = get_json(server, "/api/providers").await;
    let models = get_json(server, "/api/models").await;
    let provider_list = providers.as_array().expect("providers array");

    for provider in provider_list {
        if !provider
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        let provider_id = provider
            .get("id")
            .and_then(|v| v.as_str())
            .expect("provider id");

        let has_key = provider_has_usable_key(provider);
        if !has_key {
            continue;
        }

        if let Some(model) = models.as_array().and_then(|list| {
            list.iter().find(|m| {
                m.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false)
                    && m.get("provider_id").and_then(|v| v.as_str()) == Some(provider_id)
            })
        }) {
            let model_name = model
                .get("name")
                .and_then(|v| v.as_str())
                .expect("model name")
                .to_string();
            return (provider_id.to_string(), model_name);
        }
    }

    panic!(
        "No routable provider+model found. In CAB, enable at least one LLM provider with a valid API key and enable one of its models. Config: {}",
        settings_path().display()
    );
}

fn provider_has_usable_key(provider: &serde_json::Value) -> bool {
    if let Some(keys) = provider.get("api_keys").and_then(|v| v.as_array())
        && keys.iter().any(|k| {
            k.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true)
                && k.get("key")
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| !s.trim().is_empty())
        })
    {
        return true;
    }
    provider
        .get("api_key")
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.trim().is_empty())
}

pub async fn post_chat_completions(
    server: &TestServer,
    model: &str,
    user_agent: &str,
    prompt: &str,
    max_tokens: u32,
) -> serde_json::Value {
    let url = format!("{}/v1/chat/completions", server.base_url);
    let response = server
        .client
        .post(&url)
        .header("content-type", "application/json")
        .header("user-agent", user_agent)
        .json(&serde_json::json!({
            "model": model,
            "messages": [{ "role": "user", "content": prompt }],
            "max_tokens": max_tokens,
        }))
        .send()
        .await
        .unwrap_or_else(|e| panic!("POST {url} failed: {e}"));

    let status = response.status();
    let body_text = response.text().await.unwrap_or_default();
    assert!(
        status.is_success(),
        "chat completion failed for model {model}: {status} {body_text}"
    );
    serde_json::from_str(&body_text)
        .unwrap_or_else(|e| panic!("invalid JSON from chat completion: {e}\nbody: {body_text}"))
}

pub fn assert_assistant_text(body: &serde_json::Value) {
    let message = body
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"));
    let content = message
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("");
    let reasoning = message
        .and_then(|m| m.get("reasoning_content"))
        .and_then(|c| c.as_str())
        .unwrap_or("");
    assert!(
        !content.trim().is_empty() || !reasoning.trim().is_empty(),
        "expected non-empty assistant content, got: {body}"
    );
}

/// Backup and restore the real `~/.claude/settings.json` while a test mutates it.
pub struct ClaudeSettingsBackup {
    path: PathBuf,
    had_file: bool,
    original: Option<Vec<u8>>,
}

impl ClaudeSettingsBackup {
    pub fn new() -> Self {
        let home = std::env::var("HOME").expect("HOME");
        let path = PathBuf::from(home).join(".claude/settings.json");
        let (had_file, original) = if path.exists() {
            (
                true,
                Some(std::fs::read(&path).expect("read claude settings")),
            )
        } else {
            (false, None)
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir .claude");
        }
        Self {
            path,
            had_file,
            original,
        }
    }
}

impl Drop for ClaudeSettingsBackup {
    fn drop(&mut self) {
        if self.had_file {
            if let Some(bytes) = &self.original {
                let _ = std::fs::write(&self.path, bytes);
            }
        } else if self.path.exists() {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub async fn put_agent_auto(server: &TestServer, agent_id: &str, strategy: &str) {
    put_json(
        server,
        &format!("/api/agents/{agent_id}"),
        serde_json::json!({ "mode": "auto", "model_id": strategy }),
    )
    .await;
}

pub async fn post_messages(
    server: &TestServer,
    model: &str,
    prompt: &str,
    max_tokens: u32,
) -> serde_json::Value {
    let url = format!("{}/v1/messages", server.base_url);
    let response = server
        .client
        .post(&url)
        .header("content-type", "application/json")
        .header("user-agent", "claude-code/1.0")
        .json(&serde_json::json!({
            "model": model,
            "max_tokens": max_tokens,
            "messages": [{ "role": "user", "content": prompt }],
        }))
        .send()
        .await
        .unwrap_or_else(|e| panic!("POST {url} failed: {e}"));

    let status = response.status();
    let body_text = response.text().await.unwrap_or_default();
    assert!(
        status.is_success(),
        "anthropic messages failed for model {model}: {status} {body_text}"
    );
    serde_json::from_str(&body_text)
        .unwrap_or_else(|e| panic!("invalid JSON from messages: {e}\nbody: {body_text}"))
}

pub fn assert_anthropic_text(body: &serde_json::Value) {
    let text = body
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|blocks| blocks.first())
        .and_then(|b| b.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    assert!(
        !text.trim().is_empty(),
        "expected non-empty anthropic content, got: {body}"
    );
}
