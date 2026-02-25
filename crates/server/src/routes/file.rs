use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use tokio::task::spawn_blocking;

use find_common::api::FileResponse;

use crate::{archive::ArchiveManager, db, AppState};

use super::{check_auth, source_db_path};

// ── GET /api/v1/file?source=X&path=Y[&archive_path=Z] ────────────────────────
//
// `path` may be a composite path ("archive.zip::member.txt") or, for backward
// compatibility, a plain path with `archive_path` supplied separately.

#[derive(Deserialize)]
pub struct FileParams {
    pub source: String,
    pub path: String,
    /// Legacy: combine with `path` into a composite path if provided.
    pub archive_path: Option<String>,
}

pub async fn get_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<FileParams>,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) {
        return (s, Json(serde_json::Value::Null)).into_response();
    }

    let db_path = match source_db_path(&state, &params.source) {
        Ok(p) => p,
        Err(s) => return (s, Json(serde_json::Value::Null)).into_response(),
    };

    // Build composite path from path + optional archive_path (backward compat).
    let full_path = match &params.archive_path {
        Some(ap) if !ap.is_empty() => format!("{}::{}", params.path, ap),
        _ => params.path.clone(),
    };

    let data_dir = state.data_dir.clone();
    match spawn_blocking(move || {
        let conn = db::open(&db_path)?;
        let archive_mgr = ArchiveManager::new(data_dir);

        // Query file metadata
        let (kind, mtime, size): (String, Option<i64>, Option<i64>) = conn
            .query_row(
                "SELECT kind, mtime, size FROM files WHERE path = ?1",
                rusqlite::params![full_path],
                |row| Ok((row.get(0)?, row.get(1).ok(), row.get(2).ok())),
            )
            .unwrap_or_else(|_| ("text".into(), None, None));

        let lines = db::get_file_lines(&conn, &archive_mgr, &full_path)?;
        let total_lines = lines.len();
        Ok::<_, anyhow::Error>(FileResponse {
            lines,
            file_kind: kind,
            total_lines,
            mtime,
            size,
        })
    })
    .await
    .unwrap_or_else(|e| Err(anyhow::anyhow!(e)))
    {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => {
            tracing::error!("get_file: {e:#}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// ── GET /api/v1/files?source=<name> ──────────────────────────────────────────

#[derive(Deserialize)]
pub struct SourceParam {
    pub source: String,
}

pub async fn list_files(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<SourceParam>,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) { return (s, Json(serde_json::Value::Null)).into_response(); }

    let db_path = match source_db_path(&state, &params.source) {
        Ok(p) => p,
        Err(s) => return (s, Json(serde_json::Value::Null)).into_response(),
    };

    match spawn_blocking(move || {
        let conn = db::open(&db_path)?;
        db::list_files(&conn)
    })
    .await
    .unwrap_or_else(|e| Err(anyhow::anyhow!(e)))
    {
        Ok(files) => Json(files).into_response(),
        Err(e) => {
            tracing::error!("list_files: {e:#}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
