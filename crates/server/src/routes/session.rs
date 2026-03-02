use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use crate::AppState;

use super::check_auth;

#[derive(Deserialize)]
pub struct SessionRequest {
    token: Option<String>,
}

/// POST /api/v1/auth/session
///
/// Validates the provided token and sets an HttpOnly session cookie so that
/// browser-native requests (e.g. `<img src>`) can be authenticated without
/// custom headers.
pub async fn create_session(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(body): Json<SessionRequest>,
) -> impl IntoResponse {
    // Accept the token from the JSON body, or fall back to the Authorization header.
    let token_valid = if let Some(ref t) = body.token {
        *t == state.config.server.token
    } else {
        check_auth(&state, &headers).is_ok()
    };

    if !token_valid {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let token = body.token.as_deref().unwrap_or(&state.config.server.token);
    let cookie = format!(
        "find_session={token}; HttpOnly; SameSite=Strict; Path=/"
    );

    (
        StatusCode::OK,
        [(axum::http::header::SET_COOKIE, cookie)],
    )
        .into_response()
}

/// DELETE /api/v1/auth/session
///
/// Clears the session cookie.
pub async fn delete_session() -> impl IntoResponse {
    let cookie = "find_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0";
    (
        StatusCode::OK,
        [(axum::http::header::SET_COOKIE, cookie)],
    )
        .into_response()
}
