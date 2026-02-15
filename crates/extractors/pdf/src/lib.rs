use std::path::Path;
use find_common::api::IndexLine;

/// Extract text content from PDF files.
///
/// Uses pdf-extract library. Handles malformed PDFs gracefully by catching panics.
///
/// # Arguments
/// * `path` - Path to the PDF file
/// * `_max_size_kb` - Maximum file size in KB (currently unused)
///
/// # Returns
/// Vector of IndexLine objects, one per non-empty line
pub fn extract(path: &Path, _max_size_kb: usize) -> anyhow::Result<Vec<IndexLine>> {
    let bytes = std::fs::read(path)?;
    let path_str = path.display().to_string();

    // pdf-extract can panic on malformed PDFs; catch_unwind turns that into
    // a recoverable error so the scan can continue with other files.
    //
    // Temporarily install a custom panic hook so the file path appears in
    // the panic output (the default hook prints no context about which file
    // triggered the panic).
    let _prev_hook = std::panic::take_hook();
    let path_for_hook = path_str.clone();
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("pdf-extract panicked while processing: {path_for_hook}");
        eprintln!("{info}");
    }));
    let result = std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem(&bytes));
    // Restore default hook
    let _prev = std::panic::take_hook();

    let text = match result {
        Ok(Ok(t)) => t,
        Ok(Err(e)) => {
            eprintln!("PDF extraction error for {path_str}: {e}");
            return Ok(vec![]);
        }
        Err(_) => {
            eprintln!("PDF extraction panicked for {path_str} (see panic output above)");
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

/// Check if a file is a PDF based on extension.
pub fn accepts(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false)
}
