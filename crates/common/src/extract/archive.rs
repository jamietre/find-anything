use std::fs::File;
use std::io::{BufRead, BufReader, Cursor, Read};
use std::path::Path;

use anyhow::{Context, Result};
use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use tracing::warn;
use xz2::read::XzDecoder;

use crate::api::IndexLine;
use crate::extract::Extractor;

pub struct ArchiveExtractor {
    pub max_depth: usize,
}

impl Extractor for ArchiveExtractor {
    fn accepts(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| is_archive_ext(e))
            .unwrap_or(false)
    }

    fn extract(&self, path: &Path) -> Result<Vec<IndexLine>> {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let kind = detect_kind_from_name(name).context("not a recognised archive")?;
        extract_archive_file(path, &kind, self.max_depth)
    }
}

pub fn is_archive_ext(ext: &str) -> bool {
    // Note: compound extensions (.tar.gz etc.) are matched by detect_kind_from_name
    // on the full filename; this is just for the quick accepts() check.
    matches!(
        ext.to_lowercase().as_str(),
        "zip" | "tar" | "gz" | "bz2" | "xz" | "tgz" | "tbz2" | "txz" | "7z"
    )
}

#[derive(Debug, Clone, PartialEq)]
enum ArchiveKind {
    Zip,
    TarGz,
    TarBz2,
    TarXz,
    Tar,
    Gz,       // single-file gzip
    Bz2,      // single-file bzip2
    Xz,       // single-file xz
    SevenZip,
}

fn detect_kind_from_name(name: &str) -> Option<ArchiveKind> {
    let n = name.to_lowercase();
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

fn extract_archive_file(path: &Path, kind: &ArchiveKind, max_depth: usize) -> Result<Vec<IndexLine>> {
    match kind {
        ArchiveKind::Zip     => extract_zip_file(path, max_depth),
        ArchiveKind::TarGz   => extract_tar(tar::Archive::new(GzDecoder::new(File::open(path)?)), max_depth),
        ArchiveKind::TarBz2  => extract_tar(tar::Archive::new(BzDecoder::new(File::open(path)?)), max_depth),
        ArchiveKind::TarXz   => extract_tar(tar::Archive::new(XzDecoder::new(File::open(path)?)), max_depth),
        ArchiveKind::Tar     => extract_tar(tar::Archive::new(File::open(path)?), max_depth),
        ArchiveKind::Gz      => extract_single_compressed(GzDecoder::new(File::open(path)?),  path),
        ArchiveKind::Bz2     => extract_single_compressed(BzDecoder::new(File::open(path)?),  path),
        ArchiveKind::Xz      => extract_single_compressed(XzDecoder::new(File::open(path)?),  path),
        ArchiveKind::SevenZip => extract_sevenz(path, max_depth),
    }
}

// ── Inner-entry extraction (from in-memory bytes) ────────────────────────────

/// Extract an archive entry already loaded into memory, at the given nesting depth.
/// Returns IndexLines with `archive_path` set to the composite member path.
fn extract_entry_bytes(
    bytes: Vec<u8>,
    entry_name: &str,
    depth: usize,
    max_depth: usize,
) -> Vec<IndexLine> {
    if depth > max_depth {
        warn!(
            "archive depth limit ({}) exceeded at '{}'; indexing filename only",
            max_depth, entry_name
        );
        return vec![IndexLine {
            archive_path: Some(entry_name.to_string()),
            line_number: 0,
            content: entry_name.to_string(),
        }];
    }

    // If this entry is itself an archive, recurse into it.
    if let Some(kind) = detect_kind_from_name(entry_name) {
        match extract_nested_archive(&bytes, entry_name, &kind, depth, max_depth) {
            Ok(mut lines) => {
                // Add a line for the archive file itself (line_number=0) so it gets indexed
                // as a standalone entry with kind="archive" and can be expanded in the tree.
                lines.push(IndexLine {
                    archive_path: Some(entry_name.to_string()),
                    line_number: 0,
                    content: entry_name.to_string(),
                });
                return lines;
            }
            Err(e) => {
                warn!("failed to extract nested archive '{}': {}", entry_name, e);
                // Fall through to binary detection — will likely return empty.
            }
        }
    }

    // Otherwise treat as text.
    read_text_lines(Cursor::new(bytes), entry_name)
}

fn extract_nested_archive(
    bytes: &[u8],
    entry_name: &str,
    kind: &ArchiveKind,
    depth: usize,
    max_depth: usize,
) -> Result<Vec<IndexLine>> {
    let inner_lines = match kind {
        ArchiveKind::Zip => extract_zip_bytes(bytes, depth + 1, max_depth)?,
        ArchiveKind::TarGz  => extract_tar(tar::Archive::new(GzDecoder::new(Cursor::new(bytes))), max_depth)?,
        ArchiveKind::TarBz2 => extract_tar(tar::Archive::new(BzDecoder::new(Cursor::new(bytes))), max_depth)?,
        ArchiveKind::TarXz  => extract_tar(tar::Archive::new(XzDecoder::new(Cursor::new(bytes))), max_depth)?,
        ArchiveKind::Tar    => extract_tar(tar::Archive::new(Cursor::new(bytes)), max_depth)?,
        ArchiveKind::Gz => {
            let inner_name = Path::new(entry_name)
                .file_stem().and_then(|s| s.to_str()).unwrap_or("file");
            read_text_lines(GzDecoder::new(Cursor::new(bytes)), inner_name)
        }
        ArchiveKind::Bz2 => {
            let inner_name = Path::new(entry_name)
                .file_stem().and_then(|s| s.to_str()).unwrap_or("file");
            read_text_lines(BzDecoder::new(Cursor::new(bytes)), inner_name)
        }
        ArchiveKind::Xz => {
            let inner_name = Path::new(entry_name)
                .file_stem().and_then(|s| s.to_str()).unwrap_or("file");
            read_text_lines(XzDecoder::new(Cursor::new(bytes)), inner_name)
        }
        // 7z from bytes isn't supported by sevenz-rust; fall back to binary (empty)
        ArchiveKind::SevenZip => return Ok(vec![]),
    };

    // Prefix inner paths with this entry name using the :: separator.
    Ok(inner_lines
        .into_iter()
        .map(|mut l| {
            let inner_path = l.archive_path.as_deref().unwrap_or("");
            if inner_path.is_empty() {
                l.archive_path = Some(entry_name.to_string());
            } else {
                l.archive_path = Some(format!("{}::{}", entry_name, inner_path));
            }
            l
        })
        .collect())
}

// ── File-based extractors ─────────────────────────────────────────────────────

fn extract_zip_file(path: &Path, max_depth: usize) -> Result<Vec<IndexLine>> {
    let f = File::open(path)?;
    let mut zip = zip::ZipArchive::new(f).context("opening zip")?;
    let mut lines = Vec::new();

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).context("reading zip entry")?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        lines.extend(extract_entry_bytes(bytes, &name, 1, max_depth));
    }
    Ok(lines)
}

