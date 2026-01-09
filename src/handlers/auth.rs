use std::sync::Arc;

use axum::Extension;
use axum::extract::{Json, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use jsonwebtoken::{Header, encode};
use serde::Serialize;
use time::OffsetDateTime;

use crate::error::{AppResult, AuthError};
use crate::route::AppState;
use crate::utils::jwt_auth::*;

pub async fn register(State(_state): State<Arc<AppState>>) -> AppResult<()> {
    unimplemented!()
}

pub async fn login(
    State(_state): State<Arc<AppState>>,
    Json(payload): Json<AuthPayload>,
) -> AppResult<Json<AuthBody>> {
    // Check if the user sent the credentials
    if payload.client_id.is_empty() || payload.client_secret.is_empty() {
        return Err(AuthError::MissingCredential)?;
    }
    // Here you can check the user credentials from a database
    if payload.client_id != "Foo" || payload.client_secret != "bar" {
        return Err(AuthError::WrongCredential)?;
    }
    let claims = Claims {
        sub: "rc@me.com".to_string(),
        company: "raincloud".to_string(),
        // Mandatory expiry time as UTC timestamp
        exp: get_timestamp_x_days_from_now(15), // 15 days
    };
    // Create the authorization token
    let token = encode(&Header::default(), &claims, &KEYS.encoding)
        .map_err(|_| AuthError::TokenCreation)?;

    // Send the authorized token
    Ok(Json(AuthBody::new(token)))
}

pub async fn protected(
    claims: Claims,
    Extension(user): Extension<CurrentUser>,
) -> AppResult<String> {
    Ok(format!(
        "Hello, {}. Welcome to the protected area :)\nYour data:\n{}",
        user.username, claims
    ))
}

fn get_timestamp_x_days_from_now(x: u64) -> u64 {
    let now = OffsetDateTime::now_utc();
    println!("Now: {:?}", &now);
    let x_days_after = now + time::Duration::days(x as i64);

    x_days_after.unix_timestamp() as u64
}

// current user middleware
#[derive(Debug, Clone, Serialize)]
pub struct CurrentUser {
    pub id: u32,
    pub username: String,
    pub email: String,
}

#[allow(unused_variables)]
pub async fn auth(mut req: Request, next: Next) -> AppResult<Response> {
    // TODO implement the authorization or null logic
    // let auth_header = req
    //     .headers()
    //     .get(http::header::AUTHORIZATION)
    //     .and_then(|header| header.to_str().ok());

    // let token = if let Some(auth_header) = auth_header {
    //     auth_header
    // } else {
    //     return Err(anyhow!(StatusCode::UNAUTHORIZED))?;
    // };

    // if let Some(current_user) = authorize_current_user(auth_header).await {
    //     // insert the current user into a request extension so the handler can
    //     // extract it
    //     req.extensions_mut().insert(current_user);
    //     Ok(next.run(req).await)
    // } else {
    //     Err(StatusCode::UNAUTHORIZED)
    // }

    req.extensions_mut().insert(CurrentUser {
        id: 1,
        username: "rc".to_string(),
        email: "rc@axes.com".to_string(),
    });
    Ok(next.run(req).await)
}
