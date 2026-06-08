use reqwest::Client;
use std::sync::Arc;

/// Shared state for all gateway handlers.
#[derive(Clone)]
pub struct GatewayState {
    pub pool: cab_db::InMemoryStore,
    pub client: Client,
}

impl GatewayState {
    pub fn new(pool: cab_db::InMemoryStore) -> Self {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .expect("Failed to create HTTP client");
        Self { pool, client }
    }
}

/// Wrap in Arc for Axum State extractor.
pub type SharedState = Arc<GatewayState>;
