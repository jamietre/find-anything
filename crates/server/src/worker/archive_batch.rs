/// Archive phase (phase 2) of the inbox worker.
///
/// Reads `.gz` files from `inbox/to-archive/`, parses them, appends content
/// chunks to ZIP archives, and inserts `content_chunks` rows in SQLite.
/// Separated from the phase-1 indexing loop so neither phase blocks the other.
///
/// # Two-phase design
///
/// Within `run_archive_batch` there are two sub-phases:
///
/// **Sub-phase A (ZIP I/O):** for each gz file, one at a time:
///   - Stream-parse the `BulkRequest` (no intermediate buffers).
///   - Read file metadata from SQLite (no write lock needed).
///   - Append content chunks to ZIP archives via the shared `ArchiveManager`.
///   - Collect lightweight `ArchivedFile` metadata (block IDs, chunk refs —
///     no content strings).
///   - Drop the `BulkRequest`; only metadata remains in memory.
///
/// **Sub-phase B (SQLite writes):** once all ZIP I/O is done, group results by
/// source and issue **one** transaction per source covering all gz files in the
/// batch. This preserves the original single-transaction efficiency regardless
/// of how many gz files are in the batch.
///
/// gz files are deleted only after their source's SQLite write succeeds, so a
/// write failure leaves them available for a future retry.
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use rusqlite::OptionalExtension;

use crate::archive::{self, ArchiveManager, ChunkRef, SharedArchiveState};
use crate::db;
use super::WorkerConfig;

macro_rules! timed {
    ($tag:expr, $label:expr, $body:expr) => {{
        tracing::debug!("{} → {}", $tag, $label);
        let __t = std::time::Instant::now();
        let __r = $body;
        tracing::debug!("{} ← {} ({:.1}ms)", $tag, $label, __t.elapsed().as_secs_f64() * 1000.0);
        __r
    }};
}

// ── Types ─────────────────────────────────────────────────────────────────────

