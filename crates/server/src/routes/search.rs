use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use tokio::task::spawn_blocking;

use find_common::api::{SearchResponse, SearchResult};

use crate::fuzzy::FuzzyScorer;
use crate::{archive::ArchiveManager, db, AppState};

use super::{check_auth, source_db_path};

// ── GET /api/v1/search ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SearchParams {
    pub q: String,
    #[serde(default = "default_mode")]
    pub mode: String,
    /// Repeatable: ?source=a&source=b. Empty = all sources.
    /// Also accepts single value: ?source=a
    #[serde(default, deserialize_with = "deserialize_string_or_seq")]
    pub source: Vec<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}

/// Deserialize either a single string or a sequence of strings into Vec<String>
fn deserialize_string_or_seq<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct StringOrVec;

    impl<'de> serde::de::Visitor<'de> for StringOrVec {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("string or sequence of strings")
        }

        fn visit_str<E>(self, value: &str) -> Result<Vec<String>, E>
        where
            E: serde::de::Error,
        {
            Ok(vec![value.to_string()])
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Vec<String>, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut vec = Vec::new();
            while let Some(value) = seq.next_element()? {
                vec.push(value);
            }
            Ok(vec)
        }
    }

    deserializer.deserialize_any(StringOrVec)
}

fn default_mode() -> String { "fuzzy".into() }
fn default_limit() -> usize { 50 }

