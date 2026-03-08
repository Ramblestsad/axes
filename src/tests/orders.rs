use axum::http::StatusCode;
use serde_json::json;
use uuid::Uuid;

use crate::orders::{
    CreateOrderRequest, InventoryProcessingOutcome, InventoryResultEvent, KafkaSettings,
    OrderCreatedEvent, OrderStatus, PrecheckDecision, RedisPrecheckOutcome, apply_inventory_result,
    decide_order_creation, determine_inventory_result, redis_stock_key,
};

#[test]
fn order_status_maps_codes_and_labels() {
    assert_eq!(OrderStatus::Pending.code(), 0);
    assert_eq!(OrderStatus::Pending.as_str(), "Pending");
    assert_eq!(OrderStatus::Confirmed.code(), 1);
    assert_eq!(OrderStatus::Confirmed.as_str(), "Confirmed");
    assert_eq!(OrderStatus::Rejected.code(), 2);
    assert_eq!(OrderStatus::Rejected.as_str(), "Rejected");
    assert_eq!(OrderStatus::try_from(0).expect("0 is valid"), OrderStatus::Pending);
    assert_eq!(OrderStatus::try_from(1).expect("1 is valid"), OrderStatus::Confirmed);
    assert_eq!(OrderStatus::try_from(2).expect("2 is valid"), OrderStatus::Rejected);
    assert!(OrderStatus::try_from(9).is_err());
}

#[test]
fn order_request_requires_positive_quantity() {
    let payload: CreateOrderRequest = serde_json::from_value(json!({
        "sku": "sku-1",
        "quantity": 3
    }))
    .expect("payload should deserialize");
    let payload = payload.validate().expect("payload should validate");

    assert_eq!(payload.sku, "sku-1");
    assert_eq!(payload.quantity, 3);

    let error = CreateOrderRequest { sku: "sku-1".to_string(), quantity: 0 }
        .validate()
        .expect_err("zero should be rejected");
    assert_eq!(error.error, "Quantity must be greater than zero");
    assert_eq!(error.status, StatusCode::BAD_REQUEST);
}

#[test]
fn redis_keys_follow_orders_naming() {
    assert_eq!(redis_stock_key("sku-42"), "demo:stock:sku-42");
}

#[test]
fn redis_precheck_rejects_when_cached_stock_is_insufficient() {
    let decision = decide_order_creation(RedisPrecheckOutcome::Known { available: 1 }, 2);

    assert_eq!(
        decision,
        PrecheckDecision::Reject {
            status: StatusCode::CONFLICT,
            reason: "insufficient_stock".to_string(),
        }
    );
}

#[test]
fn redis_precheck_allows_when_cache_missing_or_unavailable() {
    assert_eq!(decide_order_creation(RedisPrecheckOutcome::Missing, 5), PrecheckDecision::Allow);
    assert_eq!(
        decide_order_creation(RedisPrecheckOutcome::Unavailable, 5),
        PrecheckDecision::Allow
    );
}

#[test]
fn inventory_result_updates_order_status_and_reason() {
    let confirmed = apply_inventory_result(true, Some("ignored".to_string()));
    assert_eq!(confirmed.status, OrderStatus::Confirmed);
    assert_eq!(confirmed.failure_reason, None);

    let rejected = apply_inventory_result(false, None);
    assert_eq!(rejected.status, OrderStatus::Rejected);
    assert_eq!(rejected.failure_reason.as_deref(), Some("insufficient_stock"));
}

#[test]
fn kafka_events_round_trip_through_json() {
    let correlation_id = Uuid::new_v4();
    let order_id = Uuid::new_v4();
    let order_created = OrderCreatedEvent {
        message_id: Uuid::new_v4(),
        correlation_id,
        order_id,
        sku: "sku-1".to_string(),
        quantity: 2,
        occurred_on_utc: "2026-03-08T00:00:00Z".to_string(),
    };
    let inventory_result = InventoryResultEvent {
        message_id: Uuid::new_v4(),
        correlation_id,
        order_id,
        sku: "sku-1".to_string(),
        quantity: 2,
        success: false,
        reason: Some("insufficient_stock".to_string()),
        occurred_on_utc: "2026-03-08T00:00:01Z".to_string(),
    };

    let order_created_json =
        serde_json::to_value(&order_created).expect("order created event should serialize");
    let inventory_result_json =
        serde_json::to_value(&inventory_result).expect("inventory result should serialize");

    assert_eq!(
        serde_json::from_value::<OrderCreatedEvent>(order_created_json)
            .expect("order created event should deserialize"),
        order_created
    );
    assert_eq!(
        serde_json::from_value::<InventoryResultEvent>(inventory_result_json)
            .expect("inventory result should deserialize"),
        inventory_result
    );
}

#[test]
fn kafka_settings_use_expected_defaults() {
    let settings = KafkaSettings::from_map(&[]);

    assert_eq!(settings.brokers, "localhost:9092");
    assert_eq!(settings.order_created_topic, "orders.created.v1");
    assert_eq!(settings.inventory_result_topic, "inventory.result.v1");
}

#[test]
fn kafka_settings_allow_overrides() {
    let settings = KafkaSettings::from_map(&[
        ("AXES_KAFKA_BROKERS", "kafka-1:9092"),
        ("AXES_KAFKA_ORDER_CREATED_TOPIC", "orders.created.custom"),
    ]);

    assert_eq!(settings.brokers, "kafka-1:9092");
    assert_eq!(settings.order_created_topic, "orders.created.custom");
    assert_eq!(settings.inventory_result_topic, "inventory.result.v1");
}

#[test]
fn inventory_processing_outcome_matches_failure_and_stock_cases() {
    assert_eq!(
        determine_inventory_result(true, 1),
        InventoryProcessingOutcome {
            success: false,
            reason: Some("simulated_inventory_failure".to_string()),
        }
    );
    assert_eq!(
        determine_inventory_result(false, 1),
        InventoryProcessingOutcome { success: true, reason: None }
    );
    assert_eq!(
        determine_inventory_result(false, 0),
        InventoryProcessingOutcome {
            success: false,
            reason: Some("insufficient_stock".to_string()),
        }
    );
}
