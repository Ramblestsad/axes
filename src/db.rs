use std::{sync::Arc, time::Duration};

use axum::{
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, request::Parts},
};
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::route::AppState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DbRole {
    Read,
    Write,
}

pub struct ReadDbConn(pub sqlx::pool::PoolConnection<sqlx::Postgres>);

pub struct WriteDbConn(pub sqlx::pool::PoolConnection<sqlx::Postgres>);

impl<S> FromRequestParts<S> for ReadDbConn
where
    Arc<AppState>: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(_parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let conn = acquire_db_conn(state, DbRole::Read).await?;
        Ok(Self(conn))
    }
}

impl<S> FromRequestParts<S> for WriteDbConn
where
    Arc<AppState>: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(_parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let conn = acquire_db_conn(state, DbRole::Write).await?;
        Ok(Self(conn))
    }
}

pub async fn connect_pool(url: &str, role: &str) -> anyhow::Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect(url)
        .await
        .map_err(|error| anyhow::anyhow!("can't connect to {role} database: {error}"))
}

async fn acquire_db_conn<S>(
    state: &S,
    role: DbRole,
) -> Result<sqlx::pool::PoolConnection<sqlx::Postgres>, (StatusCode, String)>
where
    Arc<AppState>: FromRef<S>,
    S: Send + Sync,
{
    let app_state = Arc::<AppState>::from_ref(state);
    let (pool, db_role) = match role {
        DbRole::Read => (&app_state.read_pool, "read"),
        DbRole::Write => (&app_state.write_pool, "write"),
    };

    tracing::info!(db_role, "acquiring postgres connection");

    pool.acquire()
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))
}
