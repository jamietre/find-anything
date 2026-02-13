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
        // Extract text from PDF using pdf-extract
        let bytes = std::fs::read(path)?;

        match pdf_extract::extract_text_from_mem(&bytes) {
            Ok(text) => {
                // Split into lines and index each one
                // For now, treat the entire PDF as one continuous text
                // Future: could extract page-by-page and use archive_path for page numbers
                let mut lines = Vec::new();
                for (idx, line) in text.lines().enumerate() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        lines.push(IndexLine {
                            archive_path: None,  // Could be "page:N" if we extract per-page
                            line_number: idx + 1,
                            content: trimmed.to_string(),
                        });
                    }
                }
                Ok(lines)
            }
            Err(_) => {
                // PDF extraction can fail for various reasons (encrypted, corrupted, scanned PDFs)
                // Just return empty results - this is normal for many PDFs
                Ok(vec![])
            }
        }
    }
}
