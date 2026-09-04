use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use cab_core::{CabError, extract_retry_after};
use futures::{Stream, TryStreamExt};
use reqwest::Client;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

/// Default timeout for waiting for response headers from upstream (Time To First Byte / TTFB).
pub const UPSTREAM_TTFB_TIMEOUT: Duration = Duration::from_secs(60);

/// Default maximum idle time between successive chunks in an active stream before timing out.
pub const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Timeout configuration for proxying requests upstream.
#[derive(Debug, Clone, Copy)]
pub struct ProxyTimeouts {
    pub ttfb: Duration,
    pub stream_idle: Duration,
}

impl Default for ProxyTimeouts {
    fn default() -> Self {
        Self {
            ttfb: UPSTREAM_TTFB_TIMEOUT,
            stream_idle: STREAM_IDLE_TIMEOUT,
        }
    }
}

/// A stream wrapper that enforces a maximum idle duration between items.
pub struct IdleTimeoutStream<S> {
    inner: Pin<Box<S>>,
    idle_timeout: Duration,
    sleep: Pin<Box<tokio::time::Sleep>>,
    timed_out: bool,
}

impl<S> IdleTimeoutStream<S>
where
    S: Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
{
    pub fn new(inner: S, idle_timeout: Duration) -> Self {
        Self {
            inner: Box::pin(inner),
            idle_timeout,
            sleep: Box::pin(tokio::time::sleep(idle_timeout)),
            timed_out: false,
        }
    }
}

impl<S> Stream for IdleTimeoutStream<S>
where
    S: Stream<Item = Result<Bytes, std::io::Error>> + Send + 'static,
{
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.timed_out {
            return Poll::Ready(None);
        }

        match self.inner.as_mut().poll_next(cx) {
            Poll::Ready(Some(Ok(bytes))) => {
                let deadline = tokio::time::Instant::now() + self.idle_timeout;
                self.sleep.as_mut().reset(deadline);
                Poll::Ready(Some(Ok(bytes)))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => {
                if self.sleep.as_mut().poll(cx).is_ready() {
                    self.timed_out = true;
                    tracing::warn!(
                        "Upstream stream idle timeout exceeded (no data for {}s)",
                        self.idle_timeout.as_secs()
                    );
                    Poll::Ready(Some(Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!(
                            "Upstream stream idle timeout exceeded (no data for {}s)",
                            self.idle_timeout.as_secs()
                        ),
                    ))))
                } else {
                    Poll::Pending
                }
            }
        }
    }
}

/// Forward a request to the upstream provider and return the response.
///
/// For streaming requests, the upstream SSE stream is piped through directly.
/// For non-streaming, the full response body is returned.
///
/// Auth uses `Authorization: Bearer <api_key>` per the HTTP authentication
/// standard. Both OpenAI and Anthropic APIs accept this scheme.
pub async fn proxy_request(
    client: &Client,
    upstream_url: &str,
    api_key: &str,
    protocol: &str,
    body: Bytes,
    headers: &HeaderMap,
    stream: bool,
) -> Result<Response, CabError> {
    proxy_request_with_timeouts(
        client,
        upstream_url,
        api_key,
        protocol,
        body,
        headers,
        stream,
        ProxyTimeouts::default(),
    )
    .await
}

