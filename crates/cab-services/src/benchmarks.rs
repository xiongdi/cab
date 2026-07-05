use cab_core::CabError;
use cab_core::benchmark_catalog::{
    BenchmarkCatalogFile, BenchmarkEvaluations, BenchmarkModelRecord, BenchmarkPerformance,
    artificial_analysis_models_path, artificial_analysis_models_url, ensure_aa_model_map_file,
    load_artificial_analysis_catalog, models_dev_catalog_path, models_dev_catalog_url,
    refresh_aa_model_map_exact_matches, resolve_artificial_analysis_api_key,
};
use cab_db::InMemoryStore;
use chrono::Utc;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ArtificialAnalysisResponse {
    data: Vec<ArtificialAnalysisModel>,
}

#[derive(Debug, Deserialize)]
struct ArtificialAnalysisModel {
    id: String,
    name: String,
    slug: String,
    model_creator: ArtificialAnalysisCreator,
    evaluations: BenchmarkEvaluations,
    #[serde(default)]
    median_output_tokens_per_second: Option<f64>,
    #[serde(default)]
    median_time_to_first_token_seconds: Option<f64>,
    #[serde(default)]
    median_time_to_first_answer_token: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ArtificialAnalysisCreator {
    slug: String,
    name: String,
}

pub async fn sync_catalogs(
    pool: &InMemoryStore,
    client: &reqwest::Client,
) -> Result<serde_json::Value, CabError> {
    let catalog = sync_models_dev_catalog(client).await?;
    sync_artificial_analysis_catalog(pool, client).await?;
    Ok(catalog)
}

pub async fn sync_models_dev_catalog(
    client: &reqwest::Client,
) -> Result<serde_json::Value, CabError> {
    sync_models_dev_json(client, models_dev_catalog_url()).await
}

async fn sync_models_dev_json(
    client: &reqwest::Client,
    url: &str,
) -> Result<serde_json::Value, CabError> {
    let resp = client.get(url).send().await.map_err(|e| {
        CabError::Proxy(format!(
            "Failed to download models.dev data from {url}: {e}"
        ))
    })?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(CabError::ProviderError {
            status,
            body,
            retry_after: None,
        });
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| {
        CabError::Proxy(format!(
            "Failed to parse models.dev response from {url}: {e}"
        ))
    })?;

    // Persist to disk so catalog-status `synced_at` (derived from file mtime)
    // reflects the actual last-sync time and so the file serves as an offline
    // cache for subsequent startups when the network is unavailable.
    let cache_path = models_dev_catalog_path();
    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&cache_path, serde_json::to_vec(&json).unwrap_or_default()) {
        tracing::warn!(
            "Failed to write models.dev catalog cache to {}: {e}",
            cache_path.display()
        );
    } else {
        tracing::info!(
            "Cached models.dev catalog to {}",
            cache_path.display()
        );
    }

    tracing::info!("Downloaded models.dev data from {url}",);
    Ok(json)
}

