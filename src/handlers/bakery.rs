use std::sync::Arc;

use anyhow::anyhow;
use axum::extract::{Json, Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
#[allow(unused_imports)]
use axum_macros::debug_handler;
use sea_orm::*;
use serde::Deserialize;
use serde_json::json;

use crate::entity::{prelude::*, *};
use crate::error::AppResult;
use crate::route::AppState;

pub async fn list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<Params>,
) -> AppResult<impl IntoResponse> {
    let page = params.page.unwrap_or(1);
    let size = params.size.unwrap_or(10);

    let paginator = Bakery::find().paginate(&state.conn, size);
    let num_pages = paginator.num_pages().await.map_err(|e| anyhow!(e))?;
    let (bakeries, pages) = paginator
        .fetch_page(page - 1)
        .await
        .map(|p| (p, num_pages))
        .map_err(|e| anyhow!(e))?;

    Ok((
        StatusCode::OK,
        Json(json!({"data": bakeries,"cuurent_page": page, "pages": pages, "size": size})),
    ))
}

pub async fn detail(
    State(state): State<Arc<AppState>>,
    Path(bid): Path<i32>,
) -> AppResult<impl IntoResponse> {
    let bakery = Bakery::find_by_id(bid)
        .one(&state.conn)
        .await
        .map_err(|e| anyhow!(e))?;

    Ok((StatusCode::OK, Json(bakery)))
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateDto>,
) -> AppResult<impl IntoResponse> {
    let happy_bakery = bakery::ActiveModel {
        name: ActiveValue::Set(payload.name),
        profit_margin: ActiveValue::Set(payload.profit_margin),
        ..Default::default()
    };
    let _res = Bakery::insert(happy_bakery)
        .exec(&state.conn)
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
    State(state): State<Arc<AppState>>,
    Json(payload): Json<UpdateDto>,
) -> AppResult<impl IntoResponse> {
    let sad_bakery = bakery::ActiveModel {
        id: ActiveValue::Set(payload.id),
        name: ActiveValue::Set(payload.name),
        profit_margin: ActiveValue::Set(payload.profit_margin),
    };
    sad_bakery
        .update(&state.conn)
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
    State(state): State<Arc<AppState>>,
    Json(payload): Json<DeleteDto>,
) -> AppResult<impl IntoResponse> {
    let to_del = bakery::ActiveModel { id: ActiveValue::Set(payload.id), ..Default::default() };
    to_del.delete(&state.conn).await.map_err(|e| anyhow!(e))?;

    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
pub struct DeleteDto {
    id: i32,
}
