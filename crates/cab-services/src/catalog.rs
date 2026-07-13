use cab_core::CabError;
use cab_db::InMemoryStore;
use chrono::{NaiveDate, TimeZone, Utc};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ModelsDevProvider {
    pub name: String,
    pub api: Option<String>,
    pub doc: Option<String>,
    pub env: Option<Vec<String>>,
    pub npm: Option<String>,
    pub models: std::collections::HashMap<String, ModelsDevModel>,
}

#[derive(Debug, Deserialize)]
pub struct ModelsDevModel {
    pub id: String,
    pub name: Option<String>,
    pub family: Option<String>,
    pub knowledge: Option<String>,
    pub release_date: Option<String>,
    pub last_updated: Option<String>,
    pub cost: Option<ModelsDevCost>,
    limit: Option<ModelsDevLimit>,
    modalities: Option<serde_json::Value>,
    benchmarks: Option<serde_json::Value>,
    weights: Option<serde_json::Value>,
    attachment: Option<bool>,
    reasoning: Option<bool>,
    temperature: Option<bool>,
    tool_call: Option<bool>,
    structured_output: Option<bool>,
    open_weights: Option<bool>,
    #[serde(flatten)]
    #[allow(dead_code)]
    extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ModelsDevCost {
    pub input: Option<f64>,
    pub output: Option<f64>,
    pub cache_read: Option<f64>,
    pub cache_write: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevLimit {
    context: Option<i64>,
    output: Option<i64>,
}

pub fn protocol_for_models_dev_provider(provider: &ModelsDevProvider) -> String {
    protocol_from_npm_and_api(provider.npm.as_deref(), provider.api.as_deref())
}

/// Per-model upstream protocol on a models.dev provider (e.g. opencode-go MiniMax → anthropic).
pub fn upstream_protocol_for_models_dev_model(
    provider: &ModelsDevProvider,
    model: &ModelsDevModel,
) -> String {
    if let Some(override_meta) = model.extra.get("provider")
        && let Some(npm) = override_meta.get("npm").and_then(|value| value.as_str())
    {
        return protocol_from_npm_and_api(Some(npm), provider.api.as_deref());
    }
    protocol_for_models_dev_provider(provider)
}

fn protocol_from_npm_and_api(npm: Option<&str>, api: Option<&str>) -> String {
    let npm = npm.unwrap_or_default().to_ascii_lowercase();
    let api = api.unwrap_or_default().to_ascii_lowercase();
    if npm.contains("anthropic") || api.contains("anthropic") {
        "anthropic".to_string()
    } else {
        "openai-chat".to_string()
    }
}

pub fn extract_huggingface_id(model: &ModelsDevModel) -> Option<String> {
    let weights = model.weights.as_ref()?.as_array()?;
    for weight in weights {
        let url = weight.get("url").and_then(|value| value.as_str())?;
        let rest = url.split("huggingface.co/").nth(1)?;
        let id = rest.trim_end_matches('/').split('?').next()?.trim();
        if !id.is_empty() {
            return Some(id.to_string());
        }
    }
    None
}

fn build_architecture_json(model: &ModelsDevModel) -> serde_json::Value {
    serde_json::json!({
        "family": model.family,
        "modalities": model.modalities,
        "attachment": model.attachment,
        "reasoning": model.reasoning,
        "temperature": model.temperature,
        "tool_call": model.tool_call,
        "structured_output": model.structured_output,
        "open_weights": model.open_weights,
    })
}

pub fn normalize_minimax_vendor_cost(
    provider_id: &str,
    native_model_id: &str,
    cost: &ModelsDevCost,
) -> ModelsDevCost {
    if provider_id != "minimax" {
        return cost.clone();
    }
    // models.dev lists M3 at pre-discount list price; MiniMax paygo is permanently 50% off (≤512k).
    // https://platform.minimax.io/docs/guides/pricing-paygo
    if native_model_id == "MiniMax-M3" && cost.input == Some(0.6) && cost.output == Some(2.4) {
        return ModelsDevCost {
            input: Some(0.3),
            output: Some(1.2),
            cache_read: cost.cache_read.map(|v| {
                if (v - 0.12).abs() < f64::EPSILON {
                    0.06
                } else {
                    v
                }
            }),
            cache_write: cost.cache_write,
        };
    }
    cost.clone()
}

fn models_dev_cost_for_provider_model(
    provider_id: &str,
    model: &ModelsDevModel,
) -> Option<ModelsDevCost> {
    model
        .cost
        .as_ref()
        .map(|cost| normalize_minimax_vendor_cost(provider_id, &model.id, cost))
}

pub fn build_pricing_json(cost: Option<&ModelsDevCost>) -> Option<serde_json::Value> {
    cost.map(|cost| {
        serde_json::json!({
            "input": cost.input,
            "output": cost.output,
            "cache_read": cost.cache_read,
            "cache_write": cost.cache_write,
        })
    })
}

pub fn build_catalog_provider_json(
    provider_id: &str,
    provider: Option<&ModelsDevProvider>,
    native_model_id: &str,
) -> serde_json::Value {
    serde_json::json!({
        "id": provider_id,
        "name": provider.map(|entry| entry.name.as_str()),
        "api": provider.and_then(|entry| entry.api.as_deref()),
        "npm": provider.and_then(|entry| entry.npm.as_deref()),
        "env": provider.and_then(|entry| entry.env.as_ref()),
        "doc": provider.and_then(|entry| entry.doc.as_deref()),
        "native_model_id": native_model_id,
    })
}

pub fn build_links_json(
    model: &ModelsDevModel,
    model_name: &str,
    native_model_id: &str,
    provider: Option<&ModelsDevProvider>,
) -> serde_json::Value {
    serde_json::json!({
        "catalog": "models.dev",
        "catalog_id": model_name,
        "native_model_id": native_model_id,
        "doc": provider.and_then(|entry| entry.doc.as_deref()),
        "weights": model.weights,
        "benchmarks": model.benchmarks,
        "last_updated": model.last_updated,
    })
}

pub fn supported_parameters_from_models_dev_model(model: &ModelsDevModel) -> serde_json::Value {
    let mut params = Vec::new();
    if model.temperature.unwrap_or(false) {
        params.push("temperature");
    }
    if model.tool_call.unwrap_or(false) {
        params.push("tools");
        params.push("tool_choice");
    }
    if model.reasoning.unwrap_or(false) {
        params.push("reasoning");
    }
    serde_json::Value::Array(
        params
            .into_iter()
            .map(|param| serde_json::Value::String(param.to_string()))
            .collect(),
    )
}

#[derive(Debug, Clone)]
pub struct ServedModelRef {
    pub provider_id: String,
    pub native_model_id: String,
    pub cost: Option<ModelsDevCost>,
}

pub fn normalize_models_dev_model_key(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace([' ', '_'], "-")
}

fn add_served_model_lookup_keys(
    lookup: &mut std::collections::HashMap<String, ServedModelRef>,
    provider_id: &str,
    model: &ModelsDevModel,
) {
    let served = ServedModelRef {
        provider_id: provider_id.to_string(),
        native_model_id: model.id.clone(),
        cost: models_dev_cost_for_provider_model(provider_id, model),
    };
    let mut keys = vec![
        model.id.clone(),
        format!("{provider_id}/{}", model.id),
        normalize_models_dev_model_key(&model.id),
        normalize_models_dev_model_key(&format!("{provider_id}/{}", model.id)),
    ];
    if let Some(name) = model.name.as_deref() {
        keys.push(name.to_string());
        keys.push(format!("{provider_id}/{name}"));
        keys.push(normalize_models_dev_model_key(name));
        keys.push(normalize_models_dev_model_key(&format!(
            "{provider_id}/{name}"
        )));
    }
    for key in keys {
        lookup.entry(key).or_insert_with(|| served.clone());
    }
}

fn resolve_canonical_model_name(
    provider_id: &str,
    model: &ModelsDevModel,
    models_data: &std::collections::HashMap<String, ModelsDevModel>,
) -> Option<String> {
    let mut candidates = vec![model.id.clone(), format!("{provider_id}/{}", model.id)];
    if let Some(name) = &model.name {
        candidates.push(name.clone());
        candidates.push(format!("{provider_id}/{name}"));
    }

    for candidate in candidates {
        if models_data.contains_key(&candidate) {
            return Some(candidate);
        }
        let normalized = normalize_models_dev_model_key(&candidate);
        for key in models_data.keys() {
            if normalize_models_dev_model_key(key) == normalized {
                return Some(key.clone());
            }
        }
    }

    // Resellers like opencode-go use bare slugs (deepseek-v4-pro) while catalog.models
    // stores vendor-qualified ids (deepseek/deepseek-v4-pro).
    if !model.id.contains('/') {
        let suffix = format!("/{}", model.id);
        let normalized_suffix = normalize_models_dev_model_key(&suffix);
        for key in models_data.keys() {
            if key.ends_with(&suffix)
                || normalize_models_dev_model_key(key).ends_with(&normalized_suffix)
            {
                return Some(key.clone());
            }
        }
    }

    None
}

pub async fn sync_model_endpoints(
    pool: &cab_db::InMemoryStore,
    providers_data: &std::collections::HashMap<String, ModelsDevProvider>,
    models_data: &std::collections::HashMap<String, ModelsDevModel>,
) -> Result<usize, CabError> {
    cab_db::endpoint::clear_all(pool)
        .await
        .map_err(CabError::Database)?;

    let catalog_providers = cab_db::provider::list_catalog(pool)
        .await
        .map_err(CabError::Database)?;
    let provider_names: std::collections::HashMap<String, String> = catalog_providers
        .iter()
        .map(|provider| (provider.id.clone(), provider.name.clone()))
        .collect();
    let provider_enabled: std::collections::HashMap<String, bool> = catalog_providers
        .iter()
        .map(|provider| (provider.id.clone(), provider.enabled))
        .collect();

    let mut seen = std::collections::HashSet::new();
    let mut count = 0usize;
    let now = Utc::now().to_rfc3339();

    for (provider_id, provider) in providers_data {
        for model in provider.models.values() {
            let Some(canonical) = resolve_canonical_model_name(provider_id, model, models_data)
            else {
                continue;
            };
            let dedupe_key = format!("{canonical}::{provider_id}");
            if !seen.insert(dedupe_key) {
                continue;
            }

            let normalized_cost = models_dev_cost_for_provider_model(provider_id, model);
            let cost = normalized_cost.as_ref();
            let limit = model.limit.as_ref();
            let upstream_protocol = upstream_protocol_for_models_dev_model(provider, model);
            let endpoint = cab_db::endpoint::ModelEndpoint {
                id: format!("{canonical}::{provider_id}"),
                model_id: canonical.clone(),
                canonical_slug: canonical.clone(),
                provider_name: provider_names
                    .get(provider_id)
                    .cloned()
                    .unwrap_or_else(|| provider.name.clone()),
                provider_tag: provider_id.clone(),
                native_model_id: model.id.clone(),
                upstream_protocol: Some(upstream_protocol),
                quantization: "unknown".to_string(),
                input_cost: cost.and_then(|c| c.input),
                output_cost: cost.and_then(|c| c.output),
                cache_read_cost: cost.and_then(|c| c.cache_read),
                context_length: limit.and_then(|l| l.context),
                max_completion_tokens: limit.and_then(|l| l.output),
                status: 0,
                uptime_30m: None,
                uptime_5m: None,
                uptime_1d: None,
                supports_tools: model.tool_call.unwrap_or(false),
                supports_streaming: true,
                enabled: provider_enabled.get(provider_id).copied().unwrap_or(false),
                updated_at: now.clone(),
            };

            cab_db::endpoint::upsert(pool, &endpoint)
                .await
                .map_err(CabError::Database)?;
            count += 1;
        }
    }

    tracing::info!("models.dev sync: indexed {count} model-provider endpoints");
    Ok(count)
}

pub fn served_model_matches_canonical(
    served: &ServedModelRef,
    provider_prefix: &str,
    model_slug: &str,
    canonical_model_id: &str,
) -> bool {
    let native = served.native_model_id.as_str();
    if native == model_slug || native == canonical_model_id {
        return true;
    }
    if format!("{provider_prefix}/{native}") == canonical_model_id {
        return true;
    }
    normalize_models_dev_model_key(native) == normalize_models_dev_model_key(model_slug)
        || normalize_models_dev_model_key(&format!("{provider_prefix}/{native}"))
            == normalize_models_dev_model_key(canonical_model_id)
}

pub fn resolve_served_model(
    lookup: &std::collections::HashMap<String, ServedModelRef>,
    canonical_model_id: &str,
) -> Option<ServedModelRef> {
    let (provider_prefix, model_slug) = canonical_model_id
        .split_once('/')
        .unwrap_or(("", canonical_model_id));

    // Prefer the model vendor's own gateway even when slug keys collide across resellers.
    if !provider_prefix.is_empty() {
        for served in lookup.values() {
            if served.provider_id == provider_prefix
                && served_model_matches_canonical(
                    served,
                    provider_prefix,
                    model_slug,
                    canonical_model_id,
                )
            {
                return Some(served.clone());
            }
        }
    }

    let candidates = [
        canonical_model_id.to_string(),
        normalize_models_dev_model_key(canonical_model_id),
        format!("{provider_prefix}/{model_slug}"),
        normalize_models_dev_model_key(&format!("{provider_prefix}/{model_slug}")),
        model_slug.to_string(),
        normalize_models_dev_model_key(model_slug),
    ];
    candidates.iter().find_map(|key| lookup.get(key).cloned())
}

pub fn parse_release_timestamp(value: Option<&str>) -> Option<i64> {
    let date = value?;
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|dt| Utc.from_utc_datetime(&dt).timestamp())
}

fn model_enabled_override(settings: &cab_core::types::Settings, model_name: &str) -> Option<bool> {
    settings
        .models
        .get(model_name)
        .and_then(|model| model.enabled)
        .or_else(|| {
            let normalized = model_name.to_ascii_lowercase();
            settings.models.iter().find_map(|(name, model)| {
                if name.to_ascii_lowercase() == normalized {
                    model.enabled
                } else {
                    None
                }
            })
        })
}

/// Synchronize provider/model catalog from models.dev.
/// Provider protocols and endpoints come from bundled defaults, overridden by user settings.
pub async fn sync_models_dev_catalog(pool: &cab_db::InMemoryStore) -> Result<usize, CabError> {
    let defaults = cab_core::load_provider_defaults();
    let settings = cab_db::settings::get(pool)
        .await
        .map_err(CabError::Database)?;

    let client = std::sync::Arc::new(
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default(),
    );

    // Download live catalog; fall back to cached file if download fails.
    // The cached file fallback reads from ~/.cab/catalog/models.dev/catalog.json,
    // a one-time migration path for users upgrading from the old JSON-file-based
    // storage. New installs that never synced before will have no cached file.
    let (providers_json, models_json) = match crate::benchmarks::sync_catalogs(pool, &client).await
    {
        Ok(json) => (json["providers"].clone(), json["models"].clone()),
        Err(e) => {
            tracing::warn!("models.dev download failed, trying cached file: {e}");
            let file = cab_core::load_models_dev_catalog_file()
                .map_err(|e2| CabError::Database(format!("{e}; cached file also: {e2}")))?;
            (file.providers, file.models)
        }
    };

    if let Err(e) = cab_core::ensure_aa_model_map_file() {
        tracing::warn!("Failed to seed AA model map: {e}");
    }
    if let Err(e) = cab_core::refresh_aa_model_map_exact_matches() {
        tracing::warn!("Failed to refresh AA model map: {e}");
    }
    let benchmark_catalog = pool
        .sqlite()
        .and_then(|p| p.get().ok())
        .and_then(|conn| cab_db::sqlite::load_aa_benchmark_catalog(&conn));
    let aa_map = cab_core::load_aa_model_map();

    let providers_data: std::collections::HashMap<String, ModelsDevProvider> =
        serde_json::from_value(providers_json).map_err(|e| {
            CabError::Database(format!("Failed to parse models.dev providers: {e}"))
        })?;
    let models_data: std::collections::HashMap<String, ModelsDevModel> =
        serde_json::from_value(models_json)
            .map_err(|e| CabError::Database(format!("Failed to parse models.dev models: {e}")))?;

    let count = sync_models_dev_models(
        pool,
        &providers_data,
        &models_data,
        &settings,
        &defaults,
        benchmark_catalog.as_ref(),
        &aa_map,
        &client,
    )
    .await?;
    cab_db::provider::apply_all_provider_configs(pool)
        .await
        .map_err(CabError::Database)?;
    sync_model_endpoints(pool, &providers_data, &models_data).await?;
    Ok(count)
}

pub async fn sync_models_internal(
    pool: &cab_db::InMemoryStore,
    _provider_id: &str,
) -> Result<usize, CabError> {
    sync_models_dev_catalog(pool).await
}

#[allow(clippy::too_many_arguments, clippy::collapsible_if)]
pub async fn sync_models_dev_models(
    pool: &cab_db::InMemoryStore,
    providers_data: &std::collections::HashMap<String, ModelsDevProvider>,
    models_data: &std::collections::HashMap<String, ModelsDevModel>,
    settings: &cab_core::types::Settings,
    defaults: &cab_core::ProviderDefaultsCatalog,
    benchmark_catalog: Option<&cab_core::BenchmarkCatalog>,
    aa_map: &cab_core::AaModelMapFile,
    client: &reqwest::Client,
) -> Result<usize, CabError> {
    let existing_models = cab_db::model::list(pool)
        .await
        .map_err(CabError::Database)?;

    let mut provider_ids = std::collections::HashSet::new();
    let mut fetched_names = std::collections::HashSet::new();
    let mut added_count = 0usize;
    let mut updated_count = 0usize;

    let mut served_lookup = std::collections::HashMap::new();

    for (provider_id, provider) in providers_data {
        provider_ids.insert(provider_id.clone());
        let protocol = protocol_for_models_dev_provider(provider);
        let default_endpoint = provider.api.as_deref().map(|api| (protocol.as_str(), api));

        let mut catalog_models: Vec<String> = provider
            .models
            .values()
            .filter_map(|model| {
                resolve_canonical_model_name(provider_id, model, models_data)
                    .or_else(|| Some(format!("{provider_id}/{}", model.id)))
            })
            .collect();
        catalog_models.sort();
        catalog_models.dedup();

        // Download logo from models.dev and save as a static file on disk
        let logo_url = cab_core::models_dev_provider_logo_url(provider_id);
        let logos_dir = cab_core::catalog_root_dir()
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("logos");
        let logo_path = logos_dir.join(format!("{provider_id}.svg"));
        // Only download if file doesn't already exist (avoids re-downloading every sync)
        if !logo_path.exists() {
            if let Ok(resp) = client.get(&logo_url).send().await {
                if resp.status().is_success() {
                    if let Ok(text) = resp.text().await {
                        if text.trim().starts_with("<svg") {
                            if let Err(e) = std::fs::create_dir_all(&logos_dir) {
                                tracing::warn!("Failed to create logos dir: {e}");
                            } else if let Err(e) = std::fs::write(&logo_path, &text) {
                                tracing::warn!("Failed to write logo for {provider_id}: {e}");
                            }
                        }
                    }
                }
            }
        }

        cab_db::provider::upsert_catalog_provider(
            pool,
            provider_id,
            &provider.name,
            default_endpoint,
            None,
            provider.doc.as_deref(),
            None,
            None,
            None,
            provider.api.as_deref(),
            provider.doc.as_deref(),
            provider.env.as_deref(),
            provider.npm.as_deref(),
            provider.models.len(),
            None,
            &catalog_models,
        )
        .await
        .map_err(CabError::Database)?;

        cab_db::provider::apply_provider_config(pool, provider_id, settings, defaults)
            .await
            .map_err(CabError::Database)?;

        for model in provider.models.values() {
            add_served_model_lookup_keys(&mut served_lookup, provider_id, model);
        }
    }

    for (canonical_id, model) in models_data {
        let model_name = model.id.trim().to_string();
        if model_name.is_empty() {
            continue;
        }
        fetched_names.insert(model_name.clone());

        let provider_prefix = model_name
            .split_once('/')
            .map(|(provider, _)| provider)
            .unwrap_or("unknown");
        let served_model = resolve_served_model(&served_lookup, &model_name);
        let provider_id = served_model
            .as_ref()
            .map(|served| served.provider_id.clone())
            .unwrap_or_else(|| provider_prefix.to_string());
        let provider = providers_data.get(&provider_id);
        let native_model_id = served_model
            .as_ref()
            .map(|served| served.native_model_id.clone())
            .unwrap_or_else(|| {
                model_name
                    .split_once('/')
                    .map(|(_, name)| name.to_string())
                    .unwrap_or_else(|| model_name.clone())
            });

        let display_name = model
            .name
            .clone()
            .unwrap_or_else(|| native_model_id.clone());
        let context_length = model
            .limit
            .as_ref()
            .and_then(|limit| limit.context)
            .unwrap_or(0);
        let selected_cost = served_model
            .as_ref()
            .and_then(|served| served.cost.as_ref())
            .or(model.cost.as_ref());
        let input_cost = selected_cost.and_then(|cost| cost.input);
        let output_cost = selected_cost.and_then(|cost| cost.output);
        let indices = cab_core::resolve_intelligence_indices(
            benchmark_catalog,
            aa_map,
            &model_name,
            Some(canonical_id),
            Some(&display_name),
            context_length,
        );
        let performance = cab_core::resolve_performance_metrics(
            benchmark_catalog,
            aa_map,
            &model_name,
            Some(canonical_id),
            Some(&display_name),
            context_length,
        );
        let configured_enabled = model_enabled_override(settings, &model_name);
        let existing_model = existing_models
            .iter()
            .find(|em| em.name.eq_ignore_ascii_case(&model_name));
        let created = parse_release_timestamp(model.release_date.as_deref());
        let pricing = build_pricing_json(selected_cost);
        let architecture = Some(build_architecture_json(model));
        let top_provider = Some(build_catalog_provider_json(
            &provider_id,
            provider,
            &native_model_id,
        ));
        let per_request_limits = model.limit.as_ref().map(|limit| {
            serde_json::json!({
                "context": limit.context,
                "output": limit.output,
            })
        });
        let links = Some(build_links_json(
            model,
            &model_name,
            &native_model_id,
            provider,
        ));
        let hugging_face_id = extract_huggingface_id(model);
        let knowledge_cutoff = model.knowledge.clone();
        let supported_parameters = supported_parameters_from_models_dev_model(model);

        if let Some(existing) = existing_model {
            let update_input = cab_core::types::UpdateModel {
                name: Some(model_name.clone()),
                display_name: Some(display_name),
                provider_id: Some(provider_id.clone()),
                protocol: Some(cab_db::provider::default_protocol_for_provider(
                    &provider_id,
                    settings,
                    defaults,
                )),
                context_length: Some(context_length),
                input_cost: Some(input_cost),
                output_cost: Some(output_cost),
                enabled: Some(configured_enabled.unwrap_or(existing.enabled)),
                overall_intelligence: Some(indices.overall_intelligence),
                coding_index: Some(indices.coding_index),
                agentic_index: Some(indices.agentic_index),
                math_index: Some(indices.math_index),
                output_speed_tps: Some(performance.output_speed_tps),
                time_to_first_token_secs: Some(performance.time_to_first_token_secs),
                canonical_slug: Some(model_name.clone()),
                hugging_face_id,
                created,
                description: model.family.clone(),
                architecture,
                pricing,
                top_provider,
                per_request_limits,
                supported_parameters: Some(supported_parameters),
                default_parameters: None,
                supported_voices: None,
                knowledge_cutoff,
                expiration_date: None,
                links,
            };
            cab_db::model::update(pool, &existing.id, &update_input)
                .await
                .map_err(CabError::Database)?;
            updated_count += 1;
        } else {
            let create_input = cab_core::types::CreateModel {
                name: model_name.clone(),
                display_name,
                provider_id: provider_id.clone(),
                protocol: cab_db::provider::default_protocol_for_provider(
                    &provider_id,
                    settings,
                    defaults,
                ),
                context_length,
                input_cost,
                output_cost,
                enabled: Some(configured_enabled.unwrap_or(false)),
                overall_intelligence: indices.overall_intelligence,
                coding_index: indices.coding_index,
                agentic_index: indices.agentic_index,
                math_index: indices.math_index,
                output_speed_tps: performance.output_speed_tps,
                time_to_first_token_secs: performance.time_to_first_token_secs,
                canonical_slug: Some(model_name),
                hugging_face_id,
                created,
                description: model.family.clone(),
                architecture,
                pricing,
                top_provider,
                per_request_limits,
                supported_parameters: Some(supported_parameters),
                default_parameters: None,
                supported_voices: None,
                knowledge_cutoff,
                expiration_date: None,
                links,
            };
            cab_db::model::create(pool, &create_input)
                .await
                .map_err(CabError::Database)?;
            added_count += 1;
        }
    }

    for model in &existing_models {
        if !fetched_names.contains(&model.name) {
            let _ = cab_db::model::delete(pool, &model.id).await;
        }
    }

    let providers = cab_db::provider::list_catalog(pool)
        .await
        .map_err(CabError::Database)?;
    for provider in providers {
        if !provider_ids.contains(&provider.id) {
            let _ = cab_db::provider::delete(pool, &provider.id).await;
        }
    }

    tracing::info!(
        "models.dev sync: added {}, updated {}, providers {}, models {}",
        added_count,
        updated_count,
        provider_ids.len(),
        fetched_names.len()
    );

    Ok(added_count + updated_count)
}

pub async fn auto_seed_known_models(
    _pool: &cab_db::InMemoryStore,
    _provider: &cab_core::types::Provider,
) {
    // Disabled to strictly avoid mocking models.
    // All models must be dynamically fetched from official API endpoints.
}

/// Startup helper used by cab-srv and Tauri.
pub async fn sync_on_startup(pool: &InMemoryStore) -> Result<usize, CabError> {
    sync_models_dev_catalog(pool).await
}

#[cfg(test)]
mod resolve_canonical_model_name_tests {
    use super::{
        ModelsDevCost, ModelsDevModel, ModelsDevProvider, normalize_minimax_vendor_cost,
        resolve_canonical_model_name, upstream_protocol_for_models_dev_model,
    };

