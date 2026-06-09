//! Shared helpers for ST/UAT tests (real TCP + isolated HOME).
#![allow(dead_code)]

pub mod local_uat;

use axum::Router;
use cab_db::InMemoryStore;
use cab_server::build_combined_router;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

pub const SUPPORTED_AGENT_IDS: &[&str] = &[
    "claude-code",
    "codex",
    "opencode",
    "hermes",
    "kilocode",
    "openclaw",
    "pi",
];

static HOME_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Isolated HOME so settings/agent config writes never touch the developer machine.
pub struct TestHome {
    _dir: tempfile::TempDir,
    _lock: std::sync::MutexGuard<'static, ()>,
}

impl TestHome {
    pub fn new() -> Self {
        let lock = HOME_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = tempfile::tempdir().expect("tempdir");
        // SAFETY: guarded by HOME_ENV_LOCK — only one test mutates HOME at a time.
        unsafe { std::env::set_var("HOME", dir.path()) };
        Self {
            _dir: dir,
            _lock: lock,
        }
    }
}

pub struct TestServer {
    pub base_url: String,
    pub client: reqwest::Client,
    _shutdown: oneshot::Sender<()>,
    _task: JoinHandle<()>,
}

/// Load bundled models.dev catalog into the store (no external network required).
pub async fn seed_catalog(store: &InMemoryStore) {
    cab_api::providers::sync_models_dev_catalog(store)
        .await
        .expect("seed bundled catalog");
}

pub async fn spawn_test_server(store: InMemoryStore) -> TestServer {
    let app = build_combined_router(store);
    spawn_router(app).await
}

pub async fn spawn_seeded_server() -> TestServer {
    let store = InMemoryStore::new();
    seed_catalog(&store).await;
    spawn_test_server(store).await
}

pub async fn spawn_router(app: Router) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let task = tokio::spawn(async move {
        let app = app.into_make_service();
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("serve");
    });

    let base_url = format!("http://{addr}");
    // Brief yield so accept loop is ready.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    TestServer {
        base_url,
        client: reqwest::Client::new(),
        _shutdown: shutdown_tx,
        _task: task,
    }
}

pub async fn get_json(server: &TestServer, path: &str) -> serde_json::Value {
    let url = format!("{}{}", server.base_url, path);
    let response = server
        .client
        .get(&url)
        .send()
        .await
        .unwrap_or_else(|e| panic!("GET {url} failed: {e}"));
    assert!(
        response.status().is_success(),
        "GET {url} status {}",
        response.status()
    );
    response.json().await.expect("json body")
}

pub async fn put_json(
    server: &TestServer,
    path: &str,
    body: serde_json::Value,
) -> serde_json::Value {
    let url = format!("{}{}", server.base_url, path);
    let response = server
        .client
        .put(&url)
        .json(&body)
        .send()
        .await
        .unwrap_or_else(|e| panic!("PUT {url} failed: {e}"));
    assert!(
        response.status().is_success(),
        "PUT {url} status {}",
        response.status()
    );
    response.json().await.expect("json body")
}

pub async fn post_json(
    server: &TestServer,
    path: &str,
    body: serde_json::Value,
) -> serde_json::Value {
    let url = format!("{}{}", server.base_url, path);
    let response = server
        .client
        .post(&url)
        .json(&body)
        .send()
        .await
        .unwrap_or_else(|e| panic!("POST {url} failed: {e}"));
    assert!(
        response.status().is_success(),
        "POST {url} status {}",
        response.status()
    );
    response.json().await.expect("json body")
}

pub async fn get_status(server: &TestServer, path: &str) -> reqwest::StatusCode {
    let url = format!("{}{}", server.base_url, path);
    server
        .client
        .get(&url)
        .send()
        .await
        .expect("request")
        .status()
}

/// Enable the first catalog provider and one of its models so gateway routing can resolve.
pub async fn enable_first_routable_model(server: &TestServer) -> (String, String) {
    let providers = get_json(server, "/api/providers").await;
    let provider = providers
        .as_array()
        .and_then(|a| a.first())
        .expect("catalog provider");
    let provider_id = provider
        .get("id")
        .and_then(|v| v.as_str())
        .expect("provider id")
        .to_string();

    put_json(
        server,
        &format!("/api/providers/{provider_id}"),
        serde_json::json!({
            "enabled": true,
            "api_key": "uat-test-key",
            "api_keys": [{ "key": "uat-test-key", "enabled": true }]
        }),
    )
    .await;

    let models = get_json(server, "/api/models").await;
    let model = models
        .as_array()
        .and_then(|a| {
            a.iter().find(|m| {
                m.get("provider_id").and_then(|v| v.as_str()) == Some(provider_id.as_str())
            })
        })
        .or_else(|| models.as_array().and_then(|a| a.first()))
        .expect("catalog model");
    let model_id = model
        .get("id")
        .and_then(|v| v.as_str())
        .expect("model id")
        .to_string();
    let model_name = model
        .get("name")
        .and_then(|v| v.as_str())
        .expect("model name")
        .to_string();

    put_json(
        server,
        &format!("/api/models/{model_id}"),
        serde_json::json!({ "enabled": true }),
    )
    .await;

    (provider_id, model_name)
}

pub async fn post_status(server: &TestServer, path: &str) -> reqwest::StatusCode {
    let url = format!("{}{}", server.base_url, path);
    server
        .client
        .post(&url)
        .send()
        .await
        .expect("request")
        .status()
}
