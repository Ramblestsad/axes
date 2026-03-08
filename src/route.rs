use std::sync::Arc;

use ::redis::Client;
use axum::{
    Json, Router,
    extract::Request,
    http::StatusCode,
    middleware,
    middleware::Next,
    response::{IntoResponse, Response},
    routing::*,
};
use serde_json::json;
use sqlx::postgres::PgPool;
use tower_http::cors::{Any, CorsLayer};

use crate::{
    db::connect_pool,
    handlers::orders as order_handlers,
    handlers::*,
    utils::{jwt_auth::Claims, observability},
    *,
};

pub struct AppState {
    pub write_pool: PgPool,
    pub read_pool: PgPool,
    pub redis_client: Client,
    pub chat_service: Arc<chat::ChatState>,
}

pub async fn route() -> Result<Router, anyhow::Error> {
    // config init
    let cfg = config::AppConfig::new()
        .expect("Configuration initialization failed, check pg .env settings.");

    // pg init
    let (write_pg_url, read_pg_url) = cfg.pg.required_urls()?;
    let redis_url = cfg.redis.url.expect("Redis URL not found, check settings.");
    // set up connection pool
    let write_pool = connect_pool(write_pg_url, "write").await?;
    let read_pool = connect_pool(read_pg_url, "read").await?;
    let redis_client = Client::open(redis_url).expect("can't create redis client");

    // app init
    Ok(Router::new()
        .route("/", get(index))
        .nest("/api/users", user_router())
        .nest("/api/auth", auth_router())
        .nest("/api/bakery", bakery_router())
        .nest("/api/orders", orders_router())
        .nest("/api/hot", hot_router())
        .nest("/api/chat", chat_router())
        .fallback(global_404)
        .layer(middleware::from_fn(global_405))
        .with_state(Arc::new(AppState {
            write_pool,
            read_pool,
            redis_client,
            chat_service: Arc::new(chat::ChatState::default()),
        }))
        .layer(tower_http::catch_panic::CatchPanicLayer::custom(|_err| {
            // _err: Box<dyn Any + Send>
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(
                    serde_json::json!( { "code": "panic", "message": "internal server error" }),
                ),
            )
                .into_response()
        })) // 将 panic 转成 500
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .layer(middleware::from_fn(auth::auth))
        .layer(middleware::from_fn(observability::http_observability)))
}

async fn global_405(req: Request, next: Next) -> Response {
    let res = next.run(req).await;

    if res.status() == StatusCode::METHOD_NOT_ALLOWED {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            Json(json!({
                "msg": "Method Not Allowed",
                "code": 1005
            })),
        )
            .into_response();
    }

    res
}

fn user_router() -> Router<Arc<AppState>> {
    // /api/users
    Router::new()
        .route("/", get(users::list))
        .route("/{id}", get(users::detail))
        .layer(middleware::from_extractor::<Claims>()) // jwt auth middleware
}

fn auth_router() -> Router<Arc<AppState>> {
    // /api/auth
    Router::new()
        .route("/register", post(auth::register))
        .route("/login", post(auth::login))
        .route("/protected", get(auth::protected))
}

fn bakery_router() -> Router<Arc<AppState>> {
    // /api/bakery
    Router::new()
        .route("/create", post(bakery::create))
        .route("/update", post(bakery::update))
        .route("/delete", post(bakery::delete))
        .layer(middleware::from_extractor::<Claims>()) // jwt auth middleware
        .route("/", get(bakery::list))
        .route("/{id}", get(bakery::detail))
}

fn hot_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/{item_id}/incr", post(stat::hot::incr_hot_score))
        .route("/top", get(stat::hot::hot_top))
        .route("/stock/{item_id}/claim", post(stat::hot::claim_stock))
}

fn orders_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", post(order_handlers::create))
        .route("/{id}", get(order_handlers::detail))
}

fn chat_router() -> Router<Arc<AppState>> {
    Router::new().route("/connect", get(chat::connect))
}
