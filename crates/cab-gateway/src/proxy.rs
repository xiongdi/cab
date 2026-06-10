use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use cab_core::{CabError, extract_retry_after};
use futures::TryStreamExt;
use reqwest::Client;

/// Forward a request to the upstream provider and return the response.
///
/// For streaming requests, the upstream SSE stream is piped through directly.
/// For non-streaming, the full response body is returned.
pub async fn proxy_request(
    client: &Client,
    upstream_url: &str,
    api_key: &str,
    protocol: &str,
    body: Bytes,
    headers: &HeaderMap,
    stream: bool,
) -> Result<Response, CabError> {
    let mut req = client.post(upstream_url).body(body.clone());

    // Forward relevant headers
    if let Some(ct) = headers.get("content-type") {
        req = req.header("content-type", ct);
    } else {
        req = req.header("content-type", "application/json");
    }

    // Set authorization header
    if !api_key.is_empty() {
        // Check if we need x-api-key (Anthropic style) or Bearer (OpenAI style)
        if protocol == "anthropic" {
            req = req.header("x-api-key", api_key);
            // Forward anthropic-version if present, or set default
            if let Some(v) = headers.get("anthropic-version") {
                req = req.header("anthropic-version", v);
            } else {
                req = req.header("anthropic-version", "2023-06-01");
            }
        } else {
            req = req.header("authorization", format!("Bearer {api_key}"));
        }
    } else {
        // Pass through existing auth headers from the client
        if let Some(auth) = headers.get("authorization") {
            req = req.header("authorization", auth);
        }
        if let Some(xkey) = headers.get("x-api-key") {
            req = req.header("x-api-key", xkey);
        }
        if let Some(xkey) = headers.get("x-goog-api-key") {
            req = req.header("x-goog-api-key", xkey);
        }
        if let Some(v) = headers.get("anthropic-version") {
            req = req.header("anthropic-version", v);
        }
    }

    let upstream_resp = req.send().await.map_err(|e| {
        tracing::error!("Upstream request failed: {e}");
        CabError::Proxy(format!("Failed to connect to upstream: {e}"))
    })?;

    let status = upstream_resp.status();

    if !status.is_success() {
        let retry_after = extract_retry_after(upstream_resp.headers());
        let body_text = upstream_resp
            .text()
            .await
            .unwrap_or_else(|_| "unknown error".to_string());
        return Err(CabError::ProviderError {
            status: status.as_u16(),
            body: body_text,
            retry_after,
        });
    }

    if stream {
        // Stream the response body through as SSE
        let content_type = upstream_resp
            .headers()
            .get("content-type")
            .cloned()
            .unwrap_or_else(|| HeaderValue::from_static("text/event-stream"));

        let byte_stream = upstream_resp.bytes_stream().map_err(std::io::Error::other);

        let body = Body::from_stream(byte_stream);

        let mut response = Response::builder()
            .status(status.as_u16())
            .header("content-type", content_type)
            .header("cache-control", "no-cache")
            .body(body)
            .map_err(|e| CabError::Proxy(format!("Failed to build response: {e}")))?;

        let resp_headers = response.headers_mut();
        let _ = resp_headers;

        Ok(response)
    } else {
        let resp_bytes = upstream_resp
            .bytes()
            .await
            .map_err(|e| CabError::Proxy(format!("Failed to read response: {e}")))?;

        Ok(Response::builder()
            .status(status.as_u16())
            .header("content-type", "application/json")
            .body(Body::from(resp_bytes))
            .map_err(|e| CabError::Proxy(format!("Failed to build response: {e}")))?)
    }
}

