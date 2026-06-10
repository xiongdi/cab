//! ST (in-process): fast subsystem wiring checks via Tower oneshot.

mod support;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cab_db::InMemoryStore;
use cab_server::build_combined_router;
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

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
async fn st_inprocess_gateway_lists_models() {
    let store = InMemoryStore::new();
    let app = build_combined_router(store.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .header("authorization", auth_header(&store))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert_eq!(body.get("object").and_then(|v| v.as_str()), Some("list"));
}

#[tokio::test]
async fn st_gateway_rejects_missing_auth() {
    let store = InMemoryStore::new();
    let app = build_combined_router(store);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/models")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn st_agent_config_survives_restart() {
    let _home = support::TestHome::new();
    let store = InMemoryStore::new();
    {
        cab_db::agent::update(
            &store,
            "codex",
            &cab_core::types::UpdateAgent {
                mode: Some("auto".to_string()),
                model_id: Some(Some("balanced".to_string())),
                api_key: None,
                endpoint: None,
            },
        )
        .await
        .expect("update agent");
    }

    let reloaded = cab_db::init_store().await.expect("init store");
    let agent = cab_db::agent::get_by_id(&reloaded, "codex")
        .await
        .expect("get agent")
        .expect("codex exists");
    assert_eq!(agent.mode, "auto");
    assert_eq!(agent.model_id.as_deref(), Some("balanced"));
}

#[tokio::test]
async fn st_inprocess_cloudcode_route_removed() {
    let store = InMemoryStore::new();
    let app = build_combined_router(store.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1internal:generateChat")
                .header("content-type", "application/json")
                .header("authorization", auth_header(&store))
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn st_inprocess_agent_catalog_has_seven_entries() {
    let store = InMemoryStore::new();
    let app = build_combined_router(store.clone());
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
    let agents = body.as_array().expect("agents");
    assert_eq!(agents.len(), 7);
    let modes: Vec<&str> = agents
        .iter()
        .map(|a| a.get("mode").and_then(|v| v.as_str()).expect("mode"))
        .collect();
    assert!(
        modes
            .iter()
            .all(|m| matches!(*m, "native" | "auto" | "manual"))
    );
    assert!(!modes.contains(&"proxy"));
}

#[tokio::test]
async fn st_settings_endpoint_available_on_combined_router() {
    let store = InMemoryStore::new();
    let app = build_combined_router(store.clone());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/settings")
                .header("authorization", auth_header(&store))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert!(body.get("gateway_port").and_then(|v| v.as_i64()).is_some());
    assert!(
        body.get("gateway_key")
            .and_then(|v| v.as_str())
            .is_some_and(|k| !k.is_empty())
    );
    assert_eq!(
        body.get("auth_enabled").and_then(|v| v.as_bool()),
        Some(true)
    );
}

#[tokio::test]
async fn st_logs_survive_restart_via_jsonl() {
    let _home = support::TestHome::new();
    let store = InMemoryStore::new();
    cab_db::log::insert(
        &store,
        &cab_core::types::RequestLog {
            id: "log-uat-11".into(),
            timestamp: "2026-06-10T12:00:00Z".into(),
            agent: "codex".into(),
            provider: "provider-1".into(),
            model: "provider/model-1".into(),
            input_tokens: 3,
            output_tokens: 5,
            total_tokens: 8,
            latency_ms: 42,
            status: 200,
            error: None,
            path: "/v1/chat/completions".into(),
            stream: false,
        },
    )
    .await
    .expect("insert log");

    let reloaded = cab_db::init_store().await.expect("init store");
    let page = cab_db::log::query(
        &reloaded,
        &cab_core::types::LogQuery {
            page: Some(1),
            per_page: Some(10),
            ..Default::default()
        },
    )
    .await
    .expect("query logs");
    assert_eq!(page.total, 1);
    assert_eq!(page.data[0].id, "log-uat-11");
}

#[tokio::test]
async fn st_routing_explain_returns_decision_trace() {
    let store = InMemoryStore::new();
    {
        let mut data = store.inner.write().unwrap();
        data.providers.insert(
            "provider-1".into(),
            cab_core::types::Provider {
                id: "provider-1".into(),
                name: "Provider One".into(),
                endpoints: vec![cab_core::types::ProviderEndpoint {
                    id: "chat".into(),
                    protocol: "openai-chat".into(),
                    url: "https://provider.test/v1".into(),
                    label: None,
                    priority: 50,
                    enabled: true,
                }],
                api_key: "key".into(),
                enabled: true,
                created_at: "now".into(),
                updated_at: "now".into(),
                privacy_policy_url: None,
                terms_of_service_url: None,
                status_page_url: None,
                headquarters: None,
                datacenters: None,
                api_keys: vec![cab_core::types::ApiKeyConfig {
                    key: "key".into(),
                    enabled: true,
                    subscribed: false,
                    quota_reset_at: None,
                }],
                api: None,
                doc: None,
                env: None,
                npm: None,
                model_count: 1,
                catalog_models: vec![],
            },
        );
        data.models.insert(
            "model-1".into(),
            cab_core::types::Model {
                id: "model-1".into(),
                name: "provider/model-1".into(),
                display_name: "Model One".into(),
                provider_id: "provider-1".into(),
                protocol: "openai-chat".into(),
                context_length: 128000,
                input_cost: Some(1.0),
                output_cost: Some(2.0),
                enabled: true,
                overall_intelligence: 80.0,
                coding_index: 85.0,
                agentic_index: 80.0,
                math_index: 75.0,
                created_at: "now".into(),
                updated_at: "now".into(),
                canonical_slug: None,
                hugging_face_id: None,
                created: None,
                description: None,
                architecture: None,
                pricing: None,
                top_provider: None,
                per_request_limits: None,
                supported_parameters: None,
                default_parameters: None,
                supported_voices: None,
                knowledge_cutoff: None,
                expiration_date: None,
                links: None,
            },
        );
    }

    let app = build_combined_router(store.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/routing/explain")
                .header("authorization", auth_header(&store))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "agent": "codex",
                        "model": "auto",
                        "body": {"messages": [{"role": "user", "content": "hello"}]}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert!(
        body.get("decision_steps")
            .and_then(|v| v.as_array())
            .is_some_and(|s| !s.is_empty())
    );
    assert!(
        body.get("ranked_candidates")
            .and_then(|v| v.as_array())
            .is_some_and(|c| !c.is_empty())
    );
    assert!(body.get("resolved").is_some());
}