/// Forward a request to the upstream provider with explicit TTFB and streaming idle timeouts.
#[allow(clippy::too_many_arguments)]
pub async fn proxy_request_with_timeouts(
    client: &Client,
    upstream_url: &str,
    api_key: &str,
    protocol: &str,
    body: Bytes,
    headers: &HeaderMap,
    stream: bool,
    timeouts: ProxyTimeouts,
) -> Result<Response, CabError> {
    let build_req = || {
        let mut req = client.post(upstream_url).body(body.clone());

        // Forward relevant headers
        if let Some(ct) = headers.get("content-type") {
            req = req.header("content-type", ct);
        } else {
            req = req.header("content-type", "application/json");
        }

        // Set authorization header
        if !api_key.is_empty() {
            req = req.header("authorization", format!("Bearer {api_key}"));

            // Anthropic-compatible endpoints expect the key as `x-api-key`;
            // some (e.g. opencode.ai Console Go) reject `Authorization: Bearer` alone.
            if protocol == "anthropic-messages" {
                req = req.header("x-api-key", api_key);
            }

            // Forward anthropic-version if the client sent it
            if let Some(v) = headers.get("anthropic-version") {
                req = req.header("anthropic-version", v);
            }
        } else {
            // Pass through existing auth headers from the client
            if let Some(auth) = headers.get("authorization") {
                req = req.header("authorization", auth);
            }
            if let Some(xkey) = headers.get("x-goog-api-key") {
                req = req.header("x-goog-api-key", xkey);
            }
            if let Some(v) = headers.get("anthropic-version") {
                req = req.header("anthropic-version", v);
            }
        }

        req
    };

    let upstream_resp = match tokio::time::timeout(timeouts.ttfb, build_req().send()).await {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) if e.is_connect() || e.is_request() => {
            tracing::warn!("Transient upstream connection error ({e}), retrying once...");
            match tokio::time::timeout(timeouts.ttfb, build_req().send()).await {
                Ok(Ok(resp)) => resp,
                Ok(Err(e2)) => {
                    tracing::error!("Upstream request failed after retry: {e2}");
                    return Err(CabError::Proxy(format!(
                        "Failed to connect to upstream: {e2}"
                    )));
                }
                Err(_) => {
                    tracing::error!(
                        "Upstream request timed out waiting for response headers (TTFB > {}s)",
                        timeouts.ttfb.as_secs()
                    );
                    return Err(CabError::Proxy(format!(
                        "Upstream request timed out waiting for response headers (TTFB > {}s)",
                        timeouts.ttfb.as_secs()
                    )));
                }
            }
        }
        Ok(Err(e)) => {
            tracing::error!("Upstream request failed: {e}");
            return Err(CabError::Proxy(format!(
                "Failed to connect to upstream: {e}"
            )));
        }
        Err(_) => {
            tracing::error!(
                "Upstream request timed out waiting for response headers (TTFB > {}s)",
                timeouts.ttfb.as_secs()
            );
            return Err(CabError::Proxy(format!(
                "Upstream request timed out waiting for response headers (TTFB > {}s)",
                timeouts.ttfb.as_secs()
            )));
        }
    };

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
        // Stream the response body through as SSE with an idle timeout between chunks
        let content_type = upstream_resp
            .headers()
            .get("content-type")
            .cloned()
            .unwrap_or_else(|| HeaderValue::from_static("text/event-stream"));

        let byte_stream = upstream_resp.bytes_stream().map_err(std::io::Error::other);
        let idle_stream = IdleTimeoutStream::new(byte_stream, timeouts.stream_idle);
        let body = Body::from_stream(idle_stream);

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
    proxy_get_with_timeout(client, upstream_url, api_key, UPSTREAM_TTFB_TIMEOUT).await
}

