#![allow(dead_code)] // some helpers reserved for future endpoints

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

use find_common::api::{ContextLine, DirEntry, FileRecord, IndexFile, KindStats, ScanHistoryPoint};

use crate::archive::{ArchiveManager, ChunkRef};

// ── Schema ────────────────────────────────────────────────────────────────────

pub fn open(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("opening {}", db_path.display()))?;
    conn.execute_batch(include_str!("schema_v2.sql"))
        .context("initialising schema")?;
    migrate_v3(&conn).context("v3 migration")?;
    Ok(conn)
}

fn migrate_v3(conn: &Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 3 {
        return Ok(());
    }
    conn.execute_batch(
        "ALTER TABLE files ADD COLUMN indexed_at INTEGER;
         ALTER TABLE files ADD COLUMN extract_ms INTEGER;
         CREATE TABLE IF NOT EXISTS scan_history (
             id          INTEGER PRIMARY KEY AUTOINCREMENT,
             scanned_at  INTEGER NOT NULL,
             total_files INTEGER NOT NULL,
             total_size  INTEGER NOT NULL,
             by_kind     TEXT    NOT NULL
         );
         PRAGMA user_version = 3;",
    )?;
    Ok(())
}

// ── File listing (for deletion detection) ────────────────────────────────────

pub fn list_files(conn: &Connection) -> Result<Vec<FileRecord>> {
    let mut stmt = conn.prepare("SELECT path, mtime, kind FROM files ORDER BY path")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(FileRecord {
                path: row.get(0)?,
                mtime: row.get(1)?,
                kind: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ── Upsert ────────────────────────────────────────────────────────────────────

pub fn upsert_files(conn: &Connection, files: &[IndexFile]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;

    for file in files {
        tx.execute(
            "INSERT INTO files (path, mtime, size, kind)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET
               mtime = excluded.mtime,
               size  = excluded.size,
               kind  = excluded.kind",
            params![file.path, file.mtime, file.size, file.kind],
        )?;

        let file_id: i64 = tx.query_row(
            "SELECT id FROM files WHERE path = ?1",
            params![file.path],
            |row| row.get(0),
        )?;

        tx.execute("DELETE FROM lines WHERE file_id = ?1", params![file_id])?;

        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO lines (file_id, line_number, content)
                 VALUES (?1, ?2, ?3)",
            )?;
            for line in &file.lines {
                stmt.execute(params![
                    file_id,
                    line.line_number as i64,
                    line.content,
                ])?;
            }
        }
    }

    tx.commit()?;
    Ok(())
}

// ── Delete ────────────────────────────────────────────────────────────────────

pub fn delete_files(
    conn: &Connection,
    archive_mgr: &mut crate::archive::ArchiveManager,
    paths: &[String],
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;

    // Collect chunk refs before the rows are gone, including all inner archive members.
    let refs = collect_chunk_refs(&tx, paths)?;

    // Delete rows for the outer path AND all inner archive members (path LIKE 'x::%').
    // ON DELETE CASCADE removes associated lines automatically.
    for path in paths {
        tx.execute(
            "DELETE FROM files WHERE path = ?1 OR path LIKE ?2",
            params![path, format!("{}::%", path)],
        )?;
    }

    // Rewrite affected ZIPs. If this fails the transaction is dropped,
    // rolling back the SQLite deletes automatically.
    archive_mgr.remove_chunks(refs)?;

    // ZIP rewrite succeeded — safe to commit.
    tx.commit()?;
    Ok(())
}

/// Collect chunk refs for all lines belonging to the given outer paths and their members.
fn collect_chunk_refs(
    tx: &rusqlite::Transaction,
    paths: &[String],
) -> Result<Vec<crate::archive::ChunkRef>> {
    let mut refs = Vec::new();
    for path in paths {
        // Collect file_ids for the outer file and all inner members (path::*).
        let mut id_stmt = tx.prepare(
            "SELECT id FROM files WHERE path = ?1 OR path LIKE ?2",
        )?;
        let file_ids: Vec<i64> = id_stmt
            .query_map(params![path, format!("{}::%", path)], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;

        for fid in file_ids {
            let mut stmt = tx.prepare(
                "SELECT DISTINCT chunk_archive, chunk_name FROM lines WHERE file_id = ?1",
            )?;
            let chunk_refs = stmt.query_map(params![fid], |row| {
                Ok(crate::archive::ChunkRef {
                    archive_name: row.get(0)?,
                    chunk_name: row.get(1)?,
                })
            })?;
            for r in chunk_refs {
                refs.push(r?);
            }
        }
    }
    Ok(refs)
}

// ── Scan timestamp ────────────────────────────────────────────────────────────

pub fn update_last_scan(conn: &Connection, timestamp: i64) -> Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES ('last_scan', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![timestamp.to_string()],
    )?;
    Ok(())
}

pub fn get_last_scan(conn: &Connection) -> Result<Option<i64>> {
    let result = conn.query_row(
        "SELECT value FROM meta WHERE key = 'last_scan'",
        [],
        |row| row.get::<_, String>(0),
    );
    match result {
        Ok(s) => Ok(s.parse().ok()),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

// ── Base URL ──────────────────────────────────────────────────────────────────

pub fn update_base_url(conn: &Connection, base_url: Option<&str>) -> Result<()> {
    if let Some(url) = base_url {
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('base_url', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![url],
        )?;
    } else {
        conn.execute("DELETE FROM meta WHERE key = 'base_url'", [])?;
    }
    Ok(())
}

pub fn get_base_url(conn: &Connection) -> Result<Option<String>> {
    let result = conn.query_row(
        "SELECT value FROM meta WHERE key = 'base_url'",
        [],
        |row| row.get(0),
    );
    match result {
        Ok(s) => Ok(Some(s)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

// ── Search ────────────────────────────────────────────────────────────────────

pub struct CandidateRow {
    /// Full path, potentially composite ("archive.zip::member.txt").
    pub file_path: String,
    pub file_kind: String,
    /// For archive members: the part after the first "::".
    /// For outer files: None.
    pub archive_path: Option<String>,
    pub line_number: usize,
    pub content: String,
}

/// FTS5 trigram pre-filter.  Returns up to `limit` candidate rows.
/// Build an FTS5 match expression from a raw query string.
/// Returns None if the query produces no matchable terms.
fn build_fts_query(query: &str, phrase: bool) -> Option<String> {
    if phrase {
        if query.len() < 3 {
            return None;
        }
        Some(format!("\"{}\"", query.replace('"', "\"\"")))
    } else {
        let terms: Vec<String> = query
            .split_whitespace()
            .filter(|w| w.len() >= 3)
            .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
            .collect();
        if terms.is_empty() {
            return None;
        }
        Some(terms.join(" AND "))
    }
}

/// Fast FTS5-only count, capped at `limit`. No ZIP reads, no JOINs.
/// Used to compute the approximate total result count efficiently.
pub fn fts_count(conn: &Connection, query: &str, limit: usize, phrase: bool) -> Result<usize> {
    let Some(fts_query) = build_fts_query(query, phrase) else {
        return Ok(0);
    };
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM (SELECT 1 FROM lines_fts WHERE lines_fts MATCH ?1 LIMIT ?2)",
        params![fts_query, limit as i64],
        |row| row.get(0),
    )?;
    Ok(count as usize)
}

pub fn fts_candidates(
    conn: &Connection,
    archive_mgr: &ArchiveManager,
    query: &str,
    limit: usize,
    phrase: bool,
) -> Result<Vec<CandidateRow>> {
    let Some(fts_query) = build_fts_query(query, phrase) else {
        return Ok(vec![]);
    };

    struct RawRow {
        file_path: String,
        file_kind: String,
        line_number: usize,
        chunk_archive: String,
        chunk_name: String,
        line_offset: usize,
    }

    let mut stmt = conn.prepare(
        "SELECT f.path, f.kind, l.line_number,
                l.chunk_archive, l.chunk_name, l.line_offset_in_chunk
         FROM lines_fts
         JOIN lines l ON l.id = lines_fts.rowid
         JOIN files f ON f.id = l.file_id
         WHERE lines_fts MATCH ?1
         LIMIT ?2",
    )?;

    let raw: Vec<RawRow> = stmt
        .query_map(params![fts_query, limit as i64], |row| {
            Ok(RawRow {
                file_path:    row.get(0)?,
                file_kind:    row.get(1)?,
                line_number:  row.get::<_, i64>(2)? as usize,
                chunk_archive: row.get(3)?,
                chunk_name:   row.get(4)?,
                line_offset:  row.get::<_, i64>(5)? as usize,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Read content from ZIP archives, caching chunks to avoid redundant reads.
    let mut chunk_cache: HashMap<(String, String), Vec<String>> = HashMap::new();
    let mut results = Vec::with_capacity(raw.len());

    for row in raw {
        let key = (row.chunk_archive.clone(), row.chunk_name.clone());
        if !chunk_cache.contains_key(&key) {
            let chunk_ref = ChunkRef { archive_name: key.0.clone(), chunk_name: key.1.clone() };
            let text = archive_mgr.read_chunk(&chunk_ref).unwrap_or_default();
            chunk_cache.insert(key.clone(), text.lines().map(|l| l.to_string()).collect());
        }
        let content = chunk_cache[&key].get(row.line_offset).cloned().unwrap_or_default();

        // Split composite path into outer path + archive_path for search result compat.
        let (file_path, archive_path) = split_composite_path(&row.file_path);

        results.push(CandidateRow {
            file_path,
            file_kind:    row.file_kind,
            archive_path,
            line_number:  row.line_number,
            content,
        });
    }

    Ok(results)
}

/// Split a potentially composite path ("zip::member") into (outer_path, archive_path).
/// Returns (path, None) for non-composite paths.
pub fn split_composite_path(path: &str) -> (String, Option<String>) {
    if let Some(pos) = path.find("::") {
        (path[..pos].to_string(), Some(path[pos + 2..].to_string()))
    } else {
        (path.to_string(), None)
    }
}

// ── File lines ────────────────────────────────────────────────────────────────

/// Returns every indexed line for a file, ordered by line number.
/// `path` may be a composite path ("archive.zip::member.txt") or a plain path.
pub fn get_file_lines(
    conn: &Connection,
    archive_mgr: &ArchiveManager,
    path: &str,
) -> Result<Vec<ContextLine>> {
    let mut stmt = conn.prepare(
        "SELECT l.line_number, l.chunk_archive, l.chunk_name, l.line_offset_in_chunk
         FROM lines l
         JOIN files f ON f.id = l.file_id
         WHERE f.path = ?1
         ORDER BY l.line_number",
    )?;

    let rows: Vec<(usize, String, String, usize)> = stmt
        .query_map(params![path], |row| {
            Ok((
                row.get::<_, i64>(0)? as usize,
                row.get(1)?,
                row.get(2)?,
                row.get::<_, i64>(3)? as usize,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(resolve_content(archive_mgr, rows))
}

// ── Context ───────────────────────────────────────────────────────────────────

pub fn get_context(
    conn: &Connection,
    archive_mgr: &ArchiveManager,
    file_path: &str,
    center: usize,
    window: usize,
) -> Result<Vec<ContextLine>> {
    let kind = get_file_kind(conn, file_path)?;

    match kind.as_str() {
        "image" | "audio" => get_metadata_context(conn, archive_mgr, file_path),
        _ => get_line_context(conn, archive_mgr, file_path, center, window),
    }
}

fn get_file_kind(conn: &Connection, file_path: &str) -> Result<String> {
    conn.query_row(
        "SELECT kind FROM files WHERE path = ?1 LIMIT 1",
        params![file_path],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn get_metadata_context(
    conn: &Connection,
    archive_mgr: &ArchiveManager,
    file_path: &str,
) -> Result<Vec<ContextLine>> {
    let mut stmt = conn.prepare(
        "SELECT l.line_number, l.chunk_archive, l.chunk_name, l.line_offset_in_chunk
         FROM lines l
         JOIN files f ON f.id = l.file_id
         WHERE f.path = ?1
           AND l.line_number = 0
         ORDER BY l.id",
    )?;

    let rows: Vec<(usize, String, String, usize)> = stmt
        .query_map([file_path], |row| {
            Ok((
                row.get::<_, i64>(0)? as usize,
                row.get(1)?,
                row.get(2)?,
                row.get::<_, i64>(3)? as usize,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(resolve_content(archive_mgr, rows))
}

fn get_line_context(
    conn: &Connection,
    archive_mgr: &ArchiveManager,
    file_path: &str,
    center: usize,
    window: usize,
) -> Result<Vec<ContextLine>> {
    let lo = center.saturating_sub(window) as i64;
    let hi = (center + window) as i64;

    let mut stmt = conn.prepare(
        "SELECT l.line_number, l.chunk_archive, l.chunk_name, l.line_offset_in_chunk
         FROM lines l
         JOIN files f ON f.id = l.file_id
         WHERE f.path = ?1
           AND l.line_number BETWEEN ?2 AND ?3
         ORDER BY l.line_number",
    )?;

    let rows: Vec<(usize, String, String, usize)> = stmt
        .query_map(params![file_path, lo, hi], |row| {
            Ok((
                row.get::<_, i64>(0)? as usize,
                row.get(1)?,
                row.get(2)?,
                row.get::<_, i64>(3)? as usize,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(resolve_content(archive_mgr, rows))
}

/// Read line content from ZIP archives, caching chunks to avoid redundant reads.
fn resolve_content(
    archive_mgr: &ArchiveManager,
    rows: Vec<(usize, String, String, usize)>,
) -> Vec<ContextLine> {
    let mut cache: HashMap<(String, String), Vec<String>> = HashMap::new();
    let mut result = Vec::with_capacity(rows.len());

    for (line_number, chunk_archive, chunk_name, offset) in rows {
        let key = (chunk_archive.clone(), chunk_name.clone());
        if !cache.contains_key(&key) {
            let chunk_ref = ChunkRef { archive_name: key.0.clone(), chunk_name: key.1.clone() };
            let text = archive_mgr.read_chunk(&chunk_ref).unwrap_or_default();
            cache.insert(key.clone(), text.lines().map(|l| l.to_string()).collect());
        }
        let content = cache[&key].get(offset).cloned().unwrap_or_default();
        result.push(ContextLine { line_number, content });
    }

    result
}

// ── Directory listing ─────────────────────────────────────────────────────────

/// List the immediate children (dirs + files) of `prefix` within the source.
///
/// `prefix` should end with `/` for non-root directory queries (e.g. `"src/"`).
/// For archive member listings, `prefix` ends with `"::"` (e.g. `"archive.zip::"`).
/// An empty string means the root of the source.
pub fn list_dir(conn: &Connection, prefix: &str) -> Result<Vec<DirEntry>> {
    let is_archive_listing = prefix.contains("::");

    let (low, high) = if prefix.is_empty() {
        (String::new(), "\u{FFFF}".to_string())
    } else {
        (prefix.to_string(), prefix_bump(prefix))
    };

    let mut stmt = conn.prepare(
        "SELECT path, kind, size, mtime FROM files WHERE path >= ?1 AND path < ?2 ORDER BY path",
    )?;

    let rows: Vec<(String, String, i64, i64)> = stmt
        .query_map(params![low, high], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut seen_dirs: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut seen_files: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut dirs: Vec<DirEntry> = Vec::new();
    let mut files: Vec<DirEntry> = Vec::new();

    // First pass: collect all actual files to avoid creating duplicate virtual dirs
    if is_archive_listing {
        for (path, _, _, _) in &rows {
            let rest = path.strip_prefix(prefix).unwrap_or(path);
            if !rest.contains("::") && !rest.contains('/') {
                seen_files.insert(rest.to_string());
            }
        }
    }

    // Second pass: build the directory listing
    for (path, kind, size, mtime) in rows {
        let rest = path.strip_prefix(prefix).unwrap_or(&path);

        if is_archive_listing {
            // Inside an archive: "::"-separated segments act like directories.
            // Treat first "::"-delimited or "/"-delimited component as the child.
            let sep_pos = rest.find("::").or_else(|| rest.find('/'));
            if let Some(pos) = sep_pos {
                let child_name = &rest[..pos];
                // Only create virtual dir if we haven't seen a real file with this path
                if !seen_files.contains(child_name) && seen_dirs.insert(child_name.to_string()) {
                    dirs.push(DirEntry {
                        name: child_name.to_string(),
                        path: format!("{}{}", prefix, child_name),
                        entry_type: "dir".to_string(),
                        kind: None,
                        size: None,
                        mtime: None,
                    });
                }
            } else {
                // Leaf member within the archive.
                files.push(DirEntry {
                    name: rest.to_string(),
                    path,
                    entry_type: "file".to_string(),
                    kind: Some(kind),
                    size: Some(size),
                    mtime: Some(mtime),
                });
            }
        } else {
            // Regular directory listing.
            // Skip inner archive members (composite paths) — they appear only when
            // the user explicitly expands the archive.
            if rest.contains("::") {
                continue;
            }

            if let Some(slash_pos) = rest.find('/') {
                let dir_name = &rest[..slash_pos];
                if seen_dirs.insert(dir_name.to_string()) {
                    dirs.push(DirEntry {
                        name: dir_name.to_string(),
                        path: format!("{}{}/", prefix, dir_name),
                        entry_type: "dir".to_string(),
                        kind: None,
                        size: None,
                        mtime: None,
                    });
                }
            } else {
                files.push(DirEntry {
                    name: rest.to_string(),
                    path,
                    entry_type: "file".to_string(),
                    kind: Some(kind),
                    size: Some(size),
                    mtime: Some(mtime),
                });
            }
        }
    }

    let mut entries = dirs;
    entries.extend(files);
    Ok(entries)
}

// ── Stats ─────────────────────────────────────────────────────────────────────

/// Returns (total_files, total_size, by_kind) aggregated from the files table.
pub fn get_stats(conn: &Connection) -> Result<(usize, i64, HashMap<String, KindStats>)> {
    let mut stmt = conn.prepare(
        "SELECT kind, COUNT(*), COALESCE(SUM(size), 0), AVG(CAST(extract_ms AS REAL))
         FROM files GROUP BY kind",
    )?;

    let rows: Vec<(String, i64, i64, Option<f64>)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<f64>>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut total_files = 0usize;
    let mut total_size = 0i64;
    let mut by_kind = HashMap::new();

    for (kind, count, size, avg_ms) in rows {
        total_files += count as usize;
        total_size += size;
        by_kind.insert(kind, KindStats { count: count as usize, size, avg_extract_ms: avg_ms });
    }

    Ok((total_files, total_size, by_kind))
}

/// Snapshot the current totals into the scan_history table.
pub fn append_scan_history(conn: &Connection, scanned_at: i64) -> Result<()> {
    let (total_files, total_size, by_kind) = get_stats(conn)?;
    let by_kind_json = serde_json::to_string(&by_kind).context("serialising by_kind")?;
    conn.execute(
        "INSERT INTO scan_history (scanned_at, total_files, total_size, by_kind)
         VALUES (?1, ?2, ?3, ?4)",
        params![scanned_at, total_files as i64, total_size, by_kind_json],
    )?;
    Ok(())
}

/// Return up to `limit` scan history points, oldest first.
pub fn get_scan_history(conn: &Connection, limit: usize) -> Result<Vec<ScanHistoryPoint>> {
    let mut stmt = conn.prepare(
        "SELECT scanned_at, total_files, total_size
         FROM scan_history ORDER BY scanned_at ASC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit as i64], |row| {
            Ok(ScanHistoryPoint {
                scanned_at:  row.get(0)?,
                total_files: row.get::<_, i64>(1)? as usize,
                total_size:  row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Produce the upper-bound key for a prefix range scan by incrementing the last byte.
fn prefix_bump(prefix: &str) -> String {
    let mut bytes = prefix.as_bytes().to_vec();
    if let Some(last) = bytes.last_mut() {
        *last += 1;
    }
    String::from_utf8(bytes).unwrap_or_else(|_| "\u{FFFF}".to_string())
}
