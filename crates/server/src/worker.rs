use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use rusqlite::{Connection, ErrorCode, OptionalExtension};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use find_common::api::{BulkRequest, IndexFile, IndexLine, IndexingFailure, WorkerStatus};

use crate::archive::{self, ArchiveManager, ChunkRef, SharedArchiveState};
use crate::db;

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// Log a warning if `start` is older than `threshold_secs`.
fn warn_slow(start: std::time::Instant, threshold_secs: u64, step: &str, context: &str) {
    let elapsed = start.elapsed();
    if elapsed.as_secs() >= threshold_secs {
        tracing::warn!(
            elapsed_secs = elapsed.as_secs(),
            step,
            context,
            "Slow step: {step} took {:.1}s for {context}",
            elapsed.as_secs_f64(),
        );
    }
}

type StatusHandle = std::sync::Arc<std::sync::Mutex<WorkerStatus>>;

/// Move any files stranded in `inbox/processing/` back to `inbox/`.
///
/// Called on every startup (including `--pause-inbox`) so that jobs
/// interrupted mid-flight by the previous run are always returned to the
/// queue before the server begins serving requests.
pub async fn recover_stranded_requests(data_dir: &Path) -> Result<()> {
    let inbox_dir = data_dir.join("inbox");
    tokio::fs::create_dir_all(&inbox_dir).await?;
    let failed_dir = inbox_dir.join("failed");
    tokio::fs::create_dir_all(&failed_dir).await?;
    let processing_dir = inbox_dir.join("processing");
    tokio::fs::create_dir_all(&processing_dir).await?;

    let mut stranded = tokio::fs::read_dir(&processing_dir).await?;
    while let Ok(Some(entry)) = stranded.next_entry().await {
        let src = entry.path();
        if src.extension() == Some(OsStr::new("gz")) {
            let dst = inbox_dir.join(entry.file_name());
            if let Err(e) = tokio::fs::rename(&src, &dst).await {
                tracing::warn!("Failed to recover stranded request {}: {e}", src.display());
            } else {
                tracing::info!("Recovered stranded request: {}", dst.display());
            }
        }
    }
    Ok(())
}

