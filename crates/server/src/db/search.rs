use std::collections::HashMap;

use anyhow::Result;
use rusqlite::{Connection, params};

use crate::archive::ArchiveManager;

use super::read_chunk_lines;
use super::split_composite_path;

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
    /// The file's row ID in the `files` table (used for alias lookup).
    pub file_id: i64,
}

/// Build an FTS5 match expression from a raw query string.
/// Returns None if the query produces no matchable terms.
pub(crate) fn build_fts_query(query: &str, phrase: bool) -> Option<String> {
    if phrase {
        if query.len() < 3 {
            return None;
        }
        Some(format!("\"{}\"", query.replace('"', "\"\"")))
    } else {
        // Use unquoted terms so FTS5 treats each word as a token query rather
        // than a phrase query.  Quoted phrases require ≥3 trigrams to match
        // (i.e. the term must be ≥5 chars), which breaks short-word searches
        // like "test" (4 chars, 2 trigrams).  Unquoted token queries have no
        // such minimum.  Strip FTS5 syntax characters to avoid query errors.
        let terms: Vec<String> = query
            .split_whitespace()
            .map(|w| w.chars().filter(|c| !matches!(c, '"' | '*' | '(' | ')' | '^')).collect::<String>())
            .filter(|w| w.len() >= 3)
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

/// FTS5 trigram pre-filter. Returns up to `limit` candidate rows.
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
        file_id: i64,
    }

    let mut stmt = conn.prepare(
        "SELECT f.path, f.kind, l.line_number,
                l.chunk_archive, l.chunk_name, l.line_offset_in_chunk, f.id
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
                file_id:      row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Read content from ZIP archives, caching chunks to avoid redundant reads.
    let mut chunk_cache: HashMap<(String, String), Vec<String>> = HashMap::new();
    let mut results = Vec::with_capacity(raw.len());

    for row in raw {
        let content = read_chunk_lines(&mut chunk_cache, archive_mgr, &row.chunk_archive, &row.chunk_name)
            .get(row.line_offset)
            .cloned()
            .unwrap_or_default();

        // Split composite path into outer path + archive_path for search result compat.
        let (file_path, archive_path) = split_composite_path(&row.file_path);

        results.push(CandidateRow {
            file_path,
            file_kind:    row.file_kind,
            archive_path,
            line_number:  row.line_number,
            content,
            file_id:      row.file_id,
        });
    }

    Ok(results)
}

/// Return type for `document_candidates`: total qualifying files + per-file (representative, extras).
pub type DocumentCandidates = (usize, Vec<(CandidateRow, Vec<CandidateRow>)>);

/// Document-level fuzzy candidate search.
///
/// Unlike `fts_candidates` (which requires all query terms on the *same* line),
/// this finds files where each query term appears on *any* line, then surfaces
/// one result per file with extra_matches carrying the best line per remaining token.
///
/// Returns `(total, Vec<(representative, extra_matches)>)`.
/// `total` is the number of qualifying files before the limit is applied.
pub fn document_candidates(
    conn: &Connection,
    archive_mgr: &ArchiveManager,
    query: &str,
    limit: usize,
) -> Result<DocumentCandidates> {
    use std::collections::HashSet;

    let tokens: Vec<String> = query
        .split_whitespace()
        .filter(|w| w.len() >= 3)
        .map(|w| w.to_string())
        .collect();

    if tokens.is_empty() {
        return Ok((0, vec![]));
    }

    // For each token, collect the set of file_ids that have at least one matching line.
    let mut per_token_ids: Vec<HashSet<i64>> = Vec::new();
    for token in &tokens {
        let fts_expr = format!("\"{}\"", token.replace('"', "\"\""));
        let mut stmt = conn.prepare(
            "SELECT DISTINCT l.file_id
             FROM lines_fts
             JOIN lines l ON l.id = lines_fts.rowid
             WHERE lines_fts MATCH ?1
             LIMIT 100000",
        )?;
        let ids: HashSet<i64> = stmt
            .query_map(params![fts_expr], |row| row.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        per_token_ids.push(ids);
    }

    // Intersect: files that have ALL tokens somewhere.
    let qualifying_ids: HashSet<i64> = per_token_ids
        .into_iter()
        .reduce(|a, b| a.intersection(&b).copied().collect())
        .unwrap_or_default();

    let total = qualifying_ids.len();
    if total == 0 {
        return Ok((0, vec![]));
    }

    let or_expr = tokens
        .iter()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" OR ");

    // Fetch up to `tokens.len()` lines per qualifying file so we can pick the best
    // line per token. We need enough rows to fill `limit` files × N tokens.
    let per_file_cap = tokens.len().max(1);
    let fetch_limit = (limit * 20 * per_file_cap).max(10_000) as i64;

    struct RawRow {
        file_path: String,
        file_kind: String,
        line_number: usize,
        chunk_archive: String,
        chunk_name: String,
        line_offset: usize,
        file_id: i64,
    }

    let mut stmt = conn.prepare(
        "SELECT f.path, f.kind, l.line_number,
                l.chunk_archive, l.chunk_name, l.line_offset_in_chunk, f.id
         FROM lines_fts
         JOIN lines l ON l.id = lines_fts.rowid
         JOIN files f ON f.id = l.file_id
         WHERE lines_fts MATCH ?1
         ORDER BY lines_fts.rank
         LIMIT ?2",
    )?;

    // Collect up to `per_file_cap` raw rows per qualifying file.
    let mut file_rows: HashMap<i64, Vec<RawRow>> = HashMap::new();
    let mut file_order: Vec<i64> = Vec::new(); // insertion order for stable output

    let mut rows = stmt.query(params![or_expr, fetch_limit])?;
    while let Some(row) = rows.next()? {
        let file_id: i64 = row.get(6)?;
        if !qualifying_ids.contains(&file_id) {
            continue;
        }
        let entry = file_rows.entry(file_id).or_insert_with(|| {
            file_order.push(file_id);
            Vec::new()
        });
        if entry.len() < per_file_cap {
            entry.push(RawRow {
                file_path:    row.get(0)?,
                file_kind:    row.get(1)?,
                line_number:  row.get::<_, i64>(2)? as usize,
                chunk_archive: row.get(3)?,
                chunk_name:   row.get(4)?,
                line_offset:  row.get::<_, i64>(5)? as usize,
                file_id,
            });
        }
        if file_order.len() >= limit && file_rows.get(&file_order[file_order.len()-1]).map_or(0, |v| v.len()) >= per_file_cap {
            break;
        }
    }

    // Read content from ZIP archives, reusing a chunk cache.
    let mut chunk_cache: HashMap<(String, String), Vec<String>> = HashMap::new();
    let tokens_lower: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();

    let mut results = Vec::new();
    for file_id in file_order.into_iter().take(limit) {
        let rows = match file_rows.remove(&file_id) {
            Some(r) => r,
            None => continue,
        };

        // First row is the top FTS-ranked line → the representative.
        let rep_row = &rows[0];
        let rep_content = read_chunk_lines(&mut chunk_cache, archive_mgr, &rep_row.chunk_archive, &rep_row.chunk_name)
            .get(rep_row.line_offset)
            .cloned()
            .unwrap_or_default();
        let rep_content_lower = rep_content.to_lowercase();
        let (file_path, archive_path) = split_composite_path(&rep_row.file_path);

        let representative = CandidateRow {
            file_path: file_path.clone(),
            file_kind: rep_row.file_kind.clone(),
            archive_path: archive_path.clone(),
            line_number: rep_row.line_number,
            content: rep_content,
            file_id,
        };

        // For each token not already covered by the representative, find the first
        // subsequent row that covers it (simple case-insensitive substring check).
        let mut uncovered: Vec<&str> = tokens_lower
            .iter()
            .filter(|t| !rep_content_lower.contains(t.as_str()))
            .map(|t| t.as_str())
            .collect();

        let mut extras: Vec<CandidateRow> = Vec::new();
        for extra_row in &rows[1..] {
            if uncovered.is_empty() {
                break;
            }
            let content = read_chunk_lines(&mut chunk_cache, archive_mgr, &extra_row.chunk_archive, &extra_row.chunk_name)
                .get(extra_row.line_offset)
                .cloned()
                .unwrap_or_default();
            let content_lower = content.to_lowercase();
            // Only include this row if it covers at least one new token.
            let newly_covered: Vec<usize> = uncovered
                .iter()
                .enumerate()
                .filter(|(_, t)| content_lower.contains(*t))
                .map(|(i, _)| i)
                .collect();
            if !newly_covered.is_empty() {
                // Skip line_number=0 (metadata/path lines) — not useful as highlights.
                if extra_row.line_number > 0 {
                    let (ep, ea) = split_composite_path(&extra_row.file_path);
                    extras.push(CandidateRow {
                        file_path: ep,
                        file_kind: extra_row.file_kind.clone(),
                        archive_path: ea,
                        line_number: extra_row.line_number,
                        content,
                        file_id,
                    });
                }
                // Remove newly covered tokens (iterate in reverse to preserve indices).
                for i in newly_covered.into_iter().rev() {
                    uncovered.swap_remove(i);
                }
            }
        }

        results.push((representative, extras));
    }

    Ok((total, results))
}

/// Fetch alias paths grouped by their canonical file ID.
/// Returns a map of canonical_id → list of alias paths.
pub fn fetch_aliases_for_canonical_ids(
    conn: &Connection,
    canonical_ids: &[i64],
) -> Result<HashMap<i64, Vec<String>>> {
    let mut map: HashMap<i64, Vec<String>> = HashMap::new();
    if canonical_ids.is_empty() {
        return Ok(map);
    }
    let mut stmt = conn.prepare(
        "SELECT canonical_file_id, path FROM files
         WHERE canonical_file_id = ?1
         ORDER BY path",
    )?;
    for &cid in canonical_ids {
        let paths: Vec<String> = stmt
            .query_map(params![cid], |row| row.get(1))?
            .collect::<rusqlite::Result<_>>()?;
        if !paths.is_empty() {
            map.insert(cid, paths);
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── build_fts_query ──────────────────────────────────────────────────────

    #[test]
    fn fts_phrase_wraps_in_quotes() {
        assert_eq!(build_fts_query("hello world", true).as_deref(), Some("\"hello world\""));
    }

    #[test]
    fn fts_phrase_too_short_returns_none() {
        assert!(build_fts_query("ab", true).is_none());
    }

    #[test]
    fn fts_phrase_exactly_3_chars_ok() {
        assert!(build_fts_query("abc", true).is_some());
    }

    #[test]
    fn fts_fuzzy_joins_terms_with_and() {
        let q = build_fts_query("foo bar", false).unwrap();
        assert!(q.contains("foo"));
        assert!(q.contains("AND"));
        assert!(q.contains("bar"));
    }

    #[test]
    fn fts_fuzzy_filters_short_terms() {
        // All terms < 3 chars → None
        assert!(build_fts_query("to go", false).is_none());
    }

    #[test]
    fn fts_fuzzy_mixed_length_keeps_long_terms() {
        // "to" (2 chars) is filtered, "foo" (3 chars) is kept
        let q = build_fts_query("to foo", false).unwrap();
        assert!(q.contains("foo"));
        assert!(!q.contains("to"));
    }

    #[test]
    fn fts_fuzzy_strips_special_chars() {
        let q = build_fts_query("test^query", false).unwrap();
        assert!(!q.contains('^'));
        assert!(q.contains("testquery") || q.contains("test"));
    }
}
