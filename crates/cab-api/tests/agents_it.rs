//! IT: Management API agent endpoints (interface contract).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cab_api::api_router;
use cab_db::InMemoryStore;
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

static AGENTS_IT_HOME_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct TestHome {
    _dir: tempfile::TempDir,
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

impl TestHome {
    async fn new() -> Self {
        let lock = AGENTS_IT_HOME_LOCK.lock().await;
        let dir = tempfile::tempdir().expect("tempdir");
        unsafe {
            std::env::set_var("HOME", dir.path());
            std::env::remove_var("USERPROFILE");
        }
        Self {
            _dir: dir,
            _lock: lock,
        }
    }
}

const SUPPORTED_AGENT_IDS: &[&str] = &[
    "claude-code",
    "codex",
    "opencode",
    "hermes",
    "kilocode",
    "openclaw",
    "pi",
];

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("read body")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("valid json")
}

fn auth_header(store: &InMemoryStore) -> String {
    format!(
        "Bearer {}",
        store.inner.read().unwrap().settings.gateway_key
    )
}

#[tokio::test]
async fn it_list_agents_returns_seven_supported_agents() {
    let store = InMemoryStore::new();
    let app = api_router(store.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/agents")
                .header("authorization", auth_header(&store))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    let agents = body.as_array().expect("agents array");
    assert_eq!(agents.len(), 7);

    let ids: Vec<&str> = agents
        .iter()
        .map(|a| a.get("id").and_then(|v| v.as_str()).expect("id"))
        .collect();
    for expected in SUPPORTED_AGENT_IDS {
        assert!(ids.contains(expected), "missing {expected}");
    }
    assert!(!ids.contains(&"cursor"));
    assert!(!ids.contains(&"antigravity"));
}

#[tokio::test]
async fn it_get_removed_agent_returns_not_found() {
    let store = InMemoryStore::new();
    let app = api_router(store.clone());
    for id in ["cursor", "antigravity"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/agents/{id}"))
                    .header("authorization", auth_header(&store))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{id}");
    }
}

#[tokio::test]
async fn it_removed_proxy_endpoints_are_not_mounted() {
    let store = InMemoryStore::new();
    let app = api_router(store.clone());

    // Dedicated hijack handler removed; POST may 405 on `/api/agents/{id}` fallback.
    let hijack = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/agents/hijack-claude")
                .header("authorization", auth_header(&store))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        matches!(
            hijack.status(),
            StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
        ),
        "hijack endpoint still active: {}",
        hijack.status()
    );
    assert!(!hijack.status().is_success());

    let install_proxy = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/agents/codex/install-proxy")
                .header("authorization", auth_header(&store))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert!(
        matches!(
            install_proxy.status(),
            StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED
        ),
        "install-proxy endpoint still active: {}",
        install_proxy.status()
    );
    assert!(!install_proxy.status().is_success());
}

#[tokio::test]
async fn it_update_agent_auto_mode_persists_strategy() {
    let _home = TestHome::new().await;

    let store = InMemoryStore::new();
    let app = api_router(store.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/agents/codex")
                .header("authorization", auth_header(&store))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "mode": "auto", "model_id": "balanced" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body.get("mode").and_then(|v| v.as_str()), Some("auto"));
    assert_eq!(
        body.get("model_id").and_then(|v| v.as_str()),
        Some("balanced")
    );
}

#[tokio::test]
async fn it_update_legacy_proxy_mode_returns_native() {
    let _home = TestHome::new().await;

    let store = InMemoryStore::new();
    let app = api_router(store.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/agents/claude-code")
                .header("authorization", auth_header(&store))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({ "mode": "proxy" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body.get("mode").and_then(|v| v.as_str()), Some("native"));
}
