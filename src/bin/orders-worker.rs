use std::{sync::Arc, time::Duration};

use anyhow::Context;
use axes::{
    config::AppConfig,
    orders::{
        KafkaSettings, ORDERS_WORKER_CONSUMER,
        store::{
            apply_inventory_result_message, list_unpublished_order_outbox,
            mark_order_outbox_failed, mark_order_outbox_published,
        },
        worker::{build_consumer, build_producer, decode_event, publish_outbox_loop},
    },
    utils::{gracefully_shutdown::shutdown_token, observability},
};
use rdkafka::consumer::{CommitMode, Consumer, StreamConsumer};
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
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(pg_url)
        .await
        .context("failed to connect postgres for orders worker")?;
    let pool = Arc::new(pool);
    let kafka = KafkaSettings::from_env();
    let producer = build_producer(&kafka)?;
    let consumer = build_consumer(&kafka, ORDERS_WORKER_CONSUMER, &kafka.inventory_result_topic)?;
    let token = shutdown_token();

    info!("orders worker started");

    tokio::try_join!(
        publish_order_outbox_loop(
            pool.clone(),
            producer,
            kafka.order_created_topic.clone(),
            token.clone(),
        ),
        consume_inventory_results_loop(pool, consumer, token),
    )?;

    observability.shutdown()?;
    Ok(())
}

async fn publish_order_outbox_loop(
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
            async move { list_unpublished_order_outbox(&pool, 50).await }
        },
        |id| {
            let pool = pool.clone();
            async move { mark_order_outbox_published(&pool, id).await }
        },
        |id, error| {
            let pool = pool.clone();
            async move { mark_order_outbox_failed(&pool, id, &error).await }
        },
    )
    .await
}

async fn consume_inventory_results_loop(
    pool: Arc<sqlx::PgPool>,
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
                        warn!(error = %error, "failed to receive inventory result message");
                        continue;
                    }
                };
                let Some(event) = decode_event(&message, "inventory_result") else {
                    consumer.commit_message(&message, CommitMode::Async)?;
                    continue;
                };

                apply_inventory_result_message(&pool, &event).await?;
                consumer.commit_message(&message, CommitMode::Async)?;
            }
        }
    }
}