/// Extract maximal sequences of non-special characters from a regex pattern
/// to use as FTS5 pre-filter terms. Special regex chars (`^$.*+?|()[]{}\`)
/// act as delimiters; escaped sequences are skipped entirely.
///
/// Examples:
///   `^fn\s+\w+`   → "fn"   (too short, filtered out by fts_candidates)
///   `class\s+Foo` → "class Foo"
///   `password`    → "password"
fn regex_to_fts_terms(pattern: &str) -> String {
    let mut terms: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut chars = pattern.chars();

    while let Some(c) = chars.next() {
        if c == '\\' {
            // Escaped sequence — flush and skip the next char.
            if !current.is_empty() {
                terms.push(std::mem::take(&mut current));
            }
            chars.next();
        } else if "^$.*+?|()[]{}".contains(c) {
            // Regex special char — flush current literal sequence.
            if !current.is_empty() {
                terms.push(std::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        terms.push(current);
    }
    terms.join(" ")
}

// Construct resource URL by joining base_url with path
fn make_resource_url(base_url: &Option<String>, path: &str) -> Option<String> {
    base_url.as_ref().map(|base| {
        let base = base.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        format!("{}/{}", base, path)
    })
}

pub async fn search(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<SearchParams>,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) { return (s, Json(serde_json::Value::Null)).into_response(); }

    let sources_dir = state.data_dir.join("sources");
    let fts_limit = state.config.search.fts_candidate_limit;
    let query = params.q.clone();
    let mode = params.mode.clone();
    let limit = params.limit.min(state.config.search.max_limit);

    // Build the list of (source_name, db_path) to query.
    let source_dbs: Vec<(String, std::path::PathBuf)> = if params.source.is_empty() {
        // All sources: scan the sources directory.
        match std::fs::read_dir(&sources_dir) {
            Err(_) => vec![],
            Ok(rd) => rd
                .filter_map(|e| {
                    let e = e.ok()?;
                    let name = e.file_name().into_string().ok()?;
                    let source_name = name.strip_suffix(".db")?.to_string();
                    Some((source_name, e.path()))
                })
                .collect(),
        }
    } else {
        params.source.iter().filter_map(|s| {
            source_db_path(&state, s).ok().map(|p| (s.clone(), p))
        }).collect()
    };

    let data_dir = state.data_dir.clone();
    let offset = params.offset;

    // Only score enough candidates to fill this page plus a buffer for fuzzy
    // filtering. This avoids reading thousands of ZIP chunks for common queries
    // where the total far exceeds what we show.
    let scoring_limit = (offset + limit + 200).min(fts_limit);

    // Query each source DB in parallel.
    let handles: Vec<_> = source_dbs
        .into_iter()
        .map(|(source_name, db_path)| {
            let query = query.clone();
            let mode = mode.clone();
            let data_dir = data_dir.clone();
            spawn_blocking(move || -> anyhow::Result<(usize, Vec<SearchResult>)> {
                if !db_path.exists() { return Ok((0, vec![])); }
                let conn = db::open(&db_path)?;
                let archive_mgr = ArchiveManager::new(data_dir);
                let base_url = db::get_base_url(&conn)?;
                // For regex mode, extract literal character sequences from the pattern
                // for FTS5 pre-filtering, then apply the full regex as a post-filter.
                // For exact mode, treat the whole query as a phrase (literal substring).
                // For fuzzy mode, AND individual words.
                let (fts_phrase, fts_query) = match mode.as_str() {
                    "fuzzy" => (false, query.clone()),
                    "regex" => (false, regex_to_fts_terms(&query)),
                    _ /* "exact" */ => (true, query.clone()),
                };

                // Fast count via FTS5 only — no ZIP reads, no JOINs.
                let source_total = db::fts_count(&conn, &fts_query, fts_limit, fts_phrase)?;

                // Score only as many candidates as needed for this page.
                let candidates = db::fts_candidates(&conn, &archive_mgr, &fts_query, scoring_limit, fts_phrase)?;

                let results: Vec<SearchResult> = match mode.as_str() {
                    "exact" => {
                        // FTS5 trigram is already a substring match — candidates are the answer.
                        candidates.into_iter().map(|c| SearchResult {
                            source: source_name.clone(),
                            path: c.file_path.clone(),
                            archive_path: c.archive_path,
                            line_number: c.line_number,
                            snippet: c.content,
                            score: 0,
                            context_lines: vec![],
                            resource_url: make_resource_url(&base_url, &c.file_path),
                        }).collect()
                    }
                    "regex" => {
                        let re = regex::RegexBuilder::new(&query).case_insensitive(true).build()?;
                        candidates.into_iter()
                            .filter(|c| re.is_match(&c.content))
                            .map(|c| SearchResult {
                                source: source_name.clone(),
                                path: c.file_path.clone(),
                                archive_path: c.archive_path,
                                line_number: c.line_number,
                                snippet: c.content,
                                score: 0,
                                context_lines: vec![],
                                resource_url: make_resource_url(&base_url, &c.file_path),
                            })
                            .collect()
                    }
                    _ /* "fuzzy" */ => {
                        let mut scorer = FuzzyScorer::new(&query);
                        candidates.into_iter()
                            .filter_map(|c| {
                                scorer.score(&c.content).map(|score| SearchResult {
                                    source: source_name.clone(),
                                    path: c.file_path.clone(),
                                    archive_path: c.archive_path,
                                    line_number: c.line_number,
                                    snippet: c.content,
                                    score,
                                    context_lines: vec![],
                                    resource_url: make_resource_url(&base_url, &c.file_path),
                                })
                            })
                            .collect()
                    }
                };

                Ok((source_total, results))
            })
        })
        .collect();

    let mut total = 0usize;
    let mut all_results: Vec<SearchResult> = Vec::new();
    for handle in handles {
        match handle.await.unwrap_or_else(|e| Err(anyhow::anyhow!(e))) {
            Ok((source_total, mut r)) => {
                total += source_total;
                all_results.append(&mut r);
            }
            Err(e) => tracing::warn!("search source error: {e:#}"),
        }
    }

    all_results.sort_by(|a, b| b.score.cmp(&a.score));

    // Deduplicate by (source, path, archive_path, line_number), keeping the
    // highest-scoring occurrence (first after sort). Duplicates arise when FTS5
    // returns multiple rows for the same logical match (e.g. two members of the
    // same archive that share a line number after composite-path splitting).
    let mut seen = std::collections::HashSet::new();
    let unique: Vec<_> = all_results
        .into_iter()
        .filter(|r| seen.insert((r.source.clone(), r.path.clone(), r.archive_path.clone(), r.line_number)))
        .collect();

    let results: Vec<_> = unique.into_iter().skip(offset).take(limit).collect();

    Json(SearchResponse { results, total }).into_response()
}
