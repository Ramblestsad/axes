use axum::{
    extract::{Json, OriginalUri, Path, Query},
    http::StatusCode,
    response::IntoResponse,
};
#[allow(unused_imports)]
use axum_macros::debug_handler;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    db::{ReadDbConn, WriteDbConn},
    error::AppResult,
};

#[derive(Deserialize)]
pub struct Params {
    pub page: Option<u64>,
    pub size: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct CursorParams {
    pub after: Option<i32>,
    pub size: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Bakery {
    pub id: i32,
    pub name: String,
    pub profit_margin: f64,
}

#[derive(Debug, Serialize)]
pub struct CursorPage {
    pub data: Vec<Bakery>,
    pub after: Option<i32>,
    pub size: u64,
    pub next_cursor: Option<i32>,
    pub has_more: bool,
}

pub async fn list(
    ReadDbConn(mut conn): ReadDbConn,
    Query(params): Query<Params>,
    OriginalUri(uri): OriginalUri,
) -> AppResult<impl IntoResponse> {
    let page = params.page.unwrap_or(1);
    let size = params.size.unwrap_or(10);
    let offset = ((page - 1) * size) as i64;

    let total = sqlx::query_scalar!(r#"SELECT COUNT(*) as"total!" FROM bakery"#)
        .fetch_one(&mut *conn)
        .await?;

    let pages = if total == 0 { 1 } else { (total as u64).div_ceil(size) };

    let bakeries = sqlx::query_as!(
        Bakery,
        r#"SELECT id, name, profit_margin FROM bakery ORDER BY id LIMIT $1 OFFSET $2"#,
        size as i64,
        offset
    )
    .fetch_all(&mut *conn)
    .await?;

    let path = uri.path();
    let previous_page =
        if page > 1 { Some(format!("{}?page={}&size={}", path, page - 1, size)) } else { None };

    let next_page =
        if page < pages { Some(format!("{}?page={}&size={}", path, page + 1, size)) } else { None };

    let first_page = format!("{}?page=1&size={}", path, size);
    let last_page = format!("{}?page={}&size={}", path, pages, size);

    Ok((
        StatusCode::OK,
        Json(json!({
            "data": bakeries,
            "current_page": page,
            "pages": pages,
            "size": size,
            "previous_page": previous_page,
            "next_page": next_page,
            "first_page": first_page,
            "last_page": last_page
        })),
    ))
}

pub async fn list_by_cursor(
    ReadDbConn(mut conn): ReadDbConn,
    Query(params): Query<CursorParams>,
) -> AppResult<impl IntoResponse> {
    let size = sanitized_cursor_page_size(params.size);
    let limit = size.saturating_add(1).min(i64::MAX as u64) as i64;
    let bakeries = sqlx::query_as!(
        Bakery,
        r#"
        SELECT id, name, profit_margin
        FROM bakery
        WHERE ($1::INT4 IS NULL OR id > $1)
        ORDER BY id
        LIMIT $2
        "#,
        params.after,
        limit
    )
    .fetch_all(&mut *conn)
    .await?;

    Ok((StatusCode::OK, Json(build_cursor_page(bakeries, params.after, size))))
}

pub async fn detail(
    ReadDbConn(mut conn): ReadDbConn,
    Path(bid): Path<i32>,
) -> AppResult<impl IntoResponse> {
    let bakery =
        sqlx::query_as!(Bakery, r#"SELECT id, name, profit_margin FROM bakery WHERE id = $1"#, bid)
            .fetch_optional(&mut *conn)
            .await?;

    match bakery {
        Some(b) => Ok((StatusCode::OK, Json(b))),
        None => Ok((
            StatusCode::NOT_FOUND,
            Json(Bakery { id: 0, name: "Not found".to_string(), profit_margin: 0.0 }),
        )),
    }
}

pub async fn create(
    WriteDbConn(mut conn): WriteDbConn,
    Json(payload): Json<CreateDto>,
) -> AppResult<impl IntoResponse> {
    sqlx::query!(
        r#"INSERT INTO bakery (name, profit_margin) VALUES ($1, $2)"#,
        payload.name,
        payload.profit_margin
    )
    .execute(&mut *conn)
    .await?;

    Ok(StatusCode::CREATED)
}

#[derive(Deserialize)]
pub struct CreateDto {
    name: String,
    profit_margin: f64,
}

pub async fn update(
    WriteDbConn(mut conn): WriteDbConn,
    Json(payload): Json<UpdateDto>,
) -> AppResult<impl IntoResponse> {
    sqlx::query!(
        r#"UPDATE bakery SET name = $1, profit_margin = $2 WHERE id = $3"#,
        payload.name,
        payload.profit_margin,
        payload.id
    )
    .execute(&mut *conn)
    .await?;

    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub struct UpdateDto {
    id: i32,
    name: String,
    profit_margin: f64,
}

pub async fn delete(
    WriteDbConn(mut conn): WriteDbConn,
    Json(payload): Json<DeleteDto>,
) -> AppResult<impl IntoResponse> {
    sqlx::query!(r#"DELETE FROM bakery WHERE id = $1"#, payload.id)
        .execute(&mut *conn)
        .await?;

    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub struct DeleteDto {
    id: i32,
}

pub(crate) fn sanitized_cursor_page_size(size: Option<u64>) -> u64 {
    match size {
        Some(0) | None => 10,
        Some(size) => size,
    }
}

pub(crate) fn build_cursor_page(
    mut bakeries: Vec<Bakery>,
    after: Option<i32>,
    size: u64,
) -> CursorPage {
    // Fetch one extra row so the handler can report whether another page exists.
    let page_size = usize::try_from(size).unwrap_or(usize::MAX);
    let has_more = bakeries.len() > page_size;
    if has_more {
        bakeries.truncate(page_size);
    }

    let next_cursor = if has_more { bakeries.last().map(|bakery| bakery.id) } else { None };

    CursorPage { data: bakeries, after, size, next_cursor, has_more }
}
