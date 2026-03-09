use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::Value;
use tracing::error;

pub type AppResult<T> = Result<T, AppError>;

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

#[derive(Debug, thiserror::Error, Serialize)]
#[error("{error}")]
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
        Self { error: error.to_string(), status: StatusCode::BAD_REQUEST, error_details: None }
    }

    pub fn internal(error: &str) -> Self {
        Self::new(error).with_status(StatusCode::INTERNAL_SERVER_ERROR)
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

impl From<AuthError> for AppError {
    fn from(error: AuthError) -> Self {
        match error {
            AuthError::InvalidToken => {
                AppError::new("Invalid token").with_status(StatusCode::UNAUTHORIZED)
            }
            AuthError::MissingCredential => {
                AppError::new("Missing credential").with_status(StatusCode::BAD_REQUEST)
            }
            AuthError::TokenCreation => AppError::internal("Failed to create token"),
            AuthError::WrongCredential => {
                AppError::new("Wrong credentials").with_status(StatusCode::UNAUTHORIZED)
            }
            AuthError::UserDoesNotExist => {
                AppError::new("User does not exist").with_status(StatusCode::UNAUTHORIZED)
            }
            AuthError::UserAlreadyExits => {
                AppError::new("User already exists").with_status(StatusCode::BAD_REQUEST)
            }
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(error: anyhow::Error) -> Self {
        error!(error = ?error, "application error");
        AppError::internal("Internal server error")
    }
}

impl From<sqlx::Error> for AppError {
    fn from(error: sqlx::Error) -> Self {
        error!(error = %error, "database error");
        AppError::internal("Internal server error")
    }
}

impl From<jsonwebtoken::errors::Error> for AppError {
    fn from(error: jsonwebtoken::errors::Error) -> Self {
        error!(error = %error, "jwt error");
        AppError::internal("Internal server error")
    }
}

impl From<redis::RedisError> for AppError {
    fn from(error: redis::RedisError) -> Self {
        error!(error = %error, "redis error");
        AppError::internal("Internal server error")
    }
}

impl From<time::error::Format> for AppError {
    fn from(error: time::error::Format) -> Self {
        error!(error = %error, "time format error");
        AppError::internal("Internal server error")
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status;
        let mut res = axum::Json(self).into_response();
        *res.status_mut() = status;
        res
    }
}
