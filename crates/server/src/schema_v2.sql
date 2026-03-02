PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;

CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS files (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    path              TEXT    NOT NULL UNIQUE,
    mtime             INTEGER NOT NULL,
    size              INTEGER NOT NULL,
    kind              TEXT    NOT NULL DEFAULT 'text',
    indexed_at        INTEGER,
    extract_ms        INTEGER,
    content_hash      TEXT,
    canonical_file_id INTEGER REFERENCES files(id) ON DELETE SET NULL
);

-- Inner archive members use composite paths: "archive.zip::member.txt"
-- No separate column needed; path IS the identifier.

CREATE INDEX IF NOT EXISTS files_content_hash ON files(content_hash)
    WHERE content_hash IS NOT NULL;
CREATE INDEX IF NOT EXISTS files_canonical ON files(canonical_file_id)
    WHERE canonical_file_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS lines (
    id                   INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id              INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    line_number          INTEGER NOT NULL,
    chunk_archive        TEXT    NOT NULL,  -- e.g., "content_00001.zip"
    chunk_name           TEXT    NOT NULL,  -- e.g., "path/to/file.chunk0.txt"
    line_offset_in_chunk INTEGER NOT NULL   -- which line within the chunk (0-indexed)
);

CREATE INDEX IF NOT EXISTS lines_file_id   ON lines(file_id);
CREATE INDEX IF NOT EXISTS lines_file_line ON lines(file_id, line_number);
CREATE INDEX IF NOT EXISTS lines_chunk     ON lines(chunk_archive, chunk_name);

-- FTS5 table with content='' (no content storage, index only)
CREATE VIRTUAL TABLE IF NOT EXISTS lines_fts USING fts5(
    content,
    content       = '',  -- Don't store content, only build index
    tokenize      = 'trigram'
);

-- Note: No triggers - FTS5 population is managed manually by worker
-- Worker will INSERT INTO lines_fts(rowid, content) after reading from ZIP

CREATE TABLE IF NOT EXISTS indexing_errors (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    path       TEXT    NOT NULL UNIQUE,
    error      TEXT    NOT NULL,
    first_seen INTEGER NOT NULL,
    last_seen  INTEGER NOT NULL,
    count      INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS scan_history (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    scanned_at  INTEGER NOT NULL,
    total_files INTEGER NOT NULL,
    total_size  INTEGER NOT NULL,
    by_kind     TEXT    NOT NULL
);
