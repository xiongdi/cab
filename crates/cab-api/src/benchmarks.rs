use cab_core::CabError;
use cab_core::benchmark_catalog::{
    BenchmarkCatalogFile, BenchmarkEvaluations, BenchmarkModelRecord,
    artificial_analysis_models_path, artificial_analysis_models_url, ensure_aa_model_map_file,
    load_artificial_analysis_catalog, models_dev_catalog_path, models_dev_catalog_url,
    refresh_aa_model_map_exact_matches, resolve_artificial_analysis_api_key, write_catalog_file,
    write_raw_catalog_file,
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
}

#[derive(Debug, Deserialize)]
struct ArtificialAnalysisCreator {
    slug: String,
    name: String,
}

pub async fn sync_catalogs(pool: &InMemoryStore, client: &reqwest::Client) -> Result<(), CabError> {
    sync_models_dev_catalog(client).await?;
    sync_artificial_analysis_catalog(pool, client).await?;
    Ok(())
}

pub async fn sync_models_dev_catalog(client: &reqwest::Client) -> Result<(), CabError> {
    sync_models_dev_json(client, models_dev_catalog_url(), models_dev_catalog_path()).await
}

async fn sync_models_dev_json(
    client: &reqwest::Client,
    url: &str,
    path: std::path::PathBuf,
) -> Result<(), CabError> {
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

    write_raw_catalog_file(&path, &json).map_err(CabError::Database)?;
    tracing::info!("Cached models.dev data from {url} at {}", path.display());
    Ok(())
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
        })
        .collect::<Vec<_>>();

    let file = BenchmarkCatalogFile {
        source: "artificialanalysis.ai".to_string(),
        synced_at: Utc::now().to_rfc3339(),
        models,
    };

    let path = artificial_analysis_models_path();
    write_catalog_file(&path, &file).map_err(CabError::Database)?;
    tracing::info!(
        "Cached {} Artificial Analysis benchmark records at {}",
        file.models.len(),
        path.display()
    );

    if let Err(e) = ensure_aa_model_map_file() {
        tracing::warn!("Failed to seed AA model map: {e}");
    }
    if let Err(e) = refresh_aa_model_map_exact_matches() {
        tracing::warn!("Failed to refresh AA model map: {e}");
    }

    Ok(())
}
