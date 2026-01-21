use std::sync::Arc;

use axum::extract::{FromRef, FromRequestParts, Request};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::{Json, routing::*};
use axum::{Router, middleware};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::handlers::auth::auth;
use crate::handlers::*;
use crate::utils::jwt_auth::Claims;
use crate::*;

pub struct AppState {
    pub pg_pool: PgPool,
}

pub struct DbConn(pub sqlx::pool::PoolConnection<sqlx::Postgres>);

impl<S> FromRequestParts<S> for DbConn
where
    Arc<AppState>: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(_parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = Arc::<AppState>::from_ref(state);
        let pool = &app_state.pg_pool;

        let conn = pool
            .acquire()
            .await
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

        Ok(Self(conn))
    }
}

pub async fn route() -> Result<Router, anyhow::Error> {
    // config init
    let cfg = config::AppConfig::new()
        .expect("Configuration initialization failed, check pg .env settings.");

    // pg init
    let pg_url = cfg.pg.url.expect("Postgres URL not found, check settings.");
    // set up connection pool
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&pg_url)
        .await
        .expect("can't connect to database");

    // app init
    Ok(
        Router::new()
            .route("/", get(index))
            .nest("/api/users", user_router())
            .nest("/api/auth", auth_router())
            .nest("/api/bakery", bakery_router())
            .fallback(global_404)
            .layer(middleware::from_fn(global_405))
            .with_state(Arc::new(AppState { pg_pool: pool }))
            .layer(TraceLayer::new_for_http()) // trace http request
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
            .layer(middleware::from_fn(auth)), // current user middleware
    )
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
