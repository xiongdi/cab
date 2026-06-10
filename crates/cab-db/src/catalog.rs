use std::collections::HashSet;

use async_trait::async_trait;
use cab_core::CabError;
use cab_core::types::{Agent, Model, Provider, Route};

use crate::InMemoryStore;

#[async_trait]
pub trait RouteCatalog: Send + Sync {
    async fn agent(&self, id: &str) -> Result<Option<Agent>, CabError>;
    async fn routes_for_agent(&self, agent: &str) -> Result<Vec<Route>, CabError>;
    async fn route_by_id(&self, id: &str) -> Result<Option<Route>, CabError>;
    async fn enabled_models(&self) -> Result<Vec<Model>, CabError>;
    async fn model_by_id(&self, id: &str) -> Result<Option<Model>, CabError>;
    async fn model_by_name(&self, name: &str) -> Result<Option<Model>, CabError>;
    async fn provider_by_id(&self, id: &str) -> Result<Option<Provider>, CabError>;
    async fn list_catalog_providers(&self) -> Result<Vec<Provider>, CabError>;
    async fn enabled_provider_tags_for_model(
        &self,
        model_name: &str,
    ) -> Result<Vec<String>, CabError>;
}

#[async_trait]
impl RouteCatalog for InMemoryStore {
    async fn agent(&self, id: &str) -> Result<Option<Agent>, CabError> {
        crate::agent::get_by_id(self, id)
            .await
            .map_err(CabError::Database)
    }

    async fn routes_for_agent(&self, agent: &str) -> Result<Vec<Route>, CabError> {
        crate::route::find_for_agent(self, agent)
            .await
            .map_err(CabError::Database)
    }

    async fn route_by_id(&self, id: &str) -> Result<Option<Route>, CabError> {
        crate::route::get_by_id(self, id)
            .await
            .map_err(CabError::Database)
    }

    async fn enabled_models(&self) -> Result<Vec<Model>, CabError> {
        let all_models = crate::model::list(self).await.map_err(CabError::Database)?;
        let all_providers = crate::provider::list(self)
            .await
            .map_err(CabError::Database)?;
        let active_provider_ids: HashSet<String> = all_providers
            .into_iter()
            .filter(|p| p.enabled && (!p.api_key.is_empty() || p.id == "provider-ollama"))
            .map(|p| p.id)
            .collect();

        Ok(all_models
            .into_iter()
            .filter(|m| {
                m.enabled
                    && active_provider_ids.contains(&m.provider_id)
                    && m.input_cost.unwrap_or(0.0) >= 0.0
                    && m.output_cost.unwrap_or(0.0) >= 0.0
            })
            .collect())
    }

    async fn model_by_id(&self, id: &str) -> Result<Option<Model>, CabError> {
        crate::model::get_by_id(self, id)
            .await
            .map_err(CabError::Database)
    }

    async fn model_by_name(&self, name: &str) -> Result<Option<Model>, CabError> {
        crate::model::get_by_name(self, name)
            .await
            .map_err(CabError::Database)
    }

    async fn provider_by_id(&self, id: &str) -> Result<Option<Provider>, CabError> {
        crate::provider::get_by_id(self, id)
            .await
            .map_err(CabError::Database)
    }

    async fn list_catalog_providers(&self) -> Result<Vec<Provider>, CabError> {
        crate::provider::list_catalog(self)
            .await
            .map_err(CabError::Database)
    }

    async fn enabled_provider_tags_for_model(
        &self,
        model_name: &str,
    ) -> Result<Vec<String>, CabError> {
        crate::endpoint::enabled_provider_tags_for_model(self, model_name)
            .await
            .map_err(CabError::Database)
    }
}
