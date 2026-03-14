/// Integration tests for find-extract-archive using real fixture archives.
///
/// Primary fixture: `fixtures.zip` — a ZIP containing inner archives in every
/// supported format (tar, tgz, tar.bz2, tar.xz, zip, 7z) plus the original
/// `fixtures.tgz` from the node `tar` package test suite.
///
/// Using ZIP as the outer archive lets us exercise both streaming content
/// extraction (which only works for ZIP) and all inner archive formats.
///
/// Each inner archive contains the same members:
///   hello.txt            — simple text
///   subdir/greet.txt     — file in a subdirectory
///   unicode/Ω.txt        — unicode filename
///   deep/a/b/c/d/e/f.txt — several levels of nesting
///   long_xxx...txt       — 200-char filename
///
/// `fixtures.tgz` (also a member of the ZIP) exercises PAX headers,
/// hardlinks, 98-level deep paths, and other TAR edge cases.
use std::io::Write as _;
use std::io::Cursor;
use std::path::PathBuf;

use find_extract_archive::{extract, extract_streaming, MemberBatch};
use find_extract_types::ExtractorConfig;

fn fixtures_zip() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fixtures.zip")
}

/// Still available for tests that target TAR-specific edge cases directly.
fn fixtures_tgz() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fixtures.tgz")
}

fn default_cfg() -> ExtractorConfig {
    ExtractorConfig {
        max_content_kb: 1024,
        max_depth: 10,
        max_line_length: 512,
        ..Default::default()
    }
}

/// Collect all archive_path values from extracted lines.
fn archive_paths(lines: &[find_extract_types::IndexLine]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|l| l.archive_path.clone())
        .collect()
}

fn has_path(lines: &[find_extract_types::IndexLine], needle: &str) -> bool {
    lines
        .iter()
        .any(|l| l.archive_path.as_deref() == Some(needle))
}

fn any_path_contains(lines: &[find_extract_types::IndexLine], sub: &str) -> bool {
    lines
        .iter()
        .any(|l| l.archive_path.as_deref().map(|p| p.contains(sub)).unwrap_or(false))
}

// ============================================================================
// Outer ZIP traversal
// ============================================================================

/// All inner archive names should appear as top-level members of fixtures.zip.
#[test]
fn outer_zip_lists_all_inner_archives() {
    let lines = extract(&fixtures_zip(), &default_cfg()).unwrap();
    for name in &["inner.tar", "inner.tgz", "inner.tar.bz2", "inner.tar.xz", "inner.zip", "inner.7z", "fixtures.tgz"] {
        assert!(
            has_path(&lines, name),
            "{name} not found as a top-level member"
        );
    }
}

// ============================================================================
// Per-format inner archive extraction
// ============================================================================

/// For each supported inner archive format, verify that members are recursively
/// extracted and appear as composite paths `<format>::<member>`.
#[test]
fn inner_tar_members_extracted() {
    let lines = extract(&fixtures_zip(), &default_cfg()).unwrap();
    assert!(has_path(&lines, "inner.tar::hello.txt"), "inner.tar::hello.txt not found");
    assert!(has_path(&lines, "inner.tar::subdir/greet.txt"));
    assert!(any_path_contains(&lines, "inner.tar::unicode/"));
}

#[test]
fn inner_tgz_members_extracted() {
    let lines = extract(&fixtures_zip(), &default_cfg()).unwrap();
    assert!(has_path(&lines, "inner.tgz::hello.txt"), "inner.tgz::hello.txt not found");
    assert!(has_path(&lines, "inner.tgz::deep/a/b/c/d/e/f.txt"));
}

#[test]
fn inner_tar_bz2_members_extracted() {
    let lines = extract(&fixtures_zip(), &default_cfg()).unwrap();
    assert!(has_path(&lines, "inner.tar.bz2::hello.txt"), "inner.tar.bz2::hello.txt not found");
}

#[test]
fn inner_tar_xz_members_extracted() {
    let lines = extract(&fixtures_zip(), &default_cfg()).unwrap();
    assert!(has_path(&lines, "inner.tar.xz::hello.txt"), "inner.tar.xz::hello.txt not found");
}

#[test]
fn inner_zip_members_extracted() {
    let lines = extract(&fixtures_zip(), &default_cfg()).unwrap();
    assert!(has_path(&lines, "inner.zip::hello.txt"), "inner.zip::hello.txt not found");
    assert!(has_path(&lines, "inner.zip::subdir/greet.txt"));
}

#[test]
fn inner_7z_members_extracted() {
    let lines = extract(&fixtures_zip(), &default_cfg()).unwrap();
    assert!(any_path_contains(&lines, "inner.7z::"), "no inner.7z members found");
    assert!(any_path_contains(&lines, "inner.7z::") && {
        lines.iter().any(|l| {
            l.archive_path.as_deref()
                .map(|p| p.starts_with("inner.7z::") && p.ends_with("hello.txt"))
                .unwrap_or(false)
        })
    }, "inner.7z::hello.txt not found");
}

// ============================================================================
// Unicode and long filenames (via inner archives)
// ============================================================================

#[test]
fn unicode_filename_in_inner_archives() {
    let lines = extract(&fixtures_zip(), &default_cfg()).unwrap();
    // Ω.txt appears in every inner archive
    assert!(
        any_path_contains(&lines, "Ω.txt"),
        "unicode filename Ω.txt not found in any inner archive"
    );
}

