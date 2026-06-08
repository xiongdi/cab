use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use cab_core::CabError;
use cab_core::types::{CreateRoute, UpdateRoute};

use crate::ApiState;

pub async fn list_routes(State(state): State<ApiState>) -> Result<impl IntoResponse, CabError> {
    let routes = cab_db::route::list(&state.pool)
        .await
        .map_err(CabError::Database)?;
    Ok(Json(routes))
}

pub async fn get_route(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, CabError> {
    let route = cab_db::route::get_by_id(&state.pool, &id)
        .await
        .map_err(CabError::Database)?
        .ok_or_else(|| CabError::NotFound(format!("Route {id} not found")))?;
    Ok(Json(route))
}

pub async fn create_route(
    State(state): State<ApiState>,
    Json(input): Json<CreateRoute>,
) -> Result<impl IntoResponse, CabError> {
    let route = cab_db::route::create(&state.pool, &input)
        .await
        .map_err(CabError::Database)?;
    Ok((StatusCode::CREATED, Json(route)))
}

pub async fn update_route(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(input): Json<UpdateRoute>,
) -> Result<impl IntoResponse, CabError> {
    let route = cab_db::route::update(&state.pool, &id, &input)
        .await
        .map_err(CabError::Database)?
        .ok_or_else(|| CabError::NotFound(format!("Route {id} not found")))?;
    Ok(Json(route))
}

pub async fn delete_route(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, CabError> {
    let deleted = cab_db::route::delete(&state.pool, &id)
        .await
        .map_err(CabError::Database)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(CabError::NotFound(format!("Route {id} not found")))
    }
}
