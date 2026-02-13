#![allow(dead_code)] // some helpers reserved for future endpoints

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

use find_common::api::{ContextLine, FileRecord, IndexFile};

use crate::archive::{ArchiveManager, ChunkRef};

// ── Schema ────────────────────────────────────────────────────────────────────

pub fn open(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("opening {}", db_path.display()))?;

    // Detect v1 schema (lines table has a `content` column instead of chunk refs).
    // If found, drop all old objects so schema_v2 initialises cleanly.
    let is_v1: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('lines') WHERE name = 'content'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0)
        > 0;

    if is_v1 {
        tracing::warn!(
            "Detected v1 schema in {}; dropping old data for v2 migration \
             (re-run find-scan to rebuild the index)",
            db_path.display()
        );
        conn.execute_batch(
            "DROP TRIGGER IF EXISTS lines_au;
             DROP TRIGGER IF EXISTS lines_ad;
             DROP TRIGGER IF EXISTS lines_ai;
             DROP TABLE  IF EXISTS lines_fts;
             DROP TABLE  IF EXISTS lines;
             DROP TABLE  IF EXISTS files;
             DROP TABLE  IF EXISTS meta;",
        )
        .context("dropping v1 schema")?;
    }

    conn.execute_batch(include_str!("schema_v2.sql"))
        .context("initialising schema")?;
    Ok(conn)
}

// ── File listing (for deletion detection) ────────────────────────────────────

