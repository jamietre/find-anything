use std::path::Path;
use crate::api::IndexLine;
use crate::extract::Extractor;

pub struct PdfExtractor;

impl Extractor for PdfExtractor {
    fn accepts(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("pdf"))
            .unwrap_or(false)
    }

    fn extract(&self, path: &Path) -> anyhow::Result<Vec<IndexLine>> {
        let bytes = std::fs::read(path)?;

        // pdf-extract can panic on malformed PDFs; catch_unwind turns that into
        // a recoverable error so the scan can continue with other files.
        let result = std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem(&bytes));

        let text = match result {
            Ok(Ok(t)) => t,
            Ok(Err(_)) | Err(_) => {
                // Extraction failed or panicked (encrypted, corrupted, unsupported font, etc.)
                return Ok(vec![]);
            }
        };

        let mut lines = Vec::new();
        for (idx, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                lines.push(IndexLine {
                    archive_path: None,
                    line_number: idx + 1,
                    content: trimmed.to_string(),
                });
            }
        }
        Ok(lines)
    }
}