/// Start the inbox worker pool.
///
/// Spawns `num_workers` worker tasks that share a bounded channel. A router
/// loop polls the inbox directory every second, sorts pending files by
/// modification time (preserving submission order), and sends each path to
/// the channel. Workers pull paths and call `process_request` concurrently,
/// each using its own exclusively-owned ZIP archive for appending.
#[allow(clippy::too_many_arguments)]
pub async fn start_inbox_worker(
    data_dir: PathBuf,
    status: StatusHandle,
    log_batch_detail_limit: usize,
    num_workers: usize,
    request_timeout: std::time::Duration,
    delete_batch_size: usize,
    shared_archive_state: Arc<SharedArchiveState>,
    inbox_paused: Arc<AtomicBool>,
    deleted_bytes_since_scan: Arc<std::sync::atomic::AtomicU64>,
    delete_notify: Arc<tokio::sync::Notify>,
) -> Result<()> {
    let inbox_dir = data_dir.join("inbox");
    let failed_dir = inbox_dir.join("failed");
    let processing_dir = inbox_dir.join("processing");

    tracing::info!(
        "Starting inbox worker pool ({num_workers} workers): {}",
        inbox_dir.display()
    );

    // Bounded channel: backpressure prevents unbounded memory growth if
    // workers are slower than the router.
    let (tx, rx) = tokio::sync::mpsc::channel::<PathBuf>(256);
    let rx = Arc::new(tokio::sync::Mutex::new(rx));

    // Spawn worker tasks.
    for worker_idx in 0..num_workers {
        let rx = Arc::clone(&rx);
        let data_dir = data_dir.clone();
        let failed_dir = failed_dir.clone();
        let inbox_dir_clone = inbox_dir.clone();
        let status = status.clone();
        let shared = Arc::clone(&shared_archive_state);
        let deleted_bytes = Arc::clone(&deleted_bytes_since_scan);
        let notify = Arc::clone(&delete_notify);

        tokio::spawn(async move {
            tracing::debug!("Inbox worker {worker_idx} started");
            loop {
                let path = {
                    let mut guard = rx.lock().await;
                    match guard.recv().await {
                        Some(p) => p,
                        None => break, // channel closed
                    }
                    // Lock released here before processing — other workers can
                    // receive the next item immediately.
                };
                process_request_async(
                    &data_dir,
                    &path,
                    &failed_dir,
                    &inbox_dir_clone,
                    status.clone(),
                    log_batch_detail_limit,
                    request_timeout,
                    delete_batch_size,
                    &shared,
                    worker_idx,
                    &deleted_bytes,
                    &notify,
                )
                .await;
            }
            tracing::debug!("Inbox worker {worker_idx} exited");
        });
    }

    // Router loop: poll inbox, sort by mtime, claim files by moving them to
    // processing/, then send the new path to the worker channel.
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;

        let mut entries = match tokio::fs::read_dir(&inbox_dir).await {
            Ok(e) => e,
            Err(e) => {
                tracing::error!("Failed to read inbox dir: {e}");
                continue;
            }
        };

        let mut gz_files: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension() == Some(OsStr::new("gz")) {
                let mtime = entry.metadata().await
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(std::time::UNIX_EPOCH);
                gz_files.push((mtime, path));
            }
        }
        // Sort ascending by mtime so older submissions are processed first.
        gz_files.sort_unstable_by_key(|(mtime, _)| *mtime);

        // When paused, do not dispatch any new work — leave files in inbox/.
        if inbox_paused.load(Ordering::Relaxed) {
            continue;
        }

        for (_, inbox_path) in gz_files {
            let file_name = match inbox_path.file_name() {
                Some(n) => n.to_owned(),
                None => continue,
            };
            let processing_path = processing_dir.join(&file_name);

            // Atomically claim this file. If the rename fails (e.g. another
            // process already moved it) just skip — it will be retried or was
            // already handled.
            if let Err(e) = tokio::fs::rename(&inbox_path, &processing_path).await {
                tracing::warn!(
                    "Failed to claim {} for processing: {e}",
                    inbox_path.display()
                );
                continue;
            }

            if tx.send(processing_path).await.is_err() {
                tracing::error!("Worker channel closed unexpectedly; stopping router");
                return Ok(());
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn process_request_async(
    data_dir: &Path,
    request_path: &Path,
    failed_dir: &Path,
    inbox_dir: &Path,
    status: StatusHandle,
    log_batch_detail_limit: usize,
    request_timeout: std::time::Duration,
    delete_batch_size: usize,
    shared_archive_state: &Arc<SharedArchiveState>,
    worker_idx: usize,
    deleted_bytes_since_scan: &Arc<std::sync::atomic::AtomicU64>,
    delete_notify: &Arc<tokio::sync::Notify>,
) {
    let status_reset = status.clone();

    let blocking_task = tokio::task::spawn_blocking({
        let data_dir = data_dir.to_path_buf();
        let request_path = request_path.to_path_buf();
        let shared = Arc::clone(shared_archive_state);
        let deleted_bytes = Arc::clone(deleted_bytes_since_scan);
        let notify = Arc::clone(delete_notify);
        move || process_request(&data_dir, &request_path, &status, log_batch_detail_limit, delete_batch_size, &shared, worker_idx, &deleted_bytes, &notify)
    });

    let timed_result = tokio::time::timeout(request_timeout, blocking_task).await;

    if let Ok(mut guard) = status_reset.lock() {
        *guard = WorkerStatus::Idle;
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
            if let Err(e) = tokio::fs::remove_file(&request_path).await {
                tracing::error!(
                    "Failed to delete processed request {}: {}",
                    request_path.display(),
                    e
                );
            } else {
                tracing::info!("Successfully processed: {}", request_path.display());
            }
        }
        Ok(Ok(Err(e))) => {
            if is_db_locked(&e) {
                tracing::warn!(
                    "Worker {worker_idx}: database locked while processing {}, returning to inbox for retry: {e:#}",
                    request_path.display(),
                );
                // Move back to inbox so the router picks it up again.
                if let Some(file_name) = request_path.file_name() {
                    let retry_path = inbox_dir.join(file_name);
                    if let Err(e) = tokio::fs::rename(request_path, &retry_path).await {
                        tracing::error!(
                            "Worker {worker_idx}: failed to return {} to inbox: {e}",
                            request_path.display()
                        );
                    }
                }
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

#[allow(clippy::too_many_arguments)]
fn process_request(
    data_dir: &Path,
    request_path: &Path,
    status: &StatusHandle,
    log_batch_detail_limit: usize,
    delete_batch_size: usize,
    shared_archive_state: &Arc<SharedArchiveState>,
    worker_idx: usize,
    deleted_bytes_since_scan: &Arc<std::sync::atomic::AtomicU64>,
    delete_notify: &Arc<tokio::sync::Notify>,
) -> Result<()> {
    let request_start = std::time::Instant::now();

    let compressed = std::fs::read(request_path)?;
    let compressed_bytes = compressed.len();
    let mut decoder = GzDecoder::new(&compressed[..]);
    let mut json = String::new();
    decoder.read_to_string(&mut json)?;

    let request: BulkRequest = serde_json::from_str(&json)
        .context("parsing bulk request JSON")?;

    let n_files = request.files.len();
    let n_deletes = request.delete_paths.len();
    let total_content_lines: usize = request.files.iter().map(|f| f.lines.len()).sum();
    let total_content_bytes: usize = request.files.iter()
        .flat_map(|f| f.lines.iter())
        .map(|l| l.content.len())
        .sum();

    if n_deletes > 0 {
        tracing::info!("[worker {worker_idx}] Processing {} deletes [{}]", n_deletes, request.source);
    }
    if n_files <= log_batch_detail_limit {
        for f in &request.files {
            tracing::info!("[worker {worker_idx}] Indexing [{}] {}", request.source, f.path);
        }
    } else {
        tracing::info!("[worker {worker_idx}] Indexing {} files [{}]", n_files, request.source);
    }

    let db_path = data_dir.join("sources").join(format!("{}.db", request.source));
    let mut conn = db::open(&db_path)?;

    // Each worker owns its own ArchiveManager with an exclusively-allocated
    // write archive. The shared state coordinates archive number allocation
    // and serialises concurrent rewrites of the same old archive.
    let mut archive_mgr = ArchiveManager::new(Arc::clone(shared_archive_state));

    // Application-level per-source write lock.  SQLite's busy_timeout relies
    // on POSIX advisory file locks, which are unreliable on some mount types.
    // We acquire this for each chunk / file and release it immediately after,
    // so a large request (e.g. 80 k deletes in 100-item chunks) only holds the
    // lock for one chunk at a time — other workers on the same source slip in
    // between chunks without needing filesystem locking at all.
    let source_lock = shared_archive_state.source_lock(&request.source);

    // Process deletes in bounded chunks, releasing the source lock between
    // each chunk so other workers can slip in.
    if !request.delete_paths.is_empty() {
        if let Ok(mut guard) = status.lock() {
            *guard = WorkerStatus::Processing {
                source: request.source.clone(),
                file: format!("(deleting {} files)", n_deletes),
            };
        }
    }
    // Accumulate all chunk refs across every delete chunk before rewriting any
    // archives.  `remove_chunks` already groups by archive internally, so
    // calling it once for the whole request means each affected archive is
    // rewritten at most once — even if many paths in different chunks share the
    // same archive.  The source_lock is still released between DB chunks so
    // other workers on the same source can slip in between transactions.
    let mut all_delete_refs: Vec<archive::ChunkRef> = Vec::new();
    for chunk in request.delete_paths.chunks(delete_batch_size) {
        let refs = {
            let _guard = source_lock.lock().unwrap();
            db::delete_files(&conn, &archive_mgr, chunk)?
        };
        all_delete_refs.extend(refs);
    }
    if !all_delete_refs.is_empty() {
        let freed = archive_mgr.remove_chunks(all_delete_refs)?;
        if freed > 0 {
            deleted_bytes_since_scan.fetch_add(freed, std::sync::atomic::Ordering::Relaxed);
            delete_notify.notify_one();
        }
    }

    let mut server_side_failures: Vec<IndexingFailure> = Vec::new();
    let mut successfully_indexed: Vec<String> = Vec::new();
    for file in &request.files {
        if let Ok(mut guard) = status.lock() {
            *guard = WorkerStatus::Processing {
                source: request.source.clone(),
                file: file.path.clone(),
            };
        }
        let file_start = std::time::Instant::now();
        let _guard = source_lock.lock().unwrap();
        match process_file(&mut conn, &mut archive_mgr, file, false) {
            Ok(()) => {
                successfully_indexed.push(file.path.clone());
            }
            Err(e) => {
                if is_db_locked(&e) {
                    tracing::warn!("Failed to index {} (db locked, will retry): {e:#}", file.path);
                } else {
                    tracing::error!("Failed to index {}: {e:#}", file.path);
                }
                let (fallback, skip_inner) = if is_outer_archive(&file.path, &file.kind) {
                    (outer_archive_stub(file), true)
                } else {
                    (filename_only_file(file), false)
                };
                if let Err(e2) = process_file(&mut conn, &mut archive_mgr, &fallback, skip_inner) {
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
        warn_slow(file_start, 30, "process_file", &file.path);
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    {
        let _guard = source_lock.lock().unwrap();
        db::clear_errors_for_paths(&conn, &successfully_indexed)?;
        db::clear_errors_for_paths(&conn, &request.delete_paths)?;

        if !request.indexing_failures.is_empty() {
            db::upsert_indexing_errors(&conn, &request.indexing_failures, now)?;
        }
        if !server_side_failures.is_empty() {
            db::upsert_indexing_errors(&conn, &server_side_failures, now)?;
        }

        if let Some(ts) = request.scan_timestamp {
            db::update_last_scan(&conn, ts)?;
            db::append_scan_history(&conn, ts)?;
        }
    }

    let elapsed = request_start.elapsed();
    let elapsed_secs = elapsed.as_secs_f64();
    let content_kb = total_content_bytes / 1024;
    let compressed_kb = compressed_bytes / 1024;
    tracing::info!(
        "[worker {worker_idx}] batch complete [{}]: {} files, {} deletes, {} lines, \
         {} KB content, {} KB compressed, {:.1}s",
        request.source, n_files, n_deletes, total_content_lines,
        content_kb, compressed_kb, elapsed_secs,
    );
    if elapsed.as_secs() >= 120 {
        tracing::warn!(
            elapsed_secs = elapsed.as_secs(),
            files = n_files,
            deletes = n_deletes,
            content_lines = total_content_lines,
            content_kb,
            compressed_kb,
            "slow batch [{}]: {:.1}s — {} files, {} deletes, {} lines, {} KB content, {} KB compressed",
            request.source, elapsed_secs, n_files, n_deletes, total_content_lines,
            content_kb, compressed_kb,
        );
    }

    Ok(())
}

/// Returns `true` if `file` is a top-level archive (kind="archive" with no
/// "::" in the path).
pub(crate) fn is_outer_archive(path: &str, kind: &str) -> bool {
    kind == "archive" && !path.contains("::")
}

fn filename_only_file(file: &IndexFile) -> IndexFile {
    IndexFile {
        path: file.path.clone(),
        mtime: file.mtime,
        size: file.size,
        kind: if file.kind == "archive" { "unknown".to_string() } else { file.kind.clone() },
        lines: vec![IndexLine {
            archive_path: None,
            line_number: 0,
            content: file.path.clone(),
        }],
        extract_ms: None,
        content_hash: None,
        scanner_version: file.scanner_version,
    }
}

fn outer_archive_stub(file: &IndexFile) -> IndexFile {
    IndexFile {
        path: file.path.clone(),
        mtime: 0,
        size: file.size,
        kind: "archive".to_string(),
        lines: vec![IndexLine {
            archive_path: None,
            line_number: 0,
            content: file.path.clone(),
        }],
        extract_ms: None,
        content_hash: None,
        scanner_version: file.scanner_version,
    }
}

fn process_file(
    conn: &mut Connection,
    archive_mgr: &mut ArchiveManager,
    file: &find_common::api::IndexFile,
    skip_inner_delete: bool,
) -> Result<()> {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // If re-indexing an outer archive, delete stale inner members first.
    if !skip_inner_delete && is_outer_archive(&file.path, &file.kind) && file.mtime == 0 {
        let like_pat = format!("{}::%", file.path);
        let inner_ids: Vec<i64> = {
            let mut stmt = conn.prepare(
                "SELECT id FROM files WHERE path LIKE ?1",
            )?;
            let ids = stmt.query_map(rusqlite::params![like_pat], |row| row.get(0))?
                .collect::<rusqlite::Result<_>>()?;
            ids
        };
        let mut old_refs: Vec<ChunkRef> = Vec::new();
        for fid in inner_ids {
            let mut stmt = conn.prepare(
                "SELECT DISTINCT chunk_archive, chunk_name FROM lines WHERE file_id = ?1",
            )?;
            for r in stmt.query_map(rusqlite::params![fid], |row| {
                Ok(ChunkRef { archive_name: row.get(0)?, chunk_name: row.get(1)? })
            })? {
                old_refs.push(r?);
            }
        }
        if !old_refs.is_empty() {
            let t = std::time::Instant::now();
            archive_mgr.remove_chunks(old_refs)?;
            warn_slow(t, 10, "remove_chunks(archive_members)", &file.path);
        }
        conn.execute(
            "DELETE FROM files WHERE path LIKE ?1",
            rusqlite::params![like_pat],
        )?;
    }

    // Dedup check: if another canonical with the same content hash exists,
    // register this file as an alias and skip chunk/lines/FTS writes.
    if let Some(hash) = &file.content_hash {
        let canonical_id: Option<i64> = conn.query_row(
            "SELECT id FROM files
             WHERE content_hash = ?1
               AND canonical_file_id IS NULL
               AND path != ?2
             LIMIT 1",
            rusqlite::params![hash, file.path],
            |row| row.get(0),
        ).optional()?;

        if let Some(canonical_id) = canonical_id {
            conn.execute(
                "INSERT INTO files (path, mtime, size, kind, indexed_at, extract_ms, content_hash, canonical_file_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(path) DO UPDATE SET
                   mtime            = excluded.mtime,
                   size             = excluded.size,
                   kind             = excluded.kind,
                   extract_ms       = excluded.extract_ms,
                   content_hash     = excluded.content_hash,
                   canonical_file_id = excluded.canonical_file_id",
                rusqlite::params![
                    file.path, file.mtime, file.size, file.kind,
                    now_secs,
                    file.extract_ms.map(|ms| ms as i64),
                    hash,
                    canonical_id,
                ],
            )?;
            return Ok(());
        }
    }

    // Stale-mtime guard: skip if the stored mtime is already newer.
    // Defends against two workers processing requests for the same file out
    // of order (the worker with the older batch finishes after the newer one).
    let stored_mtime: Option<i64> = conn.query_row(
        "SELECT mtime FROM files WHERE path = ?1",
        rusqlite::params![file.path],
        |row| row.get(0),
    ).optional()?;
    if let Some(stored) = stored_mtime {
        if file.mtime > 0 && file.mtime < stored {
            tracing::debug!(
                "skipping stale upsert for {} (incoming mtime={} < stored={})",
                file.path, file.mtime, stored
            );
            return Ok(());
        }
    }

    // Remove old chunks for this file before writing new ones.
    let existing_id: Option<i64> = conn.query_row(
        "SELECT id FROM files WHERE path = ?1",
        rusqlite::params![file.path],
        |row| row.get(0),
    ).optional()?;

    if let Some(fid) = existing_id {
        let mut stmt = conn.prepare(
            "SELECT DISTINCT chunk_archive, chunk_name FROM lines WHERE file_id = ?1",
        )?;
        let old_refs: Vec<ChunkRef> = stmt
            .query_map(rusqlite::params![fid], |row| {
                Ok(ChunkRef { archive_name: row.get(0)?, chunk_name: row.get(1)? })
            })?
            .collect::<rusqlite::Result<_>>()?;
        if !old_refs.is_empty() {
            let t = std::time::Instant::now();
            archive_mgr.remove_chunks(old_refs)?;
            warn_slow(t, 10, "remove_chunks(reindex)", &file.path);
        }
    }

    let line_data: Vec<(usize, String)> = file.lines.iter()
        .map(|l| (l.line_number, l.content.clone()))
        .collect();

    let chunk_result = archive::chunk_lines(&file.path, &line_data);

    let t_append = std::time::Instant::now();
    let chunk_refs = archive_mgr.append_chunks(chunk_result.chunks.clone())?;
    warn_slow(t_append, 10, "append_chunks", &file.path);

    let mut chunk_ref_map: HashMap<usize, ChunkRef> = HashMap::new();
    for (chunk, chunk_ref) in chunk_result.chunks.iter().zip(chunk_refs.iter()) {
        chunk_ref_map.insert(chunk.chunk_number, chunk_ref.clone());
    }

    let mut line_content_map: HashMap<usize, String> = HashMap::new();
    for line in &file.lines {
        line_content_map.insert(line.line_number, line.content.clone());
    }

    let t_fts = std::time::Instant::now();
    let tx = conn.transaction()?;

    tx.execute(
        "INSERT INTO files (path, mtime, size, kind, indexed_at, extract_ms, content_hash, canonical_file_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)
         ON CONFLICT(path) DO UPDATE SET
           mtime             = excluded.mtime,
           size              = excluded.size,
           kind              = excluded.kind,
           indexed_at        = excluded.indexed_at,
           extract_ms        = excluded.extract_ms,
           content_hash      = excluded.content_hash,
           canonical_file_id = NULL",
        rusqlite::params![
            file.path, file.mtime, file.size, file.kind,
            now_secs,
            file.extract_ms.map(|ms| ms as i64),
            file.content_hash.as_deref(),
        ],
    )?;

    let file_id: i64 = tx.query_row(
        "SELECT id FROM files WHERE path = ?1",
        rusqlite::params![file.path],
        |row| row.get(0),
    )?;

    tx.execute("DELETE FROM lines WHERE file_id = ?1", rusqlite::params![file_id])?;

    for mapping in &chunk_result.line_mappings {
        let chunk_ref = chunk_ref_map.get(&mapping.chunk_number)
            .context("chunk ref not found")?;

        let line_content = line_content_map.get(&mapping.line_number)
            .context("line content not found")?;

        let line_id = tx.query_row(
            "INSERT INTO lines (file_id, line_number, chunk_archive, chunk_name, line_offset_in_chunk)
             VALUES (?1, ?2, ?3, ?4, ?5)
             RETURNING id",
            rusqlite::params![
                file_id,
                mapping.line_number as i64,
                chunk_ref.archive_name,
                chunk_ref.chunk_name,
                mapping.offset_in_chunk as i64,
            ],
            |row| row.get::<_, i64>(0),
        )?;

        tx.execute(
            "INSERT INTO lines_fts(rowid, content) VALUES (?1, ?2)",
            rusqlite::params![line_id, line_content],
        )?;
    }

    tx.commit()?;
    warn_slow(t_fts, 10, "fts_insert", &file.path);

    Ok(())
}

/// Returns true if `error` (or any cause in its chain) is a SQLite
/// "database is locked" / "database is busy" error.  These are transient:
/// the file should stay in the inbox and be retried on the next poll cycle.
fn is_db_locked(error: &anyhow::Error) -> bool {
    for cause in error.chain() {
        if let Some(rusqlite::Error::SqliteFailure(e, _)) = cause.downcast_ref::<rusqlite::Error>() {
            if matches!(e.code, ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked) {
                return true;
            }
        }
    }
    false
}

async fn handle_failure(path: &Path, failed_dir: &Path, error: anyhow::Error) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use find_common::api::IndexLine;

    fn make_file(path: &str, kind: &str) -> IndexFile {
        IndexFile {
            path: path.to_string(),
            mtime: 1000,
            size: Some(100),
            kind: kind.to_string(),
            lines: vec![IndexLine {
                archive_path: None,
                line_number: 0,
                content: path.to_string(),
            }],
            extract_ms: None,
            content_hash: None,
            scanner_version: 0,
        }
    }

    #[test]
    fn outer_archive_detected() {
        assert!(is_outer_archive("data.zip", "archive"));
    }

    #[test]
    fn archive_member_not_outer() {
        assert!(!is_outer_archive("data.zip::inner.txt", "archive"));
    }

    #[test]
    fn non_archive_kind_not_outer() {
        assert!(!is_outer_archive("data.zip", "text"));
    }

    #[test]
    fn filename_only_converts_archive_kind_to_unknown() {
        let f = make_file("data.zip", "archive");
        let fallback = filename_only_file(&f);
        assert_eq!(fallback.kind, "unknown");
    }

    #[test]
    fn filename_only_keeps_non_archive_kind() {
        let f = make_file("notes.md", "text");
        let fallback = filename_only_file(&f);
        assert_eq!(fallback.kind, "text");
    }

    #[test]
    fn filename_only_has_single_path_line() {
        let f = make_file("docs/report.pdf", "pdf");
        let fallback = filename_only_file(&f);
        assert_eq!(fallback.lines.len(), 1);
        assert_eq!(fallback.lines[0].line_number, 0);
        assert_eq!(fallback.lines[0].content, "docs/report.pdf");
    }

    #[test]
    fn filename_only_preserves_mtime_and_size() {
        let f = make_file("file.txt", "text");
        let fallback = filename_only_file(&f);
        assert_eq!(fallback.mtime, f.mtime);
        assert_eq!(fallback.size, f.size);
    }

    #[test]
    fn outer_archive_stub_preserves_archive_kind() {
        let f = make_file("backup.7z", "archive");
        let stub = outer_archive_stub(&f);
        assert_eq!(stub.kind, "archive");
    }

    #[test]
    fn outer_archive_stub_uses_zero_mtime() {
        let f = make_file("backup.7z", "archive");
        let stub = outer_archive_stub(&f);
        assert_eq!(stub.mtime, 0);
    }

    #[test]
    fn outer_archive_stub_has_single_path_line() {
        let f = make_file("backups/big.tar.gz", "archive");
        let stub = outer_archive_stub(&f);
        assert_eq!(stub.lines.len(), 1);
        assert_eq!(stub.lines[0].line_number, 0);
        assert_eq!(stub.lines[0].content, "backups/big.tar.gz");
    }
}
