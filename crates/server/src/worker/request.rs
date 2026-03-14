/// Processing a single inbox request from start to finish.
///
/// A request goes through two phases:
///  - Phase 1 (this module): decode the `.gz`, update SQLite (deletes, renames,
///    upserts), write the activity log, and write a normalised `.gz` to
///    `inbox/to-archive/` for the archive phase.
///  - Phase 2 (archive_batch): read from `to-archive/`, coalesce chunks, rewrite
///    ZIP archives, and update chunk refs in SQLite.
use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use rusqlite::ErrorCode;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

use find_common::api::{BulkRequest, IndexingFailure, RecentFile};
use find_common::path::is_composite;

use crate::archive::SharedArchiveState;
use crate::db;
use crate::normalize;

use super::{StatusHandle, WorkerConfig, timed, warn_slow};
use super::pipeline;

// ── Public entry point ─────────────────────────────────────────────────────────

/// Async wrapper: runs `process_request_phase1` in a blocking task with a
/// configurable timeout, then moves the file to `failed/` on error.
#[allow(clippy::too_many_arguments)]
pub(super) async fn process_request_async(
    data_dir: &Path,
    request_path: &Path,
    failed_dir: &Path,
    to_archive_dir: &Path,
    status: StatusHandle,
    cfg: WorkerConfig,
    archive_notify: &Arc<tokio::sync::Notify>,
    shared_archive_state: Arc<SharedArchiveState>,
    recent_tx: tokio::sync::broadcast::Sender<RecentFile>,
) {
    let status_reset = status.clone();
    let request_timeout = cfg.request_timeout;

    let blocking_task = tokio::task::spawn_blocking({
        let data_dir = data_dir.to_path_buf();
        let request_path = request_path.to_path_buf();
        let to_archive_dir = to_archive_dir.to_path_buf();
        move || process_request_phase1(&data_dir, &request_path, &to_archive_dir, &status, cfg, &shared_archive_state, &recent_tx)
    });

    let timed_result = tokio::time::timeout(request_timeout, blocking_task).await;

    if let Ok(mut guard) = status_reset.lock() {
        *guard = find_common::api::WorkerStatus::Idle;
    }

    match timed_result {
        Err(_timeout) => {
            tracing::error!(
                "Request processing timed out after {}s, abandoning: {}",
                request_timeout.as_secs(),
                request_path.display(),
            );
            handle_failure(
                request_path,
                failed_dir,
                anyhow::anyhow!("Processing timed out after {}s", request_timeout.as_secs()),
            )
            .await;
        }
        Ok(Ok(Ok(()))) => {
            // The normalized .gz was already written to to-archive/ by the blocking task.
            // Delete the original from inbox/.
            if let Err(e) = tokio::fs::remove_file(request_path).await {
                tracing::error!(
                    "Failed to delete processed request {}: {}",
                    request_path.display(),
                    e
                );
            } else {
                tracing::debug!("Phase 1 complete, queued for archive: {}", request_path.display());
                archive_notify.notify_one();
            }
        }
        Ok(Ok(Err(e))) => {
            if is_db_locked(&e) {
                // File is still in inbox/ — the router will rediscover and
                // retry it on the next scan tick.
                tracing::warn!(
                    "Database locked while processing {}, will retry: {e:#}",
                    request_path.display(),
                );
            } else {
                handle_failure(request_path, failed_dir, e).await;
            }
        }
        Ok(Err(e)) => {
            handle_failure(
                request_path,
                failed_dir,
                anyhow::anyhow!("Task error: {}", e),
            )
            .await;
        }
    }
}

// ── Phase 1: synchronous request processing ───────────────────────────────────

