//! CAB combined HTTP application (gateway + management API).

use axum::Router;
use cab_api::api_router;
use cab_db::InMemoryStore;
use cab_gateway::{GatewayState, gateway_router};

/// Build the same router stack used in production (without static frontend assets).
pub fn build_combined_router(store: InMemoryStore) -> Router {
    let gateway = gateway_router(GatewayState::new(store.clone()));
    let api = api_router(store);
    gateway.merge(api)
}
