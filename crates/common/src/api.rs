use serde::{Deserialize, Serialize};

/// Classify a file by its extension alone — no extractor lib deps.
/// Used by `find-watch` (subprocess mode) and `batch.rs` for archive member kinds.
pub fn detect_kind_from_ext(ext: &str) -> &'static str {
    match ext.to_lowercase().as_str() {
        "zip" | "tar" | "gz" | "bz2" | "xz" | "tgz" | "tbz2" | "txz" | "7z" => "archive",
        "pdf" => "pdf",
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "ico" | "webp" | "heic"
        | "tiff" | "tif" | "raw" | "cr2" | "nef" | "arw" => "image",
        "mp3" | "flac" | "ogg" | "m4a" | "aac" | "wav" | "wma" | "opus" => "audio",
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "webm" | "m4v" | "flv" => "video",
        "docx" | "xlsx" | "xls" | "xlsm" | "pptx" | "epub" => "document",
        _ => "text",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_kind_archives() {
        for ext in &["zip", "tar", "gz", "bz2", "xz", "tgz", "tbz2", "txz", "7z"] {
            assert_eq!(detect_kind_from_ext(ext), "archive", "ext={ext}");
        }
    }

    #[test]
    fn test_detect_kind_pdf() {
        assert_eq!(detect_kind_from_ext("pdf"), "pdf");
    }

    #[test]
    fn test_detect_kind_images() {
        for ext in &["jpg", "jpeg", "png", "gif", "bmp", "ico", "webp", "heic",
                     "tiff", "tif", "raw", "cr2", "nef", "arw"] {
            assert_eq!(detect_kind_from_ext(ext), "image", "ext={ext}");
        }
    }

    #[test]
    fn test_detect_kind_audio() {
        for ext in &["mp3", "flac", "ogg", "m4a", "aac", "wav", "wma", "opus"] {
            assert_eq!(detect_kind_from_ext(ext), "audio", "ext={ext}");
        }
    }

    #[test]
    fn test_detect_kind_video() {
        for ext in &["mp4", "mkv", "avi", "mov", "wmv", "webm", "m4v", "flv"] {
            assert_eq!(detect_kind_from_ext(ext), "video", "ext={ext}");
        }
    }

    #[test]
    fn test_detect_kind_text_fallback() {
        for ext in &["rs", "py", "toml", "md", "txt", "json", "", "unknown"] {
            assert_eq!(detect_kind_from_ext(ext), "text", "ext={ext}");
        }
    }

    #[test]
    fn test_detect_kind_documents() {
        for ext in &["docx", "xlsx", "xls", "xlsm", "pptx", "epub"] {
            assert_eq!(detect_kind_from_ext(ext), "document", "ext={ext}");
        }
    }

    #[test]
    fn test_detect_kind_case_insensitive() {
        assert_eq!(detect_kind_from_ext("PDF"), "pdf");
        assert_eq!(detect_kind_from_ext("ZIP"), "archive");
        assert_eq!(detect_kind_from_ext("JPG"), "image");
        assert_eq!(detect_kind_from_ext("MP3"), "audio");
        assert_eq!(detect_kind_from_ext("MP4"), "video");
        assert_eq!(detect_kind_from_ext("DOCX"), "document");
    }
}

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
    /// For inner archive members this is a composite path: "archive.zip::member.txt".
    /// Nesting is supported: "outer.zip::inner.tar.gz::file.txt".
    pub path: String,
    pub mtime: i64,
    pub size: i64,
    /// "text" | "pdf" | "archive" | "image" | "audio"
    pub kind: String,
    pub lines: Vec<IndexLine>,
    /// Milliseconds taken to extract content for this file, measured by the client.
    /// Set on the outer file; None for inner archive members.
    #[serde(default)]
    pub extract_ms: Option<u64>,
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
    /// Line number of the first element in `lines`. Client computes each
    /// line's number as `start + index` (approximate — gaps exist in sparse
    /// files like PDFs where empty lines are not stored).
    pub start: usize,
    /// Index within `lines` of the matched line, or null if the center line
    /// was not found in the returned window (e.g. it fell in a gap).
    pub match_index: Option<usize>,
    pub lines: Vec<String>,
    pub kind: String,
}

/// GET /api/v1/file response.
#[derive(Debug, Serialize, Deserialize)]
pub struct FileResponse {
    pub lines: Vec<ContextLine>,
    pub file_kind: String,
    pub total_lines: usize,
    pub mtime: Option<i64>,
    pub size: Option<i64>,
}

/// GET /api/v1/files response entry (for deletion detection / Ctrl+P).
#[derive(Debug, Serialize, Deserialize)]
pub struct FileRecord {
    pub path: String,
    pub mtime: i64,
    pub kind: String,
}

/// One entry in a directory listing.
#[derive(Debug, Serialize, Deserialize)]
pub struct DirEntry {
    /// Last path component (file or directory name).
    pub name: String,
    /// Full relative path within the source, including `::` for archive members.
    pub path: String,
    /// `"dir"` or `"file"`. Archive files have `kind = "archive"` and can be expanded.
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
    pub start: usize,
    pub match_index: Option<usize>,
    pub lines: Vec<String>,
    pub kind: String,
}

/// POST /api/v1/context-batch response.
#[derive(Debug, Serialize, Deserialize)]
pub struct ContextBatchResponse {
    pub results: Vec<ContextBatchResult>,
}

/// GET /api/v1/settings response — display configuration for the web UI.
#[derive(Debug, Serialize, Deserialize)]
pub struct AppSettingsResponse {
    /// Lines shown before and after each match in search result cards.
    /// Total lines = 2 × context_window + 1.
    pub context_window: usize,
    /// Server version string (from Cargo.toml).
    pub version: String,
}

// ── Stats types ───────────────────────────────────────────────────────────────

/// Per-kind breakdown entry in `SourceStats`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KindStats {
    pub count: usize,
    pub size: i64,
    pub avg_extract_ms: Option<f64>,
}

/// One point in the scan history time series.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanHistoryPoint {
    pub scanned_at: i64,
    pub total_files: usize,
    pub total_size: i64,
}

/// Stats for one source, returned by `GET /api/v1/stats`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceStats {
    pub name: String,
    pub last_scan: Option<i64>,
    pub total_files: usize,
    pub total_size: i64,
    pub by_kind: std::collections::HashMap<String, KindStats>,
    pub history: Vec<ScanHistoryPoint>,
}

/// `GET /api/v1/stats` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsResponse {
    pub sources: Vec<SourceStats>,
    pub inbox_pending: usize,
    pub failed_requests: usize,
    pub total_archives: usize,
    /// Total on-disk size of all SQLite source databases (bytes).
    pub db_size_bytes: u64,
    /// Total on-disk size of all ZIP content archives (bytes).
    pub archive_size_bytes: u64,
}
