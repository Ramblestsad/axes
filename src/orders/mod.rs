use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::AppError;

pub mod store;
pub mod worker;

pub const ORDER_CREATED_EVENT_TYPE: &str = "OrderCreated";
pub const INVENTORY_RESULT_EVENT_TYPE: &str = "InventoryResult";
pub const ORDERS_WORKER_CONSUMER: &str = "axes-orders-worker";
pub const INVENTORY_WORKER_CONSUMER: &str = "axes-inventory-worker";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Pending,
    Confirmed,
    Rejected,
}

impl OrderStatus {
    pub fn code(self) -> i32 {
        match self {
            Self::Pending => 0,
            Self::Confirmed => 1,
            Self::Rejected => 2,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::Confirmed => "Confirmed",
            Self::Rejected => "Rejected",
        }
    }
}

impl TryFrom<i32> for OrderStatus {
    type Error = AppError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Pending),
            1 => Ok(Self::Confirmed),
            2 => Ok(Self::Rejected),
            _ => Err(AppError::new("Invalid order status")
                .with_status(StatusCode::INTERNAL_SERVER_ERROR)),
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct CreateOrderRequest {
    pub sku: String,
    pub quantity: i32,
}

impl CreateOrderRequest {
    pub fn validate(self) -> Result<Self, AppError> {
        if self.sku.trim().is_empty() {
            return Err(AppError::new("Sku is required").with_status(StatusCode::BAD_REQUEST));
        }

        if self.quantity <= 0 {
            return Err(AppError::new("Quantity must be greater than zero")
                .with_status(StatusCode::BAD_REQUEST));
        }

        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedisPrecheckOutcome {
    Known { available: i32 },
    Missing,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrecheckDecision {
    Allow,
    Reject { status: StatusCode, reason: String },
}

pub fn redis_stock_key(sku: &str) -> String {
    format!("demo:stock:{sku}")
}

pub fn decide_order_creation(precheck: RedisPrecheckOutcome, quantity: i32) -> PrecheckDecision {
    match precheck {
        RedisPrecheckOutcome::Known { available } if available < quantity => {
            PrecheckDecision::Reject {
                status: StatusCode::CONFLICT,
                reason: "insufficient_stock".to_string(),
            }
        }
        RedisPrecheckOutcome::Known { .. }
        | RedisPrecheckOutcome::Missing
        | RedisPrecheckOutcome::Unavailable => PrecheckDecision::Allow,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedInventoryResult {
    pub status: OrderStatus,
    pub failure_reason: Option<String>,
}

pub fn apply_inventory_result(success: bool, reason: Option<String>) -> AppliedInventoryResult {
    if success {
        AppliedInventoryResult { status: OrderStatus::Confirmed, failure_reason: None }
    } else {
        AppliedInventoryResult {
            status: OrderStatus::Rejected,
            failure_reason: reason.or_else(|| Some("insufficient_stock".to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrderCreatedEvent {
    pub message_id: Uuid,
    pub correlation_id: Uuid,
    pub order_id: Uuid,
    pub sku: String,
    pub quantity: i32,
    pub occurred_on_utc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InventoryResultEvent {
    pub message_id: Uuid,
    pub correlation_id: Uuid,
    pub order_id: Uuid,
    pub sku: String,
    pub quantity: i32,
    pub success: bool,
    pub reason: Option<String>,
    pub occurred_on_utc: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KafkaSettings {
    pub brokers: String,
    pub order_created_topic: String,
    pub inventory_result_topic: String,
}

impl KafkaSettings {
    pub fn from_env() -> Self {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_map(values: &[(&str, &str)]) -> Self {
        Self::from_lookup(|key| {
            values
                .iter()
                .find(|(candidate, _)| *candidate == key)
                .map(|(_, value)| (*value).to_string())
        })
    }

    fn from_lookup<F>(lookup: F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        Self {
            brokers: lookup("AXES_KAFKA_BROKERS").unwrap_or_else(|| "localhost:9092".to_string()),
            order_created_topic: lookup("AXES_KAFKA_ORDER_CREATED_TOPIC")
                .unwrap_or_else(|| "orders.created.v1".to_string()),
            inventory_result_topic: lookup("AXES_KAFKA_INVENTORY_RESULT_TOPIC")
                .unwrap_or_else(|| "inventory.result.v1".to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryProcessingOutcome {
    pub success: bool,
    pub reason: Option<String>,
}

pub fn determine_inventory_result(
    simulate_inventory_failure: bool,
    updated_rows: u64,
) -> InventoryProcessingOutcome {
    if simulate_inventory_failure {
        return InventoryProcessingOutcome {
            success: false,
            reason: Some("simulated_inventory_failure".to_string()),
        };
    }

    if updated_rows == 1 {
        InventoryProcessingOutcome { success: true, reason: None }
    } else {
        InventoryProcessingOutcome {
            success: false,
            reason: Some("insufficient_stock".to_string()),
        }
    }
}

pub fn utc_now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}
