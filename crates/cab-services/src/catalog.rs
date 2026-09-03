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

#[derive(Debug, Deserialize, Clone)]
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

#[derive(Debug, Deserialize, Clone)]
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
    // OpenCode Go publishes a per-model endpoint table. Prefer it over models.dev
    // npm defaults so Claude Code / Codex hit /v1/chat/completions vs /v1/messages
    // vs /v1/responses on the first hop.
    if is_opencode_go_models_dev_provider(provider)
        && let Some(protocol) = cab_core::sniff_opencode_go_protocol(&model.id)
    {
        return protocol;
    }

    // OpenAI reasoning models (GPT-5.6 Luna/Sol/Terra, o-series, etc.) require the
    // Responses API for proper reasoning effort, reasoning items, and tool-call
    // interleaving. The Chat Completions API lacks first-class reasoning support
    // for these models, producing malformed SSE (keep-alive comments, empty {})
    // when routed through an Anthropic endpoint on reseller gateways.
    if model.reasoning.unwrap_or(false) && is_openai_served_model(provider, model) {
        return "openai-responses".to_string();
    }
    if let Some(override_meta) = model.extra.get("provider")
        && let Some(npm) = override_meta.get("npm").and_then(|value| value.as_str())
    {
        return protocol_from_npm_and_api(Some(npm), provider.api.as_deref());
    }
    protocol_for_models_dev_provider(provider)
}

fn is_opencode_go_models_dev_provider(provider: &ModelsDevProvider) -> bool {
    provider.name.eq_ignore_ascii_case("OpenCode Go")
        || provider
            .api
            .as_deref()
            .is_some_and(|api| api.contains("opencode.ai/zen/go"))
}

/// Detect whether a model is served via OpenAI's native API surface — either
/// through a model-level `provider.npm` override (e.g. opencode-go reselling
/// `gpt-5.6-luna` with `{"provider": {"npm": "@ai-sdk/openai"}}`) or the
/// provider-level npm. Used to decide whether a reasoning model should use the
/// OpenAI Responses API instead of Chat Completions.
///
/// IMPORTANT: `@ai-sdk/openai-compatible` is for OpenAI-compatible APIs
/// (DeepSeek, vLLM, etc.) that only support Chat Completions — it must NOT
/// match here, otherwise non-OpenAI reasoning models get misrouted to the
/// Responses API they don't support.
fn is_openai_served_model(provider: &ModelsDevProvider, model: &ModelsDevModel) -> bool {
    if let Some(override_meta) = model.extra.get("provider")
        && let Some(npm) = override_meta.get("npm").and_then(|value| value.as_str())
    {
        let npm_lower = npm.to_ascii_lowercase();
        if npm_lower == "@ai-sdk/openai" {
            return true;
        }
    }
    let provider_npm = provider
        .npm
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    provider_npm == "@ai-sdk/openai"
}

