//! Bearer token authentication middleware for Gateway routes.

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

pub async fn gateway_middleware(
    axum::extract::State(state): axum::extract::State<Arc<crate::state::GatewayState>>,
    request: Request,
    next: Next,
) -> Response {
    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    if let Err(err) = cab_db::auth::verify(&state.pool, auth_header).await {
        tracing::warn!(
            "Gateway auth rejected request to {}: {:?} (auth header: {:?})",
            request.uri(),
            err,
            auth_header.map(|h| {
                if h.len() > 20 {
                    format!("{}...", &h[..20])
                } else {
                    h.to_string()
                }
            }),
        );
        return err.into_response();
    }
    next.run(request).await
}


