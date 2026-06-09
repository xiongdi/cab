use axum::{
    Router,
    routing::{get, post},
};
use std::sync::Arc;

use crate::state::GatewayState;
use crate::{anthropic, openai};

/// Build the gateway router mounted at `/v1`.
pub fn gateway_router(state: GatewayState) -> Router {
    let shared = Arc::new(state);

    Router::new()
        .route(
            "/v1/chat/completions",
            post(openai::handle_chat_completions),
        )
        .route("/v1/responses", post(openai::handle_responses))
        .route("/v1/messages", post(anthropic::handle_messages))
        .route("/v1/models", get(openai::handle_list_models))
        .with_state(shared)
}
