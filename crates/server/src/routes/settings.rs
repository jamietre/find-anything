use std::sync::Arc;

use axum::{extract::State, http::HeaderMap, response::IntoResponse, Json};

use find_common::api::AppSettingsResponse;

use crate::AppState;

use super::check_auth;

// ── GET /api/v1/settings ──────────────────────────────────────────────────────

pub async fn get_settings(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) {
        return (s, Json(serde_json::Value::Null)).into_response();
    }

    Json(AppSettingsResponse {
        context_window: state.config.search.context_window,
    })
    .into_response()
}
