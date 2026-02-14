use std::path::Path;
use crate::api::IndexLine;
use crate::extract::Extractor;
use tracing::warn;

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
        let path_str = path.display().to_string();

        // pdf-extract can panic on malformed PDFs; catch_unwind turns that into
        // a recoverable error so the scan can continue with other files.
        //
        // Temporarily install a custom panic hook so the file path appears in
        // the panic output (the default hook prints no context about which file
        // triggered the panic).
        let prev_hook = std::panic::take_hook();
        let path_for_hook = path_str.clone();
        std::panic::set_hook(Box::new(move |info| {
            eprintln!("pdf-extract panicked while processing: {path_for_hook}");
            prev_hook(info);
        }));
        let result = std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem(&bytes));
        // Restore default hook (our custom hook, and thus prev_hook, is dropped here).
        drop(std::panic::take_hook());

        let text = match result {
            Ok(Ok(t)) => t,
            Ok(Err(e)) => {
                warn!("pdf extraction error for {path_str}: {e}");
                return Ok(vec![]);
            }
            Err(_) => {
                warn!("pdf extraction panicked for {path_str} (see panic output above)");
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
