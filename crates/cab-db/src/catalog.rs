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
    async fn list_routable_entries(
        &self,
    ) -> Result<Vec<crate::routability::RoutableModelEntry>, CabError>;
    fn is_provider_healthy(&self, _provider_id: &str) -> bool {
        true
    }
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
        let routable = crate::routability::list_routable_models(self)
            .await
            .map_err(CabError::Database)?;

        Ok(routable
            .into_iter()
            .filter(|m| {
                matches!(
                    (m.input_cost, m.output_cost),
                    (Some(i), Some(o)) if i >= 0.0 && o >= 0.0
                )
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

    async fn list_routable_entries(
        &self,
    ) -> Result<Vec<crate::routability::RoutableModelEntry>, CabError> {
        crate::routability::list_routable_model_entries(self)
            .await
            .map_err(CabError::Database)
    }

    fn is_provider_healthy(&self, provider_id: &str) -> bool {
        self.health.is_healthy(provider_id)
    }
}
