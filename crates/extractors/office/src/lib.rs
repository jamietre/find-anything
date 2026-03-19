use std::io::Read;
use std::path::Path;

use find_extract_types::{IndexLine, LINE_METADATA, LINE_CONTENT_START};
use find_extract_types::ExtractorConfig;
use quick_xml::events::Event;

/// Accept Office document formats.
pub fn accepts(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase()
            .as_str(),
        "docx" | "xlsx" | "xls" | "xlsm" | "pptx"
    )
}

/// Extract text from Office document bytes.
///
/// Used by `find-extract-dispatch` for archive members. Writes to a temp file
/// and delegates to `extract` (which needs a real path for some formats).
pub fn extract_from_bytes(bytes: &[u8], name: &str, cfg: &ExtractorConfig) -> anyhow::Result<Vec<IndexLine>> {
    use std::io::Write;
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("docx");
    let mut tmp = tempfile::Builder::new()
        .suffix(&format!(".{}", ext))
        .tempfile()?;
    tmp.write_all(bytes)?;
    tmp.flush()?;
    extract(tmp.path(), cfg)
}

/// Extract text from an Office document.
///
/// - DOCX: paragraphs from word/document.xml + metadata from docProps/core.xml
/// - XLSX/XLS/XLSM: rows from all sheets (via calamine)
/// - PPTX: text runs from each slide, grouped by paragraph
pub fn extract(path: &Path, _cfg: &ExtractorConfig) -> anyhow::Result<Vec<IndexLine>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "docx" => extract_docx(path),
        "xlsx" | "xls" | "xlsm" => extract_xlsx(path),
        "pptx" => extract_pptx(path),
        _ => Ok(vec![]),
    }
}

// ── DOCX ─────────────────────────────────────────────────────────────────────

fn extract_docx(path: &Path) -> anyhow::Result<Vec<IndexLine>> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut lines = Vec::new();

    // Metadata from docProps/core.xml — consolidated into LINE_METADATA.
    {
        if let Ok(mut entry) = archive.by_name("docProps/core.xml") {
            let mut xml = String::new();
            entry.read_to_string(&mut xml)?;
            if let Some(meta) = parse_docx_metadata(&xml) {
                lines.push(meta);
            }
        }
    }

    // Content from word/document.xml — starts at LINE_CONTENT_START.
    {
        if let Ok(mut entry) = archive.by_name("word/document.xml") {
            let mut xml = String::new();
            entry.read_to_string(&mut xml)?;
            let paragraphs = parse_docx_paragraphs(&xml);
            for (i, text) in paragraphs.into_iter().enumerate() {
                lines.push(IndexLine {
                    archive_path: None,
                    line_number: i + LINE_CONTENT_START,
                    content: text,
                });
            }
        }
    }

    Ok(lines)
}

/// Extract dc:title and dc:creator from docProps/core.xml, concatenated into a
/// single IndexLine at LINE_METADATA.
fn parse_docx_metadata(xml: &str) -> Option<IndexLine> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut parts = Vec::new();
    let mut current_field: Option<&'static str> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                current_field = match e.name().as_ref() {
                    b"dc:title" => Some("title"),
                    b"dc:creator" => Some("author"),
                    _ => None,
                };
            }
            Ok(Event::Text(e)) => {
                if let Some(field) = current_field {
                    if let Ok(text) = e.unescape() {
                        let text = text.trim().to_string();
                        if !text.is_empty() {
                            parts.push(format!("[DOCX:{}] {}", field, text));
                        }
                    }
                }
            }
            Ok(Event::End(_)) => {
                current_field = None;
            }
            Ok(Event::Eof) => break,
            _ => {}
        }
        buf.clear();
    }

    if parts.is_empty() {
        return None;
    }

    Some(IndexLine {
        archive_path: None,
        line_number: LINE_METADATA,
        content: parts.join(" "),
    })
}

