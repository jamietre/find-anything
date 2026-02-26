use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};

use find_common::api::{BulkRequest, WorkerStatus};

use crate::archive::{self, ArchiveManager, ChunkRef};
use crate::db;

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

type StatusHandle = std::sync::Arc<std::sync::Mutex<WorkerStatus>>;

/// Start the inbox worker that processes index requests asynchronously.
/// Polls the inbox directory every second for new `.gz` files.
pub async fn start_inbox_worker(data_dir: PathBuf, status: StatusHandle) -> Result<()> {
    let inbox_dir = data_dir.join("inbox");
    tokio::fs::create_dir_all(&inbox_dir).await?;

    let failed_dir = inbox_dir.join("failed");
    tokio::fs::create_dir_all(&failed_dir).await?;

    tracing::info!("Starting inbox worker, monitoring: {}", inbox_dir.display());

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

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension() == Some(OsStr::new("gz")) {
                process_request_async(&data_dir, &path, &failed_dir, status.clone()).await;
            }
        }
    }
}

async fn process_request_async(
    data_dir: &Path,
    request_path: &Path,
    failed_dir: &Path,
    status: StatusHandle,
) {
    let status_reset = status.clone(); // held outside the closure to ensure Idle on any exit path

    let result = tokio::task::spawn_blocking({
        let data_dir = data_dir.to_path_buf();
        let request_path = request_path.to_path_buf();
        move || process_request(&data_dir, &request_path, &status)
    })
    .await;

    // Ensure idle is set even if process_request errored or panicked.
    if let Ok(mut guard) = status_reset.lock() {
        *guard = WorkerStatus::Idle;
    }

    match result {
        Ok(Ok(())) => {
            // Success - delete request file
            if let Err(e) = tokio::fs::remove_file(&request_path).await {
                tracing::error!("Failed to delete processed request {}: {}", request_path.display(), e);
            } else {
                tracing::info!("Successfully processed: {}", request_path.display());
            }
        }
        Ok(Err(e)) => {
            // Processing error - move to failed
            handle_failure(request_path, failed_dir, e).await;
        }
        Err(e) => {
            // Task panicked or was cancelled
            handle_failure(
                request_path,
                failed_dir,
                anyhow::anyhow!("Task error: {}", e),
            )
            .await;
        }
    }
}

fn process_request(data_dir: &Path, request_path: &Path, status: &StatusHandle) -> Result<()> {
    tracing::info!("Processing request: {}", request_path.display());

    // Decompress and parse request
    let compressed = std::fs::read(request_path)?;
    let mut decoder = GzDecoder::new(&compressed[..]);
    let mut json = String::new();
    decoder.read_to_string(&mut json)?;

    let request: BulkRequest = serde_json::from_str(&json)
        .context("parsing bulk request JSON")?;

    // Open source database
    let db_path = data_dir.join("sources").join(format!("{}.db", request.source));
    let conn = db::open(&db_path)?;

    // Initialize archive manager
    let mut archive_mgr = ArchiveManager::new(data_dir.to_path_buf());

    // 1. Deletions first (handles renames where path appears in both lists)
    if !request.delete_paths.is_empty() {
        db::delete_files(&conn, &mut archive_mgr, &request.delete_paths)?;
    }

    // 2. Upserts — update status per file so the UI shows what is being indexed
    for file in &request.files {
        if let Ok(mut guard) = status.lock() {
            *guard = WorkerStatus::Processing {
                source: request.source.clone(),
                file: file.path.clone(),
            };
        }
        process_file(&conn, &mut archive_mgr, file)?;
    }

    // 3. Indexing error tracking
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Clear errors for successfully (re-)indexed paths.
    let upserted: Vec<String> = request.files.iter().map(|f| f.path.clone()).collect();
    db::clear_errors_for_paths(&conn, &upserted)?;

    // Clear errors for explicitly deleted paths.
    db::clear_errors_for_paths(&conn, &request.delete_paths)?;

    // Store new extraction failures reported by the client.
    if !request.indexing_failures.is_empty() {
        db::upsert_indexing_errors(&conn, &request.indexing_failures, now)?;
    }

    // 4. Metadata
    if let Some(ts) = request.scan_timestamp {
        db::update_last_scan(&conn, ts)?;
        db::append_scan_history(&conn, ts)?;
    }
    if let Some(base_url) = &request.base_url {
        db::update_base_url(&conn, Some(base_url))?;
    }

    Ok(())
}