/// Simple proxy for passing through a GET request.
pub async fn proxy_get(
    client: &Client,
    upstream_url: &str,
    api_key: &str,
) -> Result<impl IntoResponse, CabError> {
    let mut req = client.get(upstream_url);

    if !api_key.is_empty() {
        req = req.header("authorization", format!("Bearer {api_key}"));
    }

    let resp = req
        .send()
        .await
        .map_err(|e| CabError::Proxy(format!("Failed to connect to upstream: {e}")))?;

    let status = resp.status();
    let body_bytes = resp
        .bytes()
        .await
        .map_err(|e| CabError::Proxy(format!("Failed to read response: {e}")))?;

    Response::builder()
        .status(status.as_u16())
        .header("content-type", "application/json")
        .body(Body::from(body_bytes))
        .map_err(|e| CabError::Proxy(format!("Failed to build response: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::{HeaderMap, StatusCode};
    use axum::response::IntoResponse;
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    struct TestServer {
        base_url: String,
        shutdown: Option<oneshot::Sender<()>>,
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            if let Some(shutdown) = self.shutdown.take() {
                let _ = shutdown.send(());
            }
        }
    }

    async fn spawn_router(app: Router) -> TestServer {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app.into_make_service())
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await
                .unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        TestServer {
            base_url: format!("http://{addr}"),
            shutdown: Some(tx),
        }
    }

    async fn echo_post(headers: HeaderMap, body: Bytes) -> impl IntoResponse {
        Json(serde_json::json!({
            "authorization": headers.get("authorization").and_then(|v| v.to_str().ok()),
            "x_api_key": headers.get("x-api-key").and_then(|v| v.to_str().ok()),
            "x_goog_api_key": headers.get("x-goog-api-key").and_then(|v| v.to_str().ok()),
            "anthropic_version": headers.get("anthropic-version").and_then(|v| v.to_str().ok()),
            "content_type": headers.get("content-type").and_then(|v| v.to_str().ok()),
            "body": serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
        }))
    }

    async fn error_post() -> impl IntoResponse {
        (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "7")],
            "rate limited",
        )
    }

    async fn stream_post() -> impl IntoResponse {
        (
            [("content-type", "text/event-stream")],
            "data: {\"ok\":true}\n\n",
        )
    }

    async fn get_handler(headers: HeaderMap) -> impl IntoResponse {
        Json(serde_json::json!({
            "authorization": headers.get("authorization").and_then(|v| v.to_str().ok()),
            "ok": true,
        }))
    }

    async fn json_from_response(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn proxy_request_sets_bearer_auth_and_default_content_type() {
        let server = spawn_router(Router::new().route("/post", post(echo_post))).await;
        let response = proxy_request(
            &Client::new(),
            &format!("{}/post", server.base_url),
            "secret",
            "openai-chat",
            Bytes::from_static(br#"{"hello":"world"}"#),
            &HeaderMap::new(),
            false,
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "application/json");
        let json = json_from_response(response).await;
        assert_eq!(json["authorization"], "Bearer secret");
        assert_eq!(json["content_type"], "application/json");
        assert_eq!(json["body"]["hello"], "world");
    }

    #[tokio::test]
    async fn proxy_request_sets_anthropic_auth_and_version() {
        let server = spawn_router(Router::new().route("/post", post(echo_post))).await;
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            HeaderValue::from_static("application/custom+json"),
        );
        headers.insert("anthropic-version", HeaderValue::from_static("2024-01-01"));

        let response = proxy_request(
            &Client::new(),
            &format!("{}/post", server.base_url),
            "anthropic-key",
            "anthropic",
            Bytes::from_static(br#"{"message":"hi"}"#),
            &headers,
            false,
        )
        .await
        .unwrap();
        let json = json_from_response(response).await;

        assert_eq!(json["x_api_key"], "anthropic-key");
        assert_eq!(json["anthropic_version"], "2024-01-01");
        assert_eq!(json["content_type"], "application/custom+json");
    }

    #[tokio::test]
    async fn proxy_request_defaults_anthropic_version_and_passes_existing_auth_without_key() {
        let server = spawn_router(Router::new().route("/post", post(echo_post))).await;
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer existing"));
        headers.insert("x-api-key", HeaderValue::from_static("x-existing"));
        headers.insert("x-goog-api-key", HeaderValue::from_static("goog-existing"));

        let response = proxy_request(
            &Client::new(),
            &format!("{}/post", server.base_url),
            "",
            "anthropic",
            Bytes::from_static(br#"{"message":"hi"}"#),
            &headers,
            false,
        )
        .await
        .unwrap();
        let json = json_from_response(response).await;

        assert_eq!(json["authorization"], "Bearer existing");
        assert_eq!(json["x_api_key"], "x-existing");
        assert_eq!(json["x_goog_api_key"], "goog-existing");
        assert_eq!(json["anthropic_version"], serde_json::Value::Null);

        let response = proxy_request(
            &Client::new(),
            &format!("{}/post", server.base_url),
            "anthropic-key",
            "anthropic",
            Bytes::from_static(br#"{"message":"hi"}"#),
            &HeaderMap::new(),
            false,
        )
        .await
        .unwrap();
        let json = json_from_response(response).await;
        assert_eq!(json["anthropic_version"], "2023-06-01");
    }

    #[tokio::test]
    async fn proxy_request_returns_provider_error_with_retry_after() {
        let server = spawn_router(Router::new().route("/error", post(error_post))).await;

        let err = proxy_request(
            &Client::new(),
            &format!("{}/error", server.base_url),
            "secret",
            "openai-chat",
            Bytes::from_static(b"{}"),
            &HeaderMap::new(),
            false,
        )
        .await
        .unwrap_err();

        match err {
            CabError::ProviderError {
                status,
                body,
                retry_after,
            } => {
                assert_eq!(status, 429);
                assert_eq!(body, "rate limited");
                assert!(retry_after.is_some());
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn proxy_request_streams_sse_response() {
        let server = spawn_router(Router::new().route("/stream", post(stream_post))).await;

        let response = proxy_request(
            &Client::new(),
            &format!("{}/stream", server.base_url),
            "secret",
            "openai-chat",
            Bytes::from_static(b"{}"),
            &HeaderMap::new(),
            true,
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "text/event-stream");
        assert_eq!(response.headers()["cache-control"], "no-cache");
        let bytes = to_bytes(response.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();
        assert_eq!(bytes, Bytes::from_static(b"data: {\"ok\":true}\n\n"));
    }

    #[tokio::test]
    async fn proxy_get_forwards_bearer_auth_and_json_body() {
        let server = spawn_router(Router::new().route("/get", get(get_handler))).await;

        let response = proxy_get(
            &Client::new(),
            &format!("{}/get", server.base_url),
            "read-key",
        )
        .await
        .unwrap()
        .into_response();
        let json = json_from_response(response).await;

        assert_eq!(json["authorization"], "Bearer read-key");
        assert_eq!(json["ok"], true);
    }

    #[tokio::test]
    async fn proxy_request_connection_failure_is_proxy_error() {
        let err = proxy_request(
            &Client::new(),
            "http://127.0.0.1:1/unavailable",
            "secret",
            "openai-chat",
            Bytes::from_static(b"{}"),
            &HeaderMap::new(),
            false,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, CabError::Proxy(message) if message.contains("Failed to connect")));
    }
}
