use std::path::Path;
use find_common::api::IndexLine;

/// Extract text content from PDF files.
///
/// Uses pdf-extract library. Handles malformed PDFs gracefully by catching panics.
///
/// # Arguments
/// * `path` - Path to the PDF file
/// * `max_size_kb` - Maximum file size in KB (currently unused)
///
/// # Returns
/// Vector of IndexLine objects, one per non-empty line
pub fn extract(path: &Path, max_size_kb: usize) -> anyhow::Result<Vec<IndexLine>> {
    let bytes = std::fs::read(path)?;
    extract_from_bytes(&bytes, &path.display().to_string(), max_size_kb)
}

/// Extract text content from PDF bytes.
///
/// Used by the archive extractor to process PDF members without writing to disk.
pub fn extract_from_bytes(bytes: &[u8], name: &str, _max_size_kb: usize) -> anyhow::Result<Vec<IndexLine>> {
    // pdf-extract can panic on malformed PDFs; catch_unwind turns that into
    // a recoverable error so the scan can continue with other files.
    //
    // Temporarily install a custom panic hook so the file path appears in
    // the panic output (the default hook prints no context about which file
    // triggered the panic).
    let name_for_hook = name.to_string();
    let _prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        eprintln!("pdf-extract panicked while processing: {name_for_hook}");
        eprintln!("{info}");
    }));
    let bytes_clone = bytes.to_vec();
    let result = std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem(&bytes_clone));
    // Restore default hook
    let _prev = std::panic::take_hook();

    let text = match result {
        Ok(Ok(t)) => t,
        Ok(Err(e)) => {
            eprintln!("PDF extraction error for {name}: {e}");
            return Ok(vec![]);
        }
        Err(_) => {
            eprintln!("PDF extraction panicked for {name} (see panic output above)");
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
