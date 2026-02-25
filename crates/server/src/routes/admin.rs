use std::sync::Arc;
use std::time::SystemTime;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use flate2::read::GzDecoder;
use serde::Deserialize;
use tokio::task::spawn_blocking;

use find_common::api::{
    InboxDeleteResponse, InboxItem, InboxRetryResponse, InboxShowFile, InboxShowResponse,
    InboxStatusResponse,
};

use crate::AppState;

use super::check_auth;

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

    let result = spawn_blocking(move || -> anyhow::Result<InboxStatusResponse> {
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
        Ok(InboxStatusResponse { pending, failed })
    })
    .await;

    match result.unwrap_or_else(|e| Err(anyhow::anyhow!(e))) {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => {
            tracing::error!("inbox_status error: {e:#}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
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

    let result = spawn_blocking(move || -> anyhow::Result<usize> {
        let delete_gz_in = |dir: &std::path::Path| -> usize {
            let rd = match std::fs::read_dir(dir) {
                Ok(rd) => rd,
                Err(_) => return 0,
            };
            let mut count = 0;
            for entry in rd.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map(|x| x == "gz").unwrap_or(false) {
                    if std::fs::remove_file(&path).is_ok() {
                        count += 1;
                    }
                }
            }
            count
        };

        let deleted = match target.as_str() {
            "failed" => delete_gz_in(&failed_dir),
            "all" => delete_gz_in(&inbox_dir) + delete_gz_in(&failed_dir),
            _ => delete_gz_in(&inbox_dir), // "pending" or anything else
        };
        Ok(deleted)
    })
    .await;

    match result.unwrap_or_else(|e| Err(anyhow::anyhow!(e))) {
        Ok(deleted) => Json(InboxDeleteResponse { deleted }).into_response(),
        Err(e) => {
            tracing::error!("inbox_clear error: {e:#}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
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

    let result = spawn_blocking(move || -> anyhow::Result<usize> {
        let rd = match std::fs::read_dir(&failed_dir) {
            Ok(rd) => rd,
            Err(_) => return Ok(0),
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
        Ok(count)
    })
    .await;

    match result.unwrap_or_else(|e| Err(anyhow::anyhow!(e))) {
        Ok(retried) => Json(InboxRetryResponse { retried }).into_response(),
        Err(e) => {
            tracing::error!("inbox_retry error: {e:#}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
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

    let result = spawn_blocking(move || -> anyhow::Result<Option<InboxShowResponse>> {
        // Normalise: ensure the name ends in .gz
        let filename = if query.name.ends_with(".gz") {
            query.name.clone()
        } else {
            format!("{}.gz", query.name)
        };

        // Look in pending first, then failed.
        let (path, queue) = if inbox_dir.join(&filename).exists() {
            (inbox_dir.join(&filename), "pending")
        } else if failed_dir.join(&filename).exists() {
            (failed_dir.join(&filename), "failed")
        } else {
            return Ok(None);
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

        Ok(Some(InboxShowResponse {
            queue: queue.to_string(),
            source: req.source,
            files,
            delete_paths: req.delete_paths,
            failures: req.indexing_failures,
            scan_timestamp: req.scan_timestamp,
        }))
    })
    .await;

    match result.unwrap_or_else(|e| Err(anyhow::anyhow!(e))) {
        Ok(Some(resp)) => Json(resp).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            tracing::error!("inbox_show error: {e:#}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
