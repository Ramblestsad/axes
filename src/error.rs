use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::Value;
use anyhow::Result;

pub type AppResult<T> = Result<T, RespError>;

pub struct RespError(anyhow::Error);

impl From<anyhow::Error> for RespError {
    fn from(error: anyhow::Error) -> Self {
        Self(error)
    }
}

impl IntoResponse for RespError {
    fn into_response(self) -> Response {
        let final_error = if let Some(error) = self.0.downcast_ref::<AuthError>() {
            match error {
                AuthError::InvalidToken => AppError::new("Invalid token").with_status(StatusCode::UNAUTHORIZED),
                AuthError::MissingCredential => AppError::new("Missing credential").with_status(StatusCode::BAD_REQUEST),
                AuthError::TokenCreation => AppError::new("Failed to create token").with_status(StatusCode::INTERNAL_SERVER_ERROR),
                AuthError::WrongCredential => AppError::new("Wrong credentials").with_status(StatusCode::UNAUTHORIZED),
                AuthError::UserDoesNotExist => AppError::new("User does not exist").with_status(StatusCode::UNAUTHORIZED),
                AuthError::UserAlreadyExits => AppError::new("User already exists").with_status(StatusCode::BAD_REQUEST),
            }
        } else if let Some(error) = self.0.downcast_ref::<AppError>() {
            error.clone()
        } else {
            AppError::new("Internal server error").with_status(StatusCode::INTERNAL_SERVER_ERROR)
        };

        final_error.into_response()
    }
}

#[derive(Debug, thiserror::Error, Serialize)]
pub enum AuthError {
    #[error("Invalid token")]
    InvalidToken,
    #[error("Wrong credentials")]
    WrongCredential,
    #[error("Missing credential")]
    MissingCredential,
    #[error("Failed to create token")]
    TokenCreation,
    #[error("User does not exist")]
    UserDoesNotExist,
    #[error("User already exists")]
    UserAlreadyExits,
}

#[derive(Debug, Clone, thiserror::Error, Serialize)]
#[error("App Errors")]
pub struct AppError {
    /// An error message.
    pub error: String,
    #[serde(skip)]
    pub status: StatusCode,
    /// Optional Additional error details.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_details: Option<Value>,
}

impl AppError {
    pub fn new(error: &str) -> Self {
        Self {
            error: error.to_string(),
            status: StatusCode::BAD_REQUEST,
            error_details: None,
        }
    }

    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }

    pub fn with_details(mut self, details: Value) -> Self {
        self.error_details = Some(details);
        self
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let status = self.status;
        let mut res = axum::Json(self).into_response();
        *res.status_mut() = status;
        res
    }
}