#[test]
fn long_filename_in_inner_archives() {
    let lines = extract(&fixtures_zip(), &default_cfg()).unwrap();
    let found = lines.iter().any(|l| {
        l.archive_path.as_deref()
            .map(|p| p.contains("long_") && p.len() > 100)
            .unwrap_or(false)
    });
    assert!(found, "200-char filename not found in any inner archive");
}

// ============================================================================
// Deeply nested path (via fixtures.tgz inside fixtures.zip)
// ============================================================================

/// The PAX-extended deeply-nested path from the node tar test suite must be
/// reachable as a doubly-composite path through the outer ZIP.
#[test]
fn deeply_nested_path_via_nested_tgz() {
    let lines = extract(&fixtures_zip(), &default_cfg()).unwrap();
    let deep_suffix = concat!(
        "fixtures/r/e/a/l/l/y/-/d/e/e/p/-/f/o/l/d/e/r/-/p/a/t/h/",
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
    );
    let found = lines.iter().any(|l| {
        l.archive_path.as_deref()
            .map(|p| p == deep_suffix || p.ends_with(&format!("::{deep_suffix}")))
            .unwrap_or(false)
    });
    assert!(found, "deeply nested path not found (expected inside fixtures.tgz::…)");
}

// ============================================================================
// Depth limiting
// ============================================================================

/// With max_depth = 0 nested archives should NOT be recursed into.
#[test]
fn nested_archives_skipped_at_depth_0() {
    let cfg = ExtractorConfig {
        max_depth: 0,
        ..default_cfg()
    };
    let lines = extract(&fixtures_zip(), &cfg).unwrap();
    let composite = lines
        .iter()
        .any(|l| l.archive_path.as_deref().map(|p| p.contains("::")).unwrap_or(false));
    assert!(!composite, "expected no composite paths at depth 0");
}

// ============================================================================
// Exclude-pattern filtering
// ============================================================================

/// `**/node_modules/**` in the exclude list suppresses matching ZIP members.
/// Uses a synthetic in-memory ZIP so the test is self-contained.
#[test]
fn exclude_patterns_filter_archive_members() {
    let mut buf = Vec::new();
    {
        let mut zip = zip::ZipWriter::new(Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("node_modules/pkg/index.js", opts).unwrap();
        zip.write_all(b"console.log('hi');\n").unwrap();
        zip.start_file("src/main.rs", opts).unwrap();
        zip.write_all(b"fn main() {}\n").unwrap();
        zip.finish().unwrap();
    }

    let mut tmp = tempfile::NamedTempFile::with_suffix(".zip").unwrap();
    tmp.write_all(&buf).unwrap();

    let cfg = ExtractorConfig {
        max_content_kb: 1024,
        max_depth: 2,
        max_line_length: 512,
        exclude_patterns: vec!["**/node_modules/**".to_string()],
        ..Default::default()
    };

    let mut all_paths: Vec<String> = Vec::new();
    extract_streaming(tmp.path(), &cfg, &mut |batch: MemberBatch| {
        for line in &batch.lines {
            if let Some(p) = &line.archive_path {
                all_paths.push(p.clone());
            }
        }
    })
    .unwrap();

    assert!(
        !all_paths.iter().any(|p| p.contains("node_modules")),
        "node_modules member was not excluded; paths = {all_paths:?}"
    );
    assert!(
        all_paths.iter().any(|p| p.contains("src/main.rs")),
        "src/main.rs was incorrectly excluded; paths = {all_paths:?}"
    );
}

/// Members NOT matching any exclude pattern must still appear.
#[test]
fn non_excluded_members_are_returned() {
    // Exclude inner.tgz members; everything else must still appear.
    let cfg = ExtractorConfig {
        exclude_patterns: vec!["inner.tgz/**".to_string(), "inner.tgz".to_string()],
        ..default_cfg()
    };
    let lines = extract(&fixtures_zip(), &cfg).unwrap();
    let paths = archive_paths(&lines);

    assert!(
        !paths.iter().any(|p| p.starts_with("inner.tgz")),
        "inner.tgz member was not excluded"
    );
    assert!(
        paths.iter().any(|p| p == "inner.tar" || p.starts_with("inner.tar::")),
        "inner.tar members were incorrectly excluded"
    );
}

// ============================================================================
// TAR-specific edge cases (direct .tgz fixture)
// ============================================================================

/// The original fixtures.tgz from the node tar test suite, used directly
/// (not via the outer ZIP) to test PAX headers, hardlinks, unicode etc.
#[test]
fn tgz_unicode_filename() {
    let lines = extract(&fixtures_tgz(), &default_cfg()).unwrap();
    assert!(
        any_path_contains(&lines, "Ω.txt"),
        "unicode filename Ω.txt not found in fixtures.tgz"
    );
}

#[test]
fn tgz_deeply_nested_path() {
    let lines = extract(&fixtures_tgz(), &default_cfg()).unwrap();
    let deep = concat!(
        "fixtures/r/e/a/l/l/y/-/d/e/e/p/-/f/o/l/d/e/r/-/p/a/t/h/",
        "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
    );
    assert!(has_path(&lines, deep), "deeply nested path not found");
}

#[test]
fn tgz_nested_tar_members_extracted() {
    let lines = extract(&fixtures_tgz(), &default_cfg()).unwrap();
    assert!(
        has_path(&lines, "fixtures/c.tar::c.txt"),
        "fixtures/c.tar::c.txt not found"
    );
}
