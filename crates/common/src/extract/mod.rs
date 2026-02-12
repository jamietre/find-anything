pub mod archive;
pub mod audio;
pub mod image;
pub mod pdf;
pub mod text;

use std::path::Path;

use crate::api::IndexLine;

/// Trait implemented by each content extractor.
pub trait Extractor: Send + Sync {
    fn accepts(&self, path: &Path) -> bool;
    fn extract(&self, path: &Path) -> anyhow::Result<Vec<IndexLine>>;
}

/// Dispatch to the first matching extractor.
/// Order matters: archives before text (zip files would otherwise be skipped as binary).
pub fn extract(path: &Path, max_bytes: u64) -> anyhow::Result<Vec<IndexLine>> {
    // Skip files that exceed the size limit
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() > max_bytes * 1024 {
            return Ok(vec![]);
        }
    }

    let extractors: Vec<Box<dyn Extractor>> = vec![
        Box::new(archive::ArchiveExtractor),
        Box::new(pdf::PdfExtractor),
        Box::new(image::ImageExtractor),
        Box::new(audio::AudioExtractor),
        Box::new(text::TextExtractor),
    ];

    for extractor in &extractors {
        if extractor.accepts(path) {
            return extractor.extract(path);
        }
    }

    Ok(vec![])
}

/// Detect the file kind string used in IndexFile.kind.
pub fn detect_kind(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if archive::is_archive_ext(&ext) {
        return "archive";
    }
    if ext == "pdf" {
        return "pdf";
    }
    if image::is_image_ext(&ext) {
        return "image";
    }
    if audio::is_audio_ext(&ext) {
        return "audio";
    }
    "text"
}
