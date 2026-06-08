use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use cab_core::CabError;
use cab_core::benchmark_catalog::{
    BenchmarkModelRecord, load_aa_model_map, load_artificial_analysis_catalog,
    load_models_dev_catalog_file,
};
use cab_core::types::{CreateModel, ModelUserSettings, UpdateModel};
use serde::{Deserialize, Serialize};

use crate::ApiState;

#[derive(Debug, Clone, Serialize)]
pub struct ModelCatalogEntry {
    pub id: String,
    pub catalog_id: String,
    pub enabled: bool,
    pub models_dev: serde_json::Value,
    pub artificial_analysis: Option<BenchmarkModelRecord>,
    pub settings: ModelUserSettings,
}

pub async fn list_model_catalog(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, CabError> {
    let db_models = cab_db::model::list(&state.pool)
        .await
        .map_err(CabError::Database)?;
    let settings = cab_db::settings::get(&state.pool)
        .await
        .map_err(CabError::Database)?;
    let catalog = load_models_dev_catalog_file().map_err(CabError::Database)?;
    let models_dev_map: std::collections::HashMap<String, serde_json::Value> =
        serde_json::from_value(catalog.models)
            .map_err(|e| CabError::Database(format!("Failed to parse models.dev models: {e}")))?;
    let aa_catalog = load_artificial_analysis_catalog();
    let aa_map = load_aa_model_map();

    let mut entries: Vec<ModelCatalogEntry> = db_models
        .into_iter()
        .map(|model| {
            let models_dev = models_dev_map
                .get(&model.name)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let settings_entry = settings
                .models
                .get(&model.name)
                .cloned()
                .unwrap_or_default();
            let enabled = settings_entry.enabled.unwrap_or(model.enabled);
            let artificial_analysis = aa_catalog.as_ref().and_then(|catalog| {
                catalog.lookup_record(
                    &model.name,
                    model.canonical_slug.as_deref(),
                    Some(&model.display_name),
                    model.context_length,
                    &aa_map,
                )
            });
            ModelCatalogEntry {
                id: model.id,
                catalog_id: model.name,
                enabled,
                models_dev,
                artificial_analysis,
                settings: settings_entry,
            }
        })
        .collect();

    entries.sort_by(|a, b| a.catalog_id.cmp(&b.catalog_id));
    Ok(Json(entries))
}

pub async fn list_models(State(state): State<ApiState>) -> Result<impl IntoResponse, CabError> {
    let models = cab_db::model::list(&state.pool)
        .await
        .map_err(CabError::Database)?;
    Ok(Json(models))
}

pub async fn get_model(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, CabError> {
    let model = cab_db::model::get_by_id(&state.pool, &id)
        .await
        .map_err(CabError::Database)?
        .ok_or_else(|| CabError::NotFound(format!("Model {id} not found")))?;
    Ok(Json(model))
}

pub async fn create_model(
    State(state): State<ApiState>,
    Json(_input): Json<CreateModel>,
) -> Result<StatusCode, CabError> {
    let _ = state;
    Err(CabError::InvalidRequest(
        "Models are synchronized from models.dev and cannot be created manually.".to_string(),
    ))
}

pub async fn update_model(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(input): Json<UpdateModel>,
) -> Result<impl IntoResponse, CabError> {
    if input.name.is_some()
        || input.display_name.is_some()
        || input.provider_id.is_some()
        || input.protocol.is_some()
        || input.context_length.is_some()
        || input.input_cost.is_some()
        || input.output_cost.is_some()
        || input.overall_intelligence.is_some()
        || input.coding_index.is_some()
        || input.agentic_index.is_some()
    {
        return Err(CabError::InvalidRequest(
            "Only model enabled status can be changed manually.".to_string(),
        ));
    }

    if input.enabled.is_none() {
        return Err(CabError::InvalidRequest(
            "Model enabled status is required.".to_string(),
        ));
    }

    let sanitized = UpdateModel {
        enabled: input.enabled,
        ..Default::default()
    };

    let model = cab_db::model::update(&state.pool, &id, &sanitized)
        .await
        .map_err(CabError::Database)?
        .ok_or_else(|| CabError::NotFound(format!("Model {id} not found")))?;

    cab_db::settings::set_model_override(
        &state.pool,
        &model.name,
        ModelUserSettings {
            enabled: Some(model.enabled),
        },
    )
    .await
    .map_err(CabError::Database)?;

    Ok(Json(model))
}

pub async fn delete_model(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<StatusCode, CabError> {
    let _ = state;
    let _ = id;
    Err(CabError::InvalidRequest(
        "Models are synchronized from models.dev and cannot be deleted manually.".to_string(),
    ))
}

/// GET /api/models/:id/endpoints
/// Returns all per-provider endpoint data for a given model.
/// :id can be the model's UUID (from our DB) or its name (e.g. deepseek/deepseek-v4-flash)
pub async fn list_model_endpoints(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, CabError> {
    // Resolve model name: first try direct DB lookup by UUID, then by name
    let model_name = if let Ok(Some(m)) = cab_db::model::get_by_id(&state.pool, &id).await {
        m.name
    } else {
        // Assume id is already the model name (e.g. "deepseek/deepseek-v4-flash")
        // URL-decode in case it came as percent-encoded
        id.replace("%2F", "/")
    };

    let endpoints = cab_db::endpoint::list_for_model(&state.pool, &model_name)
        .await
        .map_err(CabError::Database)?;

    Ok(Json(endpoints))
}

#[derive(Debug, Deserialize)]
pub struct EndpointUpdateInput {
    id: String,
    enabled: bool,
}

/// PUT /api/model-endpoints
/// Enable or disable a single downstream endpoint.
pub async fn update_model_endpoint(
    State(state): State<ApiState>,
    Json(input): Json<EndpointUpdateInput>,
) -> Result<impl IntoResponse, CabError> {
    let endpoint = cab_db::endpoint::set_enabled(&state.pool, &input.id, input.enabled)
        .await
        .map_err(CabError::Database)?
        .ok_or_else(|| CabError::NotFound(format!("Endpoint {} not found", input.id)))?;
    Ok(Json(endpoint))
}
