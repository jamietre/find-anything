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

/// GET /api/v1/files response entry (for deletion detection).
#[derive(Debug, Serialize, Deserialize)]
pub struct FileRecord {
    pub path: String,
    pub mtime: i64,
}
