use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Query, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use tokio::task::spawn_blocking;

use find_common::{
    api::{
        ContextBatchRequest, ContextBatchResponse, ContextBatchResult,
        ContextResponse, FileResponse, SearchResponse, SearchResult, SourceInfo, TreeResponse,
    },
    fuzzy::FuzzyScorer,
};

use crate::{archive::ArchiveManager, db, AppState};

// ── Auth helper ───────────────────────────────────────────────────────────────

fn check_auth(state: &AppState, headers: &HeaderMap) -> Result<(), StatusCode> {
    let ok = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t == state.config.server.token)
        .unwrap_or(false);
    if ok { Ok(()) } else { Err(StatusCode::UNAUTHORIZED) }
}

fn source_db_path(state: &AppState, source: &str) -> Result<std::path::PathBuf, StatusCode> {
    // Validate source name to prevent path traversal.
    if !source.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(state.data_dir.join("sources").join(format!("{}.db", source)))
}

// ── GET /api/v1/sources ───────────────────────────────────────────────────────

pub async fn list_sources(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) {
        return (s, Json(serde_json::Value::Null)).into_response();
    }
    let sources_dir = state.data_dir.join("sources");
    let names: Vec<String> = match std::fs::read_dir(&sources_dir) {
        Err(_) => vec![],
        Ok(rd) => rd
            .filter_map(|e| {
                let e = e.ok()?;
                let name = e.file_name().into_string().ok()?;
                name.strip_suffix(".db").map(|s| s.to_string())
            })
            .collect(),
    };
    let mut infos: Vec<SourceInfo> = names
        .into_iter()
        .map(|name| {
            let db_path = sources_dir.join(format!("{}.db", name));
            let base_url = db::open(&db_path).ok().and_then(|conn| {
                db::get_base_url(&conn).ok().flatten()
            });
            SourceInfo { name, base_url }
        })
        .collect();
    infos.sort_by(|a, b| a.name.cmp(&b.name));
    Json(infos).into_response()
}

// ── GET /api/v1/file?source=X&path=Y[&archive_path=Z] ────────────────────────
//
// `path` may be a composite path ("archive.zip::member.txt") or, for backward
// compatibility, a plain path with `archive_path` supplied separately.

#[derive(Deserialize)]
pub struct FileParams {
    pub source: String,
    pub path: String,
    /// Legacy: combine with `path` into a composite path if provided.
    pub archive_path: Option<String>,
}

