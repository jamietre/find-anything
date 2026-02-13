use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use rusqlite::Connection;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::Read;
use std::path::{Path, PathBuf};

use find_common::api::UpsertRequest;

use crate::archive::{self, ArchiveManager, ChunkRef};
use crate::db;

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

/// Start the inbox worker that processes index requests asynchronously.
/// Polls the inbox directory every second for new `.gz` files.
pub async fn start_inbox_worker(data_dir: PathBuf) -> Result<()> {
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
                process_request_async(&data_dir, &path, &failed_dir).await;
            }
        }
    }
}

async fn process_request_async(data_dir: &Path, request_path: &Path, failed_dir: &Path) {
    let result = tokio::task::spawn_blocking({
        let data_dir = data_dir.to_path_buf();
        let request_path = request_path.to_path_buf();
        move || process_request(&data_dir, &request_path)
    })
    .await;

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

fn process_request(data_dir: &Path, request_path: &Path) -> Result<()> {
    tracing::info!("Processing request: {}", request_path.display());

    // Decompress and parse request
    let compressed = std::fs::read(request_path)?;
    let mut decoder = GzDecoder::new(&compressed[..]);
    let mut json = String::new();
    decoder.read_to_string(&mut json)?;

    let request: UpsertRequest = serde_json::from_str(&json)
        .context("parsing index request JSON")?;

    // Open source database
    let db_path = data_dir.join("sources").join(format!("{}.db", request.source));
    let conn = db::open(&db_path)?;

    // Initialize archive manager
    let mut archive_mgr = ArchiveManager::new(data_dir.to_path_buf());

    // Process files: chunk content, append to archives, update database
    for file in &request.files {
        process_file(&conn, &mut archive_mgr, file)?;
    }

    // Update metadata
    db::update_last_scan(&conn, chrono::Utc::now().timestamp())?;
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
    // Prepare lines for chunking (line_number, content)
    let line_data: Vec<(usize, String)> = file.lines.iter()
        .map(|l| (l.line_number, l.content.clone()))
        .collect();

    // Chunk lines into ~1KB pieces
    let chunk_result = archive::chunk_lines(&file.path, None, &line_data);

    // Append chunks to ZIP archives
    let chunk_refs = archive_mgr.append_chunks(chunk_result.chunks.clone())?;

    // Build mapping: chunk_number → chunk_ref
    let mut chunk_ref_map: HashMap<usize, ChunkRef> = HashMap::new();
    for (chunk, chunk_ref) in chunk_result.chunks.iter().zip(chunk_refs.iter()) {
        chunk_ref_map.insert(chunk.chunk_number, chunk_ref.clone());
    }

    // Upsert file record
    conn.execute(
        "INSERT INTO files (path, mtime, size, kind)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(path) DO UPDATE SET
           mtime = excluded.mtime,
           size  = excluded.size,
           kind  = excluded.kind",
        rusqlite::params![file.path, file.mtime, file.size, file.kind],
    )?;

    let file_id: i64 = conn.query_row(
        "SELECT id FROM files WHERE path = ?1",
        rusqlite::params![file.path],
        |row| row.get(0),
    )?;

    // Delete old lines for this file
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

        // Insert into lines table with offset information
        let line_id = conn.query_row(
            "INSERT INTO lines (file_id, archive_path, line_number, chunk_archive, chunk_name, line_offset_in_chunk)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             RETURNING id",
            rusqlite::params![
                file_id,
                None::<String>, // archive_path handled separately if needed
                mapping.line_number as i64,
                chunk_ref.archive_name,
                chunk_ref.chunk_name,
                mapping.offset_in_chunk as i64,
            ],
            |row| row.get::<_, i64>(0),
        )?;

        // Populate FTS5 with content (still in memory)
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
