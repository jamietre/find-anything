use std::io::Cursor;
use std::path::Path;

use find_extract_types::{ExtractorConfig, IndexLine, LINE_METADATA};
use tracing::warn;

const DICOM_EXTENSIONS: &[&str] = &["dcm", "dicom"];

/// True if `path` has a `.dcm` or `.dicom` extension (case-insensitive).
pub fn accepts(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| DICOM_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// True if `bytes` contain the DICOM preamble magic at offset 128.
///
/// Modern DICOM files (post-1993) have a 128-byte preamble followed by the
/// four ASCII bytes `DICM`. Legacy DICOM files have no magic marker and cannot
/// be detected reliably without an extension.
pub fn accepts_bytes(bytes: &[u8]) -> bool {
    bytes.len() >= 132 && &bytes[128..132] == b"DICM"
}

/// Extract metadata from a DICOM file at `path`.
pub fn extract(path: &Path, _cfg: &ExtractorConfig) -> anyhow::Result<Vec<IndexLine>> {
    let obj = dicom_object::open_file(path)?;
    Ok(build_lines(&obj))
}

/// Extract metadata from DICOM bytes (used for archive members).
pub fn extract_from_bytes(bytes: &[u8], name: &str, _cfg: &ExtractorConfig) -> anyhow::Result<Vec<IndexLine>> {
    let cursor = Cursor::new(bytes);
    let obj = dicom_object::from_reader(cursor)
        .map_err(|e| anyhow::anyhow!("DICOM parse error for '{}': {}", name, e))?;
    Ok(build_lines(&obj))
}

// ── Tag helpers ───────────────────────────────────────────────────────────────

use dicom_dictionary_std::tags;
use dicom_object::Tag;

fn tag_str(obj: &dicom_object::DefaultDicomObject, tag: Tag) -> Option<String> {
    obj.element(tag)
        .ok()
        .and_then(|e| e.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn tag_u32(obj: &dicom_object::DefaultDicomObject, tag: Tag) -> Option<u32> {
    obj.element(tag)
        .ok()
        .and_then(|e| e.to_int::<u32>().ok())
}

// ── Metadata assembly ─────────────────────────────────────────────────────────

fn build_lines(obj: &dicom_object::DefaultDicomObject) -> Vec<IndexLine> {
    let mut parts: Vec<String> = Vec::new();

    macro_rules! push_tag {
        ($tag:expr, $label:literal) => {
            if let Some(v) = tag_str(obj, $tag) {
                parts.push(format!("[DICOM:{}] {}", $label, v));
            }
        };
    }

    // Patient
    push_tag!(tags::PATIENT_NAME, "PatientName");
    push_tag!(tags::PATIENT_ID, "PatientID");

    // Study / series
    push_tag!(tags::STUDY_DATE, "StudyDate");
    push_tag!(tags::SERIES_DATE, "SeriesDate");
    push_tag!(tags::STUDY_DESCRIPTION, "StudyDescription");
    push_tag!(tags::SERIES_DESCRIPTION, "SeriesDescription");

    // Acquisition
    push_tag!(tags::MODALITY, "Modality");
    push_tag!(tags::BODY_PART_EXAMINED, "BodyPart");
    push_tag!(tags::INSTITUTION_NAME, "Institution");
    push_tag!(tags::MANUFACTURER, "Manufacturer");

    // Image geometry
    if let (Some(rows), Some(cols)) = (tag_u32(obj, tags::ROWS), tag_u32(obj, tags::COLUMNS)) {
        parts.push(format!("[DICOM:Dimensions] {}x{}", cols, rows));
    }
    if let Some(frames) = tag_u32(obj, tags::NUMBER_OF_FRAMES) {
        if frames > 1 {
            parts.push(format!("[DICOM:Frames] {}", frames));
        }
    }

    // Transfer syntax (useful to know for preview capability)
    if let Some(ts) = tag_str(obj, tags::TRANSFER_SYNTAX_UID) {
        parts.push(format!("[DICOM:TransferSyntax] {}", ts));
    }

    if parts.is_empty() {
        warn!("DICOM file yielded no metadata");
    }

    vec![IndexLine {
        archive_path: None,
        line_number: LINE_METADATA,
        content: parts.join(" "),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_dcm_extension() {
        assert!(accepts(Path::new("scan.dcm")));
        assert!(accepts(Path::new("SCAN.DCM")));
        assert!(accepts(Path::new("file.dicom")));
        assert!(!accepts(Path::new("file.png")));
        assert!(!accepts(Path::new("file")));
    }

    #[test]
    fn accepts_bytes_checks_magic_at_128() {
        let mut buf = vec![0u8; 132];
        // Not yet DICOM
        assert!(!accepts_bytes(&buf));
        // Set magic
        buf[128..132].copy_from_slice(b"DICM");
        assert!(accepts_bytes(&buf));
        // Too short
        assert!(!accepts_bytes(&buf[..131]));
    }
}
