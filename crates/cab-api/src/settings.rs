use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;
use cab_core::CabError;
use cab_core::types::Settings;
use cab_core::{
    CatalogSourceStatus, aa_model_map_status, artificial_analysis_catalog_status,
    models_dev_catalog_status,
};
use serde::Serialize;

use crate::ApiState;

#[derive(Debug, Serialize)]
pub struct CatalogStatusResponse {
    pub sources: Vec<CatalogSourceStatus>,
}

#[derive(Debug, Serialize)]
pub struct SyncCatalogResponse {
    pub success: bool,
    pub applied_models: usize,
    pub providers: usize,
    pub sources: Vec<CatalogSourceStatus>,
}

pub async fn get_settings(State(state): State<ApiState>) -> Result<impl IntoResponse, CabError> {
    let settings = cab_db::settings::get(&state.pool)
        .await
        .map_err(CabError::Database)?;
    Ok(Json(settings))
}

pub async fn get_catalog_status(
    State(_state): State<ApiState>,
) -> Result<impl IntoResponse, CabError> {
    Ok(Json(CatalogStatusResponse {
        sources: vec![
            models_dev_catalog_status(),
            artificial_analysis_catalog_status(),
            aa_model_map_status(),
        ],
    }))
}

pub async fn sync_catalog(State(state): State<ApiState>) -> Result<impl IntoResponse, CabError> {
    let applied_models = crate::providers::sync_models_dev_catalog(&state.pool).await?;
    let providers = cab_db::provider::list_catalog(&state.pool)
        .await
        .map_err(CabError::Database)?
        .len();

    Ok(Json(SyncCatalogResponse {
        success: true,
        applied_models,
        providers,
        sources: vec![
            models_dev_catalog_status(),
            artificial_analysis_catalog_status(),
        ],
    }))
}

pub async fn update_settings(
    State(state): State<ApiState>,
    Json(input): Json<Settings>,
) -> Result<impl IntoResponse, CabError> {
    let settings = cab_db::settings::update(&state.pool, &input)
        .await
        .map_err(CabError::Database)?;

    // Sync all configured agents to use the new gateway key
    if let Err(e) = crate::agents::sync_all_agent_configs(&state.pool).await {
        tracing::error!(
            "Failed to sync agent configs after settings update: {:?}",
            e
        );
    }

    Ok(Json(settings))
}
