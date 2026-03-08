use std::{future::Future, time::Duration};

use anyhow::Context;
use rdkafka::{
    ClientConfig, Message,
    consumer::{Consumer, StreamConsumer},
    producer::{FutureProducer, FutureRecord},
};
use tokio::time::{MissedTickBehavior, interval};
use tracing::{error, warn};

use super::{KafkaSettings, store::OutboxMessageRecord};

pub fn build_producer(kafka: &KafkaSettings) -> anyhow::Result<FutureProducer> {
    ClientConfig::new()
        .set("bootstrap.servers", &kafka.brokers)
        .set("message.timeout.ms", "5000")
        .create()
        .context("failed to build kafka producer")
}

pub fn build_consumer(
    kafka: &KafkaSettings,
    group_id: &str,
    topic: &str,
) -> anyhow::Result<StreamConsumer> {
    let consumer: StreamConsumer = ClientConfig::new()
        .set("group.id", group_id)
        .set("bootstrap.servers", &kafka.brokers)
        .set("enable.auto.commit", "false")
        .set("auto.offset.reset", "earliest")
        .create()
        .context("failed to build kafka consumer")?;
    consumer
        .subscribe(&[topic])
        .context("failed to subscribe kafka topic")?;
    Ok(consumer)
}

pub async fn publish_outbox_loop<ListFuture, ListFn, OkFuture, OkFn, ErrFuture, ErrFn>(
    producer: FutureProducer,
    topic: String,
    token: tokio_util::sync::CancellationToken,
    list_messages: ListFn,
    mark_published: OkFn,
    mark_failed: ErrFn,
) -> anyhow::Result<()>
where
    ListFn: Fn() -> ListFuture,
    ListFuture: Future<Output = anyhow::Result<Vec<OutboxMessageRecord>>>,
    OkFn: Fn(i64) -> OkFuture,
    OkFuture: Future<Output = anyhow::Result<()>>,
    ErrFn: Fn(i64, String) -> ErrFuture,
    ErrFuture: Future<Output = anyhow::Result<()>>,
{
    let mut ticker = interval(Duration::from_millis(500));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = token.cancelled() => return Ok(()),
            _ = ticker.tick() => {
                let messages = list_messages().await?;
                for message in messages {
                    let key = message.message_id.to_string();
                    match producer
                        .send(
                            FutureRecord::to(&topic).payload(&message.payload).key(&key),
                            Duration::from_secs(5),
                        )
                        .await
                    {
                        Ok(_) => mark_published(message.id).await?,
                        Err((error, _)) => {
                            warn!(error = %error, outbox_id = message.id, "failed to publish outbox message");
                            mark_failed(message.id, error.to_string()).await?;
                        }
                    }
                }
            }
        }
    }
}

pub fn decode_event<T>(message: &rdkafka::message::BorrowedMessage<'_>, label: &str) -> Option<T>
where
    T: serde::de::DeserializeOwned,
{
    let payload = match message.payload_view::<str>() {
        Some(Ok(payload)) => payload,
        Some(Err(error)) => {
            error!(error = %error, %label, "kafka payload is not valid utf8");
            return None;
        }
        None => {
            warn!(%label, "kafka payload missing");
            return None;
        }
    };

    match serde_json::from_str(payload) {
        Ok(event) => Some(event),
        Err(error) => {
            error!(error = %error, %label, "failed to deserialize kafka event");
            None
        }
    }
}
