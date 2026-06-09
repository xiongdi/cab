//! UAT: local user-acceptance tests with real ~/.cab/settings.json keys and upstream LLM calls.
//!
//! ```bash
//! # Prerequisites: enable ≥1 provider + model with valid API keys in CAB UI (~/.cab/settings.json)
//! CAB_RUN_UAT=1 cargo test -p cab-server --test uat_scenarios -- --test-threads=1
//! ```
//!
//! Uses an ephemeral TCP port (not gateway_port 3125) so a running cab-server is not required.
//! Agent config tests (UAT-06) backup/restore the real ~/.claude/settings.json.

mod support;

use std::fs;
use support::local_uat::{
    ClaudeSettingsBackup, assert_anthropic_text, assert_assistant_text, post_chat_completions,
    post_messages, put_agent_auto, spawn_local_server,
};
use support::{get_json, post_json};

/// UAT-01: Real OpenAI-style chat completion through CAB with the user's keys.
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys)"]
async fn uat_01_real_chat_completion_returns_content() {
    if support::local_uat::skip_unless_enabled() {
        return;
    }
    let server = spawn_local_server().await;
    let (_provider_id, model_name) = support::local_uat::first_routable_model(&server).await;

    let body = post_chat_completions(
        &server,
        &model_name,
        "claude-code/1.0",
        "Reply with exactly: CAB-UAT-OK",
        128,
    )
    .await;
    assert_assistant_text(&body);
}

/// UAT-02: Auto mode + routing strategy alias routes a real request (e.g. `balanced`).
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys)"]
async fn uat_02_auto_strategy_routes_real_request() {
    if support::local_uat::skip_unless_enabled() {
        return;
    }
    let server = spawn_local_server().await;
    support::local_uat::first_routable_model(&server).await;

    put_agent_auto(&server, "codex", "balanced").await;
    let body = post_chat_completions(
        &server,
        "balanced",
        "codex-cli/1.0",
        "Say CAB balanced route works.",
        128,
    )
    .await;
    assert_assistant_text(&body);
}

/// UAT-03: User-configured keys → dashboard reports active providers.
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys)"]
async fn uat_03_active_providers_from_user_config() {
    if support::local_uat::skip_unless_enabled() {
        return;
    }
    let server = spawn_local_server().await;
    support::local_uat::first_routable_model(&server).await;

    let stats = get_json(&server, "/api/dashboard/stats").await;
    let active = stats
        .get("active_providers")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    assert!(
        active >= 1,
        "expected active_providers >= 1 from ~/.cab/settings.json"
    );
}

/// UAT-04: Fallback route can be configured with real catalog model names.
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys)"]
async fn uat_04_fallback_route_with_real_model_ids() {
    if support::local_uat::skip_unless_enabled() {
        return;
    }
    let server = spawn_local_server().await;
    let models = get_json(&server, "/api/models").await;
    let enabled: Vec<&str> = models
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false))
        .filter_map(|m| m.get("name").and_then(|v| v.as_str()))
        .take(3)
        .collect();
    assert!(
        enabled.len() >= 2,
        "need at least two enabled models in ~/.cab for fallback route UAT"
    );

    let route = post_json(
        &server,
        "/api/routes",
        serde_json::json!({
            "name": "UAT fallback",
            "agent_pattern": "uat-*",
            "primary_model_id": enabled[0],
            "fallback_model_ids": [enabled[1]],
            "routing_strategy": "auto",
            "enabled": true
        }),
    )
    .await;

    let fallbacks = route
        .get("fallback_model_ids")
        .and_then(|v| v.as_array())
        .expect("fallback_model_ids");
    assert_eq!(fallbacks.len(), 1);
}

/// UAT-05: Real gateway requests appear in logs with filter + pagination.
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys)"]
async fn uat_05_real_requests_logged_and_filterable() {
    if support::local_uat::skip_unless_enabled() {
        return;
    }
    let server = spawn_local_server().await;
    let (_provider_id, model_name) = support::local_uat::first_routable_model(&server).await;

    for i in 0..3 {
        let _ = post_chat_completions(
            &server,
            &model_name,
            if i % 2 == 0 {
                "claude-code/1.0"
            } else {
                "codex-cli/1.0"
            },
            &format!("UAT log ping {i}"),
            16,
        )
        .await;
    }

    let logs = get_json(&server, "/api/logs?per_page=2&page=1").await;
    let total = logs.get("total").and_then(|v| v.as_i64()).unwrap_or(0);
    assert!(
        total >= 3,
        "expected >= 3 log entries after real requests, got {total}"
    );

    let filtered = get_json(&server, "/api/logs?agent=claude-code").await;
    let data = filtered
        .get("data")
        .and_then(|v| v.as_array())
        .expect("log data");
    assert!(!data.is_empty());
    assert!(
        data.iter()
            .all(|l| { l.get("agent").and_then(|v| v.as_str()) == Some("claude-code") })
    );
}

/// UAT-06: Claude Code auto mode writes real ~/.claude/settings.json (backed up).
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys)"]
async fn uat_06_claude_auto_writes_real_settings_json() {
    if support::local_uat::skip_unless_enabled() {
        return;
    }
    let _backup = ClaudeSettingsBackup::new();
    let server = spawn_local_server().await;

    put_agent_auto(&server, "claude-code", "balanced").await;

    let home = std::env::var("HOME").expect("HOME");
    let path = std::path::Path::new(&home).join(".claude/settings.json");
    let content = fs::read_to_string(&path).expect("read ~/.claude/settings.json");
    let json: serde_json::Value = serde_json::from_str(&content).expect("parse settings");
    let env = json.get("env").and_then(|v| v.as_object()).expect("env");
    assert!(env.contains_key("ANTHROPIC_BASE_URL"));
    assert!(env.contains_key("ANTHROPIC_AUTH_TOKEN"));
}

/// UAT-07: Desktop shell — manual only.
#[tokio::test]
#[ignore = "manual: npm run tauri:dev — seven pages + i18n (UAT-07)"]
async fn uat_07_desktop_shell_manual() {}

/// UAT-08: Anthropic Messages with user's keys (skips if no anthropic-protocol model enabled).
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys)"]
async fn uat_08_anthropic_messages_when_available() {
    if support::local_uat::skip_unless_enabled() {
        return;
    }
    let server = spawn_local_server().await;
    let models = get_json(&server, "/api/models").await;
    let Some(model_name) = models
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| {
            m.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false)
                && m.get("protocol")
                    .and_then(|v| v.as_str())
                    .is_some_and(|p| p == "anthropic")
        })
        .and_then(|m| m.get("name").and_then(|v| v.as_str()))
    else {
        eprintln!("skip UAT-08 anthropic: no enabled anthropic-protocol model in ~/.cab");
        return;
    };

    let body = post_messages(&server, model_name, "Reply CAB anthropic UAT", 32).await;
    assert_anthropic_text(&body);
}

/// UAT-09: Catalog sync endpoint (bundled models.dev; AA may use network).
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys)"]
async fn uat_09_sync_catalog_with_user_store() {
    if support::local_uat::skip_unless_enabled() {
        return;
    }
    let server = spawn_local_server().await;
    let url = format!("{}/api/settings/sync-catalog", server.base_url);
    let response = server.client.post(&url).send().await.expect("sync-catalog");
    assert!(
        response.status().is_success(),
        "status {}",
        response.status()
    );
    let body: serde_json::Value = response.json().await.expect("json");
    assert_eq!(body.get("success").and_then(|v| v.as_bool()), Some(true));
    assert!(
        body.get("applied_models")
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
            > 0
    );
}
