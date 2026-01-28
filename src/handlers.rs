use axum::{
    http::StatusCode,
    response::IntoResponse,
};

pub mod auth;
pub mod bakery;
pub mod users;

pub async fn index() -> impl IntoResponse {
    "Index from Axum"
}

pub async fn global_404() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "Nothing flourished here.")
}