/// Lightweight record of one successfully ZIP-archived file.
/// Contains only metadata (IDs and chunk location refs) — no content strings.
/// Accumulated across all gz files in the batch and written to SQLite in bulk.
struct ArchivedFile {
    block_id: i64,
    chunk_ranges: Vec<archive::ChunkRange>,
    chunk_refs: Vec<ChunkRef>,
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Scan `to_archive_dir` for `.gz` files, process up to `cfg.archive_batch_size`
/// of them through the archive phase, and return the number processed.
pub(super) fn run_archive_batch(
    data_dir: &Path,
    to_archive_dir: &Path,
    cfg: WorkerConfig,
    shared_archive_state: &Arc<SharedArchiveState>,
) -> Result<usize> {
    let mut gz_files: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(to_archive_dir)?.flatten() {
        let path = entry.path();
        if path.extension() == Some(OsStr::new("gz")) {
            let mtime = entry.metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(std::time::UNIX_EPOCH);
            gz_files.push((mtime, path));
        }
    }
    gz_files.sort_unstable_by_key(|(mtime, _)| *mtime);

    if gz_files.is_empty() {
        return Ok(0);
    }

    let batch: Vec<PathBuf> = gz_files.into_iter()
        .take(cfg.archive_batch_size)
        .map(|(_, p)| p)
        .collect();
    let n_processed = batch.len();

    // One ArchiveManager per source, shared across all gz files so ZIP archives
    // are packed efficiently (the current open archive carries over between
    // requests for the same source).
    let mut archive_managers: HashMap<String, ArchiveManager> = HashMap::new();

    // --- Sub-phase A: ZIP I/O, one gz at a time ---
    //
    // Each entry is `(gz_path, source, archived_files)` for a successfully
    // processed gz, or `None` for a failed one (left in to-archive/ for retry).
    let mut phase_a_results: Vec<(PathBuf, String, Vec<ArchivedFile>)> = Vec::new();

    for gz_path in batch {
        match zip_phase_for_gz(data_dir, &gz_path, shared_archive_state, &mut archive_managers) {
            Ok((source, archived_files)) => {
                phase_a_results.push((gz_path, source, archived_files));
            }
            Err(e) => {
                tracing::error!(
                    "Archive batch: ZIP phase failed for {}: {e:#}",
                    gz_path.display()
                );
                // Leave the file in to-archive/ for the next batch tick.
            }
        }
    }

    // --- Sub-phase B: SQLite writes, one transaction per source ---
    //
    // Group the phase-A results by source so we open one DB connection and
    // acquire the source lock exactly once per source per batch, regardless of
    // how many gz files that source had.
    let mut by_source: HashMap<String, (Vec<PathBuf>, Vec<ArchivedFile>)> = HashMap::new();
    for (gz_path, source, archived_files) in phase_a_results {
        let entry = by_source.entry(source).or_default();
        entry.0.push(gz_path);
        entry.1.extend(archived_files);
    }

    for (source, (gz_paths, archived_files)) in by_source {
        match write_content_chunks(data_dir, &source, &archived_files, shared_archive_state) {
            Ok(()) => {
                tracing::info!(
                    "[archive:{source}] {} requests: archived {} files",
                    gz_paths.len(),
                    archived_files.len(),
                );
                for gz_path in &gz_paths {
                    if let Err(e) = std::fs::remove_file(gz_path) {
                        tracing::error!(
                            "Archive batch: failed to delete {}: {e}",
                            gz_path.display()
                        );
                    }
                }
            }
            Err(e) => {
                tracing::error!(
                    "Archive batch: SQLite write failed for source {source}: {e:#}. \
                     Leaving {} gz file(s) for retry.",
                    gz_paths.len()
                );
                // gz files are left in to-archive/ — they will be reprocessed.
                // The already_archived check prevents double-writing chunks.
            }
        }
    }

    Ok(n_processed)
}

// ── Internal ──────────────────────────────────────────────────────────────────

/// Sub-phase A for one gz file: stream-parse, check DB metadata, write chunks
/// to ZIP archives, and return lightweight `ArchivedFile` metadata.
///
/// The `BulkRequest` is dropped before this function returns; only the metadata
/// (`block_id`, chunk ranges and refs — no content strings) escapes.
///
/// **Stale-content check:** Phase 1 may have processed multiple requests for
/// the same file path. The DB `files.content_hash` reflects the *latest*
/// indexed version. If this request's `IndexFile.content_hash` differs, the
/// content is stale — a later gz will archive the correct version.
fn zip_phase_for_gz(
    data_dir: &Path,
    gz_path: &Path,
    shared_archive_state: &Arc<SharedArchiveState>,
    archive_managers: &mut HashMap<String, ArchiveManager>,
) -> Result<(String, Vec<ArchivedFile>)> {
    let request = parse_gz_request(gz_path)?;
    let source = request.source;
    let files = request.files; // move out; request is now (mostly) dropped
    let tag = format!("[archive:{source}]");

    let db_path = data_dir.join("sources").join(format!("{source}.db"));
    let conn = db::open(&db_path)
        .with_context(|| format!("opening DB for source {source}"))?;

    let archive_mgr = archive_managers
        .entry(source.clone())
        .or_insert_with(|| ArchiveManager::new(Arc::clone(shared_archive_state)));

    // Collect work items.  Consuming `files` lets us move `file.lines` into
    // `line_data` without cloning the content strings.
    struct ArchiveWork {
        #[allow(dead_code)]
        file_id: i64,
        block_id: i64,
        path: String,
        line_data: Vec<(usize, String)>,
    }
    let mut archive_works: Vec<ArchiveWork> = Vec::new();
    let mut seen_block_ids: HashSet<i64> = HashSet::new();

    for file in files {
        let file_row: Option<(i64, Option<String>)> = conn.query_row(
            "SELECT id, content_hash FROM files WHERE path = ?1",
            rusqlite::params![file.path],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional().unwrap_or(None);

        let Some((file_id, Some(db_content_hash))) = file_row else {
            continue;
        };

        // Skip stale content: if this gz carries an older version of the file,
        // the newer gz will archive the correct content when it is processed.
        if let Some(ref file_hash) = file.content_hash {
            if file_hash != &db_content_hash {
                tracing::debug!(
                    "{tag} skipping {} (stale: gz={}, DB={})",
                    file.path, file_hash, db_content_hash
                );
                continue;
            }
        }

        let block_id: Option<i64> = conn.query_row(
            "SELECT id FROM content_blocks WHERE content_hash = ?1",
            rusqlite::params![&db_content_hash],
            |row| row.get(0),
        ).optional().unwrap_or(None);

        let Some(block_id) = block_id else { continue; };

        let already_archived: i64 = conn.query_row(
            "SELECT COUNT(*) FROM content_chunks WHERE block_id = ?1",
            rusqlite::params![block_id],
            |row| row.get(0),
        ).unwrap_or(0);
        if already_archived > 0 { continue; }

        let is_inline: i64 = conn.query_row(
            "SELECT COUNT(*) FROM file_content WHERE file_id = ?1",
            rusqlite::params![file_id],
            |row| row.get(0),
        ).unwrap_or(0);
        if is_inline > 0 { continue; }

        // Move lines by value — no clone of content strings.
        let line_data: Vec<(usize, String)> = file.lines.into_iter()
            .map(|l| (l.line_number, l.content))
            .collect();

        if line_data.is_empty() { continue; }

        if !seen_block_ids.insert(block_id) { continue; }

        archive_works.push(ArchiveWork { file_id, block_id, path: file.path, line_data });
    }
    // `files` (and all content strings in it) is now fully consumed and dropped.

    if archive_works.is_empty() {
        return Ok((source, Vec::new()));
    }

    // Append chunks to ZIP archives; collect lightweight metadata.
    let n_works = archive_works.len();
    let mut archived_files: Vec<ArchivedFile> = Vec::new();

    timed!(tag, format!("append chunks for {n_works} files"), {
        for work in archive_works {
            let chunk_result = archive::chunk_lines(work.block_id, &work.line_data);
            // line_data content strings are freed after chunk_lines returns.
            match archive_mgr.append_chunks(chunk_result.chunks) {
                Ok(chunk_refs) => {
                    archived_files.push(ArchivedFile {
                        block_id: work.block_id,
                        chunk_ranges: chunk_result.ranges,
                        chunk_refs,
                    });
                }
                Err(e) => {
                    tracing::error!("{tag} failed to append chunks for {}: {e:#}", work.path);
                }
            }
            // work (and any remaining line_data) is dropped at end of iteration.
        }
    });

    Ok((source, archived_files))
}

/// Sub-phase B for one source: open one DB connection, acquire the source lock
/// once, and commit all `content_chunks` rows in a single transaction.
fn write_content_chunks(
    data_dir: &Path,
    source: &str,
    archived_files: &[ArchivedFile],
    shared_archive_state: &Arc<SharedArchiveState>,
) -> Result<()> {
    if archived_files.is_empty() {
        return Ok(());
    }

    let tag = format!("[archive:{source}]");
    let db_path = data_dir.join("sources").join(format!("{source}.db"));
    let conn = db::open(&db_path)
        .with_context(|| format!("opening DB for source {source}"))?;
    let source_lock = shared_archive_state.source_lock(source);

    let _guard = timed!(tag, "acquire source lock for chunk insert", {
        source_lock.lock()
            .map_err(|_| anyhow::anyhow!("source lock poisoned for {source}"))?
    });

    timed!(tag, format!("insert content_chunks for {} files", archived_files.len()), {
        let tx = conn.unchecked_transaction()?;
        for af in archived_files {
            // Re-check inside the transaction: a concurrent batch (e.g. from a
            // parallel source) may have committed these chunks since the
            // already_archived pre-scan check above.
            let already_committed: i64 = tx.query_row(
                "SELECT COUNT(*) FROM content_chunks WHERE block_id = ?1",
                rusqlite::params![af.block_id],
                |r| r.get(0),
            )?;
            if already_committed > 0 {
                tracing::debug!(
                    "block_id={} already committed by concurrent batch — skipping",
                    af.block_id
                );
                continue;
            }

            let chunk_ref_by_number: HashMap<usize, &ChunkRef> =
                af.chunk_refs.iter().enumerate().collect();

            for range in &af.chunk_ranges {
                let Some(chunk_ref) = chunk_ref_by_number.get(&range.chunk_number) else {
                    continue;
                };

                tx.execute(
                    "INSERT OR IGNORE INTO content_archives(name) VALUES(?1)",
                    rusqlite::params![chunk_ref.archive_name],
                )?;
                let archive_id: i64 = tx.query_row(
                    "SELECT id FROM content_archives WHERE name = ?1",
                    rusqlite::params![chunk_ref.archive_name],
                    |r| r.get(0),
                )?;

                if let Err(e) = tx.execute(
                    "INSERT INTO content_chunks(block_id, chunk_number, archive_id, start_line, end_line)
                     VALUES(?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![
                        af.block_id,
                        range.chunk_number as i64,
                        archive_id,
                        range.start_line as i64,
                        range.end_line as i64,
                    ],
                ) {
                    tracing::error!(
                        "{tag} failed to insert content_chunk for block_id={}: {e}",
                        af.block_id
                    );
                }
            }
        }
        tx.commit()?;
    });

    Ok(())
}

pub(super) fn parse_gz_request(gz_path: &Path) -> Result<find_common::api::BulkRequest> {
    let file = std::fs::File::open(gz_path)
        .with_context(|| format!("opening {}", gz_path.display()))?;
    let decoder = flate2::read::GzDecoder::new(BufReader::new(file));
    serde_json::from_reader(decoder).context("parsing bulk request JSON")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    use find_common::api::{BulkRequest, FileKind, IndexFile, IndexLine};
    use find_common::config::NormalizationSettings;

    use crate::db::encode_fts_rowid;

    fn setup_data_dir(data_dir: &Path) {
        std::fs::create_dir_all(data_dir.join("sources").join("content")).unwrap();
    }

    fn read_chunk_ranges(conn: &rusqlite::Connection, block_id: i64) -> Vec<(i64, i64)> {
        let mut stmt = conn.prepare(
            "SELECT start_line, end_line FROM content_chunks WHERE block_id = ?1 ORDER BY chunk_number"
        ).unwrap();
        stmt.query_map(rusqlite::params![block_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        }).unwrap().map(|r| r.unwrap()).collect()
    }

    fn write_bulk_gz(path: &Path, req: &BulkRequest) {
        let json = serde_json::to_vec(req).unwrap();
        let file = std::fs::File::create(path).unwrap();
        let mut enc = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        enc.write_all(&json).unwrap();
        enc.finish().unwrap();
    }

    fn make_worker_config() -> WorkerConfig {
        WorkerConfig {
            request_timeout: std::time::Duration::from_secs(30),
            inline_threshold_bytes: 0,
            archive_batch_size: 10,
            activity_log_max_entries: 100,
            normalization: NormalizationSettings::default(),
        }
    }

    fn make_bulk_request(source: &str, path: &str, content: &str) -> BulkRequest {
        BulkRequest {
            source: source.to_string(),
            files: vec![IndexFile {
                path: path.to_string(),
                mtime: 1000,
                size: Some(content.len() as i64),
                kind: FileKind::Text,
                scanner_version: 1,
                lines: vec![
                    IndexLine { archive_path: None, line_number: 0, content: path.to_string() },
                    IndexLine { archive_path: None, line_number: 1, content: content.to_string() },
                ],
                extract_ms: None,
                content_hash: Some("testhash".to_string()),
                is_new: true,
            }],
            delete_paths: vec![],
            rename_paths: vec![],
            scan_timestamp: None,
            indexing_failures: vec![],
        }
    }

    /// Seed the DB with a file + content_block + FTS entries (no content_chunks yet).
    fn seed_db(data_dir: &Path, source: &str, path: &str) -> (rusqlite::Connection, i64, i64) {
        let db_path = data_dir.join("sources").join(format!("{source}.db"));
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let conn = crate::db::open(&db_path).unwrap();

        conn.execute(
            "INSERT INTO files (path, mtime, size, kind, indexed_at, extract_ms, content_hash, line_count)
             VALUES (?1, 1000, 100, 'text', 0, NULL, 'testhash', 2)",
            rusqlite::params![path],
        ).unwrap();
        let file_id = conn.last_insert_rowid();

        conn.execute("INSERT OR IGNORE INTO content_blocks(content_hash) VALUES('testhash')", []).unwrap();
        let block_id: i64 = conn.query_row(
            "SELECT id FROM content_blocks WHERE content_hash = 'testhash'",
            [], |r| r.get(0),
        ).unwrap();

        conn.execute(
            "INSERT INTO lines_fts(rowid, content) VALUES (?1, ?2)",
            rusqlite::params![encode_fts_rowid(file_id, 0), path],
        ).unwrap();
        conn.execute(
            "INSERT INTO lines_fts(rowid, content) VALUES (?1, ?2)",
            rusqlite::params![encode_fts_rowid(file_id, 1), "hello world"],
        ).unwrap();

        (conn, file_id, block_id)
    }

    #[test]
    fn chunks_written_and_content_chunks_inserted() {
        let data_tmp = tempfile::tempdir().unwrap();
        let to_archive_tmp = tempfile::tempdir().unwrap();
        let data_dir = data_tmp.path();
        let to_archive_dir = to_archive_tmp.path();
        setup_data_dir(data_dir);

        let (conn, _file_id, block_id) = seed_db(data_dir, "test_source", "docs/readme.txt");
        write_bulk_gz(&to_archive_dir.join("batch_001.gz"), &make_bulk_request("test_source", "docs/readme.txt", "hello world"));

        let processed = run_archive_batch(data_dir, to_archive_dir, make_worker_config(),
            &crate::archive::SharedArchiveState::new(data_dir.to_path_buf()).unwrap()).unwrap();
        assert_eq!(processed, 1);

        let gz_count = std::fs::read_dir(to_archive_dir).unwrap().flatten()
            .filter(|e| e.path().extension().map_or(false, |x| x == "gz")).count();
        assert_eq!(gz_count, 0, "gz should be removed after processing");

        let ranges = read_chunk_ranges(&conn, block_id);
        assert!(!ranges.is_empty(), "content_chunks should have entries");

        let content_dir = data_dir.join("sources").join("content");
        let zip_count = std::fs::read_dir(&content_dir).unwrap().flatten()
            .filter(|e| e.path().is_dir())
            .flat_map(|d| std::fs::read_dir(d.path()).unwrap().flatten())
            .filter(|e| e.path().extension().map_or(false, |x| x == "zip")).count();
        assert!(zip_count > 0, "at least one ZIP archive should have been created");
    }

    #[test]
    fn gz_file_removed_after_processing() {
        let data_tmp = tempfile::tempdir().unwrap();
        let to_archive_tmp = tempfile::tempdir().unwrap();
        let data_dir = data_tmp.path();
        let to_archive_dir = to_archive_tmp.path();
        setup_data_dir(data_dir);
        // No DB — every file lookup returns nothing; gz is still deleted.
        write_bulk_gz(&to_archive_dir.join("ghost_001.gz"), &make_bulk_request("ghost_source", "nonexistent.txt", "x"));

        let processed = run_archive_batch(data_dir, to_archive_dir, make_worker_config(),
            &crate::archive::SharedArchiveState::new(data_dir.to_path_buf()).unwrap()).unwrap();
        assert_eq!(processed, 1);

        let gz_count = std::fs::read_dir(to_archive_dir).unwrap().flatten()
            .filter(|e| e.path().extension().map_or(false, |x| x == "gz")).count();
        assert_eq!(gz_count, 0, "gz should be removed even when file has no DB entry");
    }

    #[test]
    fn already_archived_file_is_skipped() {
        let data_tmp = tempfile::tempdir().unwrap();
        let to_archive_tmp = tempfile::tempdir().unwrap();
        let data_dir = data_tmp.path();
        let to_archive_dir = to_archive_tmp.path();
        setup_data_dir(data_dir);

        let (conn, _file_id, block_id) = seed_db(data_dir, "test_source", "docs/readme.txt");
        let shared = crate::archive::SharedArchiveState::new(data_dir.to_path_buf()).unwrap();

        write_bulk_gz(&to_archive_dir.join("first_001.gz"), &make_bulk_request("test_source", "docs/readme.txt", "hello world"));
        run_archive_batch(data_dir, to_archive_dir, make_worker_config(), &shared).unwrap();
        let ranges_after_first = read_chunk_ranges(&conn, block_id);
        assert!(!ranges_after_first.is_empty());

        write_bulk_gz(&to_archive_dir.join("second_001.gz"), &make_bulk_request("test_source", "docs/readme.txt", "hello world"));
        run_archive_batch(data_dir, to_archive_dir, make_worker_config(), &shared).unwrap();
        let ranges_after_second = read_chunk_ranges(&conn, block_id);
        assert_eq!(ranges_after_first, ranges_after_second, "content_chunks should not change on second run");
    }

    /// Stale content (gz hash ≠ DB hash) must not be archived under the wrong
    /// block_id. The gz is still deleted (it was processed without error).
    #[test]
    fn stale_content_hash_is_skipped() {
        let data_tmp = tempfile::tempdir().unwrap();
        let to_archive_tmp = tempfile::tempdir().unwrap();
        let data_dir = data_tmp.path();
        let to_archive_dir = to_archive_tmp.path();
        setup_data_dir(data_dir);

        // DB has "newhash" (simulating Phase 1 having processed a newer request).
        let db_path = data_dir.join("sources").join("test_source.db");
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let conn = crate::db::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO files (path, mtime, size, kind, indexed_at, extract_ms, content_hash, line_count)
             VALUES ('docs/readme.txt', 2000, 100, 'text', 0, NULL, 'newhash', 1)",
            [],
        ).unwrap();
        conn.execute("INSERT OR IGNORE INTO content_blocks(content_hash) VALUES('newhash')", []).unwrap();
        conn.execute("INSERT OR IGNORE INTO content_blocks(content_hash) VALUES('oldhash')", []).unwrap();
        let block_id_new: i64 = conn.query_row(
            "SELECT id FROM content_blocks WHERE content_hash = 'newhash'", [], |r| r.get(0),
        ).unwrap();

        // gz carries "oldhash" — stale.
        let stale_req = BulkRequest {
            source: "test_source".to_string(),
            files: vec![IndexFile {
                path: "docs/readme.txt".to_string(),
                mtime: 1000, size: Some(10), kind: FileKind::Text, scanner_version: 1,
                lines: vec![IndexLine { archive_path: None, line_number: 1, content: "old content".to_string() }],
                extract_ms: None,
                content_hash: Some("oldhash".to_string()),
                is_new: false,
            }],
            delete_paths: vec![], rename_paths: vec![], scan_timestamp: None, indexing_failures: vec![],
        };
        write_bulk_gz(&to_archive_dir.join("stale_001.gz"), &stale_req);

        run_archive_batch(data_dir, to_archive_dir, make_worker_config(),
            &crate::archive::SharedArchiveState::new(data_dir.to_path_buf()).unwrap()).unwrap();

        let gz_count = std::fs::read_dir(to_archive_dir).unwrap().flatten()
            .filter(|e| e.path().extension().map_or(false, |x| x == "gz")).count();
        assert_eq!(gz_count, 0, "stale gz should be deleted");

        let ranges = read_chunk_ranges(&conn, block_id_new);
        assert!(ranges.is_empty(), "stale content must not be archived under newhash block_id");
    }

    /// Multiple gz files for the same source should use a single SQLite
    /// transaction (sub-phase B). This test verifies both are archived correctly
    /// and both gz files are deleted.
    #[test]
    fn multiple_gz_files_same_source_both_archived() {
        let data_tmp = tempfile::tempdir().unwrap();
        let to_archive_tmp = tempfile::tempdir().unwrap();
        let data_dir = data_tmp.path();
        let to_archive_dir = to_archive_tmp.path();
        setup_data_dir(data_dir);

        let db_path = data_dir.join("sources").join("src.db");
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let conn = crate::db::open(&db_path).unwrap();

        // Seed two files with different hashes.
        for (path, hash, fid_offset) in [("a.txt", "hash_a", 0i64), ("b.txt", "hash_b", 1)] {
            conn.execute(
                "INSERT INTO files (path, mtime, size, kind, indexed_at, extract_ms, content_hash, line_count)
                 VALUES (?1, 1000, 50, 'text', 0, NULL, ?2, 2)",
                rusqlite::params![path, hash],
            ).unwrap();
            let file_id = conn.last_insert_rowid();
            conn.execute("INSERT OR IGNORE INTO content_blocks(content_hash) VALUES(?1)", rusqlite::params![hash]).unwrap();
            conn.execute(
                "INSERT INTO lines_fts(rowid, content) VALUES (?1, ?2)",
                rusqlite::params![encode_fts_rowid(file_id, 0), path],
            ).unwrap();
            let _ = fid_offset; // suppress warning
        }

        let block_id_a: i64 = conn.query_row(
            "SELECT id FROM content_blocks WHERE content_hash = 'hash_a'", [], |r| r.get(0)).unwrap();
        let block_id_b: i64 = conn.query_row(
            "SELECT id FROM content_blocks WHERE content_hash = 'hash_b'", [], |r| r.get(0)).unwrap();

        // Two separate gz files for the same source.
        let req_a = BulkRequest {
            source: "src".to_string(),
            files: vec![IndexFile {
                path: "a.txt".to_string(), mtime: 1000, size: Some(50), kind: FileKind::Text,
                scanner_version: 1,
                lines: vec![
                    IndexLine { archive_path: None, line_number: 0, content: "a.txt".to_string() },
                    IndexLine { archive_path: None, line_number: 1, content: "content of a".to_string() },
                ],
                extract_ms: None, content_hash: Some("hash_a".to_string()), is_new: true,
            }],
            delete_paths: vec![], rename_paths: vec![], scan_timestamp: None, indexing_failures: vec![],
        };
        let req_b = BulkRequest {
            source: "src".to_string(),
            files: vec![IndexFile {
                path: "b.txt".to_string(), mtime: 1000, size: Some(50), kind: FileKind::Text,
                scanner_version: 1,
                lines: vec![
                    IndexLine { archive_path: None, line_number: 0, content: "b.txt".to_string() },
                    IndexLine { archive_path: None, line_number: 1, content: "content of b".to_string() },
                ],
                extract_ms: None, content_hash: Some("hash_b".to_string()), is_new: true,
            }],
            delete_paths: vec![], rename_paths: vec![], scan_timestamp: None, indexing_failures: vec![],
        };
        write_bulk_gz(&to_archive_dir.join("req_a.gz"), &req_a);
        write_bulk_gz(&to_archive_dir.join("req_b.gz"), &req_b);

        run_archive_batch(data_dir, to_archive_dir, make_worker_config(),
            &crate::archive::SharedArchiveState::new(data_dir.to_path_buf()).unwrap()).unwrap();

        // Both gz files deleted.
        let gz_count = std::fs::read_dir(to_archive_dir).unwrap().flatten()
            .filter(|e| e.path().extension().map_or(false, |x| x == "gz")).count();
        assert_eq!(gz_count, 0, "both gz files should be deleted");

        // Both files archived.
        let ranges_a = read_chunk_ranges(&conn, block_id_a);
        let ranges_b = read_chunk_ranges(&conn, block_id_b);
        assert!(!ranges_a.is_empty(), "a.txt should have content_chunks");
        assert!(!ranges_b.is_empty(), "b.txt should have content_chunks");
    }
}
