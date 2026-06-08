use axum::{
    Router,
    routing::{get, post},
};
use std::sync::Arc;

use crate::state::GatewayState;
use crate::{anthropic, cloudcode, gemini, openai};

/// Build the gateway router mounted at `/v1`, `/v1beta`, and Cloud Code proxy paths.
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
        .route(
            "/v1beta/models/{*model_action}",
            post(gemini::handle_model_action),
        )
        .route("/{*rpc}", post(cloudcode::handle_v1internal))
        .with_state(shared)
}
