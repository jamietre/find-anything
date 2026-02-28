use std::sync::Arc;

use axum::{extract::State, http::HeaderMap, response::IntoResponse, Json};
use tokio::task::spawn_blocking;

use find_common::api::{SourceStats, StatsResponse, WorkerStatus};

use crate::{db, AppState};

use super::check_auth;

// ── GET /api/v1/stats ─────────────────────────────────────────────────────────

pub async fn get_stats(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) {
        return (s, Json(serde_json::Value::Null)).into_response();
    }

    let sources_dir = state.data_dir.join("sources");
    let inbox_dir = state.data_dir.join("inbox");
    let failed_dir = inbox_dir.join("failed");

    let count_gz = |dir: &std::path::Path| -> usize {
        std::fs::read_dir(dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().map(|x| x == "gz").unwrap_or(false))
                    .count()
            })
            .unwrap_or(0)
    };

    let inbox_pending = count_gz(&inbox_dir);
    let failed_requests = count_gz(&failed_dir);

    let (total_archives, archive_size_bytes) = {
        let content_dir = sources_dir.join("content");
        let mut count = 0usize;
        let mut size = 0u64;
        if let Ok(rd) = std::fs::read_dir(&content_dir) {
            for entry in rd.filter_map(|e| e.ok()) {
                if entry.path().is_dir() {
                    if let Ok(subdir) = std::fs::read_dir(entry.path()) {
                        for e in subdir.filter_map(|e| e.ok()) {
                            if e.path().extension().map(|x| x == "zip").unwrap_or(false) {
                                count += 1;
                                size += e.metadata().map(|m| m.len()).unwrap_or(0);
                            }
                        }
                    }
                }
            }
        }
        (count, size)
    };

    let db_size_bytes: u64 = std::fs::read_dir(&sources_dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|x| x == "db").unwrap_or(false))
                .filter_map(|e| e.metadata().ok())
                .map(|m| m.len())
                .sum()
        })
        .unwrap_or(0);

    // Collect all source DBs.
    let source_dbs: Vec<(String, std::path::PathBuf)> = match std::fs::read_dir(&sources_dir) {
        Err(_) => vec![],
        Ok(rd) => rd
            .filter_map(|e| {
                let e = e.ok()?;
                let name = e.file_name().into_string().ok()?;
                let source_name = name.strip_suffix(".db")?.to_string();
                Some((source_name, e.path()))
            })
            .collect(),
    };

    let handles: Vec<_> = source_dbs
        .into_iter()
        .map(|(source_name, db_path)| {
            spawn_blocking(move || -> anyhow::Result<SourceStats> {
                if !db_path.exists() {
                    return Ok(SourceStats {
                        name: source_name,
                        last_scan: None,
                        total_files: 0,
                        total_size: 0,
                        by_kind: Default::default(),
                        history: vec![],
                        indexing_error_count: 0,
                        fts_row_count: 0,
                    });
                }
                let conn = db::open(&db_path)?;
                let last_scan = db::get_last_scan(&conn)?;
                let (total_files, total_size, by_kind) = db::get_stats(&conn)?;
                let history = db::get_scan_history(&conn, 100)?;
                let indexing_error_count = db::get_indexing_error_count(&conn)?;
                let fts_row_count = db::get_fts_row_count(&conn).unwrap_or(0);
                Ok(SourceStats { name: source_name, last_scan, total_files, total_size, by_kind, history, indexing_error_count, fts_row_count })
            })
        })
        .collect();

    let mut sources: Vec<SourceStats> = Vec::new();
    for handle in handles {
        match handle.await.unwrap_or_else(|e| Err(anyhow::anyhow!(e))) {
            Ok(stats) => sources.push(stats),
            Err(e) => tracing::warn!("stats source error: {e:#}"),
        }
    }
    sources.sort_by(|a, b| a.name.cmp(&b.name));

    let worker_status = state.worker_status
        .lock()
        .map(|g| g.clone())
        .unwrap_or(WorkerStatus::Idle);

    Json(StatsResponse {
        sources,
        inbox_pending,
        failed_requests,
        total_archives,
        db_size_bytes,
        archive_size_bytes,
        worker_status,
    })
    .into_response()
}
