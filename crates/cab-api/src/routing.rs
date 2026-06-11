use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;
use cab_core::CabError;
use cab_core::types::RouteExplainRequest;

use crate::ApiState;

pub async fn explain_routing(
    State(state): State<ApiState>,
    Json(request): Json<RouteExplainRequest>,
) -> Result<impl IntoResponse, CabError> {
    let result = cab_services::route_explainer::explain(&state.pool, &request).await;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cab_core::types::{ApiKeyConfig, Model, Provider, ProviderEndpoint, RouteExplainResult};

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
                subscribed: false,
                quota_reset_at: None,
            }],
            api: None,
            doc: None,
            env: None,
            npm: None,
            model_count: 1,
            catalog_models: vec![],
        }
    }

    fn model() -> Model {
        Model {
            id: "model-1".into(),
            name: "provider/model-1".into(),
            display_name: "Model One".into(),
            provider_id: "provider-1".into(),
            protocol: "openai-chat".into(),
            context_length: 128000,
            input_cost: Some(1.0),
            output_cost: Some(2.0),
            enabled: true,
            overall_intelligence: 80.0,
            coding_index: 85.0,
            agentic_index: 80.0,
            math_index: 75.0,
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

    #[tokio::test]
    async fn explain_routing_returns_steps_and_candidates() {
        let pool = cab_db::InMemoryStore::new();
        {
            let mut data = pool.inner.write().unwrap();
            data.providers.insert("provider-1".into(), provider());
            data.models.insert("model-1".into(), model());
        }

        let state = ApiState { pool };
        let response = explain_routing(
            axum::extract::State(state),
            axum::Json(RouteExplainRequest {
                agent: "codex".into(),
                model: Some("auto".into()),
                body: Some(serde_json::json!({
                    "messages": [{"role": "user", "content": "hello"}]
                })),
            }),
        )
        .await
        .unwrap();
        let body = axum::body::to_bytes(response.into_response().into_body(), usize::MAX)
            .await
            .unwrap();
        let result: RouteExplainResult = serde_json::from_slice(&body).unwrap();
        assert!(!result.decision_steps.is_empty());
        assert!(result.resolved.is_some());
        assert!(!result.ranked_candidates.is_empty());
    }
}
