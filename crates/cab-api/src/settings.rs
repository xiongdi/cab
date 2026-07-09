use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;
use cab_core::CabError;
use cab_core::types::UpdateSettings;
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
    Json(input): Json<UpdateSettings>,
) -> Result<impl IntoResponse, CabError> {
    let settings = cab_db::settings::apply_update(&state.pool, &input)
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn json_body(response: impl IntoResponse) -> serde_json::Value {
        let response = response.into_response();
        let bytes = to_bytes(response.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn settings_handlers_get_update_and_catalog_status() {
        let _home = crate::TestHome::new().await;
        let state = ApiState {
            pool: cab_db::InMemoryStore::new(),
        };

        let current = get_settings(State(state.clone())).await.unwrap();
        assert_eq!(json_body(current).await["gateway_port"], 3125);

        cab_db::settings::set_provider_override(
            &state.pool,
            "provider-1",
            cab_core::types::ProviderUserSettings {
                enabled: Some(true),
                api_key: Some("keep-me".into()),
                api_keys: None,
                endpoints: None,
            },
        )
        .await
        .unwrap();

        let updated = update_settings(
            State(state.clone()),
            Json(UpdateSettings {
                gateway_port: Some(4567),
                gateway_key: Some("updated-key".into()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
        let json = json_body(updated).await;
        assert_eq!(json["gateway_port"], 4567);
        let stored = cab_db::settings::get(&state.pool).await.unwrap();
        assert_eq!(stored.gateway_key, "updated-key");
        assert_eq!(
            stored.providers["provider-1"].api_key.as_deref(),
            Some("keep-me")
        );

        let status = get_catalog_status(State(state)).await.unwrap();
        let json = json_body(status).await;
        assert_eq!(json["sources"].as_array().unwrap().len(), 3);
    }
}

pub async fn get_logo_svg(
    _state: State<ApiState>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Result<impl IntoResponse, CabError> {
    use axum::http::header;

    // Logos live at ~/.cab/logos/{provider_id}.svg
    let logos_dir = cab_core::catalog_root_dir()
        .parent()
        .map(|p| p.join("logos"))
        .unwrap_or_else(|| std::path::PathBuf::from("logos"));

    // path is e.g. "anthropic.svg" or "labs/google.svg"
    let logo_path = logos_dir.join(&path);

    // 1. Serve from disk if available
    if logo_path.exists() {
        if let Ok(bytes) = std::fs::read(&logo_path) {
            return Ok(([(header::CONTENT_TYPE, "image/svg+xml")], String::from_utf8_lossy(&bytes).into_owned()));
        }
    }

    // 2. Fallback: proxy from models.dev and cache to disk
    let logo_url = format!("https://models.dev/logos/{path}");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    if let Ok(resp) = client.get(&logo_url).send().await {
        if resp.status().is_success() {
            if let Ok(text) = resp.text().await {
                if text.trim().starts_with("<svg") {
                    // Cache to disk for future requests
                    if let Some(parent) = logo_path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::write(&logo_path, &text);
                    return Ok(([(header::CONTENT_TYPE, "image/svg+xml")], text));
                }
            }
        }
    }

    Err(CabError::NotFound(format!("Logo for {path} not found")))
}
