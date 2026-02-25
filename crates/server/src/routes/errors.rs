use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use tokio::task::spawn_blocking;

use find_common::api::ErrorsResponse;

use crate::{db, AppState};

use super::{check_auth, source_db_path};

// ── GET /api/v1/errors?source=X[&limit=200&offset=0] ─────────────────────────

#[derive(Deserialize)]
pub struct ErrorsParams {
    pub source: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

fn default_limit() -> usize { 200 }

pub async fn get_errors(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<ErrorsParams>,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) {
        return (s, Json(serde_json::Value::Null)).into_response();
    }

    let db_path = match source_db_path(&state, &params.source) {
        Ok(p) => p,
        Err(s) => return (s, Json(serde_json::Value::Null)).into_response(),
    };

    let limit = params.limit.min(1000);
    let offset = params.offset;

    match spawn_blocking(move || {
        let conn = db::open(&db_path)?;
        let total = db::get_indexing_error_count(&conn)?;
        let errors = db::get_indexing_errors(&conn, limit, offset)?;
        Ok::<_, anyhow::Error>(ErrorsResponse { errors, total })
    })
    .await
    .unwrap_or_else(|e| Err(anyhow::anyhow!(e)))
    {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => {
            tracing::error!("get_errors: {e:#}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
