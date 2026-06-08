use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;
use cab_core::CabError;

use crate::ApiState;

pub async fn get_stats(State(state): State<ApiState>) -> Result<impl IntoResponse, CabError> {
    let stats = cab_db::dashboard::get_stats(&state.pool)
        .await
        .map_err(CabError::Database)?;
    Ok(Json(stats))
}
