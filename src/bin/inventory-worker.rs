use std::{sync::Arc, time::Duration};

use anyhow::Context;
use axes::{
    config::AppConfig,
    orders::{
        INVENTORY_WORKER_CONSUMER, KafkaSettings, redis_stock_key,
        store::{
            handle_order_created_message, list_unpublished_inventory_outbox,
            load_inventory_stock_quantity, mark_inventory_outbox_failed,
            mark_inventory_outbox_published,
        },
        worker::{build_consumer, build_producer, decode_event, publish_outbox_loop},
    },
    utils::{gracefully_shutdown::shutdown_token, observability},
};
use rdkafka::consumer::{CommitMode, Consumer, StreamConsumer};
use redis::AsyncCommands;
use sqlx::postgres::PgPoolOptions;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let observability = observability::init_observability();
    let config = AppConfig::new().context("failed to load app config")?;
    // Worker keeps using the primary because outbox/inbox processing needs strong consistency.
    let pg_url = config
        .pg
        .required_write_url()
        .context("Postgres write URL not found, check settings.")?;
    let redis_url = config
        .redis
        .url
        .context("Redis URL not found, check settings.")?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(pg_url)
        .await
        .context("failed to connect postgres for inventory worker")?;
    let pool = Arc::new(pool);
    let redis_client =
        Arc::new(redis::Client::open(redis_url).context("failed to create redis client")?);
    let kafka = KafkaSettings::from_env();
    let producer = build_producer(&kafka)?;
    let consumer = build_consumer(&kafka, INVENTORY_WORKER_CONSUMER, &kafka.order_created_topic)?;
    let token = shutdown_token();

    info!("inventory worker started");

    tokio::try_join!(
        publish_inventory_outbox_loop(
            pool.clone(),
            producer,
            kafka.inventory_result_topic.clone(),
            token.clone(),
        ),
        consume_order_created_loop(pool, redis_client, consumer, token),
    )?;

    observability.shutdown()?;
    Ok(())
}

async fn publish_inventory_outbox_loop(
    pool: Arc<sqlx::PgPool>,
    producer: rdkafka::producer::FutureProducer,
    topic: String,
    token: tokio_util::sync::CancellationToken,
) -> anyhow::Result<()> {
    publish_outbox_loop(
        producer,
        topic,
        token,
        || {
            let pool = pool.clone();
            async move { list_unpublished_inventory_outbox(&pool, 50).await }
        },
        |id| {
            let pool = pool.clone();
            async move { mark_inventory_outbox_published(&pool, id).await }
        },
        |id, error| {
            let pool = pool.clone();
            async move { mark_inventory_outbox_failed(&pool, id, &error).await }
        },
    )
    .await
}

async fn consume_order_created_loop(
    pool: Arc<sqlx::PgPool>,
    redis_client: Arc<redis::Client>,
    consumer: StreamConsumer,
    token: tokio_util::sync::CancellationToken,
) -> anyhow::Result<()> {
    loop {
        tokio::select! {
            _ = token.cancelled() => return Ok(()),
            message = consumer.recv() => {
                let message = match message {
                    Ok(message) => message,
                    Err(error) => {
                        warn!(error = %error, "failed to receive order created message");
                        continue;
                    }
                };
                let Some(event) = decode_event(&message, "order_created") else {
                    consumer.commit_message(&message, CommitMode::Async)?;
                    continue;
                };

                if let Some(sku) = handle_order_created_message(&pool, &event).await? {
                    refresh_redis_stock(&pool, &redis_client, &sku).await;
                }

                consumer.commit_message(&message, CommitMode::Async)?;
            }
        }
    }
}

async fn refresh_redis_stock(pool: &sqlx::PgPool, redis_client: &redis::Client, sku: &str) {
    let quantity = match load_inventory_stock_quantity(pool, sku).await {
        Ok(quantity) => quantity,
        Err(error) => {
            warn!(error = %error, sku, "failed to load inventory stock for redis refresh");
            return;
        }
    };

    let mut conn = match redis_client.get_multiplexed_async_connection().await {
        Ok(conn) => conn,
        Err(error) => {
            warn!(error = %error, sku, "failed to get redis connection for stock refresh");
            return;
        }
    };

    let key = redis_stock_key(sku);
    let result = match quantity {
        Some(quantity) => conn.set::<_, _, ()>(&key, quantity).await,
        None => conn.del::<_, ()>(&key).await,
    };

    if let Err(error) = result {
        warn!(error = %error, sku, "failed to refresh redis stock cache");
    }
}
