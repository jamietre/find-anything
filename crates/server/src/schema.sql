PRAGMA journal_mode=WAL;
PRAGMA foreign_keys=ON;

CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS files (
    id    INTEGER PRIMARY KEY AUTOINCREMENT,
    path  TEXT    NOT NULL UNIQUE,
    mtime INTEGER NOT NULL,
    size  INTEGER NOT NULL,
    kind  TEXT    NOT NULL DEFAULT 'text'
);

CREATE TABLE IF NOT EXISTS lines (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    file_id      INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    archive_path TEXT,
    line_number  INTEGER NOT NULL,
    content      TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS lines_file_id   ON lines(file_id);
CREATE INDEX IF NOT EXISTS lines_file_line ON lines(file_id, archive_path, line_number);

CREATE VIRTUAL TABLE IF NOT EXISTS lines_fts USING fts5(
    content,
    content     = 'lines',
    content_rowid = 'id',
    tokenize    = 'trigram'
);

CREATE TRIGGER IF NOT EXISTS lines_ai AFTER INSERT ON lines BEGIN
    INSERT INTO lines_fts(rowid, content) VALUES (new.id, new.content);
END;

CREATE TRIGGER IF NOT EXISTS lines_ad AFTER DELETE ON lines BEGIN
    INSERT INTO lines_fts(lines_fts, rowid, content)
    VALUES ('delete', old.id, old.content);
END;

CREATE TRIGGER IF NOT EXISTS lines_au AFTER UPDATE OF content ON lines BEGIN
    INSERT INTO lines_fts(lines_fts, rowid, content)
    VALUES ('delete', old.id, old.content);
    INSERT INTO lines_fts(rowid, content) VALUES (new.id, new.content);
END;
