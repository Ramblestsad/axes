use anyhow::anyhow;
use axum::extract::{Json, Path, Query};
use axum::http::StatusCode;
use axum::response::IntoResponse;
#[allow(unused_imports)]
use axum_macros::debug_handler;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::route::DbConn;
use crate::error::AppResult;

#[derive(Deserialize)]
pub struct Params {
    pub page: Option<u64>,
    pub size: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Bakery {
    pub id: i32,
    pub name: String,
    pub profit_margin: f64,
}

pub async fn list(
    DbConn(mut conn): DbConn,
    Query(params): Query<Params>,
) -> AppResult<impl IntoResponse> {
    let page = params.page.unwrap_or(1);
    let size = params.size.unwrap_or(10);
    let offset = ((page - 1) * size) as i64;

    let total = sqlx::query_scalar!(r#"SELECT COUNT(*) FROM bakery"#)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| anyhow!(e))?
        .unwrap_or(0);

    let pages = (total as u64 + size - 1) / size;

    let bakeries = sqlx::query_as!(
        Bakery,
        r#"SELECT id, name, profit_margin FROM bakery ORDER BY id LIMIT $1 OFFSET $2"#,
        size as i64,
        offset
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| anyhow!(e))?;

    Ok((
        StatusCode::OK,
        Json(json!({"data": bakeries, "current_page": page, "pages": pages, "size": size})),
    ))
}

pub async fn detail(
    DbConn(mut conn): DbConn,
    Path(bid): Path<i32>,
) -> AppResult<impl IntoResponse> {
    let bakery = sqlx::query_as!(
        Bakery,
        r#"SELECT id, name, profit_margin FROM bakery WHERE id = $1"#,
        bid
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| anyhow!(e))?;

    match bakery {
        Some(b) => Ok((StatusCode::OK, Json(b))),
        None => Ok((StatusCode::NOT_FOUND, Json(Bakery { id: 0, name: "Not found".to_string(), profit_margin: 0.0 }))),
    }
}

pub async fn create(
    DbConn(mut conn): DbConn,
    Json(payload): Json<CreateDto>,
) -> AppResult<impl IntoResponse> {
    sqlx::query!(
        r#"INSERT INTO bakery (name, profit_margin) VALUES ($1, $2)"#,
        payload.name,
        payload.profit_margin
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| anyhow!(e))?;

    Ok(StatusCode::CREATED)
}

#[derive(Deserialize)]
pub struct CreateDto {
    name: String,
    profit_margin: f64,
}

pub async fn update(
    DbConn(mut conn): DbConn,
    Json(payload): Json<UpdateDto>,
) -> AppResult<impl IntoResponse> {
    sqlx::query!(
        r#"UPDATE bakery SET name = $1, profit_margin = $2 WHERE id = $3"#,
        payload.name,
        payload.profit_margin,
        payload.id
    )
    .execute(&mut *conn)
    .await
    .map_err(|e| anyhow!(e))?;

    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub struct UpdateDto {
    id: i32,
    name: String,
    profit_margin: f64,
}

pub async fn delete(
    DbConn(mut conn): DbConn,
    Json(payload): Json<DeleteDto>,
) -> AppResult<impl IntoResponse> {
    sqlx::query!(r#"DELETE FROM bakery WHERE id = $1"#, payload.id)
        .execute(&mut *conn)
        .await
        .map_err(|e| anyhow!(e))?;

    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub struct DeleteDto {
    id: i32,
}
