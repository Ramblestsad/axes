use anyhow::Context;
use sqlx::{PgPool, Postgres, Row, Transaction};
use time::OffsetDateTime;
use uuid::Uuid;

use super::{
    CreateOrderRequest, INVENTORY_RESULT_EVENT_TYPE, INVENTORY_WORKER_CONSUMER,
    InventoryResultEvent, ORDER_CREATED_EVENT_TYPE, ORDERS_WORKER_CONSUMER, OrderCreatedEvent,
    OrderStatus, apply_inventory_result, determine_inventory_result, utc_now,
};

#[derive(Debug, Clone)]
pub struct OrderRecord {
    pub id: Uuid,
    pub sku: String,
    pub quantity: i32,
    pub simulate_inventory_failure: bool,
    pub status: OrderStatus,
    pub failure_reason: Option<String>,
    pub created_at_utc: OffsetDateTime,
    pub updated_at_utc: OffsetDateTime,
}

#[derive(Debug, Clone)]
pub struct OutboxMessageRecord {
    pub id: i64,
    pub message_id: Uuid,
    pub payload: String,
}

pub async fn insert_order_with_outbox(
    pool: &PgPool,
    payload: &CreateOrderRequest,
) -> anyhow::Result<OrderRecord> {
    let mut tx = pool.begin().await?;
    let now = utc_now();
    let occurred_on_utc = now.format(&time::format_description::well_known::Rfc3339)?;
    let order_id = Uuid::new_v4();
    let event = OrderCreatedEvent {
        message_id: Uuid::new_v4(),
        correlation_id: order_id,
        order_id,
        sku: payload.sku.clone(),
        quantity: payload.quantity,
        occurred_on_utc,
    };
    let event_payload = serde_json::to_string(&event)?;

    sqlx::query(
        r#"
        INSERT INTO "orders"
            ("Id", "Sku", "Quantity", "SimulateInventoryFailure", "Status", "FailureReason", "CreatedAtUtc", "UpdatedAtUtc")
        VALUES
            ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(order_id)
    .bind(&payload.sku)
    .bind(payload.quantity)
    .bind(false)
    .bind(OrderStatus::Pending.code())
    .bind(Option::<String>::None)
    .bind(now)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    sqlx::query(
        r#"
        INSERT INTO "order_outbox_messages"
            ("MessageId", "CorrelationId", "EventType", "Payload", "OccurredOnUtc", "PublishedOnUtc", "RetryCount", "LastError")
        VALUES
            ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(event.message_id)
    .bind(event.correlation_id)
    .bind(ORDER_CREATED_EVENT_TYPE)
    .bind(event_payload)
    .bind(now)
    .bind(Option::<OffsetDateTime>::None)
    .bind(0_i32)
    .bind(Option::<String>::None)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(OrderRecord {
        id: order_id,
        sku: payload.sku.clone(),
        quantity: payload.quantity,
        simulate_inventory_failure: false,
        status: OrderStatus::Pending,
        failure_reason: None,
        created_at_utc: now,
        updated_at_utc: now,
    })
}

pub async fn get_order_by_id(pool: &PgPool, order_id: Uuid) -> anyhow::Result<Option<OrderRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            "Id" AS id,
            "Sku" AS sku,
            "Quantity" AS quantity,
            "SimulateInventoryFailure" AS simulate_inventory_failure,
            "Status" AS status,
            "FailureReason" AS failure_reason,
            "CreatedAtUtc" AS created_at_utc,
            "UpdatedAtUtc" AS updated_at_utc
        FROM "orders"
        WHERE "Id" = $1
        "#,
    )
    .bind(order_id)
    .fetch_optional(pool)
    .await?;

    row.map(map_order_row).transpose()
}

pub async fn list_unpublished_order_outbox(
    pool: &PgPool,
    limit: i64,
) -> anyhow::Result<Vec<OutboxMessageRecord>> {
    list_unpublished_outbox(pool, "order_outbox_messages", limit).await
}

pub async fn list_unpublished_inventory_outbox(
    pool: &PgPool,
    limit: i64,
) -> anyhow::Result<Vec<OutboxMessageRecord>> {
    list_unpublished_outbox(pool, "inventory_outbox_messages", limit).await
}

