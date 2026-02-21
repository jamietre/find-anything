use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;

use anyhow::{Context, Result};
use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use tracing::warn;
use xz2::read::XzDecoder;

use find_common::api::IndexLine;
use find_common::config::ExtractorConfig;

/// Extract content from archive files (ZIP, TAR, TGZ, TBZ2, TXZ, GZ, BZ2, XZ, 7Z).
///
/// For each archive member:
/// - Always indexes the filename (line_number=0, archive_path=member_path)
/// - Text files: extracts line content via text extractor
/// - PDF files: extracts text via PDF extractor (in-memory, no temp file)
/// - Media files: extracts metadata via media extractor (writes to temp file)
/// - Nested archives: recursively extracts up to cfg.max_depth
///
/// Returns IndexLine objects with archive_path set to the member path within the archive.
/// For nested archives, archive_path uses `::` as a separator (e.g. "inner.zip::file.txt").
pub fn extract(path: &Path, cfg: &ExtractorConfig) -> Result<Vec<IndexLine>> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let kind = detect_kind_from_name(name).context("not a recognized archive")?;
    extract_archive_file(path, &kind, cfg)
}

/// Check if a file is an archive based on extension.
pub fn accepts(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(is_archive_ext)
        .unwrap_or(false)
}

pub fn is_archive_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "zip" | "tar" | "gz" | "bz2" | "xz" | "tgz" | "tbz2" | "txz" | "7z"
    )
}

// ============================================================================
// ARCHIVE KIND DETECTION
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
enum ArchiveKind {
    Zip,
    TarGz,
    TarBz2,
    TarXz,
    Tar,
    Gz,      // single-file gzip (e.g. foo.log.gz)
    Bz2,     // single-file bzip2
    Xz,      // single-file xz
    SevenZip,
}

fn detect_kind_from_name(name: &str) -> Option<ArchiveKind> {
    let n = name.to_lowercase();
    // Compound extensions must be checked before simple ones
    if n.ends_with(".tar.gz") || n.ends_with(".tgz")   { return Some(ArchiveKind::TarGz);   }
    if n.ends_with(".tar.bz2") || n.ends_with(".tbz2") { return Some(ArchiveKind::TarBz2);  }
    if n.ends_with(".tar.xz") || n.ends_with(".txz")   { return Some(ArchiveKind::TarXz);   }
    if n.ends_with(".tar")                              { return Some(ArchiveKind::Tar);     }
    if n.ends_with(".zip")                              { return Some(ArchiveKind::Zip);     }
    if n.ends_with(".gz")                               { return Some(ArchiveKind::Gz);      }
    if n.ends_with(".bz2")                              { return Some(ArchiveKind::Bz2);     }
    if n.ends_with(".xz")                               { return Some(ArchiveKind::Xz);      }
    if n.ends_with(".7z")                               { return Some(ArchiveKind::SevenZip);}
    None
}

// ============================================================================
// FILE-BASED ARCHIVE EXTRACTION (top-level entry points)
// ============================================================================

fn extract_archive_file(path: &Path, kind: &ArchiveKind, cfg: &ExtractorConfig) -> Result<Vec<IndexLine>> {
    match kind {
        ArchiveKind::Zip      => extract_zip_file(path, cfg),
        ArchiveKind::TarGz    => extract_tar(tar::Archive::new(GzDecoder::new(File::open(path)?)), cfg),
        ArchiveKind::TarBz2   => extract_tar(tar::Archive::new(BzDecoder::new(File::open(path)?)), cfg),
        ArchiveKind::TarXz    => extract_tar(tar::Archive::new(XzDecoder::new(File::open(path)?)), cfg),
        ArchiveKind::Tar      => extract_tar(tar::Archive::new(File::open(path)?), cfg),
        ArchiveKind::Gz       => extract_single_compressed(GzDecoder::new(File::open(path)?), path),
        ArchiveKind::Bz2      => extract_single_compressed(BzDecoder::new(File::open(path)?), path),
        ArchiveKind::Xz       => extract_single_compressed(XzDecoder::new(File::open(path)?), path),
        ArchiveKind::SevenZip => extract_7z(path, cfg),
    }
}

fn extract_zip_file(path: &Path, cfg: &ExtractorConfig) -> Result<Vec<IndexLine>> {
    let file = File::open(path)?;
    let mut archive = zip::ZipArchive::new(file).context("opening zip")?;
    let mut lines = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).context("reading zip entry")?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        lines.extend(extract_member_bytes(bytes, &name, cfg, 1));
    }
    Ok(lines)
}

