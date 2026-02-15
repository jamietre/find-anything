use std::fs::File;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use find_common::api::IndexLine;

/// Extract content from archive files (ZIP, TAR, etc.).
///
/// Simplified version that handles basic archive extraction.
/// For each member file:
/// - Indexes the filename
/// - If it's a text file, extracts content
///
/// # Arguments
/// * `path` - Path to the archive file
/// * `max_size_kb` - Maximum file size in KB
///
/// # Returns
/// Vector of IndexLine objects with archive_path set to member paths
pub fn extract(path: &Path, max_size_kb: usize) -> Result<Vec<IndexLine>> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let kind = detect_kind_from_name(name).context("not a recognized archive")?;
    extract_archive_file(path, &kind, max_size_kb)
}

/// Check if a file is an archive based on extension.
pub fn accepts(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| is_archive_ext(e))
        .unwrap_or(false)
}

pub fn is_archive_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "zip" | "tar" | "gz" | "tgz" | "7z"
    )
}

#[derive(Debug, Clone, PartialEq)]
enum ArchiveKind {
    Zip,
    Tar,
    TarGz,
    SevenZip,
}

fn detect_kind_from_name(name: &str) -> Option<ArchiveKind> {
    let n = name.to_lowercase();
    if n.ends_with(".tar.gz") || n.ends_with(".tgz") {
        return Some(ArchiveKind::TarGz);
    }
    if n.ends_with(".tar") {
        return Some(ArchiveKind::Tar);
    }
    if n.ends_with(".zip") {
        return Some(ArchiveKind::Zip);
    }
    if n.ends_with(".7z") {
        return Some(ArchiveKind::SevenZip);
    }
    None
}

fn extract_archive_file(path: &Path, kind: &ArchiveKind, max_size_kb: usize) -> Result<Vec<IndexLine>> {
    match kind {
        ArchiveKind::Zip => extract_zip(path, max_size_kb),
        ArchiveKind::Tar => extract_tar(tar::Archive::new(File::open(path)?), max_size_kb),
        ArchiveKind::TarGz => {
            extract_tar(
                tar::Archive::new(flate2::read::GzDecoder::new(File::open(path)?)),
                max_size_kb,
            )
        }
        ArchiveKind::SevenZip => extract_7z(path, max_size_kb),
    }
}

// ============================================================================
// ZIP EXTRACTION
// ============================================================================

fn extract_zip(path: &Path, max_size_kb: usize) -> Result<Vec<IndexLine>> {
    let file = File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut lines = Vec::new();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let entry_name = entry.name().to_string();

        // Skip directories
        if entry.is_dir() {
            continue;
        }

        // Always index the filename
        lines.push(IndexLine {
            archive_path: Some(entry_name.clone()),
            line_number: 0,
            content: entry_name.clone(),
        });

        // Try to extract text content if it's a text file and not too large
        let size_kb = entry.size() / 1024;
        if size_kb > max_size_kb as u64 {
            continue;
        }

        if is_text_filename(&entry_name) {
            let mut content = String::new();
            if entry.read_to_string(&mut content).is_ok() {
                let member_lines = find_extract_text::lines_from_str(&content, Some(entry_name));
                lines.extend(member_lines);
            }
        }
    }

    Ok(lines)
}

// ============================================================================
// TAR EXTRACTION
// ============================================================================

fn extract_tar<R: Read>(mut archive: tar::Archive<R>, max_size_kb: usize) -> Result<Vec<IndexLine>> {
    let mut lines = Vec::new();

    for entry_result in archive.entries()? {
        let mut entry = entry_result?;
        let path = entry.path()?;
        let entry_name = path.to_string_lossy().to_string();

        // Skip directories
        if entry.header().entry_type().is_dir() {
            continue;
        }

        // Always index the filename
        lines.push(IndexLine {
            archive_path: Some(entry_name.clone()),
            line_number: 0,
            content: entry_name.clone(),
        });

        // Try to extract text content if it's a text file and not too large
        let size_kb = entry.size() / 1024;
        if size_kb > max_size_kb as u64 {
            continue;
        }

        if is_text_filename(&entry_name) {
            let mut content = String::new();
            if entry.read_to_string(&mut content).is_ok() {
                let member_lines = find_extract_text::lines_from_str(&content, Some(entry_name));
                lines.extend(member_lines);
            }
        }
    }

    Ok(lines)
}

// ============================================================================
// 7Z EXTRACTION
// ============================================================================

fn extract_7z(path: &Path, max_size_kb: usize) -> Result<Vec<IndexLine>> {
    let mut lines = Vec::new();
    let mut sz = sevenz_rust::SevenZReader::open(path, sevenz_rust::Password::empty())?;

    sz.for_each_entries(|entry, reader| {
        let entry_name = entry.name();

        // Always index the filename
        lines.push(IndexLine {
            archive_path: Some(entry_name.to_string()),
            line_number: 0,
            content: entry_name.to_string(),
        });

        // Try to extract text content if it's a text file and not too large
        let size_kb = entry.size() / 1024;
        if size_kb > max_size_kb as u64 {
            return Ok(true); // Continue to next entry
        }

        if is_text_filename(entry_name) {
            let mut content = Vec::new();
            if reader.read_to_end(&mut content).is_ok() {
                if let Ok(text) = String::from_utf8(content) {
                    let member_lines =
                        find_extract_text::lines_from_str(&text, Some(entry_name.to_string()));
                    lines.extend(member_lines);
                }
            }
        }

        Ok(true) // Continue to next entry
    })?;

    Ok(lines)
}

// ============================================================================
// HELPERS
// ============================================================================

/// Check if a filename suggests it's a text file.
fn is_text_filename(name: &str) -> bool {
    if let Some(ext) = Path::new(name).extension().and_then(|e| e.to_str()) {
        find_extract_text::is_text_ext(ext)
    } else {
        false
    }
}
