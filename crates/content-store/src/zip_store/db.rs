//! content.db schema and SQL helpers.
//!
//! The content database lives at `data_dir/content.db` and is owned entirely
//! by `ZipContentStore`.  No other crate reads or writes it.

use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::Connection;

pub const SCHEMA_SQL: &str = "
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;

-- One row per unique content blob.
CREATE TABLE IF NOT EXISTS blobs (
    key TEXT PRIMARY KEY          -- blake3 hex hash
);

-- ZIP archive files on disk.
CREATE TABLE IF NOT EXISTS archives (
    id   INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE     -- e.g. \"content_00042.zip\"
);

-- One row per chunk per blob.
-- ZIP member name: \"{first 16 hex chars of key}.{chunk_num}\"
CREATE TABLE IF NOT EXISTS chunks (
    blob_key   TEXT    NOT NULL REFERENCES blobs(key) ON DELETE CASCADE,
    chunk_num  INTEGER NOT NULL,
    archive_id INTEGER NOT NULL REFERENCES archives(id),
    start_line INTEGER NOT NULL,
    end_line   INTEGER NOT NULL,
    PRIMARY KEY (blob_key, chunk_num)
);

CREATE INDEX IF NOT EXISTS idx_chunks_archive   ON chunks(archive_id);
CREATE INDEX IF NOT EXISTS idx_chunks_key_start ON chunks(blob_key, start_line);
";

/// Open (or create) `content.db` with a 30 s busy timeout and WAL mode.
/// Applies the schema on first open.
pub fn open_write(data_dir: &Path) -> Result<Connection> {
    let path = data_dir.join("content.db");
    let conn = Connection::open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(30))?;
    conn.execute_batch(SCHEMA_SQL).context("applying content.db schema")?;
    Ok(conn)
}

/// Open `content.db` in read-only mode with a 1 s busy timeout.
pub fn open_read(data_dir: &Path) -> Result<Connection> {
    let path = data_dir.join("content.db");
    let conn = Connection::open_with_flags(
        &path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("opening {} (read-only)", path.display()))?;
    conn.busy_timeout(std::time::Duration::from_secs(1))?;
    Ok(conn)
}

/// Insert or ignore a blob key; returns `true` if the row was newly inserted.
pub fn insert_blob(tx: &rusqlite::Transaction, key: &str) -> Result<bool> {
    let rows = tx.execute(
        "INSERT OR IGNORE INTO blobs(key) VALUES(?1)",
        rusqlite::params![key],
    )?;
    Ok(rows > 0)
}

/// Check whether a blob key exists.
pub fn blob_exists(conn: &Connection, key: &str) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM blobs WHERE key = ?1",
        rusqlite::params![key],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// Insert or ignore an archive by name; return its integer id.
pub fn upsert_archive(tx: &rusqlite::Transaction, name: &str) -> Result<i64> {
    tx.execute(
        "INSERT OR IGNORE INTO archives(name) VALUES(?1)",
        rusqlite::params![name],
    )?;
    let id: i64 = tx.query_row(
        "SELECT id FROM archives WHERE name = ?1",
        rusqlite::params![name],
        |r| r.get(0),
    )?;
    Ok(id)
}

