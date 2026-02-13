use serde::{Deserialize, Serialize};

/// GET /api/v1/sources response entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    pub name: String,
    pub base_url: Option<String>,
}

/// A single extracted line sent from client → server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexLine {
    /// NULL for regular files; inner path for archive entries; "page:N" for PDFs.
    pub archive_path: Option<String>,
    pub line_number: usize,
    pub content: String,
}

/// A file record sent from client → server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexFile {
    /// Relative path within the source base_path.
    pub path: String,
    pub mtime: i64,
    pub size: i64,
    /// "text" | "pdf" | "archive" | "image" | "audio"
    pub kind: String,
    pub lines: Vec<IndexLine>,
}

/// POST /api/v1/bulk request body.
/// Combines upserts, deletes, and scan-complete into a single async operation.
#[derive(Debug, Serialize, Deserialize)]
pub struct BulkRequest {
    pub source: String,
    /// Files to upsert into the index.
    #[serde(default)]
    pub files: Vec<IndexFile>,
    /// Paths to remove from the index.
    #[serde(default)]
    pub delete_paths: Vec<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    /// If present, update the last_scan timestamp for this source.
    #[serde(default)]
    pub scan_timestamp: Option<i64>,
}

/// One search result.
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub source: String,
    pub path: String,
    pub archive_path: Option<String>,
    pub line_number: usize,
    pub snippet: String,
    pub score: u32,
    /// Populated when ?context=N is passed to the search endpoint.
    #[serde(default)]
    pub context_lines: Vec<ContextLine>,
    /// Full URL to access the resource (if base_url configured for source).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_url: Option<String>,
}

/// GET /api/v1/search response.
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub total: usize,
}

/// One line in a context window.
#[derive(Debug, Serialize, Deserialize)]
pub struct ContextLine {
    pub line_number: usize,
    pub content: String,
}

/// GET /api/v1/context response.
#[derive(Debug, Serialize, Deserialize)]
pub struct ContextResponse {
    pub lines: Vec<ContextLine>,
    pub file_kind: String,
}

/// GET /api/v1/file response.
#[derive(Debug, Serialize, Deserialize)]
pub struct FileResponse {
    pub lines: Vec<ContextLine>,
    pub file_kind: String,
    pub total_lines: usize,
}

/// GET /api/v1/files response entry (for deletion detection).
#[derive(Debug, Serialize, Deserialize)]
pub struct FileRecord {
    pub path: String,
    pub mtime: i64,
}

/// One entry in a directory listing.
#[derive(Debug, Serialize, Deserialize)]
pub struct DirEntry {
    /// Last path component (file or directory name).
    pub name: String,
    /// Full relative path within the source.
    pub path: String,
    /// `"dir"` or `"file"`.
    pub entry_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtime: Option<i64>,
}

/// GET /api/v1/tree response.
#[derive(Debug, Serialize, Deserialize)]
pub struct TreeResponse {
    pub entries: Vec<DirEntry>,
}

/// One item in a POST /api/v1/context-batch request.
#[derive(Debug, Serialize, Deserialize)]
pub struct ContextBatchItem {
    pub source: String,
    pub path: String,
    #[serde(default)]
    pub archive_path: Option<String>,
    pub line: usize,
    #[serde(default = "default_context_window")]
    pub window: usize,
}

fn default_context_window() -> usize { 5 }

/// POST /api/v1/context-batch request body.
#[derive(Debug, Serialize, Deserialize)]
pub struct ContextBatchRequest {
    pub requests: Vec<ContextBatchItem>,
}

/// One result within a POST /api/v1/context-batch response.
#[derive(Debug, Serialize, Deserialize)]
pub struct ContextBatchResult {
    pub source: String,
    pub path: String,
    pub line: usize,
    pub lines: Vec<ContextLine>,
    pub file_kind: String,
}

/// POST /api/v1/context-batch response.
#[derive(Debug, Serialize, Deserialize)]
pub struct ContextBatchResponse {
    pub results: Vec<ContextBatchResult>,
}