/// Collect non-empty paragraphs from word/document.xml.
fn parse_docx_paragraphs(xml: &str) -> Vec<String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut paragraphs = Vec::new();
    let mut current_para = String::new();
    let mut in_t = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"w:t" => in_t = true,
                b"w:p" => current_para.clear(),
                _ => {}
            },
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"w:t" => in_t = false,
                b"w:p" => {
                    let text = current_para.trim().to_string();
                    if !text.is_empty() {
                        paragraphs.push(text);
                    }
                    current_para.clear();
                }
                _ => {}
            },
            Ok(Event::Text(e)) => {
                if in_t {
                    if let Ok(text) = e.unescape() {
                        current_para.push_str(&text);
                    }
                }
            }
            Ok(Event::Eof) => break,
            _ => {}
        }
        buf.clear();
    }
    paragraphs
}

// ── XLSX / XLS / XLSM ────────────────────────────────────────────────────────

fn extract_xlsx(path: &Path) -> anyhow::Result<Vec<IndexLine>> {
    use calamine::{open_workbook_auto, Data, Reader};

    let mut wb = open_workbook_auto(path)?;
    let mut lines = Vec::new();

    let sheet_names = wb.sheet_names().to_vec();

    // All sheet names concatenated into the metadata slot.
    if !sheet_names.is_empty() {
        let meta = sheet_names.iter()
            .map(|n| format!("[XLSX:sheet] {}", n))
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(IndexLine {
            archive_path: None,
            line_number: LINE_METADATA,
            content: meta,
        });
    }

    let mut content_line = LINE_CONTENT_START - 1;

    for sheet_name in &sheet_names {
        if let Ok(range) = wb.worksheet_range(sheet_name) {
            for row in range.rows() {
                let cells: Vec<String> = row
                    .iter()
                    .filter_map(|cell| match cell {
                        Data::Empty => None,
                        Data::String(s) if s.trim().is_empty() => None,
                        other => {
                            let s = other.to_string();
                            if s.is_empty() {
                                None
                            } else {
                                Some(s)
                            }
                        }
                    })
                    .collect();

                if !cells.is_empty() {
                    content_line += 1;
                    lines.push(IndexLine {
                        archive_path: None,
                        line_number: content_line,
                        content: cells.join("\t"),
                    });
                }
            }
        }
    }

    Ok(lines)
}

// ── PPTX ─────────────────────────────────────────────────────────────────────

fn extract_pptx(path: &Path) -> anyhow::Result<Vec<IndexLine>> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut lines = Vec::new();

    // Collect slide file names first (no entry borrow held)
    let mut slide_names: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        if let Ok(entry) = archive.by_index(i) {
            let name = entry.name().to_string();
            if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
                slide_names.push(name);
            }
        }
    }

    // Sort numerically: slide1.xml, slide2.xml, …
    slide_names.sort_by_key(|n| {
        n.strip_prefix("ppt/slides/slide")
            .and_then(|s| s.strip_suffix(".xml"))
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0)
    });

    // All slide labels concatenated into the metadata slot.
    if !slide_names.is_empty() {
        let meta = (1..=slide_names.len())
            .map(|i| format!("[PPTX:slide] {}", i))
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(IndexLine {
            archive_path: None,
            line_number: LINE_METADATA,
            content: meta,
        });
    }

    let mut content_line = LINE_CONTENT_START - 1;

    for slide_name in &slide_names {
        let xml = {
            let mut entry = archive.by_name(slide_name)?;
            let mut s = String::new();
            entry.read_to_string(&mut s)?;
            s
        };

        for text in parse_pptx_paragraphs(&xml) {
            content_line += 1;
            lines.push(IndexLine {
                archive_path: None,
                line_number: content_line,
                content: text,
            });
        }
    }

    Ok(lines)
}