/// Insert a chunk row.
pub fn insert_chunk(
    tx: &rusqlite::Transaction,
    blob_key: &str,
    chunk_num: usize,
    archive_id: i64,
    start_pos: usize,
    end_pos: usize,
) -> Result<()> {
    tx.execute(
        "INSERT OR IGNORE INTO chunks(blob_key, chunk_num, archive_id, start_line, end_line)
         VALUES(?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![blob_key, chunk_num as i64, archive_id, start_pos as i64, end_pos as i64],
    )?;
    Ok(())
}

/// Delete a blob and all its chunks (cascade).
pub fn delete_blob(tx: &rusqlite::Transaction, key: &str) -> Result<()> {
    tx.execute("DELETE FROM blobs WHERE key = ?1", rusqlite::params![key])?;
    Ok(())
}

/// Chunk metadata row returned by `query_chunks_for_range`.
pub struct ChunkMeta {
    pub chunk_num:    i64,
    pub archive_name: String,
    pub start_pos:    i64,
    #[allow(dead_code)]
    pub end_pos:      i64,
}

/// Return all chunks for `key` that overlap with the line range `[lo, hi]`.
pub fn query_chunks_for_range(
    conn: &Connection,
    key: &str,
    lo: usize,
    hi: usize,
) -> Result<Vec<ChunkMeta>> {
    let mut stmt = conn.prepare_cached(
        "SELECT c.chunk_num, a.name, c.start_line, c.end_line
         FROM chunks c
         JOIN archives a ON a.id = c.archive_id
         WHERE c.blob_key = ?1 AND c.start_line <= ?2 AND c.end_line >= ?3
         ORDER BY c.chunk_num",
    )?;
    let rows = stmt
        .query_map(
            rusqlite::params![key, hi as i64, lo as i64],
            |row| {
                Ok(ChunkMeta {
                    chunk_num:    row.get(0)?,
                    archive_name: row.get(1)?,
                    start_pos:    row.get(2)?,
                    end_pos:      row.get(3)?,
                })
            },
        )?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Collect all (archive_name, chunk_member_name) pairs for chunks not in `live_keys`.
pub struct OrphanRef {
    pub archive_name: String,
    pub chunk_member: String,
}

pub fn collect_orphan_chunks(conn: &Connection, live_keys: &[&str]) -> Result<Vec<OrphanRef>> {
    if live_keys.is_empty() {
        // All blobs are orphaned.
        let mut stmt = conn.prepare(
            "SELECT a.name, b.key, c.chunk_num
             FROM chunks c
             JOIN archives a ON a.id = c.archive_id
             JOIN blobs b ON b.key = c.blob_key",
        )?;
        let rows = stmt
            .query_map([], |row| {
                let archive_name: String = row.get(0)?;
                let key: String = row.get(1)?;
                let chunk_num: i64 = row.get(2)?;
                let key_prefix = key.chars().take(16).collect::<String>();
                Ok(OrphanRef {
                    archive_name,
                    chunk_member: format!("{key_prefix}.{chunk_num}"),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        return Ok(rows);
    }

    // Build NOT IN clause.
    let ph: String = (1..=live_keys.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT a.name, b.key, c.chunk_num
         FROM chunks c
         JOIN archives a ON a.id = c.archive_id
         JOIN blobs b ON b.key = c.blob_key
         WHERE b.key NOT IN ({ph})"
    );
    let params: Vec<&dyn rusqlite::ToSql> =
        live_keys.iter().map(|k| k as &dyn rusqlite::ToSql).collect();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            let archive_name: String = row.get(0)?;
            let key: String = row.get(1)?;
            let chunk_num: i64 = row.get(2)?;
            let key_prefix = key.chars().take(16).collect::<String>();
            Ok(OrphanRef {
                archive_name,
                chunk_member: format!("{key_prefix}.{chunk_num}"),
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Delete blob rows for orphaned keys. Call inside a transaction after ZIP compaction.
pub fn delete_orphan_blobs(tx: &rusqlite::Transaction, live_keys: &[&str]) -> Result<usize> {
    if live_keys.is_empty() {
        let n = tx.execute("DELETE FROM blobs", [])?;
        return Ok(n);
    }
    let ph: String = (1..=live_keys.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!("DELETE FROM blobs WHERE key NOT IN ({ph})");
    let params: Vec<&dyn rusqlite::ToSql> =
        live_keys.iter().map(|k| k as &dyn rusqlite::ToSql).collect();
    Ok(tx.execute(&sql, params.as_slice())?)
}

#[allow(dead_code)]
/// Return the name of every archive that has at least one chunk referenced by
/// any blob in `content.db` (used to find archives that can be fully deleted).
pub fn all_referenced_archive_names(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT a.name FROM archives a
         JOIN chunks c ON c.archive_id = a.id",
    )?;
    let rows = stmt
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

#[allow(dead_code)]
/// Remove an archive row (after the file has been deleted from disk).
pub fn delete_archive(tx: &rusqlite::Transaction, name: &str) -> Result<()> {
    tx.execute("DELETE FROM archives WHERE name = ?1", rusqlite::params![name])?;
    Ok(())
}
