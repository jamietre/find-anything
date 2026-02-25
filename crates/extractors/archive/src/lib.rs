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

// Internal callback alias for brevity.
type CB<'a> = &'a mut dyn FnMut(Vec<IndexLine>);

/// Returns true if any path component starts with `.` (and is not `.` or `..`).
/// Used to skip hidden members (e.g. `.terraform/`, `.git/`) when
/// `cfg.include_hidden` is false.
fn has_hidden_component(name: &str) -> bool {
    name.split('/').any(|c| c.starts_with('.') && c.len() > 1 && c != "..")
}

/// Extract content from archive files (ZIP, TAR, TGZ, TBZ2, TXZ, GZ, BZ2, XZ, 7Z).
///
/// Calls `callback` once per top-level archive member with that member's lines
/// (including recursively extracted nested-archive content).  This keeps memory
/// usage proportional to one member at a time rather than the whole archive.
///
/// Use `extract` if you need a `Vec<IndexLine>` instead of a callback.
pub fn extract_streaming<F>(path: &Path, cfg: &ExtractorConfig, callback: &mut F) -> Result<()>
where
    F: FnMut(Vec<IndexLine>),
{
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let kind = detect_kind_from_name(name).context("not a recognized archive")?;
    dispatch_streaming(path, &kind, cfg, callback)
}

