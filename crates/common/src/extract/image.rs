// Post-MVP: EXIF metadata extraction via kamadak-exif crate.
// Stub returns no lines until implemented.

use std::path::Path;
use crate::api::IndexLine;
use crate::extract::Extractor;

pub struct ImageExtractor;

impl Extractor for ImageExtractor {
    fn accepts(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| is_image_ext(e))
            .unwrap_or(false)
    }

    fn extract(&self, _path: &Path) -> anyhow::Result<Vec<IndexLine>> {
        Ok(vec![])
    }
}

pub fn is_image_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "jpg" | "jpeg" | "tiff" | "tif" | "heic" | "heif" | "webp"
        | "png" | "cr2" | "cr3" | "nef" | "arw" | "orf" | "rw2"
    )
}