pub async fn mark_order_outbox_published(pool: &PgPool, id: i64) -> anyhow::Result<()> {
    mark_outbox_published(pool, "order_outbox_messages", id).await
}

pub async fn mark_inventory_outbox_published(pool: &PgPool, id: i64) -> anyhow::Result<()> {
    mark_outbox_published(pool, "inventory_outbox_messages", id).await
}

pub async fn mark_order_outbox_failed(pool: &PgPool, id: i64, error: &str) -> anyhow::Result<()> {
    mark_outbox_failed(pool, "order_outbox_messages", id, error).await
}

pub async fn mark_inventory_outbox_failed(
    pool: &PgPool,
    id: i64,
    error: &str,
) -> anyhow::Result<()> {
    mark_outbox_failed(pool, "inventory_outbox_messages", id, error).await
}

pub async fn apply_inventory_result_message(
    pool: &PgPool,
    event: &InventoryResultEvent,
) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await?;
    let inserted = insert_inbox_once(
        &mut tx,
        "order_inbox_messages",
        event.message_id,
        ORDERS_WORKER_CONSUMER,
    )
    .await?;

    if !inserted {
        tx.commit().await?;
        return Ok(false);
    }

    let applied = apply_inventory_result(event.success, event.reason.clone());
    let rows = sqlx::query(
        r#"
        UPDATE "orders"
        SET "Status" = $2, "FailureReason" = $3, "UpdatedAtUtc" = $4
        WHERE "Id" = $1
        "#,
    )
    .bind(event.order_id)
    .bind(applied.status.code())
    .bind(applied.failure_reason)
    .bind(utc_now())
    .execute(&mut *tx)
    .await?;

    anyhow::ensure!(rows.rows_affected() == 1, "order {} not found", event.order_id);

    tx.commit().await?;
    Ok(true)
}

pub async fn handle_order_created_message(
    pool: &PgPool,
    event: &OrderCreatedEvent,
) -> anyhow::Result<Option<String>> {
    let mut tx = pool.begin().await?;
    let inserted = insert_inbox_once(
        &mut tx,
        "inventory_inbox_messages",
        event.message_id,
        INVENTORY_WORKER_CONSUMER,
    )
    .await?;

    if !inserted {
        tx.commit().await?;
        return Ok(None);
    }

    let simulate_inventory_failure = load_simulate_inventory_failure(&mut tx, event.order_id)
        .await?
        .unwrap_or(false);
    let updated_rows = if simulate_inventory_failure {
        0
    } else {
        sqlx::query(
            r#"
            UPDATE "inventory_stocks"
            SET "AvailableQuantity" = "AvailableQuantity" - $2, "UpdatedAtUtc" = $3
            WHERE "Sku" = $1 AND "AvailableQuantity" >= $2
            "#,
        )
        .bind(&event.sku)
        .bind(event.quantity)
        .bind(utc_now())
        .execute(&mut *tx)
        .await?
        .rows_affected()
    };
    let outcome = determine_inventory_result(simulate_inventory_failure, updated_rows);
    let now = utc_now();
    let occurred_on_utc = now.format(&time::format_description::well_known::Rfc3339)?;
    let outbox_event = InventoryResultEvent {
        message_id: Uuid::new_v4(),
        correlation_id: event.correlation_id,
        order_id: event.order_id,
        sku: event.sku.clone(),
        quantity: event.quantity,
        success: outcome.success,
        reason: outcome.reason,
        occurred_on_utc,
    };
    let payload = serde_json::to_string(&outbox_event)?;

    sqlx::query(
        r#"
        INSERT INTO "inventory_outbox_messages"
            ("MessageId", "CorrelationId", "EventType", "Payload", "OccurredOnUtc", "PublishedOnUtc", "RetryCount", "LastError")
        VALUES
            ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(outbox_event.message_id)
    .bind(outbox_event.correlation_id)
    .bind(INVENTORY_RESULT_EVENT_TYPE)
    .bind(payload)
    .bind(now)
    .bind(Option::<OffsetDateTime>::None)
    .bind(0_i32)
    .bind(Option::<String>::None)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Some(event.sku.clone()))
}

