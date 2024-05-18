use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

pub type AppResult<T> = Result<T, RespError>;

pub struct RespError(anyhow::Error);

impl From<anyhow::Error> for RespError {
    fn from(error: anyhow::Error) -> Self {
        Self(error)
    }
}

impl IntoResponse for RespError {
    fn into_response(self) -> Response {
        let status_code = if let Some(error) = self.0.downcast_ref::<AuthError>() {
            match error {
                AuthError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid token"),
                AuthError::MissingCredential => (StatusCode::BAD_REQUEST, "Missing credential"),
                AuthError::TokenCreation => {
                    (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create token")
                }
                AuthError::WrongCredential => (StatusCode::UNAUTHORIZED, "Wrong credentials"),
                AuthError::UserDoesNotExist => (StatusCode::UNAUTHORIZED, "User does not exist"),
                AuthError::UserAlreadyExits => (StatusCode::BAD_REQUEST, "User already exists"),
            }
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Error! Errors other than AuthError haven't been implemented yet, please check the code to see what",
            )
        };
        (status_code.0, Json(json!({ "error": status_code.1 }))).into_response()
    }
}

#[derive(Debug, thiserror::Error)]
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
