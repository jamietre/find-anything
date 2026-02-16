use std::path::Path;
use find_common::api::IndexLine;
use anyhow::Result;

/// Dispatch to the appropriate extractor based on file type.
///
/// This replaces the old find_common::extract module with the new
/// standalone extractor crates.
///
/// # Arguments
/// * `path` - Path to the file
/// * `max_size_kb` - Maximum file size in KB
/// * `_max_archive_depth` - Maximum archive nesting depth (unused in current implementation)
pub fn extract(path: &Path, max_size_kb: u64, max_archive_depth: usize) -> Result<Vec<IndexLine>> {
    // Skip files that exceed the size limit
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() > max_size_kb * 1024 {
            return Ok(vec![]);
        }
    }

    let max_size_kb_usize = max_size_kb as usize;

    // Dispatch to extractors in priority order
    // Archives first (before text, since ZIPs would otherwise be detected as binary)
    if find_extract_archive::accepts(path) {
        return find_extract_archive::extract(path, max_size_kb_usize, max_archive_depth);
    }

    if find_extract_pdf::accepts(path) {
        return find_extract_pdf::extract(path, max_size_kb_usize);
    }

    if find_extract_media::accepts(path) {
        return find_extract_media::extract(path, max_size_kb_usize);
    }

    // HTML before text (text's accepts() matches .html via extension list)
    if find_extract_html::accepts(path) {
        return find_extract_html::extract(path, max_size_kb_usize);
    }

    if find_extract_office::accepts(path) {
        return find_extract_office::extract(path, max_size_kb_usize);
    }

    if find_extract_epub::accepts(path) {
        return find_extract_epub::extract(path, max_size_kb_usize);
    }

    // Text extractor is last (most permissive, will accept many files)
    if find_extract_text::accepts(path) {
        return find_extract_text::extract(path, max_size_kb_usize);
    }

    // No extractor matched
    Ok(vec![])
}

/// Detect the file kind string used in IndexFile.kind.
pub fn detect_kind(path: &Path) -> &'static str {
    if find_extract_archive::accepts(path) {
        return "archive";
    }
    if find_extract_pdf::accepts(path) {
        return "pdf";
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
    "text"
}