fn extract_tar<R: Read>(mut archive: tar::Archive<R>, cfg: &ExtractorConfig) -> Result<Vec<IndexLine>> {
    let mut lines = Vec::new();
    for entry_result in archive.entries().context("reading tar entries")? {
        let mut entry = entry_result.context("reading tar entry")?;
        if entry.header().entry_type().is_dir() {
            continue;
        }
        let name = entry.path()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        lines.extend(extract_member_bytes(bytes, &name, cfg, 1));
    }
    Ok(lines)
}

fn extract_7z(path: &Path, cfg: &ExtractorConfig) -> Result<Vec<IndexLine>> {
    let mut lines = Vec::new();
    let mut sz = sevenz_rust::SevenZReader::open(path, sevenz_rust::Password::empty())?;

    sz.for_each_entries(|entry, reader| {
        if entry.is_directory() {
            return Ok(true);
        }
        let name = entry.name().to_string();
        let mut bytes = Vec::new();
        let _ = reader.read_to_end(&mut bytes);
        lines.extend(extract_member_bytes(bytes, &name, cfg, 1));
        Ok(true)
    })?;

    Ok(lines)
}

/// Extract a single-file compressed archive (bare .gz, .bz2, .xz).
/// Decompresses and indexes the inner content as a member with archive_path set.
fn extract_single_compressed<R: Read>(mut reader: R, path: &Path) -> Result<Vec<IndexLine>> {
    let inner_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();

    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;

    // Bare compressed files are always text; use a no-wrapping config.
    let text_cfg = ExtractorConfig { max_size_kb: usize::MAX, max_depth: 1, max_line_length: 0 };
    let lines = extract_member_bytes(bytes, &inner_name, &text_cfg, 1);
    Ok(lines)
}

// ============================================================================
// MEMBER EXTRACTION (handles bytes from any archive format)
// ============================================================================

/// Extract an archive member from raw bytes.
///
/// Returns IndexLines with `archive_path = Some(entry_name)` for all lines.
/// For nested archives, inner lines have their archive_path prefixed with entry_name.
///
/// # Arguments
/// * `bytes` - Raw bytes of the archive member
/// * `entry_name` - The member's path within the outer archive
/// * `cfg` - Extractor configuration (size limit, depth limit, line wrapping)
/// * `depth` - Current nesting depth (1 = direct member of outer archive)
fn extract_member_bytes(
    bytes: Vec<u8>,
    entry_name: &str,
    cfg: &ExtractorConfig,
    depth: usize,
) -> Vec<IndexLine> {
    // Always index the filename so the member is discoverable by name.
    let mut lines = vec![IndexLine {
        archive_path: Some(entry_name.to_string()),
        line_number: 0,
        content: entry_name.to_string(),
    }];

    // Skip content extraction if the member is too large.
    if bytes.len() > cfg.max_size_kb * 1024 {
        return lines;
    }

    // Depth limit: only the filename was already added above.
    if depth > cfg.max_depth {
        warn!(
            "archive depth limit ({}) exceeded at '{}'; indexing filename only",
            cfg.max_depth, entry_name
        );
        return lines;
    }

    let member_path = Path::new(entry_name);

    // ── Nested archive ────────────────────────────────────────────────────────
    if let Some(kind) = detect_kind_from_name(entry_name) {
        match kind {
            // Single-file compressed: decompress and index the inner content
            // under the same archive_path as the compressed file itself.
            ArchiveKind::Gz | ArchiveKind::Bz2 | ArchiveKind::Xz => {
                let decompressed: Option<Vec<u8>> = match kind {
                    ArchiveKind::Gz => {
                        let mut out = Vec::new();
                        GzDecoder::new(Cursor::new(&bytes)).read_to_end(&mut out).ok().map(|_| out)
                    }
                    ArchiveKind::Bz2 => {
                        let mut out = Vec::new();
                        BzDecoder::new(Cursor::new(&bytes)).read_to_end(&mut out).ok().map(|_| out)
                    }
                    ArchiveKind::Xz => {
                        let mut out = Vec::new();
                        XzDecoder::new(Cursor::new(&bytes)).read_to_end(&mut out).ok().map(|_| out)
                    }
                    _ => unreachable!(),
                };

                if let Some(inner_bytes) = decompressed {
                    if let Ok(text) = String::from_utf8(inner_bytes) {
                        // Index the inner content under the compressed file's archive_path.
                        let text_lines = find_extract_text::lines_from_str(&text, Some(entry_name.to_string()));
                        lines.extend(text_lines);
                    }
                }
                return lines;
            }

            // Multi-file archive: extract recursively, prefix all member paths.
            _ => {
                match extract_archive_from_bytes(&bytes, &kind, cfg, depth + 1) {
                    Ok(inner_lines) => {
                        let prefixed = inner_lines.into_iter().map(|mut l| {
                            let inner = l.archive_path.as_deref().unwrap_or("");
                            l.archive_path = Some(if inner.is_empty() {
                                entry_name.to_string()
                            } else {
                                format!("{}::{}", entry_name, inner)
                            });
                            l
                        });
                        lines.extend(prefixed);
                    }
                    Err(e) => warn!("failed to extract nested archive '{}': {}", entry_name, e),
                }
                return lines;
            }
        }
    }

    // ── PDF ───────────────────────────────────────────────────────────────────
    if find_extract_pdf::accepts(member_path) {
        match find_extract_pdf::extract_from_bytes(&bytes, entry_name, cfg) {
            Ok(pdf_lines) => {
                let with_path = pdf_lines.into_iter().map(|mut l| {
                    l.archive_path = Some(entry_name.to_string());
                    l
                });
                lines.extend(with_path);
            }
            Err(e) => warn!("PDF extraction failed for '{}': {}", entry_name, e),
        }
        return lines;
    }

    // ── Media (image / audio / video) ─────────────────────────────────────────
    if find_extract_media::accepts(member_path) {
        match extract_media_from_bytes(&bytes, entry_name, cfg) {
            Ok(media_lines) => {
                let with_path = media_lines.into_iter().map(|mut l| {
                    l.archive_path = Some(entry_name.to_string());
                    l
                });
                lines.extend(with_path);
            }
            Err(e) => warn!("media extraction failed for '{}': {}", entry_name, e),
        }
        return lines;
    }

    // ── Text ──────────────────────────────────────────────────────────────────
    if find_extract_text::accepts(member_path) {
        if let Ok(text) = String::from_utf8(bytes) {
            let text_lines = find_extract_text::lines_from_str(&text, Some(entry_name.to_string()));
            lines.extend(text_lines);
        }
    }

    lines
}

