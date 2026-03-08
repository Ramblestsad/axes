use std::sync::Arc;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use redis::AsyncCommands;
use serde::Serialize;
use time::format_description::well_known::Rfc3339;
use tracing::warn;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    orders::{
        CreateOrderRequest, PrecheckDecision, RedisPrecheckOutcome, decide_order_creation,
        redis_stock_key,
        store::{OrderRecord, get_order_by_id, insert_order_with_outbox},
    },
    route::AppState,
};

#[derive(Debug, Serialize)]
pub struct OrderStatusView {
    pub code: i32,
    pub label: String,
}

#[derive(Debug, Serialize)]
pub struct OrderResponse {
    pub id: Uuid,
    pub sku: String,
    pub quantity: i32,
    pub status: OrderStatusView,
    pub failure_reason: Option<String>,
    pub created_at_utc: String,
    pub updated_at_utc: String,
}

pub async fn create(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateOrderRequest>,
) -> AppResult<impl IntoResponse> {
    let payload = payload.validate()?;
    match redis_precheck(&state, &payload.sku, payload.quantity).await {
        PrecheckDecision::Allow => {}
        PrecheckDecision::Reject { status, reason } => {
            return Err(AppError::new("Insufficient stock")
                .with_status(status)
                .with_details(serde_json::json!({ "reason": reason }))
                .into());
        }
    }

    tracing::info!(db_role = "write", "handling order write request");
    let order = insert_order_with_outbox(&state.write_pool, &payload).await?;
    Ok((StatusCode::CREATED, Json(to_order_response(order)?)))
}

pub async fn detail(
    State(state): State<Arc<AppState>>,
    Path(order_id): Path<Uuid>,
) -> AppResult<impl IntoResponse> {
    tracing::info!(db_role = "read", "handling order read request");
    let order = get_order_by_id(&state.read_pool, order_id).await?;
    let order =
        order.ok_or_else(|| AppError::new("Order not found").with_status(StatusCode::NOT_FOUND))?;

    Ok((StatusCode::OK, Json(to_order_response(order)?)))
}

fn to_order_response(order: OrderRecord) -> AppResult<OrderResponse> {
    let created_at_utc = order
        .created_at_utc
        .format(&Rfc3339)
        .map_err(anyhow::Error::from)?;
    let updated_at_utc = order
        .updated_at_utc
        .format(&Rfc3339)
        .map_err(anyhow::Error::from)?;

    Ok(OrderResponse {
        id: order.id,
        sku: order.sku,
        quantity: order.quantity,
        status: OrderStatusView {
            code: order.status.code(),
            label: order.status.as_str().to_string(),
        },
        failure_reason: order.failure_reason,
        created_at_utc,
        updated_at_utc,
    })
}

async fn redis_precheck(state: &Arc<AppState>, sku: &str, quantity: i32) -> PrecheckDecision {
    let outcome = match state.redis_client.get_multiplexed_async_connection().await {
        Ok(mut conn) => match conn.get::<_, Option<i32>>(redis_stock_key(sku)).await {
            Ok(Some(available)) => RedisPrecheckOutcome::Known { available },
            Ok(None) => RedisPrecheckOutcome::Missing,
            Err(error) => {
                warn!(error = %error, sku, "redis stock precheck failed");
                RedisPrecheckOutcome::Unavailable
            }
        },
        Err(error) => {
            warn!(error = %error, sku, "redis connection for stock precheck failed");
            RedisPrecheckOutcome::Unavailable
        }
    };

    decide_order_creation(outcome, quantity)
}