pub fn list_files(conn: &Connection) -> Result<Vec<FileRecord>> {
    let mut stmt = conn.prepare("SELECT path, mtime FROM files ORDER BY path")?;
    let rows = stmt
        .query_map([], |row| {
            Ok(FileRecord {
                path: row.get(0)?,
                mtime: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ── Upsert ────────────────────────────────────────────────────────────────────

pub fn upsert_files(conn: &Connection, files: &[IndexFile]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;

    for file in files {
        // Upsert the file row, getting back its id.
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

        // Replace all lines for this file.
        tx.execute("DELETE FROM lines WHERE file_id = ?1", params![file_id])?;

        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO lines (file_id, archive_path, line_number, content)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for line in &file.lines {
                stmt.execute(params![
                    file_id,
                    line.archive_path,
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

pub fn delete_files(conn: &Connection, paths: &[String]) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    for path in paths {
        // CASCADE deletes associated lines automatically.
        tx.execute("DELETE FROM files WHERE path = ?1", params![path])?;
    }
    tx.commit()?;
    Ok(())
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
        // Remove base_url if None
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
    pub file_path: String,
    pub file_kind: String,
    pub archive_path: Option<String>,
    pub line_number: usize,
    pub content: String,
}

/// FTS5 trigram pre-filter.  Returns up to `limit` candidate rows.
/// For fuzzy mode the caller re-scores with nucleo.  For exact/regex the FTS5
/// result is already a substring match; the caller can apply regex post-filtering.
///
/// `phrase=true`  → wraps the whole query in double-quotes (literal substring /
///                  phrase match).  Used by exact and regex modes.
/// `phrase=false` → splits on whitespace and ANDs each word (≥3 chars) as a
///                  separate trigram term.  Used by fuzzy mode so that e.g.
///                  "pass strength" finds "password strength".
///
/// FTS5 trigram needs at least 3 characters per term; short tokens are dropped
/// from the FTS5 query (nucleo re-scores the candidates afterwards).  If no
/// usable tokens remain we fall back to a LIKE scan.
pub fn fts_candidates(
    conn: &Connection,
    archive_mgr: &ArchiveManager,
    query: &str,
    limit: usize,
    phrase: bool,
) -> Result<Vec<CandidateRow>> {
    // Short queries can't use FTS5 trigrams; content is in ZIPs so LIKE fallback
    // is too expensive. Return empty results for very short queries.
    let fts_query = if phrase {
        if query.len() < 3 {
            return Ok(vec![]);
        }
        format!("\"{}\"", query.replace('"', "\"\""))
    } else {
        let terms: Vec<String> = query
            .split_whitespace()
            .filter(|w| w.len() >= 3)
            .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
            .collect();
        if terms.is_empty() {
            return Ok(vec![]);
        }
        terms.join(" AND ")
    };

    struct RawRow {
        file_path: String,
        file_kind: String,
        archive_path: Option<String>,
        line_number: usize,
        chunk_archive: String,
        chunk_name: String,
        line_offset: usize,
    }

    let mut stmt = conn.prepare(
        "SELECT f.path, f.kind, l.archive_path, l.line_number,
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
                archive_path: row.get(2)?,
                line_number:  row.get::<_, i64>(3)? as usize,
                chunk_archive: row.get(4)?,
                chunk_name:   row.get(5)?,
                line_offset:  row.get::<_, i64>(6)? as usize,
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
        results.push(CandidateRow {
            file_path:    row.file_path,
            file_kind:    row.file_kind,
            archive_path: row.archive_path,
            line_number:  row.line_number,
            content,
        });
    }

    Ok(results)
}

// ── File lines ────────────────────────────────────────────────────────────────

/// Returns every indexed line for a file, ordered by line number.
/// Used by the GET /api/v1/file endpoint.
pub fn get_file_lines(
    conn: &Connection,
    archive_mgr: &ArchiveManager,
    path: &str,
    archive_path: Option<&str>,
) -> Result<Vec<ContextLine>> {
    let mut stmt = conn.prepare(
        "SELECT l.line_number, l.chunk_archive, l.chunk_name, l.line_offset_in_chunk
         FROM lines l
         JOIN files f ON f.id = l.file_id
         WHERE f.path = ?1
           AND ((?2 IS NULL AND l.archive_path IS NULL)
                OR l.archive_path = ?2)
         ORDER BY l.line_number",
    )?;

    let rows: Vec<(usize, String, String, usize)> = stmt
        .query_map(params![path, archive_path], |row| {
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
    archive_path: Option<&str>,
    center: usize,
    window: usize,
) -> Result<Vec<ContextLine>> {
    let kind = get_file_kind(conn, file_path)?;

    match kind.as_str() {
        "image" | "audio" => get_metadata_context(conn, archive_mgr, file_path),
        "pdf" => get_pdf_context(conn, archive_mgr, file_path, archive_path, center, window),
        _ => get_line_context(conn, archive_mgr, file_path, archive_path, center, window),
    }
}

fn get_file_kind(conn: &Connection, file_path: &str) -> Result<String> {
    conn.query_row(
        "SELECT kind FROM files WHERE path = ?1",
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
    // For images and audio, return ALL metadata tags (stored at line_number=0).
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

fn get_pdf_context(
    conn: &Connection,
    archive_mgr: &ArchiveManager,
    file_path: &str,
    archive_path: Option<&str>,
    center: usize,
    window: usize,
) -> Result<Vec<ContextLine>> {
    // For PDFs, use character-based context (avg 80 chars/line)
    let window_chars = window * 80;

    let mut before = get_lines_before_with_limit(conn, archive_mgr, file_path, archive_path, center, window_chars)?;
    let matched = get_line_exact(conn, archive_mgr, file_path, archive_path, center)?;
    let after = get_lines_after_with_limit(conn, archive_mgr, file_path, archive_path, center, window_chars)?;

    before.extend(matched);
    before.extend(after);
    Ok(before)
}

fn get_line_exact(
    conn: &Connection,
    archive_mgr: &ArchiveManager,
    file_path: &str,
    archive_path: Option<&str>,
    line_number: usize,
) -> Result<Vec<ContextLine>> {
    let mut stmt = conn.prepare(
        "SELECT l.line_number, l.chunk_archive, l.chunk_name, l.line_offset_in_chunk
         FROM lines l
         JOIN files f ON f.id = l.file_id
         WHERE f.path = ?1
           AND ((?2 IS NULL AND l.archive_path IS NULL)
                OR l.archive_path = ?2)
           AND l.line_number = ?3",
    )?;

    let rows: Vec<(usize, String, String, usize)> = stmt
        .query_map(params![file_path, archive_path, line_number as i64], |row| {
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

fn get_lines_before_with_limit(
    conn: &Connection,
    archive_mgr: &ArchiveManager,
    file_path: &str,
    archive_path: Option<&str>,
    center: usize,
    max_chars: usize,
) -> Result<Vec<ContextLine>> {
    // Fetch lines before `center` in reverse order (newest first) so we can
    // apply the char limit before reading from ZIP.
    let mut stmt = conn.prepare(
        "SELECT l.line_number, l.chunk_archive, l.chunk_name, l.line_offset_in_chunk
         FROM lines l
         JOIN files f ON f.id = l.file_id
         WHERE f.path = ?1
           AND ((?2 IS NULL AND l.archive_path IS NULL)
                OR l.archive_path = ?2)
           AND l.line_number < ?3
         ORDER BY l.line_number DESC",
    )?;

    // We need the content to count chars, so resolve all and then trim.
    let all_raw: Vec<(usize, String, String, usize)> = stmt
        .query_map(params![file_path, archive_path, center as i64], |row| {
            Ok((
                row.get::<_, i64>(0)? as usize,
                row.get(1)?,
                row.get(2)?,
                row.get::<_, i64>(3)? as usize,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Resolve content, then trim by char budget.
    let resolved = resolve_content(archive_mgr, all_raw);
    let mut lines = Vec::new();
    let mut char_count = 0;
    for line in resolved {
        char_count += line.content.len();
        if char_count > max_chars && !lines.is_empty() {
            break;
        }
        lines.push(line);
    }
    lines.reverse(); // restore natural order
    Ok(lines)
}

fn get_lines_after_with_limit(
    conn: &Connection,
    archive_mgr: &ArchiveManager,
    file_path: &str,
    archive_path: Option<&str>,
    center: usize,
    max_chars: usize,
) -> Result<Vec<ContextLine>> {
    let mut stmt = conn.prepare(
        "SELECT l.line_number, l.chunk_archive, l.chunk_name, l.line_offset_in_chunk
         FROM lines l
         JOIN files f ON f.id = l.file_id
         WHERE f.path = ?1
           AND ((?2 IS NULL AND l.archive_path IS NULL)
                OR l.archive_path = ?2)
           AND l.line_number > ?3
         ORDER BY l.line_number",
    )?;

    let all_raw: Vec<(usize, String, String, usize)> = stmt
        .query_map(params![file_path, archive_path, center as i64], |row| {
            Ok((
                row.get::<_, i64>(0)? as usize,
                row.get(1)?,
                row.get(2)?,
                row.get::<_, i64>(3)? as usize,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let resolved = resolve_content(archive_mgr, all_raw);
    let mut lines = Vec::new();
    let mut char_count = 0;
    for line in resolved {
        char_count += line.content.len();
        if char_count > max_chars && !lines.is_empty() {
            break;
        }
        lines.push(line);
    }
    Ok(lines)
}

fn get_line_context(
    conn: &Connection,
    archive_mgr: &ArchiveManager,
    file_path: &str,
    archive_path: Option<&str>,
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
           AND ((?2 IS NULL AND l.archive_path IS NULL)
                OR l.archive_path = ?2)
           AND l.line_number BETWEEN ?3 AND ?4
         ORDER BY l.line_number",
    )?;

    let rows: Vec<(usize, String, String, usize)> = stmt
        .query_map(params![file_path, archive_path, lo, hi], |row| {
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
/// Input: `(line_number, chunk_archive, chunk_name, line_offset_in_chunk)` tuples.
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