/// Simple proxy for passing through a GET request with explicit TTFB timeout.
pub async fn proxy_get_with_timeout(
    client: &Client,
    upstream_url: &str,
    api_key: &str,
    ttfb_timeout: Duration,
) -> Result<impl IntoResponse, CabError> {
    let mut req = client.get(upstream_url);

    if !api_key.is_empty() {
        req = req.header("authorization", format!("Bearer {api_key}"));
    }

    let resp = match tokio::time::timeout(ttfb_timeout, req.send()).await {
        Ok(Ok(resp)) => resp,
        Ok(Err(e)) => {
            return Err(CabError::Proxy(format!(
                "Failed to connect to upstream: {e}"
            )));
        }
        Err(_) => {
            return Err(CabError::Proxy(format!(
                "Upstream request timed out waiting for response headers (TTFB > {}s)",
                ttfb_timeout.as_secs()
            )));
        }
    };

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
            "openai-compatible",
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
    async fn proxy_request_uses_bearer_for_all_protocols() {
        let server = spawn_router(Router::new().route("/post", post(echo_post))).await;
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            HeaderValue::from_static("application/custom+json"),
        );
        headers.insert("anthropic-version", HeaderValue::from_static("2024-01-01"));

        // anthropic protocol — Bearer + x-api-key auth, anthropic-version forwarded
        let response = proxy_request(
            &Client::new(),
            &format!("{}/post", server.base_url),
            "anthropic-key",
            "anthropic-messages",
            Bytes::from_static(br#"{"message":"hi"}"#),
            &headers,
            false,
        )
        .await
        .unwrap();
        let json = json_from_response(response).await;

        assert_eq!(json["authorization"], "Bearer anthropic-key");
        assert_eq!(json["x_api_key"], "anthropic-key");
        assert_eq!(json["anthropic_version"], "2024-01-01");
        assert_eq!(json["content_type"], "application/custom+json");
    }

    #[tokio::test]
    async fn proxy_request_passes_through_auth_when_no_key() {
        let server = spawn_router(Router::new().route("/post", post(echo_post))).await;
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer existing"));
        headers.insert("x-goog-api-key", HeaderValue::from_static("goog-existing"));

        let response = proxy_request(
            &Client::new(),
            &format!("{}/post", server.base_url),
            "",
            "anthropic-messages",
            Bytes::from_static(br#"{"message":"hi"}"#),
            &headers,
            false,
        )
        .await
        .unwrap();
        let json = json_from_response(response).await;

        assert_eq!(json["authorization"], "Bearer existing");
        assert_eq!(json["x_goog_api_key"], "goog-existing");

        // Without an API key, no default anthropic-version is set
        let response = proxy_request(
            &Client::new(),
            &format!("{}/post", server.base_url),
            "key",
            "openai-compatible",
            Bytes::from_static(br#"{"message":"hi"}"#),
            &HeaderMap::new(),
            false,
        )
        .await
        .unwrap();
        let json = json_from_response(response).await;
        assert_eq!(json["authorization"], "Bearer key");
        assert_eq!(json["anthropic_version"], serde_json::Value::Null);
    }

    #[tokio::test]
    async fn proxy_request_returns_provider_error_with_retry_after() {
        let server = spawn_router(Router::new().route("/error", post(error_post))).await;

        let err = proxy_request(
            &Client::new(),
            &format!("{}/error", server.base_url),
            "secret",
            "openai-compatible",
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
            "openai-compatible",
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
            "openai-compatible",
            Bytes::from_static(b"{}"),
            &HeaderMap::new(),
            false,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, CabError::Proxy(message) if message.contains("Failed to connect")));
    }

    #[tokio::test]
    async fn proxy_request_ttfb_timeout_returns_proxy_error() {
        async fn slow_post() -> impl IntoResponse {
            tokio::time::sleep(Duration::from_millis(100)).await;
            Json(serde_json::json!({"ok": true}))
        }
        let server = spawn_router(Router::new().route("/slow", post(slow_post))).await;

        let err = proxy_request_with_timeouts(
            &Client::new(),
            &format!("{}/slow", server.base_url),
            "secret",
            "openai-compatible",
            Bytes::from_static(b"{}"),
            &HeaderMap::new(),
            false,
            ProxyTimeouts {
                ttfb: Duration::from_millis(20),
                stream_idle: Duration::from_secs(60),
            },
        )
        .await
        .unwrap_err();

        assert!(matches!(
            err,
            CabError::Proxy(message) if message.contains("timed out waiting for response headers")
        ));
    }

    #[tokio::test]
    async fn proxy_request_stream_idle_timeout_terminates_stream() {
        async fn stalled_stream() -> impl IntoResponse {
            let s = futures::stream::unfold(0, |state| async move {
                if state == 0 {
                    Some((
                        Ok::<_, std::io::Error>(Bytes::from_static(b"data: chunk1\n\n")),
                        1,
                    ))
                } else {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    Some((
                        Ok::<_, std::io::Error>(Bytes::from_static(b"data: chunk2\n\n")),
                        2,
                    ))
                }
            });
            (
                [("content-type", "text/event-stream")],
                Body::from_stream(s),
            )
        }
        let server = spawn_router(Router::new().route("/stalled", post(stalled_stream))).await;

        let response = proxy_request_with_timeouts(
            &Client::new(),
            &format!("{}/stalled", server.base_url),
            "secret",
            "openai-compatible",
            Bytes::from_static(b"{}"),
            &HeaderMap::new(),
            true,
            ProxyTimeouts {
                ttfb: Duration::from_secs(5),
                stream_idle: Duration::from_millis(50),
            },
        )
        .await
        .unwrap();

        let result = to_bytes(response.into_body(), 10 * 1024 * 1024).await;
        assert!(result.is_err());
    }
}
