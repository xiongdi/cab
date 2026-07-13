use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use cab_core::CabError;
use cab_core::benchmark_catalog::{
    BenchmarkModelRecord, load_aa_model_map, load_models_dev_catalog_file,
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
    let aa_catalog = state
        .pool
        .sqlite()
        .and_then(|p| p.get().ok())
        .and_then(|conn| cab_db::sqlite::load_aa_benchmark_catalog(&conn));
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

pub async fn list_routable_models(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, CabError> {
    let models = cab_db::routability::list_routable_model_entries(&state.pool)
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use cab_core::types::{ApiKeyConfig, Model, Provider, ProviderEndpoint};

    fn provider() -> Provider {
        Provider {
            id: "provider-1".into(),
            name: "Provider One".into(),
            endpoints: vec![ProviderEndpoint {
                id: "chat".into(),
                protocol: "openai-chat".into(),
                url: "https://provider.test/v1".into(),
                label: None,
                priority: 50,
                enabled: true,
            }],
            api_key: "key".into(),
            enabled: true,
            created_at: "now".into(),
            updated_at: "now".into(),
            privacy_policy_url: None,
            terms_of_service_url: None,
            status_page_url: None,
            headquarters: None,
            datacenters: None,
            api_keys: vec![ApiKeyConfig {
                key: "key".into(),
                enabled: true,
                quota_reset_at: None,
            }],
            api: None,
            doc: None,
            env: None,
            npm: None,
            model_count: 0,
            logo: None,
            catalog_models: vec![],
        }
    }

    fn model(id: &str, name: &str, enabled: bool) -> Model {
        Model {
            id: id.into(),
            name: name.into(),
            display_name: format!("Display {name}"),
            provider_id: "provider-1".into(),
            protocol: "openai-chat".into(),
            context_length: 128000,
            input_cost: Some(1.0),
            output_cost: Some(2.0),
            enabled,
            overall_intelligence: Some(50.0),
            coding_index: Some(50.0),
            agentic_index: Some(50.0),
            math_index: Some(50.0),
            output_speed_tps: None,
            time_to_first_token_secs: None,
            created_at: "now".into(),
            updated_at: "now".into(),
            canonical_slug: None,
            hugging_face_id: None,
            created: None,
            description: None,
            architecture: None,
            pricing: None,
            top_provider: None,
            per_request_limits: None,
            supported_parameters: None,
            default_parameters: None,
            supported_voices: None,
            knowledge_cutoff: None,
            expiration_date: None,
            links: None,
        }
    }

    fn endpoint(id: &str, model_name: &str, enabled: bool) -> cab_db::endpoint::ModelEndpoint {
        cab_db::endpoint::ModelEndpoint {
            id: id.into(),
            model_id: model_name.into(),
            canonical_slug: model_name.into(),
            provider_name: "provider".into(),
            provider_tag: format!("provider/{id}"),
            native_model_id: model_name.into(),
            upstream_protocol: None,
            quantization: "unknown".into(),
            input_cost: Some(0.0),
            output_cost: Some(0.0),
            cache_read_cost: None,
            context_length: Some(128000),
            max_completion_tokens: None,
            status: 1,
            uptime_30m: None,
            uptime_5m: None,
            uptime_1d: None,
            supports_tools: true,
            supports_streaming: true,
            enabled,
            updated_at: "now".into(),
        }
    }

    fn state() -> ApiState {
        let pool = cab_db::InMemoryStore::new();
        {
            let mut data = pool.inner.write().unwrap();
            data.providers.insert("provider-1".into(), provider());
            data.models
                .insert("model-1".into(), model("model-1", "provider/model-1", true));
            data.models.insert(
                "model-2".into(),
                model("model-2", "provider/model-2", false),
            );
            data.model_endpoints.insert(
                "endpoint-1".into(),
                endpoint("endpoint-1", "provider/model-1", true),
            );
            data.model_endpoints.insert(
                "endpoint-2".into(),
                endpoint("endpoint-2", "provider/model-2", false),
            );
        }
        ApiState { pool }
    }

    async fn json_body(response: impl IntoResponse) -> serde_json::Value {
        let response = response.into_response();
        let bytes = to_bytes(response.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn expect_err<T>(result: Result<T, CabError>) -> CabError {
        match result {
            Ok(_) => panic!("expected handler error"),
            Err(err) => err,
        }
    }

    #[tokio::test]
    async fn model_handlers_cover_list_get_update_delete_and_endpoint_paths() {
        let _home = crate::TestHome::new().await;
        let state = state();

        let list = list_models(State(state.clone())).await.unwrap();
        let json = json_body(list).await;
        assert_eq!(json.as_array().unwrap().len(), 2);

        let got = get_model(State(state.clone()), Path("model-1".into()))
            .await
            .unwrap();
        assert_eq!(json_body(got).await["name"], "provider/model-1");
        let missing = expect_err(get_model(State(state.clone()), Path("missing".into())).await);
        assert!(matches!(missing, CabError::NotFound(_)));

        let create_err = create_model(
            State(state.clone()),
            Json(CreateModel {
                name: "manual".into(),
                ..Default::default()
            }),
        )
        .await
        .unwrap_err();
        assert!(matches!(create_err, CabError::InvalidRequest(_)));

        let bad_update = expect_err(
            update_model(
                State(state.clone()),
                Path("model-1".into()),
                Json(UpdateModel {
                    display_name: Some("Nope".into()),
                    enabled: Some(false),
                    ..Default::default()
                }),
            )
            .await,
        );
        assert!(matches!(bad_update, CabError::InvalidRequest(_)));

        let missing_enabled = expect_err(
            update_model(
                State(state.clone()),
                Path("model-1".into()),
                Json(UpdateModel::default()),
            )
            .await,
        );
        assert!(matches!(missing_enabled, CabError::InvalidRequest(_)));

        let updated = update_model(
            State(state.clone()),
            Path("model-1".into()),
            Json(UpdateModel {
                enabled: Some(false),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        assert_eq!(json_body(updated).await["enabled"], false);
        assert_eq!(
            cab_db::settings::get(&state.pool).await.unwrap().models["provider/model-1"].enabled,
            Some(false)
        );

        let update_missing = expect_err(
            update_model(
                State(state.clone()),
                Path("missing".into()),
                Json(UpdateModel {
                    enabled: Some(true),
                    ..Default::default()
                }),
            )
            .await,
        );
        assert!(matches!(update_missing, CabError::NotFound(_)));

        let delete_err = delete_model(State(state.clone()), Path("model-1".into()))
            .await
            .unwrap_err();
        assert!(matches!(delete_err, CabError::InvalidRequest(_)));

        let endpoints = list_model_endpoints(State(state.clone()), Path("model-1".into()))
            .await
            .unwrap();
        assert_eq!(json_body(endpoints).await.as_array().unwrap().len(), 1);
        let endpoints_by_name =
            list_model_endpoints(State(state.clone()), Path("provider%2Fmodel-2".into()))
                .await
                .unwrap();
        assert_eq!(
            json_body(endpoints_by_name).await[0]["model_id"],
            "provider/model-2"
        );

        let endpoint_update = update_model_endpoint(
            State(state.clone()),
            Json(EndpointUpdateInput {
                id: "endpoint-1".into(),
                enabled: false,
            }),
        )
        .await
        .unwrap();
        assert_eq!(json_body(endpoint_update).await["enabled"], false);
        let endpoint_missing = expect_err(
            update_model_endpoint(
                State(state),
                Json(EndpointUpdateInput {
                    id: "missing".into(),
                    enabled: true,
                }),
            )
            .await,
        );
        assert!(matches!(endpoint_missing, CabError::NotFound(_)));
    }
}
