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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    struct TestHome {
        _dir: tempfile::TempDir,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl TestHome {
        fn new() -> Self {
            let lock = crate::TEST_HOME_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let dir = tempfile::tempdir().unwrap();
            unsafe {
                std::env::set_var("HOME", dir.path());
                std::env::remove_var("USERPROFILE");
            }
            Self {
                _dir: dir,
                _lock: lock,
            }
        }
    }

    async fn json_body(response: impl IntoResponse) -> serde_json::Value {
        let response = response.into_response();
        let bytes = to_bytes(response.into_body(), 10 * 1024 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn settings_handlers_get_update_and_catalog_status() {
        let _home = TestHome::new();
        let state = ApiState {
            pool: cab_db::InMemoryStore::new(),
        };

        let current = get_settings(State(state.clone())).await.unwrap();
        assert_eq!(json_body(current).await["gateway_port"], 3125);

        let mut settings = cab_db::settings::default_settings();
        settings.gateway_port = 4567;
        settings.gateway_key = "updated-key".into();
        let updated = update_settings(State(state.clone()), Json(settings))
            .await
            .unwrap();
        let json = json_body(updated).await;
        assert_eq!(json["gateway_port"], 4567);
        assert_eq!(
            cab_db::settings::get(&state.pool)
                .await
                .unwrap()
                .gateway_key,
            "updated-key"
        );

        let status = get_catalog_status(State(state)).await.unwrap();
        let json = json_body(status).await;
        assert_eq!(json["sources"].as_array().unwrap().len(), 3);
    }
}
