use core::fmt;

use anyhow::anyhow;
use axum::{http::StatusCode, response::IntoResponse};

#[derive(Debug)]
pub struct WebError {
    code: StatusCode,
    error: anyhow::Error,
}

impl WebError {
    pub const fn new(code: StatusCode, error: anyhow::Error) -> Self {
        Self { code, error }
    }

    pub fn not_found(item: &str) -> Self {
        Self {
            code: StatusCode::NOT_FOUND,
            error: anyhow!("{item} not found"),
        }
    }

    pub fn bad_request(msg: &str) -> Self {
        Self {
            code: StatusCode::BAD_REQUEST,
            error: anyhow!("{}", msg),
        }
    }
}

impl fmt::Display for WebError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} - {}", self.code, self.error)
    }
}

impl IntoResponse for WebError {
    fn into_response(self) -> axum::response::Response {
        (self.code, format!("{}", self.error)).into_response()
    }
}

pub type Result<T> = anyhow::Result<T>;
pub type WebResult<T> = std::result::Result<T, WebError>;

impl From<anyhow::Error> for WebError {
    fn from(error: anyhow::Error) -> Self {
        Self {
            code: StatusCode::INTERNAL_SERVER_ERROR,
            error,
        }
    }
}