// ============================================================================
// IN-MEMORY ARCHIVE EXTRACTION (for nested archives read from bytes)
// ============================================================================

/// Extract a nested archive from in-memory bytes.
/// Returns lines where archive_path is relative to the nested archive (no outer prefix).
/// The caller is responsible for prefixing with the outer entry_name.
fn extract_archive_from_bytes(
    bytes: &[u8],
    kind: &ArchiveKind,
    cfg: &ExtractorConfig,
    depth: usize,
) -> Result<Vec<IndexLine>> {
    match kind {
        ArchiveKind::Zip => {
            let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).context("opening nested zip")?;
            let mut lines = Vec::new();
            for i in 0..archive.len() {
                let mut entry = archive.by_index(i)?;
                if entry.is_dir() {
                    continue;
                }
                let name = entry.name().to_string();
                let mut entry_bytes = Vec::new();
                entry.read_to_end(&mut entry_bytes)?;
                lines.extend(extract_member_bytes(entry_bytes, &name, cfg, depth));
            }
            Ok(lines)
        }
        ArchiveKind::TarGz  => extract_tar_from_bytes(tar::Archive::new(GzDecoder::new(Cursor::new(bytes))), cfg, depth),
        ArchiveKind::TarBz2 => extract_tar_from_bytes(tar::Archive::new(BzDecoder::new(Cursor::new(bytes))), cfg, depth),
        ArchiveKind::TarXz  => extract_tar_from_bytes(tar::Archive::new(XzDecoder::new(Cursor::new(bytes))), cfg, depth),
        ArchiveKind::Tar    => extract_tar_from_bytes(tar::Archive::new(Cursor::new(bytes)), cfg, depth),
        // 7z from in-memory bytes is not supported by sevenz-rust; index filename only.
        ArchiveKind::SevenZip => Ok(vec![]),
        // Single-file compressed nested archives handled in extract_member_bytes.
        _ => Ok(vec![]),
    }
}

fn extract_tar_from_bytes<R: Read>(
    mut archive: tar::Archive<R>,
    cfg: &ExtractorConfig,
    depth: usize,
) -> Result<Vec<IndexLine>> {
    let mut lines = Vec::new();
    for entry_result in archive.entries()? {
        let mut entry = entry_result?;
        if entry.header().entry_type().is_dir() {
            continue;
        }
        let name = entry.path()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        lines.extend(extract_member_bytes(bytes, &name, cfg, depth));
    }
    Ok(lines)
}

// ============================================================================
// MEDIA EXTRACTION VIA TEMP FILE
// ============================================================================

/// Write archive member bytes to a temp file with the correct extension,
/// then delegate to the media extractor which needs a real file path.
fn extract_media_from_bytes(
    bytes: &[u8],
    entry_name: &str,
    cfg: &ExtractorConfig,
) -> Result<Vec<IndexLine>> {
    use std::io::Write;

    let ext = Path::new(entry_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let mut tmp = tempfile::Builder::new()
        .suffix(&format!(".{}", ext))
        .tempfile()?;
    tmp.write_all(bytes)?;
    tmp.flush()?;

    find_extract_media::extract(tmp.path(), cfg)
}
