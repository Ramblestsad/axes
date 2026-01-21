#![allow(unused_variables)]

use std::sync::Arc;

use axum::extract::{Path, State};

use crate::error::AppResult;
use crate::routes::AppState;

pub async fn list(State(state): State<Arc<AppState>>) -> AppResult<String> {
    Ok("Users List".into())
}

pub async fn detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i32>,
) -> AppResult<String> {
    Ok(format!("Hello, user-{}", id))
}
