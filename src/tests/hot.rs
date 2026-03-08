use axum::http::StatusCode;
use serde_json::json;

use crate::handlers::stat::hot::{
    ClaimStockRequest, ClaimStockResult, HotTopQuery, IncrHotRequest, build_hot_top_items,
    claim_status_code, redis_hot_key, redis_stock_key, redis_stock_users_key,
};

#[test]
fn incr_hot_request_uses_defaults() {
    let payload: IncrHotRequest =
        serde_json::from_value(json!({})).expect("incr hot request should deserialize");

    assert_eq!(payload.delta, 1.0);
    assert_eq!(payload.ttl_seconds, 604_800);
}

#[test]
fn hot_top_query_uses_default_limit() {
    let query: HotTopQuery =
        serde_json::from_value(json!({})).expect("hot top query should deserialize");

    assert_eq!(query.limit, 10);
}

#[test]
fn claim_status_code_matches_result_shape() {
    assert_eq!(
        claim_status_code(&ClaimStockResult {
            applied: true,
            duplicate: false,
            remaining_stock: 3,
        }),
        StatusCode::OK
    );
    assert_eq!(
        claim_status_code(&ClaimStockResult {
            applied: false,
            duplicate: true,
            remaining_stock: 3,
        }),
        StatusCode::CONFLICT
    );
    assert_eq!(
        claim_status_code(&ClaimStockResult {
            applied: false,
            duplicate: false,
            remaining_stock: 0,
        }),
        StatusCode::BAD_REQUEST
    );
}

#[test]
fn redis_keys_follow_demo_naming() {
    assert_eq!(redis_hot_key(), "demo:hot:zset");
    assert_eq!(redis_stock_key(42), "demo:stock:42");
    assert_eq!(redis_stock_users_key(42), "demo:stock:users:42");
}

#[test]
fn build_hot_top_items_parses_numeric_members() {
    let items = build_hot_top_items(vec![("2".to_string(), 8.5), ("7".to_string(), 3.0)])
        .expect("top items should be parsed");

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].item_id, 2);
    assert_eq!(items[0].score, 8.5);
    assert_eq!(items[1].item_id, 7);
    assert_eq!(items[1].score, 3.0);
}

#[test]
fn build_hot_top_items_rejects_non_numeric_members() {
    let error = build_hot_top_items(vec![("bad-id".to_string(), 1.0)]).unwrap_err();

    assert_eq!(error.error, "Invalid hot leaderboard member");
    assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn claim_stock_request_accepts_optional_initial_stock() {
    let payload: ClaimStockRequest = serde_json::from_value(json!({
        "user_key": "user-1",
        "initial_stock": 5
    }))
    .expect("claim stock request should deserialize");

    assert_eq!(payload.user_key, "user-1");
    assert_eq!(payload.initial_stock, Some(5));
}