pub async fn load_inventory_stock_quantity(
    pool: &PgPool,
    sku: &str,
) -> anyhow::Result<Option<i32>> {
    let row = sqlx::query(
        r#"
        SELECT "AvailableQuantity" AS available_quantity
        FROM "inventory_stocks"
        WHERE "Sku" = $1
        "#,
    )
    .bind(sku)
    .fetch_optional(pool)
    .await?;

    row.map(|row| row.try_get::<i32, _>("available_quantity"))
        .transpose()
        .context("failed to decode inventory stock quantity")
}

async fn insert_inbox_once(
    tx: &mut Transaction<'_, Postgres>,
    table_name: &str,
    message_id: Uuid,
    consumer: &str,
) -> anyhow::Result<bool> {
    let sql = format!(
        r#"
        INSERT INTO "{table_name}" ("MessageId", "Consumer", "ProcessedAtUtc")
        VALUES ($1, $2, $3)
        ON CONFLICT ("MessageId", "Consumer") DO NOTHING
        "#
    );
    let result = sqlx::query(&sql)
        .bind(message_id)
        .bind(consumer)
        .bind(utc_now())
        .execute(&mut **tx)
        .await?;

    Ok(result.rows_affected() == 1)
}

async fn load_simulate_inventory_failure(
    tx: &mut Transaction<'_, Postgres>,
    order_id: Uuid,
) -> anyhow::Result<Option<bool>> {
    let row =
        sqlx::query(r#"SELECT "SimulateInventoryFailure" AS value FROM "orders" WHERE "Id" = $1"#)
            .bind(order_id)
            .fetch_optional(&mut **tx)
            .await?;

    row.map(|row| row.try_get::<bool, _>("value"))
        .transpose()
        .context("failed to decode order simulate flag")
}

async fn list_unpublished_outbox(
    pool: &PgPool,
    table_name: &str,
    limit: i64,
) -> anyhow::Result<Vec<OutboxMessageRecord>> {
    let sql = format!(
        r#"
        SELECT "Id" AS id, "MessageId" AS message_id, "Payload" AS payload
        FROM "{table_name}"
        WHERE "PublishedOnUtc" IS NULL
        ORDER BY "Id"
        LIMIT $1
        "#
    );
    let rows = sqlx::query(&sql).bind(limit).fetch_all(pool).await?;

    rows.into_iter()
        .map(|row| {
            Ok(OutboxMessageRecord {
                id: row.try_get("id")?,
                message_id: row.try_get("message_id")?,
                payload: row.try_get("payload")?,
            })
        })
        .collect()
}

async fn mark_outbox_published(pool: &PgPool, table_name: &str, id: i64) -> anyhow::Result<()> {
    let sql = format!(
        r#"
        UPDATE "{table_name}"
        SET "PublishedOnUtc" = $2, "LastError" = NULL
        WHERE "Id" = $1
        "#
    );
    sqlx::query(&sql)
        .bind(id)
        .bind(utc_now())
        .execute(pool)
        .await?;
    Ok(())
}

async fn mark_outbox_failed(
    pool: &PgPool,
    table_name: &str,
    id: i64,
    error: &str,
) -> anyhow::Result<()> {
    let sql = format!(
        r#"
        UPDATE "{table_name}"
        SET "RetryCount" = "RetryCount" + 1, "LastError" = $2
        WHERE "Id" = $1
        "#
    );
    sqlx::query(&sql).bind(id).bind(error).execute(pool).await?;
    Ok(())
}

fn map_order_row(row: sqlx::postgres::PgRow) -> anyhow::Result<OrderRecord> {
    let status_code: i32 = row
        .try_get("status")
        .context("failed to decode order status")?;

    Ok(OrderRecord {
        id: row.try_get("id").context("failed to decode order id")?,
        sku: row.try_get("sku").context("failed to decode order sku")?,
        quantity: row
            .try_get("quantity")
            .context("failed to decode order quantity")?,
        simulate_inventory_failure: row
            .try_get("simulate_inventory_failure")
            .context("failed to decode order simulate flag")?,
        status: OrderStatus::try_from(status_code)?,
        failure_reason: row
            .try_get("failure_reason")
            .context("failed to decode order failure reason")?,
        created_at_utc: row
            .try_get("created_at_utc")
            .context("failed to decode order created time")?,
        updated_at_utc: row
            .try_get("updated_at_utc")
            .context("failed to decode order updated time")?,
    })
}
