use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum CabError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Store error: {0}")]
    Database(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Proxy error: {0}")]
    Proxy(String),

    #[error("Provider error (status {status}): {body}")]
    ProviderError { status: u16, body: String },

    #[error("Invalid request: {0}")]
    InvalidRequest(String),
}

impl IntoResponse for CabError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            CabError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            CabError::Database(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            CabError::Config(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            CabError::Proxy(msg) => (StatusCode::BAD_GATEWAY, msg.clone()),
            CabError::ProviderError { status, body } => {
                let code = StatusCode::from_u16(*status).unwrap_or(StatusCode::BAD_GATEWAY);
                (code, body.clone())
            }
            CabError::InvalidRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
        };

        let body = json!({
            "error": {
                "message": message,
                "type": format!("{:?}", status),
            }
        });

        (status, axum::Json(body)).into_response()
    }
}

/// Convenience Result alias used across the crate ecosystem.
pub type CabResult<T> = Result<T, CabError>;
