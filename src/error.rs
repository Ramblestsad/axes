use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::Value;

pub type AppResult<T> = Result<T, ApiError>;

pub struct ApiError(anyhow::Error);

impl From<AuthError> for ApiError {
    fn from(error: AuthError) -> Self {
        Self(anyhow::Error::new(error))
    }
}

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        Self(anyhow::Error::new(error))
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self(error)
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        Self(anyhow::Error::new(error))
    }
}

impl From<jsonwebtoken::errors::Error> for ApiError {
    fn from(error: jsonwebtoken::errors::Error) -> Self {
        Self(anyhow::Error::new(error))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let err = self.0;

        let final_error =  match err.downcast::<AuthError>() {
            Ok(auth) => match auth {
                AuthError::InvalidToken => AppError::new("Invalid token").with_status(StatusCode::UNAUTHORIZED),
                AuthError::MissingCredential => AppError::new("Missing credential").with_status(StatusCode::BAD_REQUEST),
                AuthError::TokenCreation => AppError::new("Failed to create token").with_status(StatusCode::INTERNAL_SERVER_ERROR),
                AuthError::WrongCredential => AppError::new("Wrong credentials").with_status(StatusCode::UNAUTHORIZED),
                AuthError::UserDoesNotExist => AppError::new("User does not exist").with_status(StatusCode::UNAUTHORIZED),
                AuthError::UserAlreadyExits => AppError::new("User already exists").with_status(StatusCode::BAD_REQUEST),
            },
            Err(err) => match err.downcast::<AppError>() {
                Ok(app) => app,
                Err(_) => AppError::new("Internal server error")
                    .with_status(StatusCode::INTERNAL_SERVER_ERROR),

            }
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

#[derive(Debug, thiserror::Error, Serialize)]
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
    fn into_response(self) -> Response {
        let status = self.status;
        let mut res = axum::Json(self).into_response();
        *res.status_mut() = status;
        res
    }
}
