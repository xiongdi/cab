//! UAT: local user-acceptance tests with real ~/.cab/settings.json keys and upstream LLM calls.
//!
//! ```bash
//! ./scripts/run-uat.sh
//! # Starts release cab-server + runs real CA CLIs; report: reports/uat/latest.md
//! ```

mod support;

use std::fs;
use support::local_uat::{
    ClaudeSettingsBackup, assert_anthropic_text, assert_assistant_text, assert_all_agents_mode,
    connect_packaged_server, post_chat_completions, post_messages, put_agent_auto, restore_agents,
    set_all_agents_mode, snapshot_agents, AUTO_STRATEGIES,
};
use support::real_ca::{assert_all_real_ca_passed, run_real_ca, summarize_results};
use support::uat_report;
use support::SUPPORTED_AGENT_IDS;
use support::{get_json, post_json};

const UAT_CA_PROMPT: &str = "Reply exactly: ok";

macro_rules! uat {
    ($id:expr, $title:expr, |$guard:ident| $body:block) => {{
        if support::local_uat::skip_unless_enabled() {
            uat_report::init_once();
            uat_report::record_case(
                $id,
                $title,
                uat_report::CaseStatus::Skipped,
                std::time::Duration::ZERO,
                "CAB_RUN_UAT not set",
            );
            return;
        }
        let mut $guard = uat_report::CaseGuard::new($id, $title);
        $body
        $guard.pass();
    }};
}

/// UAT-01: Real OpenAI-style chat completion through CAB with the user's keys.
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys)"]
async fn uat_01_real_chat_completion_returns_content() {
    uat!("UAT-01", "Real chat completion via gateway", |guard| {
        let server = connect_packaged_server().await;
        let (provider_id, model_name) = support::local_uat::first_routable_model(&server).await;
        guard.note(&format!("provider={provider_id}, model={model_name}"));

        let body = post_chat_completions(
            &server,
            &model_name,
            "claude-code/1.0",
            "Reply with exactly: CAB-UAT-OK",
            128,
        )
        .await;
        assert_assistant_text(&body);
    });
}

/// UAT-02: Auto mode + routing strategy alias routes a real request.
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys)"]
async fn uat_02_auto_strategy_routes_real_request() {
    uat!("UAT-02", "Auto strategy (balanced) routes real request", |guard| {
        let server = connect_packaged_server().await;
        support::local_uat::first_routable_model(&server).await;
        put_agent_auto(&server, "codex", "balanced").await;
        guard.note("agent=codex, strategy=balanced");

        let body = post_chat_completions(
            &server,
            "balanced",
            "codex-cli/1.0",
            "Say CAB balanced route works.",
            128,
        )
        .await;
        assert_assistant_text(&body);
    });
}

/// UAT-03: User-configured keys → dashboard reports active providers.
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys)"]
async fn uat_03_active_providers_from_user_config() {
    uat!("UAT-03", "Dashboard active providers from user config", |guard| {
        let server = connect_packaged_server().await;
        support::local_uat::first_routable_model(&server).await;

        let stats = get_json(&server, "/api/dashboard/stats").await;
        let active = stats
            .get("active_providers")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        guard.note(&format!("active_providers={active}"));
        assert!(active >= 1, "expected active_providers >= 1");
    });
}

/// UAT-04: Fallback route can be configured with real catalog model names.
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys)"]
async fn uat_04_fallback_route_with_real_model_ids() {
    uat!("UAT-04", "Fallback route with real model ids", |guard| {
        let server = connect_packaged_server().await;
        let models = get_json(&server, "/api/models").await;
        let enabled: Vec<&str> = models
            .as_array()
            .into_iter()
            .flatten()
            .filter(|m| m.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false))
            .filter_map(|m| m.get("name").and_then(|v| v.as_str()))
            .take(3)
            .collect();
        assert!(enabled.len() >= 2, "need >= 2 enabled models");

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

        guard.note(&format!("primary={}, fallback={}", enabled[0], enabled[1]));
        let fallbacks = route
            .get("fallback_model_ids")
            .and_then(|v| v.as_array())
            .expect("fallback_model_ids");
        assert_eq!(fallbacks.len(), 1);
    });
}

/// UAT-05: Real gateway requests appear in logs with filter + pagination.
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys)"]
async fn uat_05_real_requests_logged_and_filterable() {
    uat!("UAT-05", "Request logs filterable and paginated", |guard| {
        let server = connect_packaged_server().await;
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
        let filtered = get_json(&server, "/api/logs?agent=claude-code").await;
        let data = filtered
            .get("data")
            .and_then(|v| v.as_array())
            .expect("log data");
        guard.note(&format!("total_logs={total}, claude-code_logs={}", data.len()));
        assert!(total >= 3);
        assert!(!data.is_empty());
    });
}

/// UAT-06: Claude Code auto mode writes real ~/.claude/settings.json (backed up).
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys)"]
async fn uat_06_claude_auto_writes_real_settings_json() {
    uat!("UAT-06", "Claude Code auto writes ~/.claude/settings.json", |guard| {
        let _backup = ClaudeSettingsBackup::new();
        let server = connect_packaged_server().await;
        put_agent_auto(&server, "claude-code", "balanced").await;

        let home = std::env::var("HOME").expect("HOME");
        let path = std::path::Path::new(&home).join(".claude/settings.json");
        let content = fs::read_to_string(&path).expect("read settings");
        let json: serde_json::Value = serde_json::from_str(&content).expect("parse settings");
        let env = json.get("env").and_then(|v| v.as_object()).expect("env");
        guard.note(&format!("path={}", path.display()));
        assert!(env.contains_key("ANTHROPIC_BASE_URL"));
        assert!(env.contains_key("ANTHROPIC_AUTH_TOKEN"));
    });
}

