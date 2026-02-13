use serde::{Deserialize, Serialize};

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

/// PUT /api/v1/files request body.
#[derive(Debug, Serialize, Deserialize)]
pub struct UpsertRequest {
    pub source: String,
    pub files: Vec<IndexFile>,
    #[serde(default)]
    pub base_url: Option<String>,
}

/// DELETE /api/v1/files request body.
#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteRequest {
    pub source: String,
    pub paths: Vec<String>,
}

/// POST /api/v1/scan-complete request body.
#[derive(Debug, Serialize, Deserialize)]
pub struct ScanCompleteRequest {
    pub source: String,
    pub timestamp: i64,
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
