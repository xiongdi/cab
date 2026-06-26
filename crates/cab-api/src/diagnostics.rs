use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;
use cab_core::CabError;

use crate::ApiState;

/// Latest per-agent tool-schema token weights observed by the gateway. Powers
/// the dashboard's cache-prefix weight diagnostics. Empty until the gateway has
/// seen at least one tool-bearing request (and only while request shaping is on).
pub async fn tool_weights(State(state): State<ApiState>) -> Result<impl IntoResponse, CabError> {
    Ok(Json(state.pool.tool_weights.snapshot()))
}
