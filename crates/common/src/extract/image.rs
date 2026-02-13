use std::fs::File;
use std::io::BufReader;
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

    fn extract(&self, path: &Path) -> anyhow::Result<Vec<IndexLine>> {
        let file = File::open(path)?;
        let mut bufreader = BufReader::new(file);

        match exif::Reader::new().read_from_container(&mut bufreader) {
            Ok(exif) => {
                let mut lines = Vec::new();

                // Extract all EXIF fields
                for field in exif.fields() {
                    let tag = field.tag.to_string();
                    let value = field.display_value().to_string();

                    // Skip empty or binary values
                    if !value.is_empty() && !value.starts_with("[") {
                        lines.push(IndexLine {
                            archive_path: None,
                            line_number: 0,  // Metadata has no line concept
                            content: format!("[EXIF:{}] {}", tag, value),
                        });
                    }
                }

                Ok(lines)
            }
            Err(_) => {
                // Many images don't have EXIF data, or we can't read it
                // This is normal, just return empty results
                Ok(vec![])
            }
        }
    }
}

pub fn is_image_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "jpg" | "jpeg" | "tiff" | "tif" | "heic" | "heif" | "webp"
        | "png" | "cr2" | "cr3" | "nef" | "arw" | "orf" | "rw2"
    )
}