fn protocol_from_npm_and_api(npm: Option<&str>, api: Option<&str>) -> String {
    let npm = npm.unwrap_or_default().to_ascii_lowercase();
    let api = api.unwrap_or_default().to_ascii_lowercase();
    if npm.contains("anthropic") || api.contains("/anthropic") {
        "anthropic-messages".to_string()
    } else {
        "openai-compatible".to_string()
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

/// Catalog key used when binding a models.dev provider model to a gateway.
/// Global `models` catalog rows are reference metadata; when no canonical row
/// exists the provider's own model id is authoritative for routing.
fn provider_binding_catalog_id(
    provider_id: &str,
    model: &ModelsDevModel,
    models_data: &std::collections::HashMap<String, ModelsDevModel>,
) -> String {
    resolve_canonical_model_name(provider_id, model, models_data)
        .unwrap_or_else(|| model.id.clone())
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
    // Provider bindings are the sole source of truth. Purge the obsolete
    // global snapshot so removed models cannot survive a sync.
    cab_db::model::clear_catalog_cache(pool)
        .await
        .map_err(CabError::Database)?;
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
    let mut added_count = 0usize;

    // canonical model name -> every provider that serves it (vendor + resellers)
    let mut served_by_canonical: std::collections::HashMap<String, Vec<ServedModelRef>> =
        std::collections::HashMap::new();
    let mut served_dedupe = std::collections::HashSet::new();

    for (provider_id, provider) in providers_data {
        provider_ids.insert(provider_id.clone());
        let protocol = protocol_for_models_dev_provider(provider);
        let default_endpoint = provider.api.as_deref().map(|api| (protocol.as_str(), api));

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
            Vec::new(),
        )
        .await
        .map_err(CabError::Database)?;

        cab_db::provider::apply_provider_config(pool, provider_id, settings, defaults)
            .await
            .map_err(CabError::Database)?;

        for model in provider.models.values() {
            let canonical = provider_binding_catalog_id(provider_id, model, models_data);
            if !served_dedupe.insert(format!("{canonical}::{provider_id}")) {
                continue;
            }
            served_by_canonical
                .entry(canonical)
                .or_default()
                .push(ServedModelRef {
                    provider_id: provider_id.to_string(),
                    native_model_id: model.id.clone(),
                    cost: models_dev_cost_for_provider_model(provider_id, model),
                });
        }
    }

    // Global catalog is reference metadata; provider model lists are authoritative
    // for bindings. Include provider-only models (no canonical row) in the bind pass.
    let mut binding_catalog = models_data.clone();
    for (provider_id, provider) in providers_data {
        for model in provider.models.values() {
            if resolve_canonical_model_name(provider_id, model, models_data).is_some() {
                continue;
            }
            let key = provider_binding_catalog_id(provider_id, model, models_data);
            binding_catalog.entry(key).or_insert_with(|| model.clone());
        }
    }

    // Models are bound to their provider (provider → model 1:N). Collect each
    // built model per provider, then set them back onto the provider in one pass.
    let mut bound_by_provider: std::collections::HashMap<String, Vec<cab_core::ProviderModel>> =
        std::collections::HashMap::new();
    let prior_enabled: std::collections::HashMap<String, bool> = existing_models
        .iter()
        .map(|em| (em.name.clone(), em.enabled))
        .collect();

    for (canonical_id, model) in &binding_catalog {
        let model_name = model.id.trim().to_string();
        if model_name.is_empty() {
            continue;
        }

        let provider_prefix = model_name
            .split_once('/')
            .map(|(provider, _)| provider)
            .unwrap_or("unknown");

        // Every provider that serves this canonical model gets its own binding,
        // so resellers (e.g. opencode-go) stay routable when the native vendor
        // is unavailable. Vendor gateway first, then alphabetical for determinism.
        let mut serving = served_by_canonical
            .get(canonical_id)
            .cloned()
            .unwrap_or_default();
        if serving.is_empty() {
            serving.push(ServedModelRef {
                provider_id: provider_prefix.to_string(),
                native_model_id: model_name
                    .split_once('/')
                    .map(|(_, name)| name.to_string())
                    .unwrap_or_else(|| model_name.clone()),
                cost: None,
            });
        }
        serving.sort_by(|a, b| {
            let a_native = a.provider_id == provider_prefix;
            let b_native = b.provider_id == provider_prefix;
            b_native
                .cmp(&a_native)
                .then_with(|| a.provider_id.cmp(&b.provider_id))
        });

        let display_name = model.name.clone().unwrap_or_else(|| model_name.clone());
        let context_length = model
            .limit
            .as_ref()
            .and_then(|limit| limit.context)
            .unwrap_or(0);
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
        let created = parse_release_timestamp(model.release_date.as_deref());
        let architecture = Some(build_architecture_json(model));
        let per_request_limits = model.limit.as_ref().map(|limit| {
            serde_json::json!({
                "context": limit.context,
                "output": limit.output,
            })
        });
        let hugging_face_id = extract_huggingface_id(model);
        let knowledge_cutoff = model.knowledge.clone();
        let supported_parameters = supported_parameters_from_models_dev_model(model);

        let enabled = configured_enabled
            .or_else(|| prior_enabled.get(&model_name).copied())
            .unwrap_or(false);
        let now = Utc::now().to_rfc3339();

        for served in &serving {
            let provider_id = served.provider_id.clone();
            let provider = providers_data.get(&provider_id);
            let native_model_id = served.native_model_id.clone();
            let selected_cost = served.cost.as_ref().or(model.cost.as_ref());
            let input_cost = selected_cost.and_then(|cost| cost.input);
            let output_cost = selected_cost.and_then(|cost| cost.output);
            let pricing = build_pricing_json(selected_cost);
            let top_provider = Some(build_catalog_provider_json(
                &provider_id,
                provider,
                &native_model_id,
            ));
            let links = Some(build_links_json(
                model,
                &model_name,
                &native_model_id,
                provider,
            ));
            let upstream_protocol =
                provider.map(|p| upstream_protocol_for_models_dev_model(p, model));
            // A provider's default is only a fallback.  OpenCode Go publishes
            // the wire protocol per model, so expose that same value as the
            // model default too.  Otherwise every Go binding is incorrectly
            // persisted as openai-compatible and routing has to probe endpoints.
            let protocol = upstream_protocol.clone().unwrap_or_else(|| {
                cab_db::provider::default_protocol_for_provider(&provider_id, settings, defaults)
            });
            let bound = cab_core::ProviderModel {
                model: cab_core::types::Model {
                    id: native_model_id.clone(),
                    name: model_name.clone(),
                    display_name: display_name.clone(),
                    provider_id: provider_id.clone(),
                    protocol,
                    upstream_protocol,
                    context_length,
                    input_cost,
                    output_cost,
                    enabled,
                    overall_intelligence: indices.overall_intelligence,
                    coding_index: indices.coding_index,
                    agentic_index: indices.agentic_index,
                    math_index: indices.math_index,
                    output_speed_tps: performance.output_speed_tps,
                    time_to_first_token_secs: performance.time_to_first_token_secs,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                    canonical_slug: Some(model_name.clone()),
                    hugging_face_id: hugging_face_id.clone(),
                    created,
                    description: model.family.clone(),
                    architecture: architecture.clone(),
                    pricing: pricing.clone(),
                    top_provider: top_provider.clone(),
                    per_request_limits: per_request_limits.clone(),
                    supported_parameters: Some(supported_parameters.clone()),
                    default_parameters: None,
                    supported_voices: None,
                    knowledge_cutoff: knowledge_cutoff.clone(),
                    expiration_date: None,
                    links: links.clone(),
                },
            };
            bound_by_provider
                .entry(provider_id)
                .or_default()
                .push(bound);
            added_count += 1;
        }
    }

    let mut sorted_provider_ids: Vec<&String> = provider_ids.iter().collect();
    sorted_provider_ids.sort();
    for provider_id in sorted_provider_ids {
        let bound_models = bound_by_provider.remove(provider_id).unwrap_or_default();
        cab_db::provider::set_provider_models(pool, provider_id, bound_models)
            .await
            .map_err(CabError::Database)?;
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
        "models.dev sync: added {} bound models across {} providers",
        added_count,
        provider_ids.len()
    );

    Ok(added_count)
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
    fn provider_only_model_uses_native_id_when_no_canonical_row() {
        let models_data = std::collections::HashMap::new();
        let model: ModelsDevModel =
            serde_json::from_value(serde_json::json!({ "id": "muse-spark-1.2-contributor" }))
                .unwrap();
        assert_eq!(
            super::provider_binding_catalog_id("opencode-go", &model, &models_data),
            "muse-spark-1.2-contributor"
        );
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
            "anthropic-messages"
        );
        assert_eq!(
            upstream_protocol_for_models_dev_model(&provider, &chat_model),
            "openai-compatible"
        );
    }

    #[test]
    fn openai_reasoning_model_uses_responses_protocol() {
        let provider: ModelsDevProvider = serde_json::from_value(serde_json::json!({
            "name": "OpenCode Go",
            "api": "https://opencode.ai/zen/go/v1",
            "npm": "@ai-sdk/openai-compatible",
            "models": {}
        }))
        .unwrap();
        // gpt-5.6-luna on opencode-go carries a model-level provider.npm override
        // pointing at @ai-sdk/openai AND has reasoning=true → must use Responses API.
        let reasoning_model: ModelsDevModel = serde_json::from_value(serde_json::json!({
            "id": "gpt-5.6-luna",
            "reasoning": true,
            "provider": { "npm": "@ai-sdk/openai" }
        }))
        .unwrap();
        // Non-reasoning OpenAI model stays on openai-compatible.
        let non_reasoning: ModelsDevModel = serde_json::from_value(serde_json::json!({
            "id": "gpt-4.1-mini",
            "reasoning": false,
            "provider": { "npm": "@ai-sdk/openai" }
        }))
        .unwrap();

        assert_eq!(
            upstream_protocol_for_models_dev_model(&provider, &reasoning_model),
            "openai-responses"
        );
        assert_eq!(
            upstream_protocol_for_models_dev_model(&provider, &non_reasoning),
            "openai-compatible"
        );
    }

    #[test]
    fn native_openai_provider_reasoning_model_uses_responses_protocol() {
        // The openai provider itself uses @ai-sdk/openai as its npm.
        let provider: ModelsDevProvider = serde_json::from_value(serde_json::json!({
            "name": "OpenAI",
            "api": "https://api.openai.com/v1",
            "npm": "@ai-sdk/openai",
            "models": {}
        }))
        .unwrap();
        let reasoning_model: ModelsDevModel = serde_json::from_value(serde_json::json!({
            "id": "gpt-5.6-luna",
            "reasoning": true
        }))
        .unwrap();

        assert_eq!(
            upstream_protocol_for_models_dev_model(&provider, &reasoning_model),
            "openai-responses"
        );
    }

    #[test]
    fn openai_compatible_provider_reasoning_model_stays_on_chat() {
        // DeepSeek V4 Flash is a reasoning model, but it's served via
        // @ai-sdk/openai-compatible (not @ai-sdk/openai). The Responses API
        // is OpenAI-specific — DeepSeek only supports Chat Completions.
        let provider: ModelsDevProvider = serde_json::from_value(serde_json::json!({
            "name": "OpenCode Go",
            "api": "https://opencode.ai/zen/go/v1",
            "npm": "@ai-sdk/openai-compatible",
            "models": {}
        }))
        .unwrap();
        let reasoning_model: ModelsDevModel = serde_json::from_value(serde_json::json!({
            "id": "deepseek-v4-flash",
            "reasoning": true
        }))
        .unwrap();

        assert_eq!(
            upstream_protocol_for_models_dev_model(&provider, &reasoning_model),
            "openai-compatible"
        );
    }

    #[test]
    fn opencode_go_lookup_table_overrides_npm_for_mimo_and_minimax() {
        let provider: ModelsDevProvider = serde_json::from_value(serde_json::json!({
            "name": "OpenCode Go",
            "api": "https://opencode.ai/zen/go/v1",
            "npm": "@ai-sdk/openai-compatible",
            "models": {}
        }))
        .unwrap();
        let mimo: ModelsDevModel = serde_json::from_value(serde_json::json!({
            "id": "mimo-v2.5",
            "reasoning": true
        }))
        .unwrap();
        let minimax: ModelsDevModel = serde_json::from_value(serde_json::json!({
            "id": "minimax-m3"
        }))
        .unwrap();
        let luna: ModelsDevModel = serde_json::from_value(serde_json::json!({
            "id": "gpt-5.6-luna"
        }))
        .unwrap();

        assert_eq!(
            upstream_protocol_for_models_dev_model(&provider, &mimo),
            "openai-compatible"
        );
        assert_eq!(
            upstream_protocol_for_models_dev_model(&provider, &minimax),
            "anthropic-messages"
        );
        assert_eq!(
            upstream_protocol_for_models_dev_model(&provider, &luna),
            "openai-responses"
        );
    }
}
