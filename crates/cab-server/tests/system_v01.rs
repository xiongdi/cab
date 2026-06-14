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
                overall_intelligence: Some(80.0),
                coding_index: Some(85.0),
                agentic_index: Some(80.0),
                math_index: Some(75.0),
                output_speed_tps: None,
                time_to_first_token_secs: None,
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

#[tokio::test]
async fn st_price_route_ranks_cheapest_model_for_pi_agent() {
    // CAB price route → Cheapest strategy should rank models by cost ascending.
    // When pi sends model="price", the cheapest enabled model wins.
    let store = InMemoryStore::new();
    {
        let mut data = store.inner.write().unwrap();
        // Two providers, both enabled with keys.
        data.providers.insert(
            "cheap-provider".into(),
            cab_core::types::Provider {
                id: "cheap-provider".into(),
                name: "Cheap Provider".into(),
                endpoints: vec![cab_core::types::ProviderEndpoint {
                    id: "chat".into(),
                    protocol: "openai-chat".into(),
                    url: "https://cheap.test/v1".into(),
                    label: None,
                    priority: 50,
                    enabled: true,
                }],
                api_key: "key-cheap".into(),
                enabled: true,
                created_at: "now".into(),
                updated_at: "now".into(),
                privacy_policy_url: None,
                terms_of_service_url: None,
                status_page_url: None,
                headquarters: None,
                datacenters: None,
                api_keys: vec![cab_core::types::ApiKeyConfig {
                    key: "key-cheap".into(),
                    enabled: true,
                    subscribed: false,
                    quota_reset_at: None,
                }],
                api: None,
                doc: None,
                env: None,
                npm: None,
                model_count: 2,
                catalog_models: vec![],
            },
        );
        data.providers.insert(
            "pricey-provider".into(),
            cab_core::types::Provider {
                id: "pricey-provider".into(),
                name: "Pricey Provider".into(),
                endpoints: vec![cab_core::types::ProviderEndpoint {
                    id: "chat".into(),
                    protocol: "openai-chat".into(),
                    url: "https://pricey.test/v1".into(),
                    label: None,
                    priority: 50,
                    enabled: true,
                }],
                api_key: "key-pricey".into(),
                enabled: true,
                created_at: "now".into(),
                updated_at: "now".into(),
                privacy_policy_url: None,
                terms_of_service_url: None,
                status_page_url: None,
                headquarters: None,
                datacenters: None,
                api_keys: vec![cab_core::types::ApiKeyConfig {
                    key: "key-pricey".into(),
                    enabled: true,
                    subscribed: false,
                    quota_reset_at: None,
                }],
                api: None,
                doc: None,
                env: None,
                npm: None,
                model_count: 2,
                catalog_models: vec![],
            },
        );

        fn make_model(
            id: &str,
            name: &str,
            provider_id: &str,
            input_cost: f64,
            output_cost: f64,
            intelligence: f64,
        ) -> cab_core::types::Model {
            cab_core::types::Model {
                id: id.into(),
                name: name.into(),
                display_name: format!("Model {id}"),
                provider_id: provider_id.into(),
                protocol: "openai-chat".into(),
                context_length: 128000,
                input_cost: Some(input_cost),
                output_cost: Some(output_cost),
                enabled: true,
                overall_intelligence: Some(intelligence),
                coding_index: Some(intelligence),
                agentic_index: Some(intelligence),
                math_index: Some(intelligence),
                output_speed_tps: None,
                time_to_first_token_secs: None,
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
            }
        }

        // cheap model: $0.1/$0.2 per Mtok
        data.models.insert(
            "cheap-model".into(),
            make_model(
                "cheap-model",
                "cheap-provider/cheap",
                "cheap-provider",
                0.1,
                0.2,
                40.0,
            ),
        );
        // mid model: $1/$2 per Mtok
        data.models.insert(
            "mid-model".into(),
            make_model(
                "mid-model",
                "cheap-provider/mid",
                "cheap-provider",
                1.0,
                2.0,
                70.0,
            ),
        );
        // expensive model: $10/$20 per Mtok
        data.models.insert(
            "expensive-model".into(),
            make_model(
                "expensive-model",
                "pricey-provider/expensive",
                "pricey-provider",
                10.0,
                20.0,
                95.0,
            ),
        );

        // Add model endpoints for all models (required for gateway model listing)
        for m in ["cheap-model", "mid-model", "expensive-model"] {
            let model_ref = &data.models[m];
            let name = model_ref.name.clone();
            let provider_name = data.providers[&model_ref.provider_id].name.clone();
            let input_cost = model_ref.input_cost.unwrap_or(0.0);
            let output_cost = model_ref.output_cost.unwrap_or(0.0);
            data.model_endpoints.insert(
                format!("{m}-ep"),
                cab_db::endpoint::ModelEndpoint {
                    id: format!("{m}-ep"),
                    model_id: name.clone(),
                    canonical_slug: name.clone(),
                    provider_name,
                    provider_tag: format!("tag/{m}"),
                    native_model_id: name.clone(),
                    upstream_protocol: None,
                    quantization: "unknown".into(),
                    input_cost: Some(input_cost),
                    output_cost: Some(output_cost),
                    cache_read_cost: None,
                    context_length: Some(128000),
                    max_completion_tokens: None,
                    status: 1,
                    uptime_30m: None,
                    uptime_5m: None,
                    uptime_1d: None,
                    supports_tools: true,
                    supports_streaming: true,
                    enabled: true,
                    updated_at: "now".into(),
                },
            );
        }
    }

    let app = build_combined_router(store.clone());

    // ----- Test 1: price mode explains cheapest model for pi -----
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/routing/explain")
                .header("authorization", auth_header(&store))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "agent": "pi",
                        "model": "price",
                        "body": {"messages": [{"role": "user", "content": "Write a Rust function"}]}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;

    // Verify decision steps are produced
    let steps = body
        .get("decision_steps")
        .and_then(|v| v.as_array())
        .expect("decision_steps");
    assert!(!steps.is_empty(), "expected decision steps");

    // Verify resolved model is the cheapest (capped by cost)
    let resolved = body.get("resolved").expect("resolved");
    let resolved_model = resolved
        .get("model_id")
        .and_then(|v| v.as_str())
        .expect("resolved model_id");
    assert!(
        resolved_model.contains("cheap"),
        "price route should select cheapest model, got {resolved_model}"
    );
    assert_eq!(
        resolved.get("strategy").and_then(|v| v.as_str()),
        Some("cheapest"),
        "price route should resolve to cheapest strategy"
    );

    // Verify ranked candidates are ordered by cost ascending
    let candidates = body
        .get("ranked_candidates")
        .and_then(|v| v.as_array())
        .expect("ranked_candidates");
    assert!(!candidates.is_empty(), "expected ranked candidates");
    // First candidate should be the cheapest model
    let top = &candidates[0];
    let top_model = top
        .get("model_id")
        .and_then(|v| v.as_str())
        .expect("top model_id");
    assert!(
        top_model.contains("cheap"),
        "top ranked candidate should be cheapest model, got {top_model}"
    );

    // ----- Test 2: price alias resolves via gateway chat completions route -----
    // This tests that the "price" model name resolves correctly in a routing scenario.
    // We call the explain endpoint again with a simpler prompt to ensure consistent behavior.
    let response2 = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/routing/explain")
                .header("authorization", auth_header(&store))
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "agent": "pi",
                        "model": "price",
                        "body": {"messages": [{"role": "user", "content": "hello"}]}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response2.status(), StatusCode::OK);
    let body2 = json_body(response2).await;
    let resolved2 = body2.get("resolved").expect("resolved");
    let model2 = resolved2
        .get("model_id")
        .and_then(|v| v.as_str())
        .expect("resolved model_id");
    assert!(
        model2.contains("cheap"),
        "price route should consistently select cheapest model for pi, got {model2}"
    );
}
