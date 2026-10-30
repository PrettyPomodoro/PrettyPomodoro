//! One error type for every handler, rendered as `{"error": …, "code": …}`.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug)]
pub enum ApiError {
    /// The request itself is wrong: missing field, malformed date, weak password.
    BadRequest(String),
    /// No session, an expired one, or the wrong password.
    Unauthorized(String),
    /// Already taken: an email that has signed up before.
    Conflict(String),
    NotFound(String),
    /// A sign-in provider this deployment has no credentials for.
    NotConfigured(String),
    /// Our fault, or the identity provider's.
    Internal(String),
}

impl ApiError {
    /// Hide the detail of internal failures from the client but keep it in the log.
    fn parts(&self) -> (StatusCode, &'static str, String) {
        match self {
            Self::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad_request", m.clone()),
            Self::Unauthorized(m) => (StatusCode::UNAUTHORIZED, "unauthorized", m.clone()),
            Self::Conflict(m) => (StatusCode::CONFLICT, "conflict", m.clone()),
            Self::NotFound(m) => (StatusCode::NOT_FOUND, "not_found", m.clone()),
            Self::NotConfigured(m) => (StatusCode::SERVICE_UNAVAILABLE, "not_configured", m.clone()),
            Self::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "The server could not complete that. Try again.".to_owned(),
            ),
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadRequest(m)
            | Self::Unauthorized(m)
            | Self::Conflict(m)
            | Self::NotFound(m)
            | Self::NotConfigured(m)
            | Self::Internal(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if let Self::Internal(detail) = &self {
            tracing::error!("{detail}");
        }
        let (status, code, message) = self.parts();
        (status, Json(json!({ "error": message, "code": code }))).into_response()
    }
}

impl From<rusqlite::Error> for ApiError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Internal(format!("database: {e}"))
    }
}

impl From<r2d2::Error> for ApiError {
    fn from(e: r2d2::Error) -> Self {
        Self::Internal(format!("database pool: {e}"))
    }
}