/// Phase 1: process a single inbox request — SQLite only, no ZIP I/O.
/// Writes a normalized `.gz` to `to_archive_dir` for the archive phase.
fn process_request_phase1(
    data_dir: &Path,
    request_path: &Path,
    to_archive_dir: &Path,
    status: &StatusHandle,
    cfg: WorkerConfig,
    shared_archive_state: &Arc<SharedArchiveState>,
    recent_tx: &tokio::sync::broadcast::Sender<RecentFile>,
) -> Result<()> {
    let request_start = std::time::Instant::now();

    // Use a placeholder tag until we've parsed the request.
    let req_stem = request_path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
    let pre_tag = format!("[indexer:?:{req_stem}]");

    let (compressed, request): (Vec<u8>, BulkRequest) = timed!(pre_tag, "read+decode gz", {
        let compressed = std::fs::read(request_path)?;
        let mut decoder = GzDecoder::new(&compressed[..]);
        let mut json = String::new();
        decoder.read_to_string(&mut json)?;
        let request: BulkRequest = serde_json::from_str(&json)
            .context("parsing bulk request JSON")?;
        (compressed, request)
    });
    let compressed_bytes = compressed.len();

    let n_files = request.files.len();
    let n_deletes = request.delete_paths.len();
    let n_renames = request.rename_paths.len();
    let total_content_lines: usize = request.files.iter().map(|f| f.lines.len()).sum();
    let total_content_bytes: usize = request.files.iter()
        .flat_map(|f| f.lines.iter())
        .map(|l| l.content.len())
        .sum();

    let tag = format!("[indexer:{}:{req_stem}]", request.source);

    tracing::debug!("{tag} start: {} files, {} deletes, {} renames", n_files, n_deletes, n_renames);

    let db_path = data_dir.join("sources").join(format!("{}.db", request.source));
    let mut conn = timed!(tag, "open db", { db::open(&db_path)? });

    // Acquire the per-source write lock before any SQLite writes.
    let source_lock = shared_archive_state.source_lock(&request.source);
    let _source_guard = timed!(tag, "acquire source lock", {
        source_lock.lock()
            .map_err(|_| anyhow::anyhow!("source lock poisoned for {}", request.source))?
    });

    // Process deletes (SQLite only — orphaned ZIP chunks cleaned up by compaction).
    if !request.delete_paths.is_empty() {
        if let Ok(mut guard) = status.lock() {
            *guard = find_common::api::WorkerStatus::Processing {
                source: request.source.clone(),
                file: format!("(deleting {} files)", n_deletes),
            };
        }
        timed!(tag, format!("delete {} paths", n_deletes), {
            db::delete_files_phase1(&conn, &request.delete_paths)?
        });
    }

    // Process renames after deletes, before upserts.
    if !request.rename_paths.is_empty() {
        timed!(tag, format!("rename {} paths", n_renames), {
            db::rename_files(&conn, &request.rename_paths)?
        });
    }

    let mut server_side_failures: Vec<IndexingFailure> = Vec::new();
    let mut successfully_indexed: Vec<String> = Vec::new();
    let mut activity_added: Vec<String> = Vec::new();
    let mut activity_modified: Vec<String> = Vec::new();
    let mut normalized_files: Vec<find_common::api::IndexFile> = Vec::with_capacity(request.files.len());
    tracing::debug!("{tag} → index {} files", n_files);
    let index_loop_start = std::time::Instant::now();
    for file in &request.files {
        if let Ok(mut guard) = status.lock() {
            *guard = find_common::api::WorkerStatus::Processing {
                source: request.source.clone(),
                file: file.path.clone(),
            };
        }
        let file_start = std::time::Instant::now();
        let normalized_file;
        let file = if file.kind == "text" || file.kind == "pdf" {
            let normalized_lines = timed!(tag, format!("normalize {}", file.path), {
                normalize::normalize_lines(
                    file.lines.clone(),
                    &file.path,
                    &cfg.normalization,
                )
            });
            normalized_file = find_common::api::IndexFile {
                lines: normalized_lines,
                ..file.clone()
            };
            &normalized_file
        } else {
            file
        };
        match pipeline::process_file_phase1(&mut conn, file, cfg.inline_threshold_bytes) {
            Ok(outcome) => {
                successfully_indexed.push(file.path.clone());
                if file.mtime != 0 && !is_composite(&file.path) {
                    match outcome {
                        pipeline::Phase1Outcome::New      => activity_added.push(file.path.clone()),
                        pipeline::Phase1Outcome::Modified => activity_modified.push(file.path.clone()),
                        pipeline::Phase1Outcome::Skipped  => {}
                    }
                }
            }
            Err(e) => {
                if is_db_locked(&e) {
                    tracing::warn!("Failed to index {} (db locked, will retry): {e:#}", file.path);
                } else {
                    tracing::error!("Failed to index {}: {e:#}", file.path);
                }
                let (fallback, skip_inner) = if pipeline::is_outer_archive(&file.path, &file.kind) {
                    (pipeline::outer_archive_stub(file), true)
                } else {
                    (pipeline::filename_only_file(file), false)
                };
                if let Err(e2) = pipeline::process_file_phase1_fallback(&mut conn, &fallback, skip_inner, cfg.inline_threshold_bytes) {
                    if is_db_locked(&e2) {
                        tracing::warn!("Filename-only fallback also failed for {} (db locked, will retry): {e2:#}", file.path);
                    } else {
                        tracing::error!("Filename-only fallback also failed for {}: {e2:#}", file.path);
                    }
                }
                server_side_failures.push(IndexingFailure {
                    path: file.path.clone(),
                    error: format!("{e:#}"),
                });
            }
        }
        warn_slow(file_start, 30, "process_file_phase1", &file.path);
        normalized_files.push(file.clone());
    }
    tracing::debug!("{tag} ← index {} files ({:.1}ms)", n_files, index_loop_start.elapsed().as_secs_f64() * 1000.0);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let all_failures: Vec<find_common::api::IndexingFailure> = request
        .indexing_failures
        .iter()
        .chain(server_side_failures.iter())
        .cloned()
        .collect();
    timed!(tag, "cleanup writes", {
        db::do_cleanup_writes(
            &conn,
            &successfully_indexed,
            &all_failures,
            now,
            request.scan_timestamp,
        )?
    });

    // Log activity and broadcast SSE events.
    {
        let deleted: Vec<String> = request.delete_paths.iter()
            .filter(|p| !is_composite(p))
            .cloned()
            .collect();
        let renamed: Vec<(String, String)> = request.rename_paths.iter()
            .filter(|r| !is_composite(&r.old_path) && !is_composite(&r.new_path))
            .map(|r| (r.old_path.clone(), r.new_path.clone()))
            .collect();
        if let Err(e) = db::log_activity(&conn, now, &activity_added, &activity_modified, &deleted, &renamed, cfg.activity_log_max_entries) {
            tracing::warn!("Failed to write activity log: {e:#}");
        } else {
            let source = &request.source;
            for path in &activity_added {
                let _ = recent_tx.send(RecentFile { source: source.clone(), path: path.clone(), indexed_at: now, action: "added".into(),    new_path: None });
            }
            for path in &activity_modified {
                let _ = recent_tx.send(RecentFile { source: source.clone(), path: path.clone(), indexed_at: now, action: "modified".into(), new_path: None });
            }
            for path in &deleted {
                let _ = recent_tx.send(RecentFile { source: source.clone(), path: path.clone(), indexed_at: now, action: "deleted".into(),  new_path: None });
            }
            for (old, new) in &renamed {
                let _ = recent_tx.send(RecentFile { source: source.clone(), path: old.clone(),  indexed_at: now, action: "renamed".into(),  new_path: Some(new.clone()) });
            }
        }
    }

    let elapsed = request_start.elapsed();
    let elapsed_secs = elapsed.as_secs_f64();
    let content_kb = total_content_bytes / 1024;
    let compressed_kb = compressed_bytes / 1024;
    tracing::info!(
        "{tag} indexed {} files, {} deletes, {} renames, {} lines, \
         {} KB content, {} KB compressed, {:.1}s",
        n_files, n_deletes, n_renames, total_content_lines,
        content_kb, compressed_kb, elapsed_secs,
    );
    if elapsed.as_secs() >= 120 {
        tracing::warn!(
            elapsed_secs = elapsed.as_secs(),
            files = n_files,
            deletes = n_deletes,
            renames = n_renames,
            content_lines = total_content_lines,
            content_kb,
            compressed_kb,
            "{tag} slow batch: {:.1}s — {} files, {} deletes, {} renames, {} lines, {} KB content, {} KB compressed",
            elapsed_secs, n_files, n_deletes, n_renames, total_content_lines,
            content_kb, compressed_kb,
        );
    }

    // Skip the archive phase entirely when there is nothing to write.
    if normalized_files.is_empty() && request.rename_paths.is_empty() {
        tracing::debug!("{tag} skipping archive phase (no chunks to write)");
        return Ok(());
    }

    // Write a normalized BulkRequest as a .gz to to-archive/.
    timed!(tag, "write normalized gz", {
        let normalized_request = BulkRequest {
            source: request.source.clone(),
            files: normalized_files,
            delete_paths: request.delete_paths.clone(),
            scan_timestamp: request.scan_timestamp,
            indexing_failures: request.indexing_failures.clone(),
            rename_paths: request.rename_paths.clone(),
        };
        let json = serde_json::to_vec(&normalized_request)
            .context("serializing normalized request")?;
        let file_name = request_path.file_name()
            .context("request path has no filename")?;
        let to_archive_path = to_archive_dir.join(file_name);
        let out = std::fs::File::create(&to_archive_path)
            .context("creating to-archive file")?;
        let mut encoder = GzEncoder::new(out, flate2::Compression::default());
        encoder.write_all(&json).context("writing normalized gz")?;
        encoder.finish().context("finalizing normalized gz")?
    });

    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────────────────────

pub(super) fn is_db_locked(error: &anyhow::Error) -> bool {
    for cause in error.chain() {
        if let Some(rusqlite::Error::SqliteFailure(e, _)) = cause.downcast_ref::<rusqlite::Error>() {
            if matches!(e.code, ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) {
                return true;
            }
        }
    }
    false
}

pub(super) async fn handle_failure(path: &Path, failed_dir: &Path, error: anyhow::Error) {
    tracing::error!("Failed to process {}: {}", path.display(), error);

    let failed_path = failed_dir.join(path.file_name().unwrap());
    if let Err(e) = tokio::fs::rename(path, &failed_path).await {
        tracing::error!(
            "Failed to move {} to failed directory: {}",
            path.display(),
            e
        );
    } else {
        tracing::warn!("Moved failed request to: {}", failed_path.display());
    }
}