/// UAT-07: Desktop shell — manual only (recorded in report, not automated).
#[tokio::test]
#[ignore = "manual: npm run tauri:dev — seven pages + i18n (UAT-07)"]
async fn uat_07_desktop_shell_manual() {}

/// UAT-08: Anthropic Messages via provider anthropic endpoints.
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys)"]
async fn uat_08_anthropic_messages_when_available() {
    uat!("UAT-08", "Anthropic Messages API (provider anthropic endpoint)", |guard| {
        let server = connect_packaged_server().await;
        let model_name = support::local_uat::first_anthropic_routable_model(&server).await;
        guard.note(&format!("model={model_name}, api=/v1/messages"));

        let body = post_messages(&server, &model_name, "Reply CAB anthropic UAT", 128).await;
        assert_anthropic_text(&body);
    });
}

/// UAT-09: Catalog sync endpoint (bundled models.dev; AA may use network).
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys)"]
async fn uat_09_sync_catalog_with_user_store() {
    uat!("UAT-09", "Sync catalog with user store", |guard| {
        let server = connect_packaged_server().await;
        let url = format!("{}/api/settings/sync-catalog", server.base_url);
        let response = server.client.post(&url).send().await.expect("sync-catalog");
        assert!(response.status().is_success(), "status {}", response.status());
        let body: serde_json::Value = response.json().await.expect("json");
        let applied = body
            .get("applied_models")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        guard.note(&format!("applied_models={applied}"));
        assert_eq!(body.get("success").and_then(|v| v.as_bool()), Some(true));
        assert!(applied > 0);
    });
}

/// UAT-10: All seven coding agents (CAs) invoke real CLIs against the packaged gateway.
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys + CA CLIs)"]
async fn uat_10_all_cas_real_chat_completion() {
    uat!("UAT-10", "All coding agents (7 CAs) real CLI completion", |guard| {
        let server = connect_packaged_server().await;
        let snapshot = snapshot_agents(&server).await;
        support::local_uat::first_routable_model(&server).await;

        let mut results = Vec::new();
        for agent_id in SUPPORTED_AGENT_IDS {
            put_agent_auto(&server, agent_id, "balanced").await;
            let result = run_real_ca(
                &server,
                agent_id,
                "balanced",
                UAT_CA_PROMPT,
            )
            .await;
            results.push(result);
        }
        restore_agents(&server, &snapshot).await;
        assert_all_real_ca_passed(&results);
        guard.note(&format!("real_cli: {}", summarize_results(&results)));
    });
}

/// UAT-11: All agents in auto mode — every built-in strategy via real CA CLIs.
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys + CA CLIs)"]
async fn uat_11_all_agents_auto_mode_real_requests() {
    uat!("UAT-11", "All agents auto mode × 4 strategies (real CLIs)", |guard| {
        let _backup = ClaudeSettingsBackup::new();
        let server = connect_packaged_server().await;
        let snapshot = snapshot_agents(&server).await;
        support::local_uat::first_routable_model(&server).await;

        let mut all_results = Vec::new();
        for strategy in AUTO_STRATEGIES {
            set_all_agents_mode(&server, "auto", Some(strategy)).await;
            assert_all_agents_mode(&server, "auto").await;

            for agent_id in SUPPORTED_AGENT_IDS {
                let result = run_real_ca(
                    &server,
                    agent_id,
                    strategy,
                    UAT_CA_PROMPT,
                )
                .await;
                all_results.push(result);
            }
        }
        restore_agents(&server, &snapshot).await;
        assert_all_real_ca_passed(&all_results);
        guard.note(&format!(
            "strategies={}; {}",
            AUTO_STRATEGIES.join(","),
            summarize_results(&all_results)
        ));
    });
}

/// UAT-12: All agents in manual mode — real CA CLIs with explicit catalog model ids.
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys + CA CLIs)"]
async fn uat_12_all_agents_manual_mode_real_requests() {
    uat!("UAT-12", "All agents manual mode real CLI requests", |guard| {
        let _backup = ClaudeSettingsBackup::new();
        let server = connect_packaged_server().await;
        let snapshot = snapshot_agents(&server).await;
        let (_provider_id, model_name) = support::local_uat::first_routable_model(&server).await;

        set_all_agents_mode(&server, "manual", None).await;
        assert_all_agents_mode(&server, "manual").await;

        let mut results = Vec::new();
        for agent_id in SUPPORTED_AGENT_IDS {
            let result = run_real_ca(
                &server,
                agent_id,
                &model_name,
                UAT_CA_PROMPT,
            )
            .await;
            results.push(result);
        }
        restore_agents(&server, &snapshot).await;
        assert_all_real_ca_passed(&results);
        guard.note(&format!(
            "model={model_name}; {}",
            summarize_results(&results)
        ));
    });
}

/// Writes the Markdown UAT report (always runs last).
#[tokio::test]
#[ignore = "local UAT only: ./scripts/run-uat.sh (requires ~/.cab API keys)"]
async fn uat_zz_write_report() {
    if support::local_uat::skip_unless_enabled() {
        return;
    }
    let path = uat_report::write_report();
    assert!(path.exists(), "report file missing: {}", path.display());
}