pub async fn sync_artificial_analysis_catalog(
    pool: &InMemoryStore,
    client: &reqwest::Client,
) -> Result<(), CabError> {
    let settings = cab_db::settings::get(pool)
        .await
        .map_err(CabError::Database)?;
    let api_key =
        resolve_artificial_analysis_api_key(settings.artificial_analysis_api_key.as_deref());

    let Some(api_key) = api_key else {
        if load_artificial_analysis_catalog().is_some() {
            tracing::info!(
                "Using cached Artificial Analysis benchmarks from {}",
                artificial_analysis_models_path().display()
            );
        } else {
            tracing::warn!(
                "Artificial Analysis API key missing; set settings.artificial_analysis_api_key or ARTIFICIAL_ANALYSIS_API_KEY to populate ~/.cab/catalog/artificial-analysis/models.json"
            );
        }
        return Ok(());
    };

    let url = artificial_analysis_models_url();
    let resp = client
        .get(url)
        .header("x-api-key", api_key)
        .send()
        .await
        .map_err(|e| {
            CabError::Proxy(format!(
                "Failed to download Artificial Analysis benchmarks from {url}: {e}"
            ))
        })?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        return Err(CabError::ProviderError {
            status,
            body,
            retry_after: None,
        });
    }

    let response: ArtificialAnalysisResponse = resp.json().await.map_err(|e| {
        CabError::Proxy(format!(
            "Failed to parse Artificial Analysis benchmarks response: {e}"
        ))
    })?;

    let models = response
        .data
        .into_iter()
        .map(|model| BenchmarkModelRecord {
            id: model.id,
            slug: model.slug,
            name: model.name,
            creator_slug: Some(model.model_creator.slug),
            creator_name: Some(model.model_creator.name),
            evaluations: model.evaluations,
            performance: BenchmarkPerformance {
                median_output_tokens_per_second: model.median_output_tokens_per_second,
                median_time_to_first_token_seconds: model.median_time_to_first_token_seconds,
                median_time_to_first_answer_token: model.median_time_to_first_answer_token,
            },
        })
        .collect::<Vec<_>>();

    let file = BenchmarkCatalogFile {
        source: "artificialanalysis.ai".to_string(),
        synced_at: Utc::now().to_rfc3339(),
        models,
    };

    tracing::info!(
        "Synced {} Artificial Analysis benchmark records",
        file.models.len(),
    );

    // Persist to disk so catalog-status synced_at reflects actual sync time
    // and so the data survives restarts without re-downloading.
    let cache_path = artificial_analysis_models_path();
    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(
        &cache_path,
        serde_json::to_vec(&file).unwrap_or_default(),
    ) {
        tracing::warn!(
            "Failed to write Artificial Analysis cache to {}: {e}",
            cache_path.display()
        );
    } else {
        tracing::info!(
            "Cached Artificial Analysis benchmarks to {}",
            cache_path.display()
        );
    }

    if let Err(e) = ensure_aa_model_map_file() {
        tracing::warn!("Failed to seed AA model map: {e}");
    }
    if let Err(e) = refresh_aa_model_map_exact_matches() {
        tracing::warn!("Failed to refresh AA model map: {e}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::http::StatusCode;
    use axum::routing::get;
    use tokio::net::TcpListener;
    use tokio::sync::oneshot;

    struct TestServer {
        base_url: String,
        shutdown: Option<oneshot::Sender<()>>,
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            if let Some(shutdown) = self.shutdown.take() {
                let _ = shutdown.send(());
            }
        }
    }

    async fn spawn_router(app: Router) -> TestServer {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            axum::serve(listener, app.into_make_service())
                .with_graceful_shutdown(async {
                    let _ = rx.await;
                })
                .await
                .unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        TestServer {
            base_url: format!("http://{addr}"),
            shutdown: Some(tx),
        }
    }

    async fn models_dev_json() -> impl axum::response::IntoResponse {
        axum::Json(serde_json::json!({
            "models": {
                "provider/model": {
                    "id": "provider/model",
                    "name": "Provider Model"
                }
            }
        }))
    }

    async fn upstream_error() -> impl axum::response::IntoResponse {
        (StatusCode::BAD_GATEWAY, "upstream unavailable")
    }

    async fn invalid_json() -> impl axum::response::IntoResponse {
        "not-json"
    }

    #[tokio::test]
    async fn sync_models_dev_json_writes_successful_response() {
        let server = spawn_router(Router::new().route("/models.json", get(models_dev_json))).await;

        sync_models_dev_json(
            &reqwest::Client::new(),
            &format!("{}/models.json", server.base_url),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn sync_models_dev_json_reports_http_and_parse_errors() {
        let error_server =
            spawn_router(Router::new().route("/models.json", get(upstream_error))).await;
        let err = sync_models_dev_json(
            &reqwest::Client::new(),
            &format!("{}/models.json", error_server.base_url),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CabError::ProviderError { status: 502, .. }));

        let invalid_server =
            spawn_router(Router::new().route("/models.json", get(invalid_json))).await;
        let err = sync_models_dev_json(
            &reqwest::Client::new(),
            &format!("{}/models.json", invalid_server.base_url),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CabError::Proxy(message) if message.contains("Failed to parse")));
    }

    #[tokio::test]
    async fn sync_artificial_analysis_catalog_is_noop_without_key() {
        unsafe {
            std::env::remove_var("ARTIFICIAL_ANALYSIS_API_KEY");
        }
        let pool = InMemoryStore::new();
        let result = sync_artificial_analysis_catalog(&pool, &reqwest::Client::new()).await;
        assert!(result.is_ok());
    }
}