    fn model(id: &str) -> ModelsDevModel {
        serde_json::from_value(serde_json::json!({ "id": id })).unwrap()
    }

    #[test]
    fn maps_bare_opencode_go_slug_to_vendor_qualified_catalog_id() {
        let mut models_data = std::collections::HashMap::new();
        models_data.insert(
            "deepseek/deepseek-v4-pro".to_string(),
            model("deepseek/deepseek-v4-pro"),
        );

        let resolved =
            resolve_canonical_model_name("opencode-go", &model("deepseek-v4-pro"), &models_data);

        assert_eq!(resolved.as_deref(), Some("deepseek/deepseek-v4-pro"));
    }

    #[test]
    fn minimax_m3_vendor_cost_uses_effective_paygo_discount() {
        let list = ModelsDevCost {
            input: Some(0.6),
            output: Some(2.4),
            cache_read: Some(0.12),
            cache_write: None,
        };
        let effective = normalize_minimax_vendor_cost("minimax", "MiniMax-M3", &list);
        assert_eq!(effective.input, Some(0.3));
        assert_eq!(effective.output, Some(1.2));
        assert_eq!(effective.cache_read, Some(0.06));

        let unchanged = normalize_minimax_vendor_cost("opencode-go", "minimax-m3", &list);
        assert_eq!(unchanged.input, Some(0.6));
    }

    #[test]
    fn opencode_go_model_npm_selects_upstream_protocol() {
        let provider: ModelsDevProvider = serde_json::from_value(serde_json::json!({
            "name": "OpenCode Go",
            "api": "https://opencode.ai/zen/go/v1",
            "npm": "@ai-sdk/openai-compatible",
            "models": {}
        }))
        .unwrap();
        let anthropic_model: ModelsDevModel = serde_json::from_value(serde_json::json!({
            "id": "minimax-m3",
            "provider": { "npm": "@ai-sdk/anthropic" }
        }))
        .unwrap();
        let chat_model: ModelsDevModel = serde_json::from_value(serde_json::json!({
            "id": "deepseek-v4-flash"
        }))
        .unwrap();

        assert_eq!(
            upstream_protocol_for_models_dev_model(&provider, &anthropic_model),
            "anthropic"
        );
        assert_eq!(
            upstream_protocol_for_models_dev_model(&provider, &chat_model),
            "openai-chat"
        );
    }
}
