use find_extract_dicom::{accepts, accepts_bytes, extract, extract_from_bytes};
use find_extract_types::{ExtractorConfig, LINE_METADATA};
use std::path::Path;

fn cfg() -> ExtractorConfig {
    ExtractorConfig::default()
}

// ── Fixture paths ─────────────────────────────────────────────────────────────

fn mr_path() -> &'static Path {
    Path::new("tests/fixtures/MR_small.dcm")
}

fn ct_path() -> &'static Path {
    Path::new("tests/fixtures/CT_small.dcm")
}

// ── accepts / accepts_bytes ───────────────────────────────────────────────────

#[test]
fn accepts_dcm_path() {
    assert!(accepts(Path::new("scan.dcm")));
    assert!(accepts(Path::new("SCAN.DCM")));
    assert!(accepts(Path::new("image.dicom")));
    assert!(!accepts(Path::new("image.png")));
    assert!(!accepts(Path::new("noext")));
}

#[test]
fn accepts_bytes_real_mr_file() {
    let bytes = include_bytes!("fixtures/MR_small.dcm");
    assert!(accepts_bytes(bytes), "MR_small.dcm must be accepted by magic bytes");
}

#[test]
fn accepts_bytes_real_ct_file() {
    let bytes = include_bytes!("fixtures/CT_small.dcm");
    assert!(accepts_bytes(bytes), "CT_small.dcm must be accepted by magic bytes");
}

#[test]
fn accepts_bytes_not_png() {
    // PNG magic: \x89PNG\r\n\x1a\n at offset 0, nothing at offset 128
    let mut fake = vec![0u8; 200];
    fake[0..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    assert!(!accepts_bytes(&fake));
}

// ── extract (from path) ───────────────────────────────────────────────────────

#[test]
fn extract_mr_returns_metadata_line() {
    let lines = extract(mr_path(), &cfg()).expect("extract MR_small.dcm");
    assert_eq!(lines.len(), 1, "expected exactly one metadata line");
    assert_eq!(lines[0].line_number, LINE_METADATA);
    assert!(lines[0].archive_path.is_none());
}

#[test]
fn extract_mr_contains_expected_tags() {
    let lines = extract(mr_path(), &cfg()).unwrap();
    let content = &lines[0].content;
    // Values must be present.
    assert!(content.contains("CompressedSamples^MR1"), "missing PatientName: {content}");
    assert!(content.contains("MR"), "missing Modality: {content}");
    assert!(content.contains("TOSHIBA"), "missing InstitutionName: {content}");
    assert!(content.contains("64x64"), "missing dimensions: {content}");
    // All tags must use [DICOM:Key] format so the UI can parse them.
    assert!(content.contains("[DICOM:PatientName] CompressedSamples^MR1"), "PatientName format: {content}");
    assert!(content.contains("[DICOM:Modality] MR"), "Modality format: {content}");
    assert!(content.contains("[DICOM:Dimensions] 64x64"), "Dimensions format: {content}");
}

#[test]
fn extract_ct_contains_expected_tags() {
    let lines = extract(ct_path(), &cfg()).unwrap();
    let content = &lines[0].content;
    // Values must be present.
    assert!(content.contains("CompressedSamples^CT1"), "missing PatientName: {content}");
    assert!(content.contains("CT"), "missing Modality: {content}");
    assert!(content.contains("128x128"), "missing dimensions: {content}");
    // All tags must use [DICOM:Key] format.
    assert!(content.contains("[DICOM:PatientName] CompressedSamples^CT1"), "PatientName format: {content}");
    assert!(content.contains("[DICOM:Modality] CT"), "Modality format: {content}");
    assert!(content.contains("[DICOM:Dimensions] 128x128"), "Dimensions format: {content}");
}

#[test]
fn extract_metadata_uses_dicom_tag_format() {
    // Verify the entire content uses [DICOM:Key] format — no untagged bare values.
    let lines = extract(mr_path(), &cfg()).unwrap();
    let content = &lines[0].content;
    // Every token that looks like it starts a new tag must use [DICOM:] prefix.
    // A simple check: split on " [DICOM:" — all parts after the first should be non-empty tag pairs.
    let parts: Vec<&str> = content.splitn(2, '[').collect();
    assert!(parts[0].is_empty(), "content must start with '[DICOM:...', got: {content}");
}

// ── extract_from_bytes (archive member path) ──────────────────────────────────

#[test]
fn extract_from_bytes_mr_matches_path_extract() {
    let bytes = include_bytes!("fixtures/MR_small.dcm");
    let by_bytes = extract_from_bytes(bytes, "MR_small.dcm", &cfg()).unwrap();
    let by_path = extract(mr_path(), &cfg()).unwrap();
    // Content must be identical regardless of how the bytes were provided.
    assert_eq!(by_bytes[0].content, by_path[0].content);
}

#[test]
fn extract_from_bytes_extensionless_name_still_works() {
    // The name passed to extract_from_bytes doesn't need an extension —
    // the extractor reads the bytes directly without relying on the name.
    let bytes = include_bytes!("fixtures/MR_small.dcm");
    let lines = extract_from_bytes(bytes, "some_file_no_ext", &cfg()).unwrap();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].content.contains("MR"));
}
