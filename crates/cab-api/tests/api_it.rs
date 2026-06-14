//! IT: Full management API contract (providers, models, routes, settings, logs, dashboard).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use cab_api::api_router;
use cab_db::InMemoryStore;

async fn store_with_catalog() -> InMemoryStore {
    let store = InMemoryStore::new();
    cab_api::providers::sync_models_dev_catalog(&store)
        .await
        .expect("seed bundled catalog");
    store
}
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

async fn request(
    store: &InMemoryStore,
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> axum::response::Response {
    let token = store.inner.read().unwrap().settings.gateway_key.clone();
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));
    let req_body = if let Some(json) = body {
        builder = builder.header("content-type", "application/json");
        Body::from(json.to_string())
    } else {
        Body::empty()
    };
    app.clone()
        .oneshot(builder.body(req_body).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn it_settings_get_and_put_roundtrip() {
    let home = std::env::temp_dir().join(format!(
        "cab-it-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&home).expect("temp home");
    unsafe { std::env::set_var("HOME", &home) };

    let store = InMemoryStore::new();
    let app = api_router(store.clone());
    let get = request(&store, &app, "GET", "/api/settings", None).await;
    assert_eq!(get.status(), StatusCode::OK);
    let mut settings = json_body(get).await;
    assert!(
        settings
            .get("gateway_port")
            .and_then(|v| v.as_i64())
            .is_some()
    );
    settings["gateway_port"] = serde_json::json!(3999);
    settings.as_object_mut().unwrap().remove("providers");
    settings.as_object_mut().unwrap().remove("models");

    let put = request(&store, &app, "PUT", "/api/settings", Some(settings)).await;
    assert_eq!(put.status(), StatusCode::OK);
    let updated = json_body(put).await;
    assert_eq!(
        updated.get("gateway_port").and_then(|v| v.as_i64()),
        Some(3999)
    );
}

#[tokio::test]
async fn it_providers_list_returns_catalog() {
    let store = store_with_catalog().await;
    let app = api_router(store.clone());
    let response = request(&store, &app, "GET", "/api/providers", None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert!(!body.as_array().expect("providers").is_empty());
}

#[tokio::test]
async fn it_routes_crud_lifecycle() {
    let store = InMemoryStore::new();
    let app = api_router(store.clone());

    let create = request(
        &store,
        &app,
        "POST",
        "/api/routes",
        Some(serde_json::json!({
            "name": "Claude default",
            "agent_pattern": "claude-code",
            "primary_model_id": "auto",
            "routing_strategy": "balanced",
            "enabled": true
        })),
    )
    .await;
    assert_eq!(create.status(), StatusCode::CREATED);
    let created = json_body(create).await;
    let id = created
        .get("id")
        .and_then(|v| v.as_str())
        .expect("route id");

    let get = request(&store, &app, "GET", &format!("/api/routes/{id}"), None).await;
    assert_eq!(get.status(), StatusCode::OK);

    let update = request(
        &store,
        &app,
        "PUT",
        &format!("/api/routes/{id}"),
        Some(serde_json::json!({ "routing_strategy": "intelligent" })),
    )
    .await;
    assert_eq!(update.status(), StatusCode::OK);
    let updated = json_body(update).await;
    assert_eq!(
        updated.get("routing_strategy").and_then(|v| v.as_str()),
        Some("intelligent")
    );

    let list = request(&store, &app, "GET", "/api/routes", None).await;
    assert_eq!(list.status(), StatusCode::OK);
    let routes = json_body(list).await;
    assert!(
        routes
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r.get("id") == Some(&Value::String(id.to_string())))
    );

    let delete = request(&store, &app, "DELETE", &format!("/api/routes/{id}"), None).await;
    assert_eq!(delete.status(), StatusCode::NO_CONTENT);

    let missing = request(&store, &app, "GET", &format!("/api/routes/{id}"), None).await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn it_models_and_catalog_endpoints_respond() {
    let store = store_with_catalog().await;
    let app = api_router(store.clone());

    let models = request(&store, &app, "GET", "/api/models", None).await;
    assert_eq!(models.status(), StatusCode::OK);

    let catalog = request(&store, &app, "GET", "/api/models/catalog", None).await;
    assert_eq!(catalog.status(), StatusCode::OK);
    let catalog_body = json_body(catalog).await;
    assert!(catalog_body.is_array() || catalog_body.get("models").is_some());
}

#[tokio::test]
async fn it_dashboard_stats_shape() {
    let store = InMemoryStore::new();
    let app = api_router(store.clone());
    let response = request(&store, &app, "GET", "/api/dashboard/stats", None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert!(body.get("total_requests").is_some());
    assert!(body.get("active_providers").is_some());
    assert!(body.get("active_models").is_some());
}

#[tokio::test]
async fn it_logs_query_pagination_defaults() {
    let store = InMemoryStore::new();
    let app = api_router(store.clone());
    let response = request(&store, &app, "GET", "/api/logs", None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert!(body.get("data").and_then(|v| v.as_array()).is_some());
    assert_eq!(body.get("page").and_then(|v| v.as_i64()), Some(1));
}

#[tokio::test]
async fn it_catalog_status_endpoint() {
    let store = InMemoryStore::new();
    let app = api_router(store.clone());
    let response = request(&store, &app, "GET", "/api/settings/catalog-status", None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = json_body(response).await;
    assert!(body.get("sources").and_then(|v| v.as_array()).is_some());
}
