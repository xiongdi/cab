use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::model_scores::ModelIntelligenceIndices;

const AA_MODELS_URL: &str = "https://artificialanalysis.ai/api/v2/data/llms/models";
const MODELS_DEV_CATALOG_URL: &str = "https://models.dev/catalog.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkCatalogFile {
    pub source: String,
    pub synced_at: String,
    pub models: Vec<BenchmarkModelRecord>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BenchmarkPerformance {
    #[serde(default)]
    pub median_output_tokens_per_second: Option<f64>,
    #[serde(default)]
    pub median_time_to_first_token_seconds: Option<f64>,
    #[serde(default)]
    pub median_time_to_first_answer_token: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkModelRecord {
    pub id: String,
    pub slug: String,
    pub name: String,
    #[serde(default)]
    pub creator_slug: Option<String>,
    #[serde(default)]
    pub creator_name: Option<String>,
    pub evaluations: BenchmarkEvaluations,
    #[serde(default)]
    pub performance: BenchmarkPerformance,
}

#[derive(Debug, Clone, Default)]
pub struct ModelPerformanceMetrics {
    pub output_speed_tps: Option<f64>,
    pub time_to_first_token_secs: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BenchmarkEvaluations {
    #[serde(default)]
    pub artificial_analysis_intelligence_index: Option<f64>,
    #[serde(default)]
    pub artificial_analysis_coding_index: Option<f64>,
    #[serde(default)]
    pub artificial_analysis_math_index: Option<f64>,
    #[serde(default)]
    pub tau2: Option<f64>,
    #[serde(default)]
    pub terminalbench_hard: Option<f64>,
    #[serde(default)]
    pub livecodebench: Option<f64>,
    #[serde(default)]
    pub scicode: Option<f64>,
    #[serde(default)]
    pub gpqa: Option<f64>,
    #[serde(default)]
    pub hle: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct BenchmarkCatalog {
    pub synced_at: Option<String>,
    by_key: HashMap<String, BenchmarkModelRecord>,
    records: Vec<BenchmarkModelRecord>,
}

pub fn catalog_root_dir() -> PathBuf {
    crate::paths::cab_home().join("catalog")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AaModelMapFile {
    #[serde(default = "default_map_version")]
    pub version: u32,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub mappings: HashMap<String, String>,
}

fn default_map_version() -> u32 {
    1
}

impl Default for AaModelMapFile {
    fn default() -> Self {
        Self {
            version: 1,
            description: String::new(),
            mappings: HashMap::new(),
        }
    }
}

const EMBEDDED_AA_MODEL_MAP: &str = include_str!("../../../config/aa-model-map.json");

pub fn aa_model_map_path() -> PathBuf {
    catalog_root_dir().join("aa-model-map.json")
}

pub fn load_aa_model_map() -> AaModelMapFile {
    let path = aa_model_map_path();
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(file) = serde_json::from_str::<AaModelMapFile>(&content) {
            return file;
        }
        tracing::warn!("Failed to parse AA model map at {}", path.display());
    }

    embedded_aa_model_map()
}

pub fn embedded_aa_model_map() -> AaModelMapFile {
    serde_json::from_str(EMBEDDED_AA_MODEL_MAP).unwrap_or_default()
}

pub fn save_aa_model_map(file: &AaModelMapFile) -> Result<(), String> {
    let path = aa_model_map_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}

/// Ensure ~/.cab/catalog/aa-model-map.json exists (seed from bundled defaults if missing).
pub fn ensure_aa_model_map_file() -> Result<AaModelMapFile, String> {
    let path = aa_model_map_path();
    if path.exists() {
        return Ok(load_aa_model_map());
    }
    let file = embedded_aa_model_map();
    save_aa_model_map(&file)?;
    tracing::info!("Seeded AA model map at {}", path.display());
    Ok(file)
}

/// Add exact models.dev -> AA slug pairs without overwriting user edits.
pub fn refresh_aa_model_map_exact_matches() -> Result<AaModelMapFile, String> {
    let mut file = load_aa_model_map();
    let catalog = load_models_dev_catalog_file()?;
    let models_dev: HashMap<String, serde_json::Value> = serde_json::from_value(catalog.models)
        .map_err(|e| format!("Failed to parse models.dev models: {e}"))?;
    let aa_catalog = load_artificial_analysis_catalog()
        .ok_or_else(|| "Artificial Analysis catalog not available".to_string())?;

    let mut added = 0usize;
    for (catalog_id, model_value) in models_dev {
        if file.mappings.contains_key(&catalog_id) {
            continue;
        }
        let display_name = model_value
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        if let Some(record) =
            aa_catalog.lookup_record_exact(&catalog_id, Some(&catalog_id), display_name.as_deref())
        {
            file.mappings.insert(catalog_id, record.slug);
            added += 1;
        }
    }

    if added > 0 {
        save_aa_model_map(&file)?;
        tracing::info!("Added {added} exact AA model map entries");
    }
    Ok(file)
}

pub fn aa_model_map_status() -> CatalogSourceStatus {
    let path = aa_model_map_path();
    let file = if path.exists() {
        load_aa_model_map()
    } else {
        embedded_aa_model_map()
    };
    CatalogSourceStatus {
        id: "aa-model-map".to_string(),
        name: "AA Model Map".to_string(),
        url: String::new(),
        cache_path: path.display().to_string(),
        available: !file.mappings.is_empty() || path.exists(),
        synced_at: file_modified_at(&path),
        providers: None,
        models: Some(file.mappings.len()),
    }
}

pub fn artificial_analysis_models_path() -> PathBuf {
    catalog_root_dir()
        .join("artificial-analysis")
        .join("models.json")
}

pub fn models_dev_catalog_path() -> PathBuf {
    catalog_root_dir().join("models.dev").join("catalog.json")
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelsDevCatalogFile {
    pub providers: serde_json::Value,
    pub models: serde_json::Value,
}

pub fn load_models_dev_catalog_file() -> Result<ModelsDevCatalogFile, String> {
    let path = models_dev_catalog_path();
    let content = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "Failed to read models.dev catalog at {}: {e}",
            path.display()
        )
    })?;
    serde_json::from_str(&content).map_err(|e| {
        format!(
            "Failed to parse models.dev catalog at {}: {e}",
            path.display()
        )
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogSourceStatus {
    pub id: String,
    pub name: String,
    pub url: String,
    pub cache_path: String,
    pub available: bool,
    pub synced_at: Option<String>,
    pub providers: Option<usize>,
    pub models: Option<usize>,
}

fn file_modified_at(path: &Path) -> Option<String> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let datetime: chrono::DateTime<chrono::Utc> = modified.into();
    Some(datetime.to_rfc3339())
}

pub fn models_dev_catalog_status() -> CatalogSourceStatus {
    let path = models_dev_catalog_path();
    let url = models_dev_catalog_url().to_string();
    let mut status = CatalogSourceStatus {
        id: "models.dev".to_string(),
        name: "models.dev".to_string(),
        url,
        cache_path: path.display().to_string(),
        available: false,
        synced_at: None,
        providers: None,
        models: None,
    };

    if let Ok(catalog) = load_models_dev_catalog_file() {
        status.available = true;
        if let Some(providers) = catalog.providers.as_object() {
            status.providers = Some(providers.len());
        }
        if let Some(models) = catalog.models.as_object() {
            status.models = Some(models.len());
        }
    }

    status.synced_at = file_modified_at(&path);
    status
}

pub fn artificial_analysis_catalog_status() -> CatalogSourceStatus {
    let path = artificial_analysis_models_path();
    let url = artificial_analysis_models_url().to_string();
    let mut status = CatalogSourceStatus {
        id: "artificial-analysis".to_string(),
        name: "Artificial Analysis".to_string(),
        url,
        cache_path: path.display().to_string(),
        available: false,
        synced_at: None,
        providers: None,
        models: None,
    };

    if let Ok(file) = std::fs::read_to_string(&path).and_then(|c| {
        serde_json::from_str::<BenchmarkCatalogFile>(&c)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }) {
        status.available = true;
        status.synced_at = Some(file.synced_at);
        status.models = Some(file.models.len());
    }

    status
}

pub fn resolve_artificial_analysis_api_key(settings_key: Option<&str>) -> Option<String> {
    settings_key
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(str::to_string)
        .or_else(|| {
            std::env::var("ARTIFICIAL_ANALYSIS_API_KEY")
                .ok()
                .filter(|key| !key.trim().is_empty())
        })
        .or_else(|| {
            std::env::var("AA_API_KEY")
                .ok()
                .filter(|key| !key.trim().is_empty())
        })
}

pub fn load_artificial_analysis_catalog() -> Option<BenchmarkCatalog> {
    let path = artificial_analysis_models_path();
    let content = std::fs::read_to_string(path).ok()?;
    let file: BenchmarkCatalogFile = serde_json::from_str(&content).ok()?;
    Some(BenchmarkCatalog::from_file(file))
}

impl BenchmarkCatalog {
    pub fn from_file(file: BenchmarkCatalogFile) -> Self {
        Self::from_records(file.models, Some(file.synced_at))
    }

    /// Build a BenchmarkCatalog from a list of records (e.g. loaded from SQLite).
    pub fn from_records(records: Vec<BenchmarkModelRecord>, synced_at: Option<String>) -> Self {
        let mut by_key = HashMap::new();
        for model in &records {
            for key in index_keys_for_record(model) {
                by_key.entry(key).or_insert_with(|| model.clone());
            }
        }
        Self {
            synced_at,
            by_key,
            records,
        }
    }

    pub fn lookup(
        &self,
        catalog_model_id: &str,
        canonical_slug: Option<&str>,
        display_name: Option<&str>,
        context_length: i64,
        aa_map: &AaModelMapFile,
    ) -> Option<ModelIntelligenceIndices> {
        self.lookup_record(
            catalog_model_id,
            canonical_slug,
            display_name,
            context_length,
            aa_map,
        )
        .map(|record| indices_from_evaluations(&record.evaluations))
    }

    pub fn lookup_record(
        &self,
        catalog_model_id: &str,
        canonical_slug: Option<&str>,
        display_name: Option<&str>,
        _context_length: i64,
        aa_map: &AaModelMapFile,
    ) -> Option<BenchmarkModelRecord> {
        if let Some(aa_slug) = aa_map.mappings.get(catalog_model_id) {
            if let Some(record) = self.find_by_slug(aa_slug) {
                return Some(record);
            }
            tracing::warn!(
                "AA model map entry {catalog_model_id} -> {aa_slug} not found in AA catalog"
            );
        }

        self.lookup_record_exact(catalog_model_id, canonical_slug, display_name)
    }

    pub fn lookup_record_exact(
        &self,
        catalog_model_id: &str,
        canonical_slug: Option<&str>,
        display_name: Option<&str>,
    ) -> Option<BenchmarkModelRecord> {
        let candidates = lookup_candidates(catalog_model_id, canonical_slug, display_name);
        for key in &candidates {
            if let Some(record) = self.by_key.get(key) {
                return Some(record.clone());
            }
        }
        None
    }

    fn find_by_slug(&self, slug: &str) -> Option<BenchmarkModelRecord> {
        let key = normalize_key(slug);
        if let Some(record) = self.by_key.get(&key) {
            return Some(record.clone());
        }
        self.records
            .iter()
            .find(|record| normalize_key(&record.slug) == key)
            .cloned()
    }

    #[allow(dead_code)]
    fn lookup_variant(
        &self,
        candidates: &[String],
        context_length: i64,
    ) -> Option<&BenchmarkModelRecord> {
        let mut best: Option<(&BenchmarkModelRecord, i64)> = None;

        for record in &self.records {
            let record_keys = index_keys_for_record(record);
            for candidate in candidates {
                for record_key in &record_keys {
                    let Some(suffix) = record_key.strip_prefix(candidate) else {
                        continue;
                    };
                    if !suffix.starts_with('-') {
                        continue;
                    }

                    let score = match (context_length > 0, context_hint_from_key(record_key)) {
                        (true, Some(record_context)) => (context_length - record_context).abs(),
                        _ => i64::MAX / 2,
                    };

                    if best
                        .map(|(_, best_score)| score < best_score)
                        .unwrap_or(true)
                    {
                        best = Some((record, score));
                    }
                }
            }
        }

        best.map(|(record, _)| record)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn performance_from_record(record: &BenchmarkModelRecord) -> ModelPerformanceMetrics {
    ModelPerformanceMetrics {
        output_speed_tps: record
            .performance
            .median_output_tokens_per_second
            .filter(|speed| *speed > 0.0),
        time_to_first_token_secs: record.performance.median_time_to_first_token_seconds,
    }
}

pub fn resolve_performance_metrics(
    catalog: Option<&BenchmarkCatalog>,
    aa_map: &AaModelMapFile,
    catalog_model_id: &str,
    canonical_slug: Option<&str>,
    display_name: Option<&str>,
    context_length: i64,
) -> ModelPerformanceMetrics {
    if let Some(catalog) = catalog
        && let Some(record) = catalog.lookup_record(
            catalog_model_id,
            canonical_slug,
            display_name,
            context_length,
            aa_map,
        )
    {
        return performance_from_record(&record);
    }
    ModelPerformanceMetrics::default()
}

pub fn resolve_intelligence_indices(
    catalog: Option<&BenchmarkCatalog>,
    aa_map: &AaModelMapFile,
    catalog_model_id: &str,
    canonical_slug: Option<&str>,
    display_name: Option<&str>,
    context_length: i64,
) -> ModelIntelligenceIndices {
    if let Some(catalog) = catalog {
        if let Some(indices) = catalog.lookup(
            catalog_model_id,
            canonical_slug,
            display_name,
            context_length,
            aa_map,
        ) {
            return indices;
        }
        return ModelIntelligenceIndices::missing();
    }
    ModelIntelligenceIndices::missing()
}

pub fn indices_from_evaluations(eval: &BenchmarkEvaluations) -> ModelIntelligenceIndices {
    let overall = eval.artificial_analysis_intelligence_index.map(clamp_score);

    let coding = eval
        .artificial_analysis_coding_index
        .map(clamp_score)
        .or_else(|| eval.livecodebench.map(|v| clamp_score(v * 100.0)))
        .or_else(|| eval.scicode.map(|v| clamp_score(v * 100.0)));

    let mut agentic_scores = Vec::new();
    if let Some(v) = eval.tau2 {
        agentic_scores.push(v * 100.0);
    }
    if let Some(v) = eval.terminalbench_hard {
        agentic_scores.push(v * 100.0);
    }
    if let Some(v) = eval.gpqa {
        agentic_scores.push(v * 100.0);
    }
    if let Some(v) = eval.hle {
        agentic_scores.push(v * 100.0);
    }

    let agentic = if agentic_scores.is_empty() {
        None
    } else {
        Some(clamp_score(
            agentic_scores.iter().sum::<f64>() / agentic_scores.len() as f64,
        ))
    };

    let math = eval.artificial_analysis_math_index.map(clamp_score);

    ModelIntelligenceIndices {
        overall_intelligence: overall,
        coding_index: coding,
        agentic_index: agentic,
        math_index: math,
    }
}

fn lookup_candidates(
    catalog_model_id: &str,
    canonical_slug: Option<&str>,
    display_name: Option<&str>,
) -> Vec<String> {
    let mut keys = Vec::new();
    let id_lower = catalog_model_id.to_ascii_lowercase();
    keys.push(normalize_key(&id_lower));

    if let Some((provider, slug)) = id_lower.split_once('/') {
        keys.push(normalize_key(slug));
        keys.push(normalize_key(&format!("{provider}/{slug}")));
        keys.push(normalize_key(&format!("{provider}-{slug}")));
        // Catalogs can vary between dots and dashes (e.g. v3.2 vs v3-2).
        keys.push(normalize_key(&slug.replace('.', "-")));
    }

    if let Some(slug) = canonical_slug {
        keys.push(normalize_key(slug));
    }

    if let Some(name) = display_name {
        keys.push(normalize_key(name));
    }

    keys.sort();
    keys.dedup();
    keys
}

fn index_keys_for_record(model: &BenchmarkModelRecord) -> Vec<String> {
    let mut keys = vec![
        normalize_key(&model.slug),
        normalize_key(&model.name),
        normalize_key(&model.id),
    ];

    if let Some(creator) = &model.creator_slug {
        keys.push(normalize_key(&format!("{creator}/{}", model.slug)));
        keys.push(normalize_key(&format!("{creator}-{}", model.slug)));
    }

    keys.sort();
    keys.dedup();
    keys
}

fn normalize_key(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace(['_', '.', ' '], "-")
}

fn context_hint_from_key(key: &str) -> Option<i64> {
    let suffix = key.rsplit('-').next()?;
    let number = suffix.strip_suffix('k')?.parse::<i64>().ok()?;
    Some(number * 1_000)
}

fn clamp_score(value: f64) -> f64 {
    value.clamp(1.0, 100.0)
}

pub fn write_catalog_file(path: &Path, file: &BenchmarkCatalogFile) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    std::fs::write(path, content).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn write_raw_catalog_file(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(path, content).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn artificial_analysis_models_url() -> &'static str {
    AA_MODELS_URL
}

pub fn models_dev_catalog_url() -> &'static str {
    MODELS_DEV_CATALOG_URL
}

pub fn models_dev_provider_logo_url(provider_id: &str) -> String {
    format!(
        "{}/logos/{}.svg",
        models_dev_origin(),
        urlencoding_encode_slug(provider_id)
    )
}

pub fn models_dev_lab_logo_url(lab_id: &str) -> String {
    format!(
        "{}/logos/labs/{}.svg",
        models_dev_origin(),
        urlencoding_encode_slug(lab_id)
    )
}

fn models_dev_origin() -> &'static str {
    "https://models.dev"
}

fn urlencoding_encode_slug(value: &str) -> String {
    value.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::test_env_lock;

    struct TestHome {
        _dir: tempfile::TempDir,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl TestHome {
        fn new() -> Self {
            let lock = test_env_lock();
            let dir = tempfile::tempdir().unwrap();
            unsafe {
                std::env::set_var("HOME", dir.path());
                std::env::remove_var("USERPROFILE");
                std::env::remove_var("CAB_HOME");
            }
            Self {
                _dir: dir,
                _lock: lock,
            }
        }
    }

    fn sample_record(slug: &str, overall: f64, coding: f64, tau2: f64) -> BenchmarkModelRecord {
        BenchmarkModelRecord {
            id: slug.to_string(),
            slug: slug.to_string(),
            name: slug.to_string(),
            creator_slug: Some("minimax".to_string()),
            creator_name: Some("MiniMax".to_string()),
            evaluations: BenchmarkEvaluations {
                artificial_analysis_intelligence_index: Some(overall),
                artificial_analysis_coding_index: Some(coding),
                tau2: Some(tau2),
                ..Default::default()
            },
            performance: BenchmarkPerformance::default(),
        }
    }

    #[test]
    fn lookup_catalog_model_by_provider_slug() {
        let catalog = BenchmarkCatalog::from_file(BenchmarkCatalogFile {
            source: "test".into(),
            synced_at: "now".into(),
            models: vec![sample_record("minimax-m3", 48.5, 44.0, 0.59)],
        });
        let aa_map = AaModelMapFile::default();

        let indices = catalog
            .lookup("minimax/minimax-m3", None, None, 128_000, &aa_map)
            .expect("match");
        assert_eq!(indices.overall_intelligence, Some(48.5));
        assert_eq!(indices.coding_index, Some(44.0));
        assert!((indices.agentic_index.unwrap() - 59.0).abs() < f64::EPSILON);
    }

    #[test]
    fn lookup_uses_explicit_aa_model_map() {
        let catalog = BenchmarkCatalog::from_file(BenchmarkCatalogFile {
            source: "test".into(),
            synced_at: "now".into(),
            models: vec![sample_record("minimax-m1-80k", 24.4, 14.5, 0.34)],
        });
        let aa_map = AaModelMapFile {
            mappings: HashMap::from([(
                "minimax/minimax-m1".to_string(),
                "minimax-m1-80k".to_string(),
            )]),
            ..Default::default()
        };

        let indices = catalog
            .lookup("minimax/minimax-m1", None, None, 1_000_000, &aa_map)
            .expect("match");
        assert_eq!(indices.overall_intelligence, Some(24.4));
        assert_eq!(indices.coding_index, Some(14.5));
    }

    #[test]
    fn resolve_prefers_catalog_over_heuristic() {
        let catalog = BenchmarkCatalog::from_file(BenchmarkCatalogFile {
            source: "test".into(),
            synced_at: "now".into(),
            models: vec![sample_record("deepseek-chat", 39.0, 31.0, 0.42)],
        });
        let aa_map = AaModelMapFile::default();

        let resolved = resolve_intelligence_indices(
            Some(&catalog),
            &aa_map,
            "deepseek/deepseek-chat",
            None,
            None,
            128_000,
        );
        assert_eq!(resolved.overall_intelligence, Some(39.0));
    }

    #[test]
    fn resolve_with_catalog_does_not_invent_scores() {
        let catalog = BenchmarkCatalog::from_file(BenchmarkCatalogFile {
            source: "test".into(),
            synced_at: "now".into(),
            models: vec![],
        });
        let aa_map = AaModelMapFile::default();

        let resolved = resolve_intelligence_indices(
            Some(&catalog),
            &aa_map,
            "minimax/minimax-01",
            None,
            None,
            1_000_000,
        );
        assert!(resolved.is_missing());
    }

    #[test]
    fn catalog_paths_statuses_and_raw_writes_use_home_cache() {
        let _home = TestHome::new();
        assert!(catalog_root_dir().ends_with("catalog"));
        assert_eq!(models_dev_catalog_url(), MODELS_DEV_CATALOG_URL);
        assert_eq!(artificial_analysis_models_url(), AA_MODELS_URL);
        assert_eq!(
            models_dev_provider_logo_url("Open AI"),
            "https://models.dev/logos/open ai.svg"
        );
        assert_eq!(
            models_dev_lab_logo_url("MiniMax"),
            "https://models.dev/logos/labs/minimax.svg"
        );

        let status = models_dev_catalog_status();
        assert!(!status.available);
        assert_eq!(status.providers, None);
        assert_eq!(status.models, None);

        write_raw_catalog_file(
            &models_dev_catalog_path(),
            &serde_json::json!({
                "providers": {"p1": {"name": "P1"}},
                "models": {"p1/m1": {"id": "p1/m1"}}
            }),
        )
        .unwrap();
        let catalog = load_models_dev_catalog_file().unwrap();
        assert_eq!(catalog.providers["p1"]["name"], "P1");
        let status = models_dev_catalog_status();
        assert!(status.available);
        assert_eq!(status.providers, Some(1));
        assert_eq!(status.models, Some(1));
        assert!(status.synced_at.is_some());

        write_catalog_file(
            &artificial_analysis_models_path(),
            &BenchmarkCatalogFile {
                source: "artificialanalysis.ai".into(),
                synced_at: "2026-01-01T00:00:00Z".into(),
                models: vec![sample_record("p1-m1", 80.0, 70.0, 0.6)],
            },
        )
        .unwrap();
        let status = artificial_analysis_catalog_status();
        assert!(status.available);
        assert_eq!(status.synced_at.as_deref(), Some("2026-01-01T00:00:00Z"));
        assert_eq!(status.models, Some(1));
        assert!(load_artificial_analysis_catalog().is_some());
    }

    #[test]
    fn aa_model_map_load_save_status_and_exact_refresh() {
        let _home = TestHome::new();
        let mut file = AaModelMapFile {
            version: 7,
            description: "custom".into(),
            mappings: HashMap::new(),
        };
        file.mappings
            .insert("existing/model".into(), "known".into());
        save_aa_model_map(&file).unwrap();
        assert_eq!(load_aa_model_map().version, 7);
        let status = aa_model_map_status();
        assert!(status.available);
        assert_eq!(status.models, Some(1));

        std::fs::write(aa_model_map_path(), "{bad-json").unwrap();
        assert_eq!(load_aa_model_map().version, 1);
        std::fs::remove_file(aa_model_map_path()).unwrap();
        let seeded = ensure_aa_model_map_file().unwrap();
        assert!(!seeded.mappings.is_empty());

        write_raw_catalog_file(
            &models_dev_catalog_path(),
            &serde_json::json!({
                "providers": {},
                "models": {
                    "minimax/minimax-m3": {"name": "minimax-m3"}
                }
            }),
        )
        .unwrap();
        write_catalog_file(
            &artificial_analysis_models_path(),
            &BenchmarkCatalogFile {
                source: "aa".into(),
                synced_at: "now".into(),
                models: vec![sample_record("minimax-m3", 48.5, 44.0, 0.59)],
            },
        )
        .unwrap();
        let refreshed = refresh_aa_model_map_exact_matches().unwrap();
        assert_eq!(
            refreshed
                .mappings
                .get("minimax/minimax-m3")
                .map(String::as_str),
            Some("minimax-m3")
        );
    }

    #[test]
    fn lookup_record_variants_scores_and_key_helpers_cover_edges() {
        let catalog = BenchmarkCatalog::from_file(BenchmarkCatalogFile {
            source: "test".into(),
            synced_at: "now".into(),
            models: vec![
                sample_record("model-32k", 120.0, 0.5, 1.2),
                BenchmarkModelRecord {
                    id: "fallback".into(),
                    slug: "fallback".into(),
                    name: "Fallback".into(),
                    creator_slug: None,
                    creator_name: None,
                    evaluations: BenchmarkEvaluations {
                        artificial_analysis_math_index: Some(40.0),
                        livecodebench: Some(0.42),
                        scicode: Some(0.5),
                        terminalbench_hard: Some(0.3),
                        gpqa: Some(0.6),
                        hle: Some(0.9),
                        ..Default::default()
                    },
                    performance: BenchmarkPerformance::default(),
                },
            ],
        });
        assert!(catalog.synced_at.is_some());
        assert_eq!(
            catalog
                .lookup_record_exact("creator/model.v1", Some("minimax/model_32k"), None)
                .unwrap()
                .slug,
            "model-32k"
        );
        assert_eq!(
            catalog
                .lookup_variant(&[normalize_key("minimax/model")], 33_000)
                .unwrap()
                .slug,
            "model-32k"
        );
        assert_eq!(context_hint_from_key("model-32k"), Some(32_000));
        assert_eq!(context_hint_from_key("model"), None);

        let fallback = catalog
            .lookup(
                "fallback",
                None,
                Some("Fallback"),
                0,
                &AaModelMapFile::default(),
            )
            .unwrap();
        assert_eq!(fallback.overall_intelligence, None);
        assert_eq!(fallback.coding_index, Some(42.0));
        assert_eq!(fallback.math_index, Some(40.0));
        assert_eq!(fallback.agentic_index, Some(60.0));
        let clamped = indices_from_evaluations(&BenchmarkEvaluations {
            artificial_analysis_intelligence_index: Some(120.0),
            artificial_analysis_coding_index: Some(0.0),
            artificial_analysis_math_index: Some(-1.0),
            tau2: Some(2.0),
            ..Default::default()
        });
        assert_eq!(clamped.overall_intelligence, Some(100.0));
        assert_eq!(clamped.coding_index, Some(1.0));
        assert_eq!(clamped.agentic_index, Some(100.0));
        assert_eq!(clamped.math_index, Some(1.0));
    }

    #[test]
    fn api_key_resolution_prefers_settings_then_env_aliases() {
        unsafe {
            std::env::remove_var("ARTIFICIAL_ANALYSIS_API_KEY");
            std::env::remove_var("AA_API_KEY");
        }
        assert_eq!(
            resolve_artificial_analysis_api_key(Some(" settings-key ")).as_deref(),
            Some("settings-key")
        );
        unsafe {
            std::env::set_var("ARTIFICIAL_ANALYSIS_API_KEY", "env-key");
        }
        assert_eq!(
            resolve_artificial_analysis_api_key(Some("")).as_deref(),
            Some("env-key")
        );
        unsafe {
            std::env::remove_var("ARTIFICIAL_ANALYSIS_API_KEY");
            std::env::set_var("AA_API_KEY", "aa-key");
        }
        assert_eq!(
            resolve_artificial_analysis_api_key(None).as_deref(),
            Some("aa-key")
        );
    }
}
