use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use cab_core::CabError;
use cab_core::types::{CreateProvider, UpdateProvider};
use chrono::{NaiveDate, TimeZone, Utc};
use serde::Deserialize;

use crate::ApiState;

pub async fn list_providers(State(state): State<ApiState>) -> Result<impl IntoResponse, CabError> {
    let providers = cab_db::provider::list_catalog(&state.pool)
        .await
        .map_err(CabError::Database)?;
    Ok(Json(providers))
}

pub async fn list_endpoint_provider_summary(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, CabError> {
    let providers = cab_db::endpoint::provider_summary(&state.pool)
        .await
        .map_err(CabError::Database)?;
    Ok(Json(providers))
}

pub async fn get_provider(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, CabError> {
    let provider = cab_db::provider::get_by_id(&state.pool, &id)
        .await
        .map_err(CabError::Database)?
        .ok_or_else(|| CabError::NotFound(format!("Provider {id} not found")))?;
    Ok(Json(provider))
}

pub async fn create_provider(
    State(state): State<ApiState>,
    Json(_input): Json<CreateProvider>,
) -> Result<StatusCode, CabError> {
    let _ = state;
    Err(CabError::InvalidRequest(
        "Providers are synchronized from models.dev and cannot be created manually.".to_string(),
    ))
}

pub async fn update_provider(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(input): Json<UpdateProvider>,
) -> Result<impl IntoResponse, CabError> {
    let existing = cab_db::provider::get_by_id(&state.pool, &id)
        .await
        .map_err(CabError::Database)?
        .ok_or_else(|| CabError::NotFound(format!("Provider {id} not found")))?;

    if input.name.is_some() {
        return Err(CabError::InvalidRequest(
            "Only provider API key, endpoints, and enabled status can be changed manually."
                .to_string(),
        ));
    }

    let api_keys = input
        .api_keys
        .clone()
        .unwrap_or_else(|| existing.api_keys.clone());
    let has_enabled_key = api_keys
        .iter()
        .any(|k| k.enabled && !k.key.trim().is_empty());
    let enabled = input.enabled.unwrap_or(existing.enabled);

    if enabled && !has_enabled_key {
        return Err(CabError::InvalidRequest(
            "Cannot enable a provider without configuring and enabling at least one API key."
                .to_string(),
        ));
    }

    let sanitized = UpdateProvider {
        name: None,
        endpoints: input.endpoints,
        api_key: input.api_key,
        enabled: input.enabled,
        privacy_policy_url: None,
        terms_of_service_url: None,
        status_page_url: None,
        headquarters: None,
        datacenters: None,
        api_keys: input.api_keys,
        api: None,
        doc: None,
        env: None,
        npm: None,
        model_count: None,
    };

    let provider = cab_db::provider::update(&state.pool, &id, &sanitized)
        .await
        .map_err(CabError::Database)?
        .ok_or_else(|| CabError::NotFound(format!("Provider {id} not found")))?;

    let user_settings = cab_core::provider_defaults::provider_user_settings_from_provider(
        provider.enabled,
        &provider.api_key,
        &provider.api_keys,
        &provider.endpoints,
    );
    cab_db::settings::set_provider_override(&state.pool, &id, user_settings)
        .await
        .map_err(CabError::Database)?;

    Ok(Json(provider))
}

pub async fn delete_provider(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<StatusCode, CabError> {
    let _ = state;
    let _ = id;
    Err(CabError::InvalidRequest(
        "Providers are synchronized from models.dev and cannot be deleted manually.".to_string(),
    ))
}

pub async fn sync_provider_models(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, CabError> {
    let _ = id;
    let synced = sync_models_dev_catalog(&state.pool).await?;
    let all_models = cab_db::model::list(&state.pool)
        .await
        .map_err(CabError::Database)?;

    Ok(Json(serde_json::json!({
        "success": true,
        "synced": synced,
        "total": all_models.len(),
    })))
}

pub async fn sync_models_dev_providers(
    State(state): State<ApiState>,
) -> Result<impl IntoResponse, CabError> {
    let synced = sync_models_dev_catalog(&state.pool).await?;
    let total = cab_db::provider::list_catalog(&state.pool)
        .await
        .map_err(CabError::Database)?
        .len();

    Ok(Json(serde_json::json!({
        "success": true,
        "synced": synced,
        "providers": total,
    })))
}

#[derive(Debug, Deserialize)]
pub struct EnabledInput {
    enabled: bool,
}

pub async fn update_endpoint_provider_status(
    State(state): State<ApiState>,
    Path(provider_name): Path<String>,
    Json(input): Json<EnabledInput>,
) -> Result<impl IntoResponse, CabError> {
    let updated =
        cab_db::endpoint::set_provider_enabled(&state.pool, &provider_name, input.enabled)
            .await
            .map_err(CabError::Database)?;

    Ok(Json(serde_json::json!({
        "success": true,
        "updated": updated,
    })))
}

#[derive(Debug, Deserialize)]
struct ModelsDevProvider {
    name: String,
    api: Option<String>,
    doc: Option<String>,
    env: Option<Vec<String>>,
    npm: Option<String>,
    models: std::collections::HashMap<String, ModelsDevModel>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevModel {
    id: String,
    name: Option<String>,
    family: Option<String>,
    knowledge: Option<String>,
    release_date: Option<String>,
    last_updated: Option<String>,
    cost: Option<ModelsDevCost>,
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
    extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Deserialize, Clone)]
struct ModelsDevCost {
    input: Option<f64>,
    output: Option<f64>,
    cache_read: Option<f64>,
    cache_write: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ModelsDevLimit {
    context: Option<i64>,
    output: Option<i64>,
}

fn protocol_for_models_dev_provider(provider: &ModelsDevProvider) -> String {
    let npm = provider
        .npm
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let api = provider
        .api
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if npm.contains("anthropic") || api.contains("anthropic") {
        "anthropic".to_string()
    } else {
        "openai-chat".to_string()
    }
}

fn extract_huggingface_id(model: &ModelsDevModel) -> Option<String> {
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

fn build_pricing_json(cost: Option<&ModelsDevCost>) -> Option<serde_json::Value> {
    cost.map(|cost| {
        serde_json::json!({
            "input": cost.input,
            "output": cost.output,
            "cache_read": cost.cache_read,
            "cache_write": cost.cache_write,
        })
    })
}

fn build_catalog_provider_json(
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

fn build_links_json(
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

fn supported_parameters_from_models_dev_model(model: &ModelsDevModel) -> serde_json::Value {
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
struct ServedModelRef {
    provider_id: String,
    native_model_id: String,
    cost: Option<ModelsDevCost>,
}

fn normalize_models_dev_model_key(value: &str) -> String {
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
        cost: model.cost.clone(),
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
    None
}

async fn sync_model_endpoints(
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

            let cost = model.cost.as_ref();
            let limit = model.limit.as_ref();
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
                quantization: "unknown".to_string(),
                input_cost: cost.and_then(|c| c.input).unwrap_or(0.0),
                output_cost: cost.and_then(|c| c.output).unwrap_or(0.0),
                cache_read_cost: cost.and_then(|c| c.cache_read),
                context_length: limit.and_then(|l| l.context).unwrap_or(0),
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

fn served_model_matches_canonical(
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

fn resolve_served_model(
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

fn parse_release_timestamp(value: Option<&str>) -> Option<i64> {
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
/// Provider protocols and endpoints come from bundled defaults, overridden by ~/.cab/settings.json.
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

    if let Err(e) = crate::benchmarks::sync_catalogs(pool, &client).await {
        tracing::warn!("Benchmark catalog sync failed (using cache/heuristics): {e}");
    }
    if let Err(e) = cab_core::ensure_aa_model_map_file() {
        tracing::warn!("Failed to seed AA model map: {e}");
    }
    if let Err(e) = cab_core::refresh_aa_model_map_exact_matches() {
        tracing::warn!("Failed to refresh AA model map: {e}");
    }
    let benchmark_catalog = cab_core::load_artificial_analysis_catalog();
    let aa_map = cab_core::load_aa_model_map();
    let catalog = cab_core::load_models_dev_catalog_file().map_err(CabError::Database)?;
    let providers_data: std::collections::HashMap<String, ModelsDevProvider> =
        serde_json::from_value(catalog.providers).map_err(|e| {
            CabError::Database(format!(
                "Failed to parse models.dev providers from catalog.json: {e}"
            ))
        })?;
    let models_data: std::collections::HashMap<String, ModelsDevModel> =
        serde_json::from_value(catalog.models).map_err(|e| {
            CabError::Database(format!(
                "Failed to parse models.dev models from catalog.json: {e}"
            ))
        })?;

    let count = sync_models_dev_models(
        pool,
        &providers_data,
        &models_data,
        &settings,
        &defaults,
        benchmark_catalog.as_ref(),
        &aa_map,
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

async fn sync_models_dev_models(
    pool: &cab_db::InMemoryStore,
    providers_data: &std::collections::HashMap<String, ModelsDevProvider>,
    models_data: &std::collections::HashMap<String, ModelsDevModel>,
    settings: &cab_core::types::Settings,
    defaults: &cab_core::ProviderDefaultsCatalog,
    benchmark_catalog: Option<&cab_core::BenchmarkCatalog>,
    aa_map: &cab_core::AaModelMapFile,
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
            input_cost,
            None,
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
                input_cost,
                output_cost,
                enabled: Some(configured_enabled.unwrap_or(existing.enabled)),
                overall_intelligence: Some(indices.overall_intelligence),
                coding_index: Some(indices.coding_index),
                agentic_index: Some(indices.agentic_index),
                math_index: Some(indices.math_index),
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
                overall_intelligence: Some(indices.overall_intelligence),
                coding_index: Some(indices.coding_index),
                agentic_index: Some(indices.agentic_index),
                math_index: Some(indices.math_index),
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

pub async fn get_provider_balance(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, CabError> {
    let _provider = cab_db::provider::get_by_id(&state.pool, &id)
        .await
        .map_err(CabError::Database)?
        .ok_or_else(|| CabError::NotFound(format!("Provider {id} not found")))?;

    Ok(Json(serde_json::json!({
        "supported": false,
        "available": false,
        "balance": "",
        "currency": "",
        "message": "Provider balances are not queried directly. models.dev catalog providers only store API keys and enabled status.",
    })))
}

#[cfg(test)]
mod handler_and_catalog_tests {
    use super::*;
    use axum::body::to_bytes;
    use cab_core::types::{
        ApiKeyConfig, ModelUserSettings, Provider, ProviderEndpoint, ProviderUserSettings,
    };

    fn api_key(key: &str, enabled: bool) -> ApiKeyConfig {
        ApiKeyConfig {
            key: key.into(),
            enabled,
            subscribed: false,
            quota_reset_at: None,
        }
    }

    fn endpoint(id: &str, protocol: &str) -> ProviderEndpoint {
        ProviderEndpoint {
            id: id.into(),
            protocol: protocol.into(),
            url: "https://provider.test/v1".into(),
            label: Some("Test".into()),
            priority: 10,
            enabled: true,
        }
    }

    fn provider(id: &str) -> Provider {
        Provider {
            id: id.into(),
            name: "Provider One".into(),
            endpoints: vec![endpoint("chat", "openai-chat")],
            api_key: "key".into(),
            enabled: true,
            created_at: "now".into(),
            updated_at: "now".into(),
            privacy_policy_url: None,
            terms_of_service_url: None,
            status_page_url: None,
            headquarters: None,
            datacenters: None,
            api_keys: vec![api_key("key", true)],
            api: Some("https://provider.test/v1".into()),
            doc: Some("https://provider.test/docs".into()),
            env: Some(vec!["PROVIDER_API_KEY".into()]),
            npm: Some("@ai-sdk/openai-compatible".into()),
            model_count: 1,
            catalog_models: vec!["provider/model-one".into()],
        }
    }

    fn state_with_provider() -> ApiState {
        let pool = cab_db::InMemoryStore::new();
        {
            let mut data = pool.inner.write().unwrap();
            data.providers
                .insert("provider-1".into(), provider("provider-1"));
            data.model_endpoints.insert(
                "provider/model-one::provider-1".into(),
                cab_db::endpoint::ModelEndpoint {
                    id: "provider/model-one::provider-1".into(),
                    model_id: "provider/model-one".into(),
                    canonical_slug: "provider/model-one".into(),
                    provider_name: "Provider One".into(),
                    provider_tag: "provider-1".into(),
                    native_model_id: "model-one".into(),
                    quantization: "unknown".into(),
                    input_cost: 1.0,
                    output_cost: 2.0,
                    cache_read_cost: None,
                    context_length: 128000,
                    max_completion_tokens: Some(4096),
                    status: 1,
                    uptime_30m: None,
                    uptime_5m: None,
                    uptime_1d: None,
                    supports_tools: true,
                    supports_streaming: true,
                    enabled: true,
                    updated_at: "now".into(),
                },
            );
        }
        ApiState { pool }
    }

    fn expect_err<T>(result: Result<T, CabError>) -> CabError {
        match result {
            Ok(_) => panic!("expected handler error"),
            Err(err) => err,
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
    async fn provider_handlers_cover_validation_updates_summary_and_balance() {
        let state = state_with_provider();

        let providers = list_providers(State(state.clone())).await.unwrap();
        assert_eq!(json_body(providers).await[0]["id"], "provider-1");
        let summary = list_endpoint_provider_summary(State(state.clone()))
            .await
            .unwrap();
        assert_eq!(json_body(summary).await[0]["provider_name"], "Provider One");

        let got = get_provider(State(state.clone()), Path("provider-1".into()))
            .await
            .unwrap();
        assert_eq!(json_body(got).await["name"], "Provider One");
        let missing = expect_err(get_provider(State(state.clone()), Path("missing".into())).await);
        assert!(matches!(missing, CabError::NotFound(_)));

        let create_err = create_provider(
            State(state.clone()),
            Json(CreateProvider {
                name: "Manual".into(),
                endpoints: None,
                api_key: String::new(),
                enabled: Some(false),
                privacy_policy_url: None,
                terms_of_service_url: None,
                status_page_url: None,
                headquarters: None,
                datacenters: None,
                api_keys: None,
                api: None,
                doc: None,
                env: None,
                npm: None,
                model_count: None,
            }),
        )
        .await
        .unwrap_err();
        assert!(matches!(create_err, CabError::InvalidRequest(_)));

        let name_update = expect_err(
            update_provider(
                State(state.clone()),
                Path("provider-1".into()),
                Json(UpdateProvider {
                    name: Some("Nope".into()),
                    endpoints: None,
                    api_key: None,
                    enabled: None,
                    privacy_policy_url: None,
                    terms_of_service_url: None,
                    status_page_url: None,
                    headquarters: None,
                    datacenters: None,
                    api_keys: None,
                    api: None,
                    doc: None,
                    env: None,
                    npm: None,
                    model_count: None,
                }),
            )
            .await,
        );
        assert!(matches!(name_update, CabError::InvalidRequest(_)));

        let no_key = expect_err(
            update_provider(
                State(state.clone()),
                Path("provider-1".into()),
                Json(UpdateProvider {
                    name: None,
                    endpoints: None,
                    api_key: None,
                    enabled: Some(true),
                    privacy_policy_url: None,
                    terms_of_service_url: None,
                    status_page_url: None,
                    headquarters: None,
                    datacenters: None,
                    api_keys: Some(vec![api_key("", true), api_key("disabled", false)]),
                    api: None,
                    doc: None,
                    env: None,
                    npm: None,
                    model_count: None,
                }),
            )
            .await,
        );
        assert!(matches!(no_key, CabError::InvalidRequest(_)));

        let updated = update_provider(
            State(state.clone()),
            Path("provider-1".into()),
            Json(UpdateProvider {
                name: None,
                endpoints: Some(vec![endpoint("responses", "openai-responses")]),
                api_key: Some("new-key".into()),
                enabled: Some(true),
                privacy_policy_url: None,
                terms_of_service_url: None,
                status_page_url: None,
                headquarters: None,
                datacenters: None,
                api_keys: Some(vec![api_key("new-key", true)]),
                api: None,
                doc: None,
                env: None,
                npm: None,
                model_count: None,
            }),
        )
        .await
        .unwrap();
        let json = json_body(updated).await;
        assert_eq!(json["api_key"], "new-key");
        assert_eq!(json["endpoints"][0]["protocol"], "openai-responses");
        assert_eq!(
            cab_db::settings::get(&state.pool).await.unwrap().providers["provider-1"]
                .api_key
                .as_deref(),
            Some("new-key")
        );

        let status = update_endpoint_provider_status(
            State(state.clone()),
            Path("Provider One".into()),
            Json(EnabledInput { enabled: false }),
        )
        .await
        .unwrap();
        assert_eq!(json_body(status).await["success"], true);
        assert!(
            !cab_db::endpoint::list_for_model(&state.pool, "provider/model-one")
                .await
                .unwrap()[0]
                .enabled
        );

        let balance = get_provider_balance(State(state.clone()), Path("provider-1".into()))
            .await
            .unwrap();
        assert_eq!(json_body(balance).await["supported"], false);
        let balance_missing =
            expect_err(get_provider_balance(State(state.clone()), Path("missing".into())).await);
        assert!(matches!(balance_missing, CabError::NotFound(_)));

        let delete_err = delete_provider(State(state), Path("provider-1".into()))
            .await
            .unwrap_err();
        assert!(matches!(delete_err, CabError::InvalidRequest(_)));
    }

    fn providers_data() -> std::collections::HashMap<String, ModelsDevProvider> {
        serde_json::from_value(serde_json::json!({
            "provider": {
                "name": "Provider",
                "api": "https://provider.test/v1",
                "doc": "https://provider.test/docs",
                "env": ["PROVIDER_API_KEY"],
                "npm": "@ai-sdk/anthropic",
                "models": {
                    "model-one": {
                        "id": "model-one",
                        "name": "Model One",
                        "family": "family-a",
                        "knowledge": "2025-01",
                        "release_date": "2025-02-03",
                        "last_updated": "2025-03-04",
                        "cost": {"input": 1.5, "output": 2.5, "cache_read": 0.1, "cache_write": 0.2},
                        "limit": {"context": 128000, "output": 8192},
                        "modalities": {"input": ["text"]},
                        "benchmarks": {"score": 1},
                        "weights": [{"url": "https://huggingface.co/org/model-one?x=1"}],
                        "attachment": true,
                        "reasoning": true,
                        "temperature": true,
                        "tool_call": true,
                        "structured_output": true,
                        "open_weights": true
                    }
                }
            },
            "reseller": {
                "name": "Reseller",
                "api": "https://reseller.test/v1",
                "models": {
                    "provider/model-one": {
                        "id": "provider/model-one",
                        "name": "Provider Model One",
                        "cost": {"input": 3.0, "output": 4.0},
                        "limit": {"context": 64000, "output": 4096}
                    }
                }
            }
        }))
        .unwrap()
    }

    fn models_data() -> std::collections::HashMap<String, ModelsDevModel> {
        serde_json::from_value(serde_json::json!({
            "provider/model-one": {
                "id": "provider/model-one",
                "name": "Canonical Model One",
                "family": "family-canonical",
                "release_date": "2025-02-03",
                "cost": {"input": 9.0, "output": 10.0},
                "limit": {"context": 200000, "output": 10000},
                "temperature": true,
                "tool_call": true,
                "reasoning": true
            }
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn models_dev_sync_builds_providers_models_and_endpoints() {
        let pool = cab_db::InMemoryStore::new();
        let mut settings = cab_db::settings::default_settings();
        settings.providers.insert(
            "provider".into(),
            ProviderUserSettings {
                enabled: Some(true),
                api_key: Some("settings-key".into()),
                api_keys: Some(vec![api_key("settings-key", true)]),
                endpoints: Some(vec![endpoint("settings-anthropic", "anthropic")]),
            },
        );
        settings.models.insert(
            "Provider/Model-One".into(),
            ModelUserSettings {
                enabled: Some(true),
            },
        );
        let defaults = cab_core::ProviderDefaultsCatalog {
            providers: Default::default(),
        };
        let providers = providers_data();
        let models = models_data();

        let synced = sync_models_dev_models(
            &pool,
            &providers,
            &models,
            &settings,
            &defaults,
            None,
            &cab_core::AaModelMapFile::default(),
        )
        .await
        .unwrap();
        assert_eq!(synced, 1);
        cab_db::provider::apply_all_provider_configs(&pool)
            .await
            .unwrap();
        let endpoint_count = sync_model_endpoints(&pool, &providers, &models)
            .await
            .unwrap();
        assert_eq!(endpoint_count, 2);

        let catalog_provider = cab_db::provider::get_by_id(&pool, "provider")
            .await
            .unwrap()
            .unwrap();
        assert!(catalog_provider.enabled);
        assert_eq!(catalog_provider.api_key, "settings-key");
        assert_eq!(catalog_provider.endpoints[0].protocol, "anthropic");

        let model = cab_db::model::get_by_name(&pool, "provider/model-one")
            .await
            .unwrap()
            .unwrap();
        assert!(model.enabled);
        assert_eq!(model.display_name, "Canonical Model One");
        assert_eq!(model.provider_id, "provider");
        assert_eq!(model.protocol, "anthropic");
        assert_eq!(model.input_cost, Some(1.5));
        assert_eq!(model.output_cost, Some(2.5));
        assert_eq!(model.created, parse_release_timestamp(Some("2025-02-03")));
        assert_eq!(model.hugging_face_id.as_deref(), None);
        assert_eq!(
            model.supported_parameters.as_ref().unwrap()[0],
            "temperature"
        );
        assert_eq!(
            model.top_provider.as_ref().unwrap()["native_model_id"],
            "model-one"
        );

        let endpoints = cab_db::endpoint::list_for_model(&pool, "provider/model-one")
            .await
            .unwrap();
        assert_eq!(endpoints.len(), 2);
        assert!(
            endpoints
                .iter()
                .any(|endpoint| endpoint.provider_tag == "provider"
                    && endpoint.enabled
                    && endpoint.supports_tools
                    && endpoint.cache_read_cost == Some(0.1))
        );
        assert!(
            endpoints
                .iter()
                .any(|endpoint| endpoint.provider_tag == "reseller"
                    && !endpoint.enabled
                    && endpoint.input_cost == 3.0)
        );

        let updated = sync_models_dev_models(
            &pool,
            &providers,
            &models,
            &settings,
            &defaults,
            None,
            &cab_core::AaModelMapFile::default(),
        )
        .await
        .unwrap();
        assert_eq!(updated, 1);
    }

    #[test]
    fn provider_catalog_helpers_cover_protocols_and_metadata_json() {
        let mut providers = providers_data();
        let anthropic = providers.remove("provider").unwrap();
        let openai = providers.remove("reseller").unwrap();
        assert_eq!(protocol_for_models_dev_provider(&anthropic), "anthropic");
        assert_eq!(protocol_for_models_dev_provider(&openai), "openai-chat");

        let model = models_data().remove("provider/model-one").unwrap();
        assert_eq!(
            extract_huggingface_id(&anthropic.models["model-one"]).as_deref(),
            Some("org/model-one")
        );
        assert_eq!(
            build_pricing_json(model.cost.as_ref()).unwrap()["input"],
            9.0
        );
        assert!(build_pricing_json(None).is_none());
        assert_eq!(
            build_catalog_provider_json("provider", Some(&anthropic), "model-one")["doc"],
            "https://provider.test/docs"
        );
        assert_eq!(
            build_links_json(&model, "provider/model-one", "model-one", Some(&anthropic))["catalog_id"],
            "provider/model-one"
        );
        assert_eq!(
            supported_parameters_from_models_dev_model(&model),
            serde_json::json!(["temperature", "tools", "tool_choice", "reasoning"])
        );
        assert_eq!(parse_release_timestamp(None), None);
        assert_eq!(parse_release_timestamp(Some("bad-date")), None);
    }
}

#[cfg(test)]
mod resolve_served_model_tests {
    use super::{
        ServedModelRef, normalize_models_dev_model_key, resolve_served_model,
        served_model_matches_canonical,
    };

    fn served(provider_id: &str, native_model_id: &str) -> ServedModelRef {
        ServedModelRef {
            provider_id: provider_id.to_string(),
            native_model_id: native_model_id.to_string(),
            cost: None,
        }
    }

    #[test]
    fn prefers_vendor_gateway_when_slug_key_is_owned_by_reseller() {
        let mut lookup = std::collections::HashMap::new();
        lookup.insert(
            "deepseek-v4-pro".to_string(),
            served("alibaba-cn", "deepseek-v4-pro"),
        );
        lookup.insert(
            "deepseek/deepseek-v4-pro".to_string(),
            served("orcarouter", "deepseek/deepseek-v4-pro"),
        );
        lookup.insert(
            "deepseek/deepseek-v4-pro-vendor".to_string(),
            served("deepseek", "deepseek-v4-pro"),
        );

        let resolved = resolve_served_model(&lookup, "deepseek/deepseek-v4-pro").expect("resolved");
        assert_eq!(resolved.provider_id, "deepseek");
        assert_eq!(resolved.native_model_id, "deepseek-v4-pro");
    }

    #[test]
    fn vendor_match_uses_native_slug_mapping() {
        let served_ref = served("deepseek", "deepseek-v4-pro");
        assert!(served_model_matches_canonical(
            &served_ref,
            "deepseek",
            "deepseek-v4-pro",
            "deepseek/deepseek-v4-pro",
        ));
    }

    #[test]
    fn falls_back_to_any_gateway_when_vendor_does_not_serve_model() {
        let mut lookup = std::collections::HashMap::new();
        lookup.insert(
            "anthropic/claude-sonnet-4".to_string(),
            served("openrouter", "anthropic/claude-sonnet-4"),
        );

        let resolved =
            resolve_served_model(&lookup, "anthropic/claude-sonnet-4").expect("resolved");
        assert_eq!(resolved.provider_id, "openrouter");
    }

    #[test]
    fn normalized_keys_match_vendor_gateway() {
        let mut lookup = std::collections::HashMap::new();
        lookup.insert(
            normalize_models_dev_model_key("deepseek/deepseek-v4-pro"),
            served("deepseek", "deepseek-v4-pro"),
        );

        let resolved = resolve_served_model(&lookup, "deepseek/deepseek-v4-pro").expect("resolved");
        assert_eq!(resolved.provider_id, "deepseek");
    }
}
