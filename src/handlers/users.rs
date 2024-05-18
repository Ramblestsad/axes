use axum::extract::Path;

use crate::error::AppResult;

pub async fn users_list() -> AppResult<String> {
    Ok("Users List".into())
}

pub async fn user_by_id(Path(id): Path<i32>) -> AppResult<String> {
    Ok(format!("Hello, user-{}", id))
}
