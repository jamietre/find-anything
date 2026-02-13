#![allow(dead_code)] // some helpers reserved for future endpoints

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

use find_common::api::{ContextLine, FileRecord, IndexFile};

// ── Schema ────────────────────────────────────────────────────────────────────

pub fn open(db_path: &Path) -> Result<Connection> {
    let conn = Connection::open(db_path)
        .with_context(|| format!("opening {}", db_path.display()))?;
    conn.execute_batch(include_str!("schema.sql"))
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
    query: &str,
    limit: usize,
    phrase: bool,
) -> Result<Vec<CandidateRow>> {
    let fts_query = if phrase {
        if query.len() < 3 {
            return like_candidates(conn, query, limit);
        }
        format!("\"{}\"", query.replace('"', "\"\""))
    } else {
        // Build "word1" AND "word2" … keeping only tokens ≥ 3 chars.
        let terms: Vec<String> = query
            .split_whitespace()
            .filter(|w| w.len() >= 3)
            .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
            .collect();
        if terms.is_empty() {
            return like_candidates(conn, query, limit);
        }
        terms.join(" AND ")
    };

    let mut stmt = conn.prepare(
        "SELECT f.path, f.kind, l.archive_path, l.line_number, l.content
         FROM lines_fts
         JOIN lines l ON l.id = lines_fts.rowid
         JOIN files f ON f.id = l.file_id
         WHERE lines_fts MATCH ?1
         LIMIT ?2",
    )?;

    let rows = stmt
        .query_map(params![fts_query, limit as i64], |row| {
            Ok(CandidateRow {
                file_path:    row.get(0)?,
                file_kind:    row.get(1)?,
                archive_path: row.get(2)?,
                line_number:  row.get::<_, i64>(3)? as usize,
                content:      row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(rows)
}

/// LIKE-based fallback for queries shorter than 3 characters.
/// No index support — full table scan — but correct.
fn like_candidates(conn: &Connection, query: &str, limit: usize) -> Result<Vec<CandidateRow>> {
    // Escape LIKE special characters in the query itself.
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{escaped}%");

    let mut stmt = conn.prepare(
        "SELECT f.path, f.kind, l.archive_path, l.line_number, l.content
         FROM lines l
         JOIN files f ON f.id = l.file_id
         WHERE l.content LIKE ?1 ESCAPE '\\'
         LIMIT ?2",
    )?;

    let rows = stmt
        .query_map(params![pattern, limit as i64], |row| {
            Ok(CandidateRow {
                file_path:    row.get(0)?,
                file_kind:    row.get(1)?,
                archive_path: row.get(2)?,
                line_number:  row.get::<_, i64>(3)? as usize,
                content:      row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(rows)
}

// ── File lines ────────────────────────────────────────────────────────────────

/// Returns every indexed line for a file, ordered by line number.
/// Used by the GET /api/v1/file endpoint.
pub fn get_file_lines(
    conn: &Connection,
    path: &str,
    archive_path: Option<&str>,
) -> Result<Vec<ContextLine>> {
    let mut stmt = conn.prepare(
        "SELECT l.line_number, l.content
         FROM lines l
         JOIN files f ON f.id = l.file_id
         WHERE f.path = ?1
           AND ((?2 IS NULL AND l.archive_path IS NULL)
                OR l.archive_path = ?2)
         ORDER BY l.line_number",
    )?;

    let rows = stmt
        .query_map(params![path, archive_path], |row| {
            Ok(ContextLine {
                line_number: row.get::<_, i64>(0)? as usize,
                content: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(rows)
}

// ── Context ───────────────────────────────────────────────────────────────────

pub fn get_context(
    conn: &Connection,
    file_path: &str,
    archive_path: Option<&str>,
    center: usize,
    window: usize,
) -> Result<Vec<ContextLine>> {
    // Get file kind to determine context strategy
    let kind = get_file_kind(conn, file_path)?;

    match kind.as_str() {
        "image" | "audio" => get_metadata_context(conn, file_path),
        "pdf" => get_pdf_context(conn, file_path, archive_path, center, window),
        _ => get_line_context(conn, file_path, archive_path, center, window),
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

fn get_metadata_context(conn: &Connection, file_path: &str) -> Result<Vec<ContextLine>> {
    // For images and audio, return ALL metadata tags
    let mut stmt = conn.prepare(
        "SELECT l.line_number, l.content
         FROM lines l
         JOIN files f ON f.id = l.file_id
         WHERE f.path = ?1
           AND l.line_number = 0
         ORDER BY l.content",
    )?;

    let rows = stmt
        .query_map([file_path], |row| {
            Ok(ContextLine {
                line_number: row.get::<_, i64>(0)? as usize,
                content: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(rows)
}

fn get_pdf_context(
    conn: &Connection,
    file_path: &str,
    archive_path: Option<&str>,
    center: usize,
    window: usize,
) -> Result<Vec<ContextLine>> {
    // For PDFs, use character-based context (avg 80 chars/line)
    let window_chars = window * 80;

    // Get lines before the match
    let mut before = get_lines_before_with_limit(
        conn,
        file_path,
        archive_path,
        center,
        window_chars,
    )?;

    // Get the matched line
    let matched = get_line_exact(conn, file_path, archive_path, center)?;

    // Get lines after the match
    let after = get_lines_after_with_limit(
        conn,
        file_path,
        archive_path,
        center,
        window_chars,
    )?;

    // Combine: before + matched + after
    before.extend(matched);
    before.extend(after);

    Ok(before)
}

fn get_line_exact(
    conn: &Connection,
    file_path: &str,
    archive_path: Option<&str>,
    line_number: usize,
) -> Result<Vec<ContextLine>> {
    let mut stmt = conn.prepare(
        "SELECT l.line_number, l.content
         FROM lines l
         JOIN files f ON f.id = l.file_id
         WHERE f.path = ?1
           AND ((?2 IS NULL AND l.archive_path IS NULL)
                OR l.archive_path = ?2)
           AND l.line_number = ?3",
    )?;

    let rows = stmt
        .query_map(
            params![file_path, archive_path, line_number as i64],
            |row| {
                Ok(ContextLine {
                    line_number: row.get::<_, i64>(0)? as usize,
                    content: row.get(1)?,
                })
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(rows)
}

fn get_lines_before_with_limit(
    conn: &Connection,
    file_path: &str,
    archive_path: Option<&str>,
    center: usize,
    max_chars: usize,
) -> Result<Vec<ContextLine>> {
    let mut stmt = conn.prepare(
        "SELECT l.line_number, l.content
         FROM lines l
         JOIN files f ON f.id = l.file_id
         WHERE f.path = ?1
           AND ((?2 IS NULL AND l.archive_path IS NULL)
                OR l.archive_path = ?2)
           AND l.line_number < ?3
         ORDER BY l.line_number DESC",
    )?;

    let mut lines = Vec::new();
    let mut char_count = 0;

    let rows = stmt.query_map(
        params![file_path, archive_path, center as i64],
        |row| {
            Ok(ContextLine {
                line_number: row.get::<_, i64>(0)? as usize,
                content: row.get(1)?,
            })
        },
    )?;

    for row in rows {
        let line = row?;
        char_count += line.content.len();

        if char_count > max_chars && !lines.is_empty() {
            break;
        }

        lines.push(line);
    }

    // Reverse to natural order
    lines.reverse();
    Ok(lines)
}

fn get_lines_after_with_limit(
    conn: &Connection,
    file_path: &str,
    archive_path: Option<&str>,
    center: usize,
    max_chars: usize,
) -> Result<Vec<ContextLine>> {
    let mut stmt = conn.prepare(
        "SELECT l.line_number, l.content
         FROM lines l
         JOIN files f ON f.id = l.file_id
         WHERE f.path = ?1
           AND ((?2 IS NULL AND l.archive_path IS NULL)
                OR l.archive_path = ?2)
           AND l.line_number > ?3
         ORDER BY l.line_number",
    )?;

    let mut lines = Vec::new();
    let mut char_count = 0;

    let rows = stmt.query_map(
        params![file_path, archive_path, center as i64],
        |row| {
            Ok(ContextLine {
                line_number: row.get::<_, i64>(0)? as usize,
                content: row.get(1)?,
            })
        },
    )?;

    for row in rows {
        let line = row?;
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
    file_path: &str,
    archive_path: Option<&str>,
    center: usize,
    window: usize,
) -> Result<Vec<ContextLine>> {
    // Standard line-based context for text and archive files
    let lo = center.saturating_sub(window) as i64;
    let hi = (center + window) as i64;

    let mut stmt = conn.prepare(
        "SELECT l.line_number, l.content
         FROM lines l
         JOIN files f ON f.id = l.file_id
         WHERE f.path = ?1
           AND ((?2 IS NULL AND l.archive_path IS NULL)
                OR l.archive_path = ?2)
           AND l.line_number BETWEEN ?3 AND ?4
         ORDER BY l.line_number",
    )?;

    let rows = stmt
        .query_map(
            params![file_path, archive_path, lo, hi],
            |row| {
                Ok(ContextLine {
                    line_number: row.get::<_, i64>(0)? as usize,
                    content: row.get(1)?,
                })
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(rows)
}
