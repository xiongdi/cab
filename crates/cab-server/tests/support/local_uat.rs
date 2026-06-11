//! Local UAT: real ~/.cab/settings.json keys, real upstream LLM calls, packaged cab-server.
//!
//! Enable with `CAB_RUN_UAT=1`. Start the release binary via `./scripts/run-uat.sh`.

use std::path::PathBuf;
use std::time::Duration;

use cab_db::InMemoryStore;

use super::{
    SUPPORTED_AGENT_IDS, TestServer, build_authed_client, get_json, put_json, spawn_test_server,
};

pub const ENV_ENABLE: &str = "CAB_RUN_UAT";

/// Built-in routing strategies exposed in Agents UI auto mode.
pub const AUTO_STRATEGIES: &[&str] = &["auto", "balanced", "intelligent", "price", "speed"];

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

fn read_gateway_key_from_settings() -> String {
    let path = settings_path();
    let content =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let json: serde_json::Value = serde_json::from_str(&content).expect("parse settings.json");
    json.get("gateway_key")
        .and_then(|v| v.as_str())
        .expect("gateway_key in settings")
        .to_string()
}

/// Connect to the packaged release `cab-server` started by `./scripts/run-uat.sh`.
pub async fn connect_packaged_server() -> TestServer {
    let base_url = std::env::var("CAB_UAT_BASE_URL").unwrap_or_else(|_| {
        panic!(
            "CAB_UAT_BASE_URL must be set — run UAT via ./scripts/run-uat.sh (packaged cab-server)"
        )
    });
    let gateway_key =
        std::env::var("CAB_UAT_GATEWAY_KEY").unwrap_or_else(|_| read_gateway_key_from_settings());
    let client = build_authed_client(&gateway_key);

    let health = format!("{base_url}/api/dashboard/stats");
    let models_url = format!("{base_url}/api/models");
    let mut ready = false;
    for _ in 0..180 {
        let stats_ok = client
            .get(&health)
            .send()
            .await
            .is_ok_and(|r| r.status().is_success());
        let models_ok = if stats_ok {
            match client.get(&models_url).send().await {
                Ok(resp) if resp.status().is_success() => resp
                    .json::<serde_json::Value>()
                    .await
                    .ok()
                    .and_then(|v| v.as_array().map(|a| !a.is_empty()))
                    .unwrap_or(false),
                _ => false,
            }
        } else {
            false
        };
        if stats_ok && models_ok {
            ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(
        ready,
        "packaged CAB server not ready at {base_url} (wait for catalog sync) — run ./scripts/run-uat.sh"
    );

    TestServer {
        base_url,
        client,
        gateway_key,
        _shutdown: None,
        _task: None,
    }
}

fn provider_has_anthropic_endpoint(provider: &serde_json::Value) -> bool {
    if !provider
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return false;
    }
    if !provider_has_usable_key(provider) {
        return false;
    }
    provider
        .get("endpoints")
        .and_then(|v| v.as_array())
        .is_some_and(|endpoints| {
            endpoints.iter().any(|endpoint| {
                endpoint
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
                    && endpoint.get("protocol").and_then(|v| v.as_str()) == Some("anthropic")
            })
        })
}

/// First enabled model whose provider exposes an enabled Anthropic Messages endpoint.
pub async fn first_anthropic_routable_model(server: &TestServer) -> String {
    let models = get_json(server, "/api/models").await;
    let providers = get_json(server, "/api/providers").await;
    let provider_list = providers.as_array().expect("providers array");

    for model in models.as_array().into_iter().flatten() {
        if !model
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        let provider_id = model
            .get("provider_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let Some(provider) = provider_list
            .iter()
            .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(provider_id))
        else {
            continue;
        };
        if !provider_has_anthropic_endpoint(provider) {
            continue;
        }
        return model
            .get("name")
            .and_then(|v| v.as_str())
            .expect("model name")
            .to_string();
    }

    panic!(
        "No enabled model with an Anthropic endpoint found. Enable a provider that has protocol \
         \"anthropic\" (e.g. minimax or deepseek) plus at least one model. Config: {}",
        settings_path().display()
    );
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
    put_agent_mode(server, agent_id, "auto", Some(strategy)).await;
}

/// Typical User-Agent strings for each supported coding agent (CA).
pub fn agent_user_agent(agent_id: &str) -> &'static str {
    match agent_id {
        "claude-code" => "claude-cli/2.1.165",
        "codex" => "codex_exec/0.134.0",
        "opencode" => "opencode/1.14.48 ai-sdk/5",
        "hermes" => "HermesAgent/0.16.0",
        "kilocode" => "Kilo-Code/7.3.40 ai-sdk/provider-utils/4.0.23",
        "openclaw" => "OpenClaw/2026.6.1 (cab-probe)",
        "pi" => "pi-coding-agent/0.79.0",
        other => panic!("unknown agent id: {other}"),
    }
}

