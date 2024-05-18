use std::sync::Arc;

use axum::routing::*;
use axum::{middleware, Router};
use dotenv::dotenv;
use sea_orm::{Database, DatabaseConnection};
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing::Level;

use crate::handlers::auth::auth;
use crate::handlers::*;
use crate::utils::jwt_auth::Claims;
use crate::*;

pub struct AppState {
    pub conn: DatabaseConnection,
}

pub async fn get_conn(state: &AppState) -> &DatabaseConnection {
    let conn = &state.conn;

    conn
}

pub async fn route() -> Result<Router, anyhow::Error> {
    // config init
    dotenv().ok();
    let cfg = config::AppConfig::from_env()
        .expect("Configuration initialization failed, check pg .env settings.");

    // pg init
    let pg_url = cfg
        .pg
        .url
        .unwrap_or("postgresql://melody:5161@localhost:5432".to_string());
    let db = Database::connect(&pg_url).await?;

    // app init
    Ok(Router::new()
        .route("/", get(index))
        .nest("/api/users", user_router())
        .nest("/api/auth", auth_router())
        .nest("/api/bakery", bakery_router())
        .fallback(global_404)
        .with_state(Arc::new(AppState { conn: db }))
        .layer(
            ServiceBuilder::new()
                // trace middleware
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(
                            tower_http::trace::DefaultMakeSpan::new().level(Level::INFO),
                        )
                        .on_request(tower_http::trace::DefaultOnRequest::new().level(Level::INFO))
                        .on_response(
                            tower_http::trace::DefaultOnResponse::new().level(Level::INFO),
                        ),
                )
                // cors middleware
                .layer(
                    CorsLayer::new()
                        .allow_origin(Any)
                        .allow_methods(Any)
                        .allow_headers(Any),
                ),
        )
        .layer(middleware::from_fn(auth)) // current user middleware
    )
}

fn user_router() -> Router<Arc<AppState>> {
    // /api/users
    Router::new()
        .route("/", get(users::list))
        .route("/:id", get(users::detail))
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
        .route("/:id", get(bakery::detail))
}
