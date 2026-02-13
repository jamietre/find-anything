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
        ContextResponse, DeleteRequest, FileResponse, ScanCompleteRequest, SearchResponse,
        SearchResult, TreeResponse,
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
    let mut sources: Vec<String> = match std::fs::read_dir(&sources_dir) {
        Err(_) => vec![],
        Ok(rd) => rd
            .filter_map(|e| {
                let e = e.ok()?;
                let name = e.file_name().into_string().ok()?;
                name.strip_suffix(".db").map(|s| s.to_string())
            })
            .collect(),
    };
    sources.sort();
    Json(sources).into_response()
}

// ── GET /api/v1/file?source=X&path=Y&archive_path=Z ──────────────────────────

#[derive(Deserialize)]
pub struct FileParams {
    pub source: String,
    pub path: String,
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

    let data_dir = state.data_dir.clone();
    match spawn_blocking(move || {
        let conn = db::open(&db_path)?;
        let archive_mgr = ArchiveManager::new(data_dir);
        let kind: String = conn
            .query_row(
                "SELECT kind FROM files WHERE path = ?1",
                rusqlite::params![params.path],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| "text".into());
        let lines =
            db::get_file_lines(&conn, &archive_mgr, &params.path, params.archive_path.as_deref())?;
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

// ── PUT /api/v1/files ─────────────────────────────────────────────────────────

pub async fn upsert_files(
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
            tracing::info!("Queued index request: {}", inbox_path.display());
            StatusCode::ACCEPTED.into_response()
        }
        Err(e) => {
            tracing::error!("Failed to write inbox request: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// ── DELETE /api/v1/files ──────────────────────────────────────────────────────

pub async fn delete_files(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<DeleteRequest>,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) { return s.into_response(); }

    let db_path = match source_db_path(&state, &req.source) {
        Ok(p) => p,
        Err(s) => return s.into_response(),
    };

    match spawn_blocking(move || {
        let conn = db::open(&db_path)?;
        db::delete_files(&conn, &req.paths)
    })
    .await
    .unwrap_or_else(|e| Err(anyhow::anyhow!(e)))
    {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => {
            tracing::error!("delete_files: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

// ── POST /api/v1/scan-complete ────────────────────────────────────────────────

pub async fn scan_complete(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<ScanCompleteRequest>,
) -> impl IntoResponse {
    if let Err(s) = check_auth(&state, &headers) { return s.into_response(); }

    let db_path = match source_db_path(&state, &req.source) {
        Ok(p) => p,
        Err(s) => return s.into_response(),
    };

    match spawn_blocking(move || {
        let conn = db::open(&db_path)?;
        db::update_last_scan(&conn, req.timestamp)
    })
    .await
    .unwrap_or_else(|e| Err(anyhow::anyhow!(e)))
    {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => {
            tracing::error!("scan_complete: {e}");
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
    #[serde(default)]
    pub source: Vec<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
    /// Lines of context to include before/after each match (0 = none).
    #[serde(default)]
    pub context: usize,
}

fn default_mode() -> String { "fuzzy".into() }
fn default_limit() -> usize { 50 }

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
    let context_size = params.context;

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
                // fuzzy: AND individual words so "pass strength" finds "password strength"
                // exact/regex: phrase query — literal substring
                let phrase = mode != "fuzzy";
                let candidates = db::fts_candidates(&conn, &archive_mgr, &query, fts_limit, phrase)?;

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
                        let re = regex::Regex::new(&query)?;
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

                // Optionally enrich each result with context lines.
                let results = if context_size > 0 {
                    results
                        .into_iter()
                        .map(|mut r| {
                            if let Ok(ctx) = db::get_context(
                                &conn,
                                &archive_mgr,
                                &r.path,
                                r.archive_path.as_deref(),
                                r.line_number,
                                context_size,
                            ) {
                                r.context_lines = ctx;
                            }
                            r
                        })
                        .collect()
                } else {
                    results
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

    let data_dir = state.data_dir.clone();
    match spawn_blocking(move || {
        let conn = db::open(&db_path)?;
        let archive_mgr = ArchiveManager::new(data_dir);
        let kind: String = conn.query_row(
            "SELECT kind FROM files WHERE path = ?1",
            rusqlite::params![params.path],
            |row| row.get(0),
        ).unwrap_or_else(|_| "text".into());

        let lines = db::get_context(
            &conn,
            &archive_mgr,
            &params.path,
            params.archive_path.as_deref(),
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

    let count_zip = |dir: &std::path::Path| -> usize {
        std::fs::read_dir(dir)
            .map(|rd| {
                rd.filter_map(|e| e.ok())
                    .filter(|e| e.path().extension().map(|x| x == "zip").unwrap_or(false))
                    .count()
            })
            .unwrap_or(0)
    };

    let inbox_queue_depth = count_gz(&inbox_dir);
    let failed_requests = count_gz(&failed_dir);
    let total_archives = count_zip(&sources_dir);

    Json(serde_json::json!({
        "inbox_queue_depth": inbox_queue_depth,
        "failed_requests":   failed_requests,
        "total_archives":    total_archives,
    }))
    .into_response()
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