fn extract_zip_bytes(bytes: &[u8], depth: usize, max_depth: usize) -> Result<Vec<IndexLine>> {
    let cursor = Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(cursor).context("opening zip from bytes")?;
    let mut lines = Vec::new();

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).context("reading zip entry")?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        let mut entry_bytes = Vec::new();
        entry.read_to_end(&mut entry_bytes)?;
        lines.extend(extract_entry_bytes(entry_bytes, &name, depth, max_depth));
    }
    Ok(lines)
}

fn extract_tar<R: Read>(mut archive: tar::Archive<R>, max_depth: usize) -> Result<Vec<IndexLine>> {
    let mut lines = Vec::new();
    for entry in archive.entries().context("reading tar entries")? {
        let mut entry = entry.context("reading tar entry")?;
        if entry.header().entry_type().is_dir() {
            continue;
        }
        let name = entry
            .path()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        lines.extend(extract_entry_bytes(bytes, &name, 1, max_depth));
    }
    Ok(lines)
}

fn extract_single_compressed<R: Read>(reader: R, path: &Path) -> Result<Vec<IndexLine>> {
    // For a single-file compressed archive, the "inner path" is the decompressed filename
    // (strip the compression extension, e.g. "foo.log.gz" → "foo.log").
    let inner_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();
    Ok(read_text_lines(reader, &inner_name))
}

fn extract_sevenz(path: &Path, max_depth: usize) -> Result<Vec<IndexLine>> {
    let mut all_lines: Vec<IndexLine> = Vec::new();

    sevenz_rust::decompress_file_with_extract_fn(
        path,
        Path::new("/dev/null"),
        |entry, reader, _dest| {
            if !entry.is_directory() {
                let name = entry.name().to_string();
                let mut bytes = Vec::new();
                reader.read_to_end(&mut bytes)?;
                all_lines.extend(extract_entry_bytes(bytes, &name, 1, max_depth));
            }
            // Return true = "we handled extraction ourselves"
            Ok(true)
        },
    )
    .ok(); // sevenz_rust errors on actual extraction to /dev/null; we don't care

    Ok(all_lines)
}

// ── Text line reader ──────────────────────────────────────────────────────────

/// Read a reader as text (sniff first 512 bytes; if binary, return empty).
/// Returns IndexLines with `archive_path = Some(archive_path)`.
fn read_text_lines<R: Read>(reader: R, archive_path: &str) -> Vec<IndexLine> {
    let mut sniff_buf = [0u8; 512];
    let mut reader = BufReader::new(reader);

    let n = match reader.read(&mut sniff_buf) {
        Ok(n) => n,
        Err(_) => return vec![],
    };

    if !content_inspector::inspect(&sniff_buf[..n]).is_text() {
        return vec![];
    }

    // Re-chain the already-read bytes with the rest of the reader.
    let full = std::io::Cursor::new(sniff_buf[..n].to_vec()).chain(reader);
    BufReader::new(full)
        .lines()
        .enumerate()
        .filter_map(|(i, line)| {
            line.ok().map(|content| IndexLine {
                archive_path: Some(archive_path.to_string()),
                line_number: i + 1,
                content,
            })
        })
        .collect()
}
