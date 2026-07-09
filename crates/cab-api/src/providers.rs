use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use cab_core::CabError;
use cab_core::types::{CreateProvider, UpdateProvider};
use serde::Deserialize;

use crate::ApiState;

pub use cab_services::catalog::{
    auto_seed_known_models, sync_models_dev_catalog, sync_models_internal,
};

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

/// Reject endpoint URLs that the gateway would later proxy to with stored
/// provider credentials. Only `http`/`https` are allowed (blocking `file://`,
/// `gopher://`, and other SSRF-prone schemes). Private/loopback hosts are
/// intentionally permitted because CAB legitimately supports self-hosted and
/// LAN model servers.
fn validate_endpoint_urls(endpoints: &[cab_core::types::ProviderEndpoint]) -> Result<(), CabError> {
    for ep in endpoints {
        let url = ep.url.trim();
        if url.is_empty() {
            continue;
        }
        let lower = url.to_ascii_lowercase();
        if !(lower.starts_with("http://") || lower.starts_with("https://")) {
            return Err(CabError::InvalidRequest(format!(
                "Endpoint URL must use http or https: {url}"
            )));
        }
        if url.chars().any(|c| c.is_whitespace()) {
            return Err(CabError::InvalidRequest(
                "Endpoint URL must not contain whitespace.".to_string(),
            ));
        }
    }
    Ok(())
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

    if let Some(ref endpoints) = input.endpoints {
        validate_endpoint_urls(endpoints)?;
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
        logo: input.logo,
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
        provider.logo.clone(),
    );
    cab_db::settings::set_provider_override(&state.pool, &id, user_settings)
        .await
        .map_err(CabError::Database)?;

    if input.enabled.is_some() {
        cab_db::endpoint::set_provider_tag_enabled(&state.pool, &id, enabled)
            .await
            .map_err(CabError::Database)?;
    }

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
    cab_db::endpoint::set_provider_enabled(&state.pool, &provider_name, input.enabled)
        .await
        .map_err(CabError::Database)?;

    Ok(Json(serde_json::json!({
        "success": true,
        "updated": (),
    })))
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
    use cab_services::catalog::{
        ModelsDevModel, build_catalog_provider_json, build_links_json, build_pricing_json,
        extract_huggingface_id, protocol_for_models_dev_provider,
        supported_parameters_from_models_dev_model, sync_models_dev_models,
    };

    fn api_key(key: &str, enabled: bool) -> ApiKeyConfig {
        ApiKeyConfig {
            key: key.into(),
            enabled,
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
                    upstream_protocol: None,
                    quantization: "unknown".into(),
                    input_cost: Some(1.0),
                    output_cost: Some(2.0),
                    cache_read_cost: None,
                    context_length: Some(128000),
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
        let _home = crate::TestHome::new().await;
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

    fn providers_data()
    -> std::collections::HashMap<String, cab_services::catalog::ModelsDevProvider> {
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
        let _home = crate::TestHome::new().await;
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
        let endpoint_count =
            cab_services::catalog::sync_model_endpoints(&pool, &providers, &models)
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
        assert_eq!(
            model.created,
            cab_services::catalog::parse_release_timestamp(Some("2025-02-03"))
        );
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
                    && endpoint.input_cost == Some(3.0))
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
        assert_eq!(cab_services::catalog::parse_release_timestamp(None), None);
        assert_eq!(
            cab_services::catalog::parse_release_timestamp(Some("bad-date")),
            None
        );
    }
}

#[cfg(test)]
mod resolve_served_model_tests {
    use cab_services::catalog::{
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
