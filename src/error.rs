use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug)]
pub enum PylonError {
    ProxyNotFound(String),
    ConfigLoadError(String),
    ConfigSaveError(String),
    UpstreamError(String),
    StreamError(String),
    InvalidRequest(String),
    Unauthorized,
    Forbidden,
    InternalError(String),
}

impl IntoResponse for PylonError {
    fn into_response(self) -> Response {
        let (status, error_type, message) = match self {
            PylonError::ProxyNotFound(model) => (
                StatusCode::NOT_FOUND,
                "proxy_not_found",
                format!("No proxy configuration found for model '{}'", model),
            ),
            PylonError::ConfigLoadError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "config_load_error",
                format!("Failed to load configuration: {}", e),
            ),
            PylonError::ConfigSaveError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "config_save_error",
                format!("Failed to save configuration: {}", e),
            ),
            PylonError::UpstreamError(e) => (
                StatusCode::BAD_GATEWAY,
                "upstream_error",
                format!("Upstream request failed: {}", e),
            ),
            PylonError::StreamError(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "stream_error",
                format!("Stream processing error: {}", e),
            ),
            PylonError::InvalidRequest(e) => (StatusCode::BAD_REQUEST, "invalid_request", e),
            PylonError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Authentication required".to_string(),
            ),
            PylonError::Forbidden => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "Admin access required".to_string(),
            ),
            PylonError::InternalError(e) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", e)
            }
        };

        let body = Json(json!({
            "error": {
                "type": error_type,
                "message": message,
                "code": error_type.to_uppercase(),
            }
        }));

        (status, body).into_response()
    }
}

impl From<std::io::Error> for PylonError {
    fn from(e: std::io::Error) -> Self {
        PylonError::ConfigLoadError(e.to_string())
    }
}

impl From<serde_json::Error> for PylonError {
    fn from(e: serde_json::Error) -> Self {
        PylonError::ConfigLoadError(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, PylonError>;
