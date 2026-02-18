use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use tokio::task::spawn_blocking;

use find_common::api::{
    ContextBatchRequest, ContextBatchResponse, ContextBatchResult, ContextResponse,
};

use crate::{archive::ArchiveManager, db, AppState};

use super::{check_auth, compact_lines, source_db_path};

// ── GET /api/v1/context ───────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ContextParams {
    pub source: String,
    pub path: String,
    /// Legacy: combined with `path` into a composite path if provided.
    pub archive_path: Option<String>,
    pub line: usize,
    /// If omitted, the server's configured `search.context_window` is used.
    pub window: Option<usize>,
}

pub async fn get_context(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<ContextParams>,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) { return (s, Json(serde_json::Value::Null)).into_response(); }

    let db_path = match source_db_path(&state, &params.source) {
        Ok(p) => p,
        Err(s) => return (s, Json(serde_json::Value::Null)).into_response(),
    };

    let full_path = match &params.archive_path {
        Some(ap) if !ap.is_empty() => format!("{}::{}", params.path, ap),
        _ => params.path.clone(),
    };

    let window = params.window.unwrap_or(state.config.search.context_window);
    let data_dir = state.data_dir.clone();
    match spawn_blocking(move || {
        let conn = db::open(&db_path)?;
        let archive_mgr = ArchiveManager::new(data_dir);
        let kind: String = conn.query_row(
            "SELECT kind FROM files WHERE path = ?1",
            rusqlite::params![full_path],
            |row| row.get(0),
        ).unwrap_or_else(|_| "text".into());

        let raw = db::get_context(
            &conn,
            &archive_mgr,
            &full_path,
            params.line,
            window,
        )?;
        let (start, match_index, lines) = compact_lines(raw, params.line);
        Ok::<_, anyhow::Error>(ContextResponse { start, match_index, lines, kind })
    })
    .await
    .unwrap_or_else(|e| Err(anyhow::anyhow!(e)))
    {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => {
            tracing::error!("context: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// ── POST /api/v1/context-batch ────────────────────────────────────────────────

pub async fn context_batch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<ContextBatchRequest>,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) {
        return (s, Json(serde_json::Value::Null)).into_response();
    }

    let data_dir = state.data_dir.clone();

    match spawn_blocking(move || {
        let archive_mgr = ArchiveManager::new(data_dir.clone());

        // Group items by source so we open each DB at most once.
        let mut by_source: std::collections::HashMap<String, (std::path::PathBuf, Vec<find_common::api::ContextBatchItem>)> = std::collections::HashMap::new();
        for item in req.requests {
            let valid = item.source.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            if valid {
                let db_path = data_dir.join("sources").join(format!("{}.db", item.source));
                by_source.entry(item.source.clone()).or_insert_with(|| (db_path, vec![])).1.push(item);
            }
        }

        let mut results: Vec<ContextBatchResult> = Vec::new();
        for (_source, (db_path, items)) in by_source {
            let conn = match db::open(&db_path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("context_batch open {}: {e}", db_path.display());
                    for item in items {
                        results.push(ContextBatchResult { source: item.source, path: item.path, line: item.line, start: 0, match_index: None, lines: vec![], kind: String::new() });
                    }
                    continue;
                }
            };

            for item in items {
                let full_path = match &item.archive_path {
                    Some(ap) if !ap.is_empty() => format!("{}::{}", item.path, ap),
                    _ => item.path.clone(),
                };

                let (kind, start, match_index, lines) = match (|| -> anyhow::Result<_> {
                    let kind = conn
                        .query_row("SELECT kind FROM files WHERE path = ?1", rusqlite::params![full_path], |row| row.get::<_, String>(0))
                        .unwrap_or_else(|_| "text".into());
                    let raw = db::get_context(&conn, &archive_mgr, &full_path, item.line, item.window)?;
                    let (start, match_index, lines) = compact_lines(raw, item.line);
                    Ok((kind, start, match_index, lines))
                })() {
                    Ok(t) => t,
                    Err(e) => {
                        tracing::warn!("context_batch item {}/{}: {e}", item.source, item.path);
                        (String::new(), 0_usize, None, vec![])
                    }
                };

                results.push(ContextBatchResult { source: item.source, path: item.path, line: item.line, start, match_index, lines, kind });
            }
        }

        Ok::<_, anyhow::Error>(ContextBatchResponse { results })
    })
    .await
    .unwrap_or_else(|e| Err(anyhow::anyhow!(e)))
    {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => {
            tracing::error!("context_batch: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
