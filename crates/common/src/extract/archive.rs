use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use anyhow::{Context, Result};
use bzip2::read::BzDecoder;
use flate2::read::GzDecoder;
use xz2::read::XzDecoder;

use crate::api::IndexLine;
use crate::extract::Extractor;

pub struct ArchiveExtractor;

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
        extract_archive(path, &kind)
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
    Gz,   // single-file gzip
    Bz2,  // single-file bzip2
    Xz,   // single-file xz
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

fn extract_archive(path: &Path, kind: &ArchiveKind) -> Result<Vec<IndexLine>> {
    match kind {
        ArchiveKind::Zip     => extract_zip(path),
        ArchiveKind::TarGz   => extract_tar(tar::Archive::new(GzDecoder::new(File::open(path)?))),
        ArchiveKind::TarBz2  => extract_tar(tar::Archive::new(BzDecoder::new(File::open(path)?))),
        ArchiveKind::TarXz   => extract_tar(tar::Archive::new(XzDecoder::new(File::open(path)?))),
        ArchiveKind::Tar     => extract_tar(tar::Archive::new(File::open(path)?)),
        ArchiveKind::Gz      => extract_single_compressed(GzDecoder::new(File::open(path)?),  path),
        ArchiveKind::Bz2     => extract_single_compressed(BzDecoder::new(File::open(path)?),  path),
        ArchiveKind::Xz      => extract_single_compressed(XzDecoder::new(File::open(path)?),  path),
        ArchiveKind::SevenZip => extract_sevenz(path),
    }
}

/// Read a reader as text (sniff first 512 bytes; if binary, return empty).
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

fn extract_zip(path: &Path) -> Result<Vec<IndexLine>> {
    let f = File::open(path)?;
    let mut zip = zip::ZipArchive::new(f).context("opening zip")?;
    let mut lines = Vec::new();

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).context("reading zip entry")?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();
        lines.extend(read_text_lines(&mut entry, &name));
    }
    Ok(lines)
}

fn extract_tar<R: Read>(mut archive: tar::Archive<R>) -> Result<Vec<IndexLine>> {
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
        lines.extend(read_text_lines(&mut entry, &name));
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

fn extract_sevenz(path: &Path) -> Result<Vec<IndexLine>> {
    let mut all_lines: Vec<IndexLine> = Vec::new();

    sevenz_rust::decompress_file_with_extract_fn(
        path,
        Path::new("/dev/null"),
        |entry, reader, _dest| {
            if !entry.is_directory() {
                let name = entry.name().to_string();
                let entry_lines = read_text_lines(reader, &name);
                all_lines.extend(entry_lines);
            }
            // Return true = "we handled extraction ourselves"
            Ok(true)
        },
    )
    .ok(); // sevenz_rust errors on actual extraction to /dev/null; we don't care

    Ok(all_lines)
}
