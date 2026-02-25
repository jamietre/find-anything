use std::path::Path;
use find_common::api::IndexLine;
use find_common::config::ExtractorConfig;
use anyhow::Result;

/// Dispatch to the appropriate extractor based on file type.
///
/// Archives are handled by `find-extract-archive` (streaming path in scan.rs).
/// All other files are routed through `find-extract-dispatch` which provides
/// unified bytes-based dispatch with MIME fallback.
pub fn extract(path: &Path, cfg: &ExtractorConfig) -> Result<Vec<IndexLine>> {
    // Archives first (before text, since ZIPs would otherwise be detected as binary)
    // Archives are exempt from the whole-file size limit — they can be arbitrarily
    // large containers, and the per-member size limit inside the extractor handles
    // skipping oversized individual members.
    if find_extract_archive::accepts(path) {
        return find_extract_archive::extract(path, cfg);
    }

    find_extract_dispatch::dispatch_from_path(path, cfg)
}

/// Detect the file kind string used in IndexFile.kind.
pub fn detect_kind(path: &Path) -> &'static str {
    if find_extract_archive::accepts(path) {
        return "archive";
    }
    if find_extract_pdf::accepts(path) {
        return "pdf";
    }
    if find_extract_pe::accepts(path) {
        return "executable";
    }
    if find_extract_media::accepts(path) {
        // Determine if it's image, audio, or video
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        if find_extract_media::is_image_ext(&ext) {
            return "image";
        }
        if find_extract_media::is_audio_ext(&ext) {
            return "audio";
        }
        if find_extract_media::is_video_ext(&ext) {
            return "video";
        }
    }
    "unknown"
}
