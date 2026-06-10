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
    if let Err(err) = cab_db::auth::verify(
        &state.pool,
        request
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok()),
    )
    .await
    {
        return err.into_response();
    }
    next.run(request).await
}
