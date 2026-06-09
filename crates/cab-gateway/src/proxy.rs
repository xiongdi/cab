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

        let byte_stream = upstream_resp
            .bytes_stream()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));

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

    Ok(Response::builder()
        .status(status.as_u16())
        .header("content-type", "application/json")
        .body(Body::from(body_bytes))
        .map_err(|e| CabError::Proxy(format!("Failed to build response: {e}")))?)
}
