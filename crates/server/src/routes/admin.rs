use std::sync::Arc;
use std::time::SystemTime;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use anyhow::Context;
use flate2::read::GzDecoder;
use serde::Deserialize;

use find_common::api::{
    InboxDeleteResponse, InboxItem, InboxRetryResponse, InboxShowFile, InboxShowResponse,
    InboxStatusResponse, SourceDeleteResponse,
};

use crate::archive::ArchiveManager;
use crate::AppState;
use crate::db;

use super::{check_auth, run_blocking, source_db_path};

// ── GET /api/v1/admin/inbox ───────────────────────────────────────────────────

pub async fn inbox_status(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) {
        return (s, Json(serde_json::Value::Null)).into_response();
    }

    let inbox_dir = state.data_dir.join("inbox");
    let failed_dir = inbox_dir.join("failed");

    run_blocking("inbox_status", move || -> anyhow::Result<_> {
        let now = SystemTime::now();

        let read_items = |dir: &std::path::Path| -> Vec<InboxItem> {
            let rd = match std::fs::read_dir(dir) {
                Ok(rd) => rd,
                Err(_) => return vec![],
            };
            let mut items = Vec::new();
            for entry in rd.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map(|x| x == "gz").unwrap_or(false) {
                    let filename = entry.file_name().to_string_lossy().into_owned();
                    let meta = match entry.metadata() {
                        Ok(m) => m,
                        Err(_) => continue,
                    };
                    let size_bytes = meta.len();
                    let age_secs = meta
                        .modified()
                        .ok()
                        .and_then(|m| now.duration_since(m).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    items.push(InboxItem { filename, size_bytes, age_secs });
                }
            }
            items.sort_by(|a, b| a.filename.cmp(&b.filename));
            items
        };

        let pending = read_items(&inbox_dir);
        let failed = read_items(&failed_dir);
        Ok(Json(InboxStatusResponse { pending, failed }))
    }).await
}

// ── DELETE /api/v1/admin/inbox ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct InboxDeleteQuery {
    #[serde(default = "default_target")]
    target: String,
}

fn default_target() -> String {
    "pending".to_string()
}

pub async fn inbox_clear(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<InboxDeleteQuery>,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) {
        return (s, Json(serde_json::Value::Null)).into_response();
    }

    let inbox_dir = state.data_dir.join("inbox");
    let failed_dir = inbox_dir.join("failed");
    let target = query.target.clone();

    run_blocking("inbox_clear", move || -> anyhow::Result<_> {
        let delete_gz_in = |dir: &std::path::Path| -> usize {
            let rd = match std::fs::read_dir(dir) {
                Ok(rd) => rd,
                Err(_) => return 0,
            };
            let mut count = 0;
            for entry in rd.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map(|x| x == "gz").unwrap_or(false)
                    && std::fs::remove_file(&path).is_ok()
                {
                    count += 1;
                }
            }
            count
        };

        let deleted = match target.as_str() {
            "failed" => delete_gz_in(&failed_dir),
            "all" => delete_gz_in(&inbox_dir) + delete_gz_in(&failed_dir),
            _ => delete_gz_in(&inbox_dir), // "pending" or anything else
        };
        Ok(Json(InboxDeleteResponse { deleted }))
    }).await
}

// ── POST /api/v1/admin/inbox/retry ────────────────────────────────────────────

pub async fn inbox_retry(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) {
        return (s, Json(serde_json::Value::Null)).into_response();
    }

    let inbox_dir = state.data_dir.join("inbox");
    let failed_dir = inbox_dir.join("failed");

    run_blocking("inbox_retry", move || -> anyhow::Result<_> {
        let rd = match std::fs::read_dir(&failed_dir) {
            Ok(rd) => rd,
            Err(_) => return Ok(Json(InboxRetryResponse { retried: 0 })),
        };
        let mut count = 0;
        for entry in rd.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().map(|x| x == "gz").unwrap_or(false) {
                let dest = inbox_dir.join(entry.file_name());
                if std::fs::rename(&path, &dest).is_ok() {
                    count += 1;
                }
            }
        }
        Ok(Json(InboxRetryResponse { retried: count }))
    }).await
}

// ── GET /api/v1/admin/inbox/show ──────────────────────────────────────────────

#[derive(Deserialize)]
pub struct InboxShowQuery {
    name: String,
}

pub async fn inbox_show(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<InboxShowQuery>,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) {
        return (s, Json(serde_json::Value::Null)).into_response();
    }

    let inbox_dir = state.data_dir.join("inbox");
    let failed_dir = inbox_dir.join("failed");

    run_blocking("inbox_show", move || -> anyhow::Result<_> {
        let filename = if query.name.ends_with(".gz") {
            query.name.clone()
        } else {
            format!("{}.gz", query.name)
        };

        let (path, queue) = if inbox_dir.join(&filename).exists() {
            (inbox_dir.join(&filename), "pending")
        } else if failed_dir.join(&filename).exists() {
            (failed_dir.join(&filename), "failed")
        } else {
            return Ok(StatusCode::NOT_FOUND.into_response());
        };

        let raw = std::fs::read(&path)?;
        let req: find_common::api::BulkRequest =
            serde_json::from_reader(GzDecoder::new(raw.as_slice()))?;

        let files = req
            .files
            .iter()
            .map(|f| InboxShowFile {
                path: f.path.clone(),
                kind: f.kind.clone(),
                content_lines: f.lines.iter().filter(|l| l.line_number != 0).count(),
            })
            .collect();

        Ok(Json(InboxShowResponse {
            queue: queue.to_string(),
            source: req.source,
            files,
            delete_paths: req.delete_paths,
            failures: req.indexing_failures,
            scan_timestamp: req.scan_timestamp,
        }).into_response())
    }).await
}

// ── DELETE /api/v1/admin/source ───────────────────────────────────────────────

#[derive(Deserialize)]
pub struct DeleteSourceQuery {
    source: String,
}

pub async fn delete_source(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(query): Query<DeleteSourceQuery>,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) {
        return (s, Json(serde_json::Value::Null)).into_response();
    }

    let db_path = match source_db_path(&state, &query.source) {
        Ok(p) => p,
        Err(s) => return (s, Json(serde_json::Value::Null)).into_response(),
    };

    if !db_path.exists() {
        return (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "source not found" }))).into_response();
    }

    let data_dir = state.data_dir.clone();

    run_blocking("delete_source", move || -> anyhow::Result<_> {
        let conn = db::open(&db_path)?;

        let files_deleted = db::count_files(&conn)?;
        let chunk_refs = db::collect_all_chunk_refs(&conn)?;
        let chunks_removed = chunk_refs.len();

        // Close the DB before deleting it.
        drop(conn);

        let mut archive_mgr = ArchiveManager::new(data_dir);
        if !chunk_refs.is_empty() {
            archive_mgr.remove_chunks(chunk_refs)?;
        }

        std::fs::remove_file(&db_path)
            .with_context(|| format!("removing {}", db_path.display()))?;

        Ok(Json(SourceDeleteResponse { files_deleted, chunks_removed }))
    }).await
}
