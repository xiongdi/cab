//! ST (TCP): real HTTP server on ephemeral port — validates full stack wiring.

mod support;

use cab_db::InMemoryStore;
use support::{SUPPORTED_AGENT_IDS, get_json, get_status, post_status, spawn_test_server};

#[tokio::test]
async fn st_tcp_serves_gateway_and_api_on_same_port() {
    let server = spawn_test_server(InMemoryStore::new()).await;

    let models = get_json(&server, "/v1/models").await;
    assert_eq!(models.get("object").and_then(|v| v.as_str()), Some("list"));

    let agents = get_json(&server, "/api/agents").await;
    assert_eq!(agents.as_array().expect("agents").len(), 7);

    let settings = get_json(&server, "/api/settings").await;
    assert!(
        settings
            .get("gateway_port")
            .and_then(|v| v.as_i64())
            .is_some()
    );
}

#[tokio::test]
async fn st_tcp_removed_routes_return_client_errors() {
    let server = spawn_test_server(InMemoryStore::new()).await;

    assert_eq!(
        post_status(&server, "/api/agents/hijack-claude").await,
        reqwest::StatusCode::METHOD_NOT_ALLOWED
    );
    assert_eq!(
        post_status(&server, "/api/agents/codex/install-proxy").await,
        reqwest::StatusCode::NOT_FOUND
    );
    assert_eq!(
        get_status(&server, "/v1internal:loadCodeAssist").await,
        reqwest::StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn st_tcp_settings_roundtrip_via_http() {
    let _home = support::TestHome::new();
    let server = spawn_test_server(InMemoryStore::new()).await;

    let mut settings = get_json(&server, "/api/settings").await;
    settings["gateway_port"] = serde_json::json!(4321);
    let updated = support::put_json(&server, "/api/settings", settings).await;
    assert_eq!(
        updated.get("gateway_port").and_then(|v| v.as_i64()),
        Some(4321)
    );

    let again = get_json(&server, "/api/settings").await;
    assert_eq!(
        again.get("gateway_port").and_then(|v| v.as_i64()),
        Some(4321)
    );
}

#[tokio::test]
async fn st_tcp_dashboard_and_logs_endpoints_respond() {
    let server = spawn_test_server(InMemoryStore::new()).await;

    let stats = get_json(&server, "/api/dashboard/stats").await;
    assert!(stats.get("total_requests").is_some());

    let logs = get_json(&server, "/api/logs").await;
    assert!(logs.get("data").and_then(|v| v.as_array()).is_some());
}

#[tokio::test]
async fn st_tcp_agent_ids_match_v01_catalog() {
    let server = spawn_test_server(InMemoryStore::new()).await;
    let agents = get_json(&server, "/api/agents").await;
    let ids: Vec<&str> = agents
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a.get("id").and_then(|v| v.as_str()).unwrap())
        .collect();
    for id in SUPPORTED_AGENT_IDS {
        assert!(ids.contains(id), "missing {id}");
    }
    assert!(!ids.contains(&"cursor"));
    assert!(!ids.contains(&"antigravity"));
}
