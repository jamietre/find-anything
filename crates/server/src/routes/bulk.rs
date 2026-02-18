use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
};

use crate::AppState;

use super::check_auth;

// ── POST /api/v1/bulk ─────────────────────────────────────────────────────────

pub async fn bulk(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) { return s.into_response(); }

    let is_gzip = headers
        .get(header::CONTENT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(|v| v == "gzip")
        .unwrap_or(false);

    if !is_gzip {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }

    let request_id = format!(
        "req_{}_{}",
        chrono::Utc::now().format("%Y%m%d_%H%M%S"),
        uuid::Uuid::new_v4().simple()
    );

    let inbox_path = state.data_dir.join("inbox").join(format!("{request_id}.gz"));

    match tokio::fs::write(&inbox_path, &body).await {
        Ok(()) => {
            tracing::info!("Queued bulk request: {}", inbox_path.display());
            StatusCode::ACCEPTED.into_response()
        }
        Err(e) => {
            tracing::error!("Failed to write inbox request: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
