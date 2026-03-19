/// FTS5 rowid encoding: `rowid = file_id * MAX_LINES_PER_FILE + line_number`.
///
/// ## Why we encode this way
///
/// SQLite's FTS5 virtual table associates each indexed row with a single integer
/// rowid.  The schema v3 design has no separate `lines` table, so there is no
/// natural integer key for a (file, line) pair.  Instead we pack both values
/// into the rowid using a fixed multiplier:
///
/// ```text
/// rowid = file_id * MAX_LINES_PER_FILE + line_number
/// ```
///
/// This lets every FTS5 search query decode the originating `file_id` and
/// `line_number` directly from the rowid via integer arithmetic, without any
/// JOIN to an auxiliary table:
///
/// ```sql
/// -- Extract file_id and line_number in a single pass:
/// SELECT (lines_fts.rowid / 1000000) AS file_id,
///        (lines_fts.rowid % 1000000) AS line_number
/// FROM lines_fts WHERE lines_fts MATCH ?
/// ```
///
/// The JOIN to `files` then becomes:
///
/// ```sql
/// JOIN files f ON f.id = (lines_fts.rowid / 1000000)
/// ```
///
/// ## Stability guarantee
///
/// **This constant must never change** once an index has been built.  Changing
/// it would corrupt all existing FTS data — every stored rowid would decode to
/// the wrong `(file_id, line_number)` pair.  If the limit ever needs to grow,
/// a full schema migration with a mandatory re-index is required.
///
/// ## Overflow safety
///
/// The maximum safe `file_id` before `i64` overflow is roughly 9.2 × 10¹²
/// (i64::MAX / 1_000_000), which is effectively unlimited for any realistic
/// corpus.  Files with `line_number ≥ MAX_LINES_PER_FILE` have their excess
/// lines dropped from the FTS index (logged as a warning at index time).
pub const MAX_LINES_PER_FILE: i64 = 1_000_000;

/// SQL expression that extracts the `file_id` from a packed FTS5 rowid.
///
/// Use in SQL queries wherever you need to resolve the file that produced an
/// FTS5 match. See [`MAX_LINES_PER_FILE`] for the encoding rationale.
///
/// Example:
/// ```sql
/// JOIN files f ON f.id = {SQL_FTS_FILE_ID}
/// ```
pub const SQL_FTS_FILE_ID: &str = "(lines_fts.rowid / 1000000)";

/// SQL expression that extracts the `line_number` from a packed FTS5 rowid.
///
/// Use in SELECT lists wherever you need the matched line number alongside
/// file metadata. See [`MAX_LINES_PER_FILE`] for the encoding rationale.
///
/// Example:
/// ```sql
/// SELECT {SQL_FTS_LINE_NUMBER} AS line_number, f.path, ...
/// ```
pub const SQL_FTS_LINE_NUMBER: &str = "(lines_fts.rowid % 1000000)";

/// SQL predicate that restricts FTS5 results to line_number = 0.
///
/// Line 0 is always the file's own path string, so this clause limits results
/// to filename-only / path-index matches (used by `filename_only` search mode).
pub const SQL_FTS_FILENAME_ONLY: &str = "(lines_fts.rowid % 1000000) = 0";

/// SQL predicate that restricts FTS5 results to line_number = 1.
///
/// Line 1 is the concatenated metadata slot (EXIF tags, audio tags, MIME type,
/// document title/author, etc.). Used to target metadata-only searches.
pub const SQL_FTS_METADATA_ONLY: &str = "(lines_fts.rowid % 1000000) = 1";

/// SQL predicate that restricts FTS5 results to content lines (line_number >= 2).
///
/// Lines 2+ are extracted file content (text, PDF paragraphs, etc.).
pub const SQL_FTS_CONTENT_ONLY: &str = "(lines_fts.rowid % 1000000) >= 2";

pub fn encode_fts_rowid(file_id: i64, line_number: i64) -> i64 {
    debug_assert!(line_number < MAX_LINES_PER_FILE, "line_number {line_number} would overflow FTS rowid");
    file_id * MAX_LINES_PER_FILE + line_number
}

pub fn decode_fts_rowid(rowid: i64) -> (i64, i64) {
    (rowid / MAX_LINES_PER_FILE, rowid % MAX_LINES_PER_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        for (file_id, line_number) in [(1, 0), (1, 1), (42, 999999), (100, 500000)] {
            let rowid = encode_fts_rowid(file_id, line_number);
            let (dec_file_id, dec_line) = decode_fts_rowid(rowid);
            assert_eq!(dec_file_id, file_id, "file_id mismatch for ({file_id}, {line_number})");
            assert_eq!(dec_line, line_number, "line_number mismatch for ({file_id}, {line_number})");
        }
    }

    #[test]
    fn sql_constants_match_max_lines_per_file() {
        // Verify the literal in the SQL constants matches MAX_LINES_PER_FILE.
        let n = MAX_LINES_PER_FILE.to_string();
        assert!(SQL_FTS_FILE_ID.contains(&n),
            "SQL_FTS_FILE_ID {SQL_FTS_FILE_ID:?} doesn't use {n}");
        assert!(SQL_FTS_LINE_NUMBER.contains(&n),
            "SQL_FTS_LINE_NUMBER {SQL_FTS_LINE_NUMBER:?} doesn't use {n}");
        assert!(SQL_FTS_FILENAME_ONLY.contains(&n),
            "SQL_FTS_FILENAME_ONLY {SQL_FTS_FILENAME_ONLY:?} doesn't use {n}");
        assert!(SQL_FTS_METADATA_ONLY.contains(&n),
            "SQL_FTS_METADATA_ONLY {SQL_FTS_METADATA_ONLY:?} doesn't use {n}");
        assert!(SQL_FTS_CONTENT_ONLY.contains(&n),
            "SQL_FTS_CONTENT_ONLY {SQL_FTS_CONTENT_ONLY:?} doesn't use {n}");
    }
}