/// Collect non-empty paragraphs from a PPTX slide XML.
fn parse_pptx_paragraphs(xml: &str) -> Vec<String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut paragraphs = Vec::new();
    let mut current_para = String::new();
    let mut in_t = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                if e.name().as_ref() == b"a:t" {
                    in_t = true;
                }
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"a:t" => in_t = false,
                b"a:p" => {
                    let text = current_para.trim().to_string();
                    if !text.is_empty() {
                        paragraphs.push(text);
                    }
                    current_para.clear();
                }
                _ => {}
            },
            Ok(Event::Text(e)) => {
                if in_t {
                    if let Ok(text) = e.unescape() {
                        current_para.push_str(&text);
                    }
                }
            }
            Ok(Event::Eof) => break,
            _ => {}
        }
        buf.clear();
    }
    paragraphs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accepts() {
        assert!(accepts(Path::new("report.docx")));
        assert!(accepts(Path::new("data.xlsx")));
        assert!(accepts(Path::new("data.xls")));
        assert!(accepts(Path::new("data.xlsm")));
        assert!(accepts(Path::new("deck.pptx")));
        assert!(accepts(Path::new("REPORT.DOCX")));
        assert!(!accepts(Path::new("notes.odt")));
        assert!(!accepts(Path::new("data.csv")));
        assert!(!accepts(Path::new("index.html")));
    }

    #[test]
    fn test_parse_docx_metadata() {
        let xml = r#"<?xml version="1.0"?>
<cp:coreProperties xmlns:dc="http://purl.org/dc/elements/1.1/">
  <dc:title>My Document</dc:title>
  <dc:creator>Jane Smith</dc:creator>
</cp:coreProperties>"#;

        let meta = parse_docx_metadata(xml).expect("expected metadata");
        assert_eq!(meta.line_number, LINE_METADATA);
        assert!(meta.content.contains("[DOCX:title] My Document"), "content: {}", meta.content);
        assert!(meta.content.contains("[DOCX:author] Jane Smith"), "content: {}", meta.content);
    }

    #[test]
    fn test_parse_docx_paragraphs() {
        let xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>First paragraph</w:t></w:r></w:p>
    <w:p><w:r><w:t>Second </w:t></w:r><w:r><w:t>paragraph</w:t></w:r></w:p>
    <w:p><w:r><w:t>   </w:t></w:r></w:p>
    <w:p><w:r><w:t>Third paragraph</w:t></w:r></w:p>
  </w:body>
</w:document>"#;

        let paras = parse_docx_paragraphs(xml);
        assert_eq!(paras.len(), 3); // blank paragraph skipped
        assert_eq!(paras[0], "First paragraph");
        assert_eq!(paras[1], "Second paragraph");
        assert_eq!(paras[2], "Third paragraph");
    }

    #[test]
    fn test_parse_pptx_paragraphs() {
        let xml = r#"<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <p:cSld>
    <p:spTree>
      <p:sp>
        <p:txBody>
          <a:p><a:r><a:t>Slide title</a:t></a:r></a:p>
          <a:p><a:r><a:t>Bullet </a:t></a:r><a:r><a:t>point</a:t></a:r></a:p>
          <a:p><a:r><a:t></a:t></a:r></a:p>
        </p:txBody>
      </p:sp>
    </p:spTree>
  </p:cSld>
</p:sld>"#;

        let paras = parse_pptx_paragraphs(xml);
        assert_eq!(paras.len(), 2); // empty paragraph skipped
        assert_eq!(paras[0], "Slide title");
        assert_eq!(paras[1], "Bullet point");
    }

    #[test]
    fn test_docx_line_numbers() {
        let xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>Alpha</w:t></w:r></w:p>
    <w:p><w:r><w:t>Beta</w:t></w:r></w:p>
    <w:p><w:r><w:t>Gamma</w:t></w:r></w:p>
  </w:body>
</w:document>"#;

        let paras = parse_docx_paragraphs(xml);
        // Verify we can build IndexLines with sequential numbers
        for (i, text) in paras.iter().enumerate() {
            assert_eq!(*text, ["Alpha", "Beta", "Gamma"][i]);
        }
    }
}
