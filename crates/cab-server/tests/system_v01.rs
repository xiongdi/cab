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

#[tokio::test]
async fn st_inprocess_gateway_lists_models() {
    let app = build_combined_router(InMemoryStore::new());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/models")
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
async fn st_inprocess_cloudcode_route_removed() {
    let app = build_combined_router(InMemoryStore::new());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1internal:generateChat")
                .header("content-type", "application/json")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn st_inprocess_agent_catalog_has_seven_entries() {
    let app = build_combined_router(InMemoryStore::new());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/agents")
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
    let app = build_combined_router(InMemoryStore::new());
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/settings")
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
}
