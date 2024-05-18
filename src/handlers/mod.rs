use axum::http::StatusCode;

pub mod auth;
pub mod users;

pub async fn index() -> &'static str {
    "Index from Axum"
}

pub async fn global_404() -> (StatusCode, &'static str) {
    (StatusCode::NOT_FOUND, "Nothing flourished here.")
}
