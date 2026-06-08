use axum::Json;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use cab_core::CabError;
use cab_core::types::LogQuery;

use crate::ApiState;

pub async fn query_logs(
    State(state): State<ApiState>,
    Query(query): Query<LogQuery>,
) -> Result<impl IntoResponse, CabError> {
    let logs = cab_db::log::query(&state.pool, &query)
        .await
        .map_err(CabError::Database)?;
    Ok(Json(logs))
}