fn process_file(
    conn: &Connection,
    archive_mgr: &mut ArchiveManager,
    file: &find_common::api::IndexFile,
) -> Result<()> {
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // If this is an outer archive file being re-indexed, delete stale inner members
    // first. They'll be re-submitted as separate IndexFile entries in the same batch.
    // We detect "outer archive" by kind == "archive" and no "::" in the path.
    let is_outer_archive = file.kind == "archive" && !file.path.contains("::");
    if is_outer_archive {
        // Collect and remove chunks for all old inner members.
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
            archive_mgr.remove_chunks(old_refs)?;
        }
        conn.execute(
            "DELETE FROM files WHERE path LIKE ?1",
            rusqlite::params![like_pat],
        )?;
    }

    // ── Dedup check ────────────────────────────────────────────────────────
    // If the file has a content hash and another canonical with the same hash
    // exists, record this file as an alias and skip chunk/lines/FTS writes.
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
            // Register as alias — no chunks/lines/FTS written.
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
    // ── End dedup check ────────────────────────────────────────────────────

    // Remove old chunks for this specific file before writing new ones.
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
            archive_mgr.remove_chunks(old_refs)?;
        }
    }

    // Prepare lines for chunking (line_number, content)
    let line_data: Vec<(usize, String)> = file.lines.iter()
        .map(|l| (l.line_number, l.content.clone()))
        .collect();

    // Chunk lines into ~1KB pieces
    let chunk_result = archive::chunk_lines(&file.path, &line_data);

    // Append chunks to ZIP archives
    let chunk_refs = archive_mgr.append_chunks(chunk_result.chunks.clone())?;

    // Build mapping: chunk_number → chunk_ref
    let mut chunk_ref_map: HashMap<usize, ChunkRef> = HashMap::new();
    for (chunk, chunk_ref) in chunk_result.chunks.iter().zip(chunk_refs.iter()) {
        chunk_ref_map.insert(chunk.chunk_number, chunk_ref.clone());
    }

    // Upsert file record as canonical (canonical_file_id = NULL).
    // indexed_at is set on first insert and not overwritten on conflict.
    conn.execute(
        "INSERT INTO files (path, mtime, size, kind, indexed_at, extract_ms, content_hash, canonical_file_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL)
         ON CONFLICT(path) DO UPDATE SET
           mtime             = excluded.mtime,
           size              = excluded.size,
           kind              = excluded.kind,
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

    let file_id: i64 = conn.query_row(
        "SELECT id FROM files WHERE path = ?1",
        rusqlite::params![file.path],
        |row| row.get(0),
    )?;

    // Delete old lines for this file.
    // TODO(fts5-stale): When lines are deleted here (and via CASCADE from `DELETE FROM files`),
    // their corresponding `lines_fts` entries are NOT cleaned up because `lines_fts` is a
    // contentless FTS5 table (content='') with no triggers. Stale FTS5 rowids accumulate
    // over time, causing `fts_count` (and thus the `total` field in search responses) to be
    // inflated. Actual search results are correct because the JOIN with `lines` filters them
    // out, but pagination may misbehave if the client uses `total` to decide whether to
    // fetch more pages.
    conn.execute("DELETE FROM lines WHERE file_id = ?1", rusqlite::params![file_id])?;

    // Build lookup: line_number → original line for FTS5 content
    let mut line_content_map: HashMap<usize, String> = HashMap::new();
    for line in &file.lines {
        line_content_map.insert(line.line_number, line.content.clone());
    }

    // Insert lines with chunk references and populate FTS5
    for mapping in &chunk_result.line_mappings {
        let chunk_ref = chunk_ref_map.get(&mapping.chunk_number)
            .context("chunk ref not found")?;

        let line_content = line_content_map.get(&mapping.line_number)
            .context("line content not found")?;

        let line_id = conn.query_row(
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

        conn.execute(
            "INSERT INTO lines_fts(rowid, content) VALUES (?1, ?2)",
            rusqlite::params![line_id, line_content],
        )?;
    }

    Ok(())
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