pub async fn snapshot_agents(server: &TestServer) -> Vec<serde_json::Value> {
    get_json(server, "/api/agents")
        .await
        .as_array()
        .expect("agents array")
        .clone()
}

pub async fn restore_agents(server: &TestServer, snapshot: &[serde_json::Value]) {
    for agent in snapshot {
        let id = agent.get("id").and_then(|v| v.as_str()).expect("agent id");
        put_json(
            server,
            &format!("/api/agents/{id}"),
            serde_json::json!({
                "mode": agent.get("mode").and_then(|v| v.as_str()).unwrap_or("native"),
                "model_id": agent.get("model_id").cloned(),
            }),
        )
        .await;
    }
}

pub async fn put_agent_mode(
    server: &TestServer,
    agent_id: &str,
    mode: &str,
    strategy: Option<&str>,
) {
    let mut body = serde_json::json!({ "mode": mode });
    if mode == "auto" {
        body["model_id"] = serde_json::json!(strategy.unwrap_or("balanced"));
    } else if mode == "manual" {
        body["model_id"] = serde_json::Value::Null;
    }
    put_json(server, &format!("/api/agents/{agent_id}"), body).await;
}

pub async fn set_all_agents_mode(server: &TestServer, mode: &str, strategy: Option<&str>) {
    for agent_id in SUPPORTED_AGENT_IDS {
        put_agent_mode(server, agent_id, mode, strategy).await;
    }
}

pub async fn assert_all_agents_mode(server: &TestServer, mode: &str) {
    let agents = get_json(server, "/api/agents").await;
    for agent in agents.as_array().expect("agents array") {
        let id = agent.get("id").and_then(|v| v.as_str()).unwrap_or("?");
        assert_eq!(
            agent.get("mode").and_then(|v| v.as_str()),
            Some(mode),
            "agent {id} expected mode {mode}"
        );
    }
}

pub async fn post_chat_as_agent(
    server: &TestServer,
    agent_id: &str,
    model: &str,
    prompt: &str,
    max_tokens: u32,
) -> serde_json::Value {
    let url = format!("{}/v1/chat/completions", server.base_url);
    let response = server
        .client
        .post(&url)
        .header("content-type", "application/json")
        .header("user-agent", agent_user_agent(agent_id))
        .header("x-cab-agent", agent_id)
        .json(&serde_json::json!({
            "model": model,
            "messages": [{ "role": "user", "content": prompt }],
            "max_tokens": max_tokens,
        }))
        .send()
        .await
        .unwrap_or_else(|e| panic!("POST {url} failed for agent {agent_id}: {e}"));

    let status = response.status();
    let body_text = response.text().await.unwrap_or_default();
    assert!(
        status.is_success(),
        "chat completion failed for agent {agent_id} model {model}: {status} {body_text}"
    );
    serde_json::from_str(&body_text)
        .unwrap_or_else(|e| panic!("invalid JSON for agent {agent_id}: {e}\nbody: {body_text}"))
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
    let has_content = body
        .get("content")
        .and_then(|c| c.as_array())
        .is_some_and(|blocks| {
            blocks
                .iter()
                .any(|block| match block.get("type").and_then(|t| t.as_str()) {
                    Some("text") => block
                        .get("text")
                        .and_then(|t| t.as_str())
                        .is_some_and(|s| !s.trim().is_empty()),
                    Some("thinking") => block
                        .get("thinking")
                        .and_then(|t| t.as_str())
                        .is_some_and(|s| !s.trim().is_empty()),
                    _ => false,
                })
        });
    assert!(
        has_content,
        "expected non-empty anthropic text or thinking content, got: {body}"
    );
}
