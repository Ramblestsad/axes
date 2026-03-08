use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::error;

use crate::{
    error::{ApiError, AppError, AppResult},
    routes::AppState,
};

const HOT_KEY: &str = "demo:hot:zset";
const DEFAULT_HOT_DELTA: f64 = 1.0;
const DEFAULT_HOT_TTL_SECONDS: u64 = 604_800;
const DEFAULT_TOP_LIMIT: usize = 10;
const CLAIM_STOCK_SCRIPT: &str = r#"
local stockKey = KEYS[1]
local usersKey = KEYS[2]
local userKey = ARGV[1]
local initialStock = tonumber(ARGV[2])

local currentStock = redis.call('GET', stockKey)
if not currentStock then
    if initialStock ~= nil and initialStock >= 0 then
        redis.call('SET', stockKey, initialStock)
        currentStock = tostring(initialStock)
    else
        return {0, -1}
    end
end

if redis.call('SISMEMBER', usersKey, userKey) == 1 then
    return {2, tonumber(currentStock)}
end

local stock = tonumber(redis.call('GET', stockKey) or '-1')
if stock <= 0 then
    return {0, stock}
end

local newStock = redis.call('DECR', stockKey)
redis.call('SADD', usersKey, userKey)
return {1, newStock}
"#;

#[derive(Debug, Deserialize)]
pub struct IncrHotRequest {
    #[serde(default = "default_hot_delta")]
    pub delta: f64,
    #[serde(default = "default_hot_ttl_seconds")]
    pub ttl_seconds: u64,
}

#[derive(Debug, Deserialize)]
pub struct HotTopQuery {
    #[serde(default = "default_top_limit")]
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct ClaimStockRequest {
    pub user_key: String,
    pub initial_stock: Option<i64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct HotScoreItem {
    pub item_id: i64,
    pub score: f64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ClaimStockResult {
    pub applied: bool,
    pub duplicate: bool,
    pub remaining_stock: i64,
}

pub async fn incr_hot_score(
    State(state): State<Arc<AppState>>,
    Path(item_id): Path<i64>,
    Json(payload): Json<IncrHotRequest>,
) -> AppResult<impl IntoResponse> {
    let mut conn = state
        .redis_client
        .get_multiplexed_async_connection()
        .await
        .map_err(redis_command_error)?;

    let score: f64 = redis::cmd("ZINCRBY")
        .arg(redis_hot_key())
        .arg(payload.delta)
        .arg(item_id)
        .query_async(&mut conn)
        .await
        .map_err(redis_command_error)?;

    let _: () = redis::cmd("EXPIRE")
        .arg(redis_hot_key())
        .arg(payload.ttl_seconds)
        .query_async(&mut conn)
        .await
        .map_err(redis_command_error)?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "item_id": item_id,
            "delta": payload.delta,
            "score": score,
            "ttl_seconds": payload.ttl_seconds,
        })),
    ))
}

pub async fn hot_top(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HotTopQuery>,
) -> AppResult<impl IntoResponse> {
    let mut conn = state
        .redis_client
        .get_multiplexed_async_connection()
        .await
        .map_err(redis_command_error)?;

    let limit = sanitized_top_limit(query.limit);
    let stop = limit.saturating_sub(1) as isize;
    let entries: Vec<(String, f64)> = redis::cmd("ZREVRANGE")
        .arg(redis_hot_key())
        .arg(0)
        .arg(stop)
        .arg("WITHSCORES")
        .query_async(&mut conn)
        .await
        .map_err(redis_command_error)?;
    let items = build_hot_top_items(entries).map_err(ApiError::from)?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "limit": limit,
            "items": items,
        })),
    ))
}

pub async fn claim_stock(
    State(state): State<Arc<AppState>>,
    Path(item_id): Path<i64>,
    Json(payload): Json<ClaimStockRequest>,
) -> AppResult<impl IntoResponse> {
    let mut conn = state
        .redis_client
        .get_multiplexed_async_connection()
        .await
        .map_err(redis_command_error)?;

    let result: (i64, i64) = redis::cmd("EVAL")
        .arg(CLAIM_STOCK_SCRIPT)
        .arg(2)
        .arg(redis_stock_key(item_id))
        .arg(redis_stock_users_key(item_id))
        .arg(&payload.user_key)
        .arg(payload.initial_stock.unwrap_or(-1))
        .query_async(&mut conn)
        .await
        .map_err(redis_command_error)?;

    let response = ClaimStockResult {
        applied: result.0 == 1,
        duplicate: result.0 == 2,
        remaining_stock: result.1,
    };
    let status = claim_status_code(&response);

    Ok((
        status,
        Json(json!({
            "item_id": item_id,
            "user_key": payload.user_key,
            "applied": response.applied,
            "duplicate": response.duplicate,
            "remaining_stock": response.remaining_stock,
        })),
    ))
}

pub(crate) fn default_hot_delta() -> f64 {
    DEFAULT_HOT_DELTA
}

pub(crate) fn default_hot_ttl_seconds() -> u64 {
    DEFAULT_HOT_TTL_SECONDS
}

pub(crate) fn default_top_limit() -> usize {
    DEFAULT_TOP_LIMIT
}

pub(crate) fn sanitized_top_limit(limit: usize) -> usize {
    if limit == 0 { DEFAULT_TOP_LIMIT } else { limit }
}

pub(crate) fn redis_hot_key() -> &'static str {
    HOT_KEY
}

pub(crate) fn redis_stock_key(item_id: i64) -> String {
    format!("demo:stock:{item_id}")
}

pub(crate) fn redis_stock_users_key(item_id: i64) -> String {
    format!("demo:stock:users:{item_id}")
}

pub(crate) fn claim_status_code(result: &ClaimStockResult) -> StatusCode {
    if result.applied {
        StatusCode::OK
    } else if result.duplicate {
        StatusCode::CONFLICT
    } else {
        StatusCode::BAD_REQUEST
    }
}

pub(crate) fn build_hot_top_items(
    entries: Vec<(String, f64)>,
) -> Result<Vec<HotScoreItem>, AppError> {
    entries
        .into_iter()
        .map(|(item_id, score)| {
            item_id
                .parse::<i64>()
                .map(|item_id| HotScoreItem { item_id, score })
                .map_err(|_| {
                    AppError::new("Invalid hot leaderboard member")
                        .with_status(StatusCode::INTERNAL_SERVER_ERROR)
                })
        })
        .collect()
}

fn redis_command_error(error: redis::RedisError) -> ApiError {
    error!(error = %error, "redis command failed");
    ApiError::from(
        AppError::new("Redis command failed").with_status(StatusCode::INTERNAL_SERVER_ERROR),
    )
}