/// Extract content from archive files, collecting all lines into a `Vec`.
///
/// For large archives prefer `extract_streaming` to avoid accumulating all
/// member lines in memory simultaneously.
pub fn extract(path: &Path, cfg: &ExtractorConfig) -> Result<Vec<IndexLine>> {
    let mut lines = Vec::new();
    extract_streaming(path, cfg, &mut |member_lines| lines.extend(member_lines))?;
    Ok(lines)
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
    Gz,       // single-file gzip (e.g. foo.log.gz)
    Bz2,      // single-file bzip2
    Xz,       // single-file xz
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

fn is_multifile_archive(kind: &ArchiveKind) -> bool {
    !matches!(kind, ArchiveKind::Gz | ArchiveKind::Bz2 | ArchiveKind::Xz)
}

// ============================================================================
// DISPATCH
// ============================================================================

/// Internal dispatch: uses `dyn FnMut` to avoid infinite monomorphisation when
/// nested archive extraction recurses back through the streaming functions.
fn dispatch_streaming(path: &Path, kind: &ArchiveKind, cfg: &ExtractorConfig, callback: CB<'_>) -> Result<()> {
    match kind {
        ArchiveKind::Zip      => zip_streaming(path, cfg, callback),
        ArchiveKind::TarGz    => tar_streaming(tar::Archive::new(GzDecoder::new(File::open(path)?)), cfg, callback),
        ArchiveKind::TarBz2   => tar_streaming(tar::Archive::new(BzDecoder::new(File::open(path)?)), cfg, callback),
        ArchiveKind::TarXz    => tar_streaming(tar::Archive::new(XzDecoder::new(File::open(path)?)), cfg, callback),
        ArchiveKind::Tar      => tar_streaming(tar::Archive::new(File::open(path)?), cfg, callback),
        ArchiveKind::Gz       => { callback(single_compressed(GzDecoder::new(File::open(path)?), path, cfg)?); Ok(()) }
        ArchiveKind::Bz2      => { callback(single_compressed(BzDecoder::new(File::open(path)?), path, cfg)?); Ok(()) }
        ArchiveKind::Xz       => { callback(single_compressed(XzDecoder::new(File::open(path)?), path, cfg)?); Ok(()) }
        ArchiveKind::SevenZip => sevenz_streaming(path, cfg, callback),
    }
}

// ============================================================================
// FORMAT-SPECIFIC STREAMING EXTRACTORS
// ============================================================================

fn zip_streaming(path: &Path, cfg: &ExtractorConfig, callback: CB<'_>) -> Result<()> {
    let file = File::open(path)?;
    let archive = zip::ZipArchive::new(file).context("opening zip")?;
    zip_from_archive(archive, cfg, callback)
}

/// Core ZIP extractor, generic over any `Read + Seek` source.
///
/// Called for top-level ZIPs (via a file path) and for nested ZIPs
/// (via a `Cursor<Vec<u8>>` read from an outer archive member).
fn zip_from_archive<R: Read + std::io::Seek>(
    mut archive: zip::ZipArchive<R>,
    cfg: &ExtractorConfig,
    callback: CB<'_>,
) -> Result<()> {
    let size_limit = cfg.max_size_kb * 1024;

    for i in 0..archive.len() {
        let mut entry = match archive.by_index(i) {
            Ok(e) => e,
            Err(e) => { warn!("zip: skipping entry {i}: {e:#}"); continue; }
        };
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_string();

        if !cfg.include_hidden && has_hidden_component(&name) {
            continue;
        }

        // Multi-file nested archive: recurse without writing to disk where possible.
        if let Some(kind) = detect_kind_from_name(&name) {
            if is_multifile_archive(&kind) {
                handle_nested_archive(&mut entry as &mut dyn Read, &name, &kind, cfg, callback);
                continue;
            }
        }

        // Check uncompressed size before allocating — skip reading oversized members.
        if entry.size() as usize > size_limit {
            callback(make_filename_line(&name));
            continue;
        }
        let mut bytes = Vec::new();
        if let Err(e) = entry.read_to_end(&mut bytes) {
            let member_path = std::path::Path::new(&name);
            if find_extract_media::accepts(member_path) {
                tracing::debug!("zip: skipping binary entry '{}': {}", name, e);
            } else {
                warn!("zip: failed to read entry '{}': {}", name, e);
            }
        }
        callback(extract_member_bytes(bytes, &name, cfg));
    }
    Ok(())
}

fn tar_streaming<R: Read>(mut archive: tar::Archive<R>, cfg: &ExtractorConfig, callback: CB<'_>) -> Result<()> {
    let size_limit = cfg.max_size_kb * 1024;

    for entry_result in archive.entries().context("reading tar entries")? {
        let mut entry = match entry_result {
            Ok(e) => e,
            Err(e) => { warn!("tar: skipping entry: {e:#}"); continue; }
        };
        if entry.header().entry_type().is_dir() {
            continue;
        }
        let name = entry.path()
            .ok()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        if !cfg.include_hidden && has_hidden_component(&name) {
            continue;
        }

        // Multi-file nested archive: recurse without writing to disk where possible.
        if let Some(kind) = detect_kind_from_name(&name) {
            if is_multifile_archive(&kind) {
                handle_nested_archive(&mut entry as &mut dyn Read, &name, &kind, cfg, callback);
                continue;
            }
        }

        // Check uncompressed size before allocating — skip reading oversized members.
        let entry_size = entry.header().size().unwrap_or(0) as usize;
        if entry_size > size_limit {
            callback(make_filename_line(&name));
            continue;
        }
        let mut bytes = Vec::new();
        if let Err(e) = entry.read_to_end(&mut bytes) {
            let member_path = std::path::Path::new(&name);
            if find_extract_media::accepts(member_path) {
                tracing::debug!("tar: skipping binary entry '{}': {}", name, e);
            } else {
                warn!("tar: failed to read entry '{}': {}", name, e);
            }
        }
        callback(extract_member_bytes(bytes, &name, cfg));
    }
    Ok(())
}

fn sevenz_streaming(path: &Path, cfg: &ExtractorConfig, callback: CB<'_>) -> Result<()> {
    let mut sz = sevenz_rust2::ArchiveReader::open(path, sevenz_rust2::Password::empty())?;
    let size_limit = cfg.max_size_kb * 1024;

    sz.for_each_entries(|entry, reader| {
        if entry.is_directory() {
            return Ok(true);
        }
        let name = entry.name().to_string();

        if !cfg.include_hidden && has_hidden_component(&name) {
            // Drain so solid-block stream stays in sync.
            let _ = std::io::copy(reader, &mut std::io::sink());
            return Ok(true);
        }

        // Multi-file nested archive: handle_nested_archive always drains `reader`,
        // maintaining solid-block integrity.
        if let Some(kind) = detect_kind_from_name(&name) {
            if is_multifile_archive(&kind) {
                handle_nested_archive(reader as &mut dyn Read, &name, &kind, cfg, callback);
                return Ok(true);
            }
        }

        // Check uncompressed size before allocating.
        if entry.size() as usize > size_limit {
            // Drain the reader to keep solid-block stream in sync.
            let _ = std::io::copy(reader, &mut std::io::sink());
            callback(make_filename_line(&name));
            return Ok(true);
        }
        let mut bytes = Vec::new();
        if let Err(e) = reader.read_to_end(&mut bytes) {
            let msg = e.to_string();
            if msg.contains("ChecksumVerificationFailed") {
                // Shouldn't happen after the drain fix, but handle defensively.
                warn!("7z: checksum mismatch for '{}': {}", name, e);
            } else {
                let member_path = std::path::Path::new(&name);
                if find_extract_media::accepts(member_path) {
                    tracing::debug!("7z: skipping binary entry '{}': {}", name, e);
                } else {
                    warn!("7z: failed to read entry '{}': {}", name, e);
                }
            }
            // bytes stays empty — filename still indexed below
        }
        callback(extract_member_bytes(bytes, &name, cfg));
        Ok(true)
    })?;

    Ok(())
}

/// Extract a single-file compressed archive (bare .gz, .bz2, .xz).
/// Decompresses up to `cfg.max_size_kb` bytes and indexes the inner content.
fn single_compressed<R: Read>(reader: R, path: &Path, cfg: &ExtractorConfig) -> Result<Vec<IndexLine>> {
    let inner_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();

    let size_limit = cfg.max_size_kb * 1024;
    let mut bytes = Vec::new();
    reader.take(size_limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > size_limit {
        return Ok(make_filename_line(&inner_name));
    }

    Ok(extract_member_bytes(bytes, &inner_name, cfg))
}

// ============================================================================
// NESTED ARCHIVE EXTRACTION
// ============================================================================

/// Extract a nested multi-file archive from a stream, recursing into its members.
///
/// - **Tar variants** (Tar, TarGz, TarBz2, TarXz): streamed directly from `reader`
///   — zero extra memory and no disk I/O beyond what the tar crate uses internally.
/// - **Zip**: bytes are read into a `Cursor<Vec<u8>>` for in-memory extraction (no
///   disk I/O); falls back to a temp file on disk if the stream exceeds `max_temp_file_mb`.
/// - **7z**: always written to a temp file on disk (the 7z API requires a seekable
///   path); bounded by `max_temp_file_mb`.
///
/// Dynamic dispatch for both callback (`dyn FnMut`) AND reader (`dyn Read`) is used
/// to prevent infinite monomorphisation when the extraction functions recurse through
/// nested archives.
///
/// **Always fully consumes `reader`**, which is required for 7z solid-block stream
/// integrity even when the depth or size limit is exceeded.
fn handle_nested_archive(
    reader: &mut dyn Read,
    outer_name: &str,
    kind: &ArchiveKind,
    cfg: &ExtractorConfig,
    callback: CB<'_>,
) {
    // Always emit the filename of the nested archive itself.
    callback(make_filename_line(outer_name));

    if cfg.max_depth == 0 {
        warn!(
            "archive nesting limit exceeded at '{}'; indexing filename only",
            outer_name
        );
        // Drain so 7z solid-block stream stays in sync.
        let _ = std::io::copy(reader, &mut std::io::sink());
        return;
    }

    let inner_cfg = ExtractorConfig {
        max_depth: cfg.max_depth.saturating_sub(1),
        ..*cfg
    };

    // Wrapper callback that prefixes inner archive_paths with `outer_name::`.
    let outer_prefix = outer_name.to_string();
    let mut prefixed = |inner_lines: Vec<IndexLine>| {
        let p: Vec<IndexLine> = inner_lines
            .into_iter()
            .map(|mut l| {
                let inner = l.archive_path.as_deref().unwrap_or("");
                l.archive_path = Some(if inner.is_empty() {
                    outer_prefix.clone()
                } else {
                    format!("{}::{}", outer_prefix, inner)
                });
                l
            })
            .collect();
        callback(p);
    };

    // Use `reader` as `&mut dyn Read` throughout so that tar_streaming<GzDecoder<&mut dyn Read>>
    // is always the same monomorphisation regardless of nesting depth.
    let result: Result<()> = match kind {
        // ── Tar variants: stream directly, zero extra memory ─────────────
        ArchiveKind::TarGz  => tar_streaming(tar::Archive::new(GzDecoder::new(reader)), &inner_cfg, &mut prefixed),
        ArchiveKind::TarBz2 => tar_streaming(tar::Archive::new(BzDecoder::new(reader)), &inner_cfg, &mut prefixed),
        ArchiveKind::TarXz  => tar_streaming(tar::Archive::new(XzDecoder::new(reader)), &inner_cfg, &mut prefixed),
        ArchiveKind::Tar    => tar_streaming(tar::Archive::new(reader), &inner_cfg, &mut prefixed),

        // ── Zip: read into memory (Cursor); temp file if too large ────────
        ArchiveKind::Zip    => nested_zip(reader, outer_name, &inner_cfg, &mut prefixed),

        // ── 7z: requires a seekable file path — always use temp file ─────
        ArchiveKind::SevenZip => nested_sevenz(reader, outer_name, &inner_cfg, &mut prefixed),

        // Single-file compressed types are not passed to handle_nested_archive.
        _ => return,
    };

    if let Err(e) = result {
        // Corrupt or truncated nested archives (e.g. "Could not find EOCD") are
        // common in real-world data and unactionable — the filename is already
        // indexed above, so demote to DEBUG to avoid noisy logs.
        tracing::debug!("failed to extract nested archive '{}': {:#}", outer_name, e);
    }
}

/// Extract a nested zip from a reader by buffering bytes into a `Cursor<Vec<u8>>`.
///
/// If the stream exceeds `max_temp_file_mb`, spills to a temp file on disk instead.
fn nested_zip(mut reader: &mut dyn Read, outer_name: &str, cfg: &ExtractorConfig, callback: CB<'_>) -> Result<()> {
    let max_bytes = (cfg.max_temp_file_mb * 1024 * 1024) as u64;

    // Read up to max_bytes+1 to detect whether the archive is over the limit.
    let mut bytes = Vec::new();
    let written = {
        let mut limited = (&mut reader).take(max_bytes + 1);
        std::io::copy(&mut limited, &mut bytes)?
    };

    if written > max_bytes {
        warn!(
            "nested zip '{}' exceeds {} MB; falling back to temp file",
            outer_name, cfg.max_temp_file_mb
        );
        // Spill already-read bytes plus remainder to a temp file, then extract from it.
        let ext = Path::new(outer_name).extension().and_then(|e| e.to_str()).unwrap_or("zip");
        let mut tmp = tempfile::Builder::new()
            .suffix(&format!(".{}", ext))
            .tempfile()?;
        {
            use std::io::Write;
            tmp.write_all(&bytes)?;
        }
        std::io::copy(&mut reader, &mut tmp)?;
        {
            use std::io::{Seek, Write};
            tmp.flush()?;
            tmp.seek(std::io::SeekFrom::Start(0))?;
        }
        let archive = zip::ZipArchive::new(tmp).context("opening oversized nested zip from temp file")?;
        return zip_from_archive(archive, cfg, callback);
    }

    let archive = zip::ZipArchive::new(Cursor::new(bytes)).context("opening nested zip")?;
    zip_from_archive(archive, cfg, callback)
}

/// Extract a nested 7z archive by streaming it to a temp file on disk.
///
/// 7z extraction requires a seekable file path; there is no in-memory API.
/// The temp file is bounded by `max_temp_file_mb`; archives larger than that are
/// skipped (filename only) and the reader is drained for solid-block integrity.
fn nested_sevenz(mut reader: &mut dyn Read, outer_name: &str, cfg: &ExtractorConfig, callback: CB<'_>) -> Result<()> {
    let max_bytes = (cfg.max_temp_file_mb * 1024 * 1024) as u64;
    let ext = Path::new(outer_name).extension().and_then(|e| e.to_str()).unwrap_or("7z");

    let mut tmp = tempfile::Builder::new()
        .suffix(&format!(".{}", ext))
        .tempfile()?;

    // Write at most max_bytes+1 so we can detect oversized archives.
    let written = {
        let mut limited = (&mut reader).take(max_bytes + 1);
        std::io::copy(&mut limited, &mut tmp)?
    };

    if written > max_bytes {
        warn!(
            "nested 7z '{}' exceeds {} MB; indexing filename only",
            outer_name, cfg.max_temp_file_mb
        );
        // Drain remaining bytes for 7z solid-block stream integrity.
        let _ = std::io::copy(&mut reader, &mut std::io::sink());
        return Ok(());
    }

    {
        use std::io::{Seek, Write};
        tmp.flush()?;
        tmp.seek(std::io::SeekFrom::Start(0))?;
    }
    sevenz_streaming(tmp.path(), cfg, callback)
}

// ============================================================================
// MEMBER EXTRACTION (handles bytes from any non-archive format)
// ============================================================================

/// Returns a Vec containing a single filename-only IndexLine for `name`.
fn make_filename_line(name: &str) -> Vec<IndexLine> {
    vec![IndexLine {
        archive_path: Some(name.to_string()),
        line_number: 0,
        content: name.to_string(),
    }]
}

/// Extract an archive member from raw bytes.
///
/// Single-file compressed formats (.gz/.bz2/.xz) are decompressed inline and
/// dispatched via `find_extract_dispatch`.  All other non-archive formats are
/// dispatched directly.  Multi-file archives are NOT handled here — the
/// caller routes those through `handle_nested_archive` before reaching
/// this function.
fn extract_member_bytes(bytes: Vec<u8>, entry_name: &str, cfg: &ExtractorConfig) -> Vec<IndexLine> {
    // Always index the filename so the member is discoverable by name.
    let mut lines = make_filename_line(entry_name);

    // Skip content extraction if the member is too large.
    if bytes.len() > cfg.max_size_kb * 1024 {
        return lines;
    }

    let size_limit = cfg.max_size_kb * 1024;

    // ── Single-file compressed (.gz / .bz2 / .xz) ────────────────────────────
    // Multi-file archive kinds (.zip, .tar, etc.) are intercepted by the caller;
    // only single-file compressed formats are handled here.
    if let Some(kind) = detect_kind_from_name(entry_name) {
        match kind {
            ArchiveKind::Gz | ArchiveKind::Bz2 | ArchiveKind::Xz => {
                // Decompress, capping output at size_limit to prevent RAM spikes
                // when a small compressed blob expands to a very large plaintext.
                let decompressed: Option<Vec<u8>> = match kind {
                    ArchiveKind::Gz => {
                        let mut out = Vec::new();
                        match GzDecoder::new(Cursor::new(&bytes))
                            .take(size_limit as u64 + 1)
                            .read_to_end(&mut out)
                        {
                            Ok(_) if out.len() <= size_limit => Some(out),
                            _ => None,
                        }
                    }
                    ArchiveKind::Bz2 => {
                        let mut out = Vec::new();
                        match BzDecoder::new(Cursor::new(&bytes))
                            .take(size_limit as u64 + 1)
                            .read_to_end(&mut out)
                        {
                            Ok(_) if out.len() <= size_limit => Some(out),
                            _ => None,
                        }
                    }
                    ArchiveKind::Xz => {
                        let mut out = Vec::new();
                        match XzDecoder::new(Cursor::new(&bytes))
                            .take(size_limit as u64 + 1)
                            .read_to_end(&mut out)
                        {
                            Ok(_) if out.len() <= size_limit => Some(out),
                            _ => None,
                        }
                    }
                    _ => unreachable!(),
                };

                if let Some(inner_bytes) = decompressed {
                    // Dispatch decompressed bytes; use inner name (strip .gz/.bz2/.xz).
                    let inner_name = Path::new(entry_name)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or(entry_name);
                    let content_lines = find_extract_dispatch::dispatch_from_bytes(&inner_bytes, inner_name, cfg);
                    let with_path = content_lines.into_iter().map(|mut l| {
                        l.archive_path = Some(entry_name.to_string());
                        l
                    });
                    lines.extend(with_path);
                }
                return lines;
            }
            // Multi-file archive: caller should have routed this through
            // handle_nested_archive; return filename only as a fallback.
            _ => return lines,
        }
    }

    // ── All other formats: unified dispatch ───────────────────────────────────
    let content_lines = find_extract_dispatch::dispatch_from_bytes(&bytes, entry_name, cfg);
    let with_path = content_lines.into_iter().map(|mut l| {
        l.archive_path = Some(entry_name.to_string());
        l
    });
    lines.extend(with_path);
    lines
}