pub async fn get_file(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<FileParams>,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) {
        return (s, Json(serde_json::Value::Null)).into_response();
    }

    let db_path = match source_db_path(&state, &params.source) {
        Ok(p) => p,
        Err(s) => return (s, Json(serde_json::Value::Null)).into_response(),
    };

    // Build composite path from path + optional archive_path (backward compat).
    let full_path = match &params.archive_path {
        Some(ap) if !ap.is_empty() => format!("{}::{}", params.path, ap),
        _ => params.path.clone(),
    };

    let data_dir = state.data_dir.clone();
    match spawn_blocking(move || {
        let conn = db::open(&db_path)?;
        let archive_mgr = ArchiveManager::new(data_dir);
        let kind: String = conn
            .query_row(
                "SELECT kind FROM files WHERE path = ?1",
                rusqlite::params![full_path],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| "text".into());
        let lines = db::get_file_lines(&conn, &archive_mgr, &full_path)?;
        let total_lines = lines.len();
        Ok::<_, anyhow::Error>(FileResponse { lines, file_kind: kind, total_lines })
    })
    .await
    .unwrap_or_else(|e| Err(anyhow::anyhow!(e)))
    {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => {
            tracing::error!("get_file: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// ── GET /api/v1/files?source=<name> ──────────────────────────────────────────

#[derive(Deserialize)]
pub struct SourceParam {
    pub source: String,
}

pub async fn list_files(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<SourceParam>,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) { return (s, Json(serde_json::Value::Null)).into_response(); }

    let db_path = match source_db_path(&state, &params.source) {
        Ok(p) => p,
        Err(s) => return (s, Json(serde_json::Value::Null)).into_response(),
    };

    match spawn_blocking(move || {
        let conn = db::open(&db_path)?;
        db::list_files(&conn)
    })
    .await
    .unwrap_or_else(|e| Err(anyhow::anyhow!(e)))
    {
        Ok(files) => Json(files).into_response(),
        Err(e) => {
            tracing::error!("list_files: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// ── POST /api/v1/bulk ─────────────────────────────────────────────────────────

pub async fn bulk(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) { return s.into_response(); }

    let is_gzip = headers
        .get(header::CONTENT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(|v| v == "gzip")
        .unwrap_or(false);

    if !is_gzip {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }

    let request_id = format!(
        "req_{}_{}",
        chrono::Utc::now().format("%Y%m%d_%H%M%S"),
        uuid::Uuid::new_v4().simple()
    );

    let inbox_path = state.data_dir.join("inbox").join(format!("{request_id}.gz"));

    match tokio::fs::write(&inbox_path, &body).await {
        Ok(()) => {
            tracing::info!("Queued bulk request: {}", inbox_path.display());
            StatusCode::ACCEPTED.into_response()
        }
        Err(e) => {
            tracing::error!("Failed to write inbox request: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

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

    // Query each source DB in parallel.
    let handles: Vec<_> = source_dbs
        .into_iter()
        .map(|(source_name, db_path)| {
            let query = query.clone();
            let mode = mode.clone();
            let data_dir = data_dir.clone();
            spawn_blocking(move || -> anyhow::Result<Vec<SearchResult>> {
                if !db_path.exists() { return Ok(vec![]); }
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
                let candidates = db::fts_candidates(&conn, &archive_mgr, &fts_query, fts_limit, fts_phrase)?;

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

                Ok(results)
            })
        })
        .collect();

    let mut all_results: Vec<SearchResult> = Vec::new();
    for handle in handles {
        match handle.await.unwrap_or_else(|e| Err(anyhow::anyhow!(e))) {
            Ok(mut r) => all_results.append(&mut r),
            Err(e) => tracing::warn!("search source error: {e}"),
        }
    }

    all_results.sort_by(|a, b| b.score.cmp(&a.score));
    let total = all_results.len();
    let results: Vec<_> = all_results.into_iter().skip(params.offset).take(limit).collect();

    Json(SearchResponse { results, total }).into_response()
}

// ── GET /api/v1/context ───────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ContextParams {
    pub source: String,
    pub path: String,
    /// Legacy: combined with `path` into a composite path if provided.
    pub archive_path: Option<String>,
    pub line: usize,
    #[serde(default = "default_window")]
    pub window: usize,
}

fn default_window() -> usize { 5 }

pub async fn get_context(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<ContextParams>,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) { return (s, Json(serde_json::Value::Null)).into_response(); }

    let db_path = match source_db_path(&state, &params.source) {
        Ok(p) => p,
        Err(s) => return (s, Json(serde_json::Value::Null)).into_response(),
    };

    let full_path = match &params.archive_path {
        Some(ap) if !ap.is_empty() => format!("{}::{}", params.path, ap),
        _ => params.path.clone(),
    };

    let data_dir = state.data_dir.clone();
    match spawn_blocking(move || {
        let conn = db::open(&db_path)?;
        let archive_mgr = ArchiveManager::new(data_dir);
        let kind: String = conn.query_row(
            "SELECT kind FROM files WHERE path = ?1",
            rusqlite::params![full_path],
            |row| row.get(0),
        ).unwrap_or_else(|_| "text".into());

        let lines = db::get_context(
            &conn,
            &archive_mgr,
            &full_path,
            params.line,
            params.window,
        )?;
        Ok::<_, anyhow::Error>(ContextResponse { lines, file_kind: kind })
    })
    .await
    .unwrap_or_else(|e| Err(anyhow::anyhow!(e)))
    {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => {
            tracing::error!("context: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// ── GET /api/v1/metrics ───────────────────────────────────────────────────────

pub async fn get_metrics(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) {
        return (s, Json(serde_json::Value::Null)).into_response();
    }

    let inbox_dir = state.data_dir.join("inbox");
    let failed_dir = inbox_dir.join("failed");
    let sources_dir = state.data_dir.join("sources");

    let count_gz = |dir: &std::path::Path| -> usize {
        std::fs::read_dir(dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().map(|x| x == "gz").unwrap_or(false))
                    .count()
            })
            .unwrap_or(0)
    };

    // Count archives across subfolders in sources/content/N/
    let total_archives = {
        let content_dir = sources_dir.join("content");
        let mut count = 0;
        if let Ok(rd) = std::fs::read_dir(&content_dir) {
            for entry in rd.filter_map(|e| e.ok()) {
                if entry.path().is_dir() {
                    if let Ok(subdir) = std::fs::read_dir(entry.path()) {
                        count += subdir
                            .filter_map(|e| e.ok())
                            .filter(|e| e.path().extension().map(|x| x == "zip").unwrap_or(false))
                            .count();
                    }
                }
            }
        }
        count
    };

    let inbox_queue_depth = count_gz(&inbox_dir);
    let failed_requests = count_gz(&failed_dir);

    Json(serde_json::json!({
        "inbox_queue_depth": inbox_queue_depth,
        "failed_requests":   failed_requests,
        "total_archives":    total_archives,
    }))
    .into_response()
}

// ── POST /api/v1/context-batch ────────────────────────────────────────────────

pub async fn context_batch(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<ContextBatchRequest>,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) {
        return (s, Json(serde_json::Value::Null)).into_response();
    }

    let data_dir = state.data_dir.clone();

    match spawn_blocking(move || {
        let archive_mgr = ArchiveManager::new(data_dir.clone());
        let mut results = Vec::with_capacity(req.requests.len());

        for item in req.requests {
            if !item.source.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                results.push(ContextBatchResult {
                    source: item.source,
                    path: item.path,
                    line: item.line,
                    lines: vec![],
                    file_kind: String::new(),
                });
                continue;
            }

            let db_path = data_dir.join("sources").join(format!("{}.db", item.source));
            if !db_path.exists() {
                results.push(ContextBatchResult {
                    source: item.source,
                    path: item.path,
                    line: item.line,
                    lines: vec![],
                    file_kind: String::new(),
                });
                continue;
            }

            let full_path = match &item.archive_path {
                Some(ap) if !ap.is_empty() => format!("{}::{}", item.path, ap),
                _ => item.path.clone(),
            };

            let (file_kind, lines) = match db::open(&db_path).and_then(|conn| {
                let kind = conn
                    .query_row(
                        "SELECT kind FROM files WHERE path = ?1",
                        rusqlite::params![full_path],
                        |row| row.get::<_, String>(0),
                    )
                    .unwrap_or_else(|_| "text".into());
                let l = db::get_context(
                    &conn,
                    &archive_mgr,
                    &full_path,
                    item.line,
                    item.window,
                )?;
                Ok((kind, l))
            }) {
                Ok(pair) => pair,
                Err(e) => {
                    tracing::warn!("context_batch item {}/{}: {e}", item.source, item.path);
                    (String::new(), vec![])
                }
            };

            results.push(ContextBatchResult {
                source: item.source,
                path: item.path,
                line: item.line,
                lines,
                file_kind,
            });
        }

        Ok::<_, anyhow::Error>(ContextBatchResponse { results })
    })
    .await
    .unwrap_or_else(|e| Err(anyhow::anyhow!(e)))
    {
        Ok(resp) => Json(resp).into_response(),
        Err(e) => {
            tracing::error!("context_batch: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// ── GET /api/v1/tree ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct TreeParams {
    pub source: String,
    /// Directory prefix to list (empty string = root). Must end with `/` for
    /// non-root queries, e.g. `"src/"`.
    #[serde(default)]
    pub prefix: String,
}

pub async fn list_dir(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<TreeParams>,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) {
        return (s, Json(serde_json::Value::Null)).into_response();
    }

    let db_path = match source_db_path(&state, &params.source) {
        Ok(p) => p,
        Err(s) => return (s, Json(serde_json::Value::Null)).into_response(),
    };

    if !db_path.exists() {
        return (StatusCode::NOT_FOUND, Json(serde_json::Value::Null)).into_response();
    }

    let prefix = params.prefix.clone();
    let result = spawn_blocking(move || {
        let conn = db::open(&db_path)?;
        db::list_dir(&conn, &prefix)
    })
    .await;

    match result {
        Ok(Ok(entries)) => Json(TreeResponse { entries }).into_response(),
        Ok(Err(e)) => {
            tracing::error!("list_dir error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
        Err(e) => {
            tracing::error!("list_dir task error: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
