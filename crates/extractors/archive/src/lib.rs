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

/// One batch of lines for a single archive member, with its content hash.
#[derive(Default)]
pub struct MemberBatch {
    pub lines: Vec<IndexLine>,
    /// blake3 hex hash of the member's raw bytes (decompressed from the archive).
    /// None for filename-only entries (too large, nested archives, or single-compressed).
    pub content_hash: Option<String>,
    /// Set when content extraction was skipped or failed.
    /// The caller (scan.rs) records this as an IndexingFailure for the member's path,
    /// so the reason is surfaced to users in the file viewer and errors panel.
    ///
    /// When `lines` is empty, the failure applies to the outer archive itself
    /// (e.g. a 7z solid block summary) rather than to a specific member.
    pub skip_reason: Option<String>,
}

// Internal callback alias for brevity.
type CB<'a> = &'a mut dyn FnMut(MemberBatch);

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
    F: FnMut(MemberBatch),
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
    extract_streaming(path, cfg, &mut |batch| lines.extend(batch.lines))?;
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
        ArchiveKind::TarGz    => tar_streaming(tar::Archive::new(GzDecoder::new(File::open(path)?)), path.to_str().unwrap_or(""), cfg, callback),
        ArchiveKind::TarBz2   => tar_streaming(tar::Archive::new(BzDecoder::new(File::open(path)?)), path.to_str().unwrap_or(""), cfg, callback),
        ArchiveKind::TarXz    => tar_streaming(tar::Archive::new(XzDecoder::new(File::open(path)?)), path.to_str().unwrap_or(""), cfg, callback),
        ArchiveKind::Tar      => tar_streaming(tar::Archive::new(File::open(path)?), path.to_str().unwrap_or(""), cfg, callback),
        ArchiveKind::Gz       => { callback(single_compressed(GzDecoder::new(File::open(path)?), path, cfg)?); Ok(()) }
        ArchiveKind::Bz2      => { callback(single_compressed(BzDecoder::new(File::open(path)?), path, cfg)?); Ok(()) }
        ArchiveKind::Xz       => { callback(single_compressed(XzDecoder::new(File::open(path)?), path, cfg)?); Ok(()) }
        ArchiveKind::SevenZip => sevenz_streaming(path, path.to_str().unwrap_or(""), cfg, callback),
    }
}

// ============================================================================
// FORMAT-SPECIFIC STREAMING EXTRACTORS
// ============================================================================

fn zip_streaming(path: &Path, cfg: &ExtractorConfig, callback: CB<'_>) -> Result<()> {
    let file = File::open(path)?;
    let archive = zip::ZipArchive::new(file).context("opening zip")?;
    zip_from_archive(archive, path.to_str().unwrap_or(""), cfg, callback)
}

/// Core ZIP extractor, generic over any `Read + Seek` source.
///
/// Called for top-level ZIPs (via a file path) and for nested ZIPs
/// (via a `Cursor<Vec<u8>>` read from an outer archive member).
fn zip_from_archive<R: Read + std::io::Seek>(
    mut archive: zip::ZipArchive<R>,
    display_prefix: &str,
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
            let reason = format!(
                "too large to index ({}, limit {})",
                fmt_size(entry.size()), fmt_size(size_limit as u64)
            );
            callback(MemberBatch { lines: make_filename_line(&name), content_hash: None, skip_reason: Some(reason) });
            continue;
        }
        // Use take() as a hard memory bound: entry.size() can be wrong for
        // streaming ZIPs with data descriptors, so guard the actual read too.
        let mut bytes = Vec::new();
        let read_result = (&mut entry as &mut dyn Read).take((size_limit + 1) as u64).read_to_end(&mut bytes);
        if bytes.len() > size_limit {
            let reason = format!("too large to index (limit {})", fmt_size(size_limit as u64));
            callback(MemberBatch { lines: make_filename_line(&name), content_hash: None, skip_reason: Some(reason) });
            continue;
        }
        let skip_reason = if let Err(ref e) = read_result {
            let member_path = std::path::Path::new(&name);
            if find_extract_media::accepts(member_path) {
                tracing::debug!("zip: skipping binary entry '{}': {}", name, e);
                None
            } else {
                warn!("zip: failed to read entry '{}': {}", name, e);
                if bytes.is_empty() { Some(format!("failed to read: {e}")) } else { None }
            }
        } else {
            None
        };
        let content_hash = if bytes.is_empty() { None } else { Some(blake3::hash(&bytes).to_hex().to_string()) };
        callback(MemberBatch { lines: extract_member_bytes(bytes, &name, display_prefix, cfg), content_hash, skip_reason });
    }
    Ok(())
}

fn tar_streaming<R: Read>(mut archive: tar::Archive<R>, display_prefix: &str, cfg: &ExtractorConfig, callback: CB<'_>) -> Result<()> {
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
            let reason = format!(
                "too large to index ({}, limit {})",
                fmt_size(entry_size as u64), fmt_size(size_limit as u64)
            );
            callback(MemberBatch { lines: make_filename_line(&name), content_hash: None, skip_reason: Some(reason) });
            continue;
        }
        // Use take() as a hard memory bound in case the header size was wrong.
        // The tar crate drains remaining entry bytes on Entry::drop(), so a
        // partial read here won't desync the stream.
        let mut bytes = Vec::new();
        let read_result = (&mut entry as &mut dyn Read).take((size_limit + 1) as u64).read_to_end(&mut bytes);
        if bytes.len() > size_limit {
            let reason = format!("too large to index (limit {})", fmt_size(size_limit as u64));
            callback(MemberBatch { lines: make_filename_line(&name), content_hash: None, skip_reason: Some(reason) });
            continue;
        }
        let skip_reason = if let Err(ref e) = read_result {
            let member_path = std::path::Path::new(&name);
            if find_extract_media::accepts(member_path) {
                tracing::debug!("tar: skipping binary entry '{}': {}", name, e);
                None
            } else {
                warn!("tar: failed to read entry '{}': {}", name, e);
                if bytes.is_empty() { Some(format!("failed to read: {e}")) } else { None }
            }
        } else {
            None
        };
        let content_hash = if bytes.is_empty() { None } else { Some(blake3::hash(&bytes).to_hex().to_string()) };
        callback(MemberBatch { lines: extract_member_bytes(bytes, &name, display_prefix, cfg), content_hash, skip_reason });
    }
    Ok(())
}

/// Process one 7z entry: check size, read content, emit to callback.
///
/// Shared by the per-block loop and the empty-file fallback path.
/// Always fully drains `reader` to keep solid-block streams in sync.
fn sevenz_process_entry(
    entry: &sevenz_rust2::ArchiveEntry,
    reader: &mut dyn Read,
    display_prefix: &str,
    size_limit: usize,
    cfg: &ExtractorConfig,
    callback: CB<'_>,
) -> Result<bool, sevenz_rust2::Error> {
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
            handle_nested_archive(reader, &name, &kind, cfg, callback);
            return Ok(true);
        }
    }

    // Fast-path: skip known-oversized entries immediately.
    // entry.size() can return 0 for some 7z entries (solid archives where the
    // per-file size isn't stored in SubStreamsInfo, or "empty file" markers),
    // so we also guard the actual read with take() below.
    if entry.size() as usize > size_limit {
        let reason = format!(
            "too large to index ({}, limit {})",
            fmt_size(entry.size()), fmt_size(size_limit as u64)
        );
        // Drain the reader to keep solid-block stream in sync.
        let _ = std::io::copy(reader, &mut std::io::sink());
        callback(MemberBatch { lines: make_filename_line(&name), content_hash: None, skip_reason: Some(reason) });
        return Ok(true);
    }
    // Bound the read to size_limit+1 bytes regardless of what entry.size()
    // reported.  If we hit the cap the entry was larger than the limit
    // (entry.size() was 0 or wrong); drain remainder and skip content.
    let mut bytes = Vec::new();
    let read_result = {
        let mut limited = (reader as &mut dyn Read).take((size_limit + 1) as u64);
        limited.read_to_end(&mut bytes)
    };
    if bytes.len() > size_limit {
        let reason = format!("too large to index (limit {})", fmt_size(size_limit as u64));
        let _ = std::io::copy(reader, &mut std::io::sink());
        callback(MemberBatch { lines: make_filename_line(&name), content_hash: None, skip_reason: Some(reason) });
        return Ok(true);
    }
    let skip_reason = if let Err(ref e) = read_result {
        let msg = e.to_string();
        if msg.contains("ChecksumVerificationFailed") {
            warn!("7z: checksum mismatch for '{}': {}", name, e);
            Some(format!("checksum verification failed: {e}"))
        } else {
            let member_path = std::path::Path::new(&name);
            if find_extract_media::accepts(member_path) {
                tracing::debug!("7z: skipping binary entry '{}': {}", name, e);
                None
            } else {
                warn!("7z: failed to read entry '{}': {}", name, e);
                if bytes.is_empty() { Some(format!("failed to read: {e}")) } else { None }
            }
        }
    } else {
        None
    };
    let content_hash = if bytes.is_empty() { None } else { Some(blake3::hash(&bytes).to_hex().to_string()) };
    callback(MemberBatch { lines: extract_member_bytes(bytes, &name, display_prefix, cfg), content_hash, skip_reason });
    Ok(true)
}

fn sevenz_streaming(path: &Path, display_prefix: &str, cfg: &ExtractorConfig, callback: CB<'_>) -> Result<()> {
    use std::collections::HashSet;

    let size_limit = cfg.max_size_kb * 1024;

    // Parse the archive header to inspect block sizes before any decompression.
    // We open the file a second time for the data stream so we can skip blocks
    // whose LZMA dictionary allocation would exhaust memory.
    //
    // Root cause: sevenz_rust2 creates the LZMA decoder (allocating a dictionary
    // buffer proportional to the block's total unpack size) BEFORE calling our
    // per-file callback.  For a solid archive with a 128 MB block this allocates
    // ~128 MB regardless of our per-file size limit, and that allocation can fail
    // on memory-constrained systems (WSL2, NAS boxes, containers).
    let archive = {
        let mut f = File::open(path)?;
        sevenz_rust2::Archive::read(&mut f, &sevenz_rust2::Password::empty())
            .context("7z: failed to parse archive header")?
    };

    // Cap on a single solid block's total unpack size, taken from config.
    // The LZMA dictionary is sized to (roughly) the block unpack size,
    // so this bounds the largest single allocation we allow.
    let max_block_bytes = cfg.max_7z_solid_block_mb * 1024 * 1024;

    let oversized: HashSet<usize> = archive
        .blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.get_unpack_size() as usize > max_block_bytes)
        .map(|(i, _)| i)
        .collect();

    if !oversized.is_empty() {
        let skipped: usize = archive
            .stream_map
            .file_block_index
            .iter()
            .filter(|opt| opt.is_some_and(|bi| oversized.contains(&bi)))
            .count();
        warn!(
            "7z: '{}': {} solid block(s) exceed {} MB; {} file(s) will be indexed by filename only",
            path.display(),
            oversized.len(),
            cfg.max_7z_solid_block_mb,
            skipped,
        );
        // Emit a single summary failure (empty lines = applies to the outer archive path
        // in scan.rs) so the user can see why content is missing when they open this file.
        let largest_block_mb = oversized
            .iter()
            .map(|&bi| archive.blocks[bi].get_unpack_size() / (1024 * 1024))
            .max()
            .unwrap_or(0);
        callback(MemberBatch {
            lines: vec![],
            content_hash: None,
            skip_reason: Some(format!(
                "7z: {} file(s) in {} solid block(s) not extracted \
                 (largest block {} MB exceeds memory limit of {} MB); \
                 filenames indexed only",
                skipped, oversized.len(), largest_block_mb, cfg.max_7z_solid_block_mb,
            )),
        });
        // Emit filename-only entries for files in oversized blocks now,
        // before we start the decode loop (which would trigger the big allocation).
        for (file_idx, block_opt) in archive.stream_map.file_block_index.iter().enumerate() {
            if let Some(bi) = *block_opt {
                if oversized.contains(&bi) {
                    let entry = &archive.files[file_idx];
                    if !entry.is_directory() {
                        callback(MemberBatch {
                            lines: make_filename_line(entry.name()),
                            content_hash: None,
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }

    // Open the archive data stream for block-by-block extraction.
    // BlockDecoder::new is cheap (no decoder created yet); the LZMA decoder and
    // its dictionary are created lazily inside for_each_entries, which we skip
    // entirely for oversized blocks.
    let thread_count = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(1);
    let mut source = File::open(path)?;

    let password = sevenz_rust2::Password::empty();
    for block_index in 0..archive.blocks.len() {
        if oversized.contains(&block_index) {
            // Filename entries already emitted above; skip decoder creation.
            continue;
        }
        let block_dec = sevenz_rust2::BlockDecoder::new(
            thread_count,
            block_index,
            &archive,
            &password,
            &mut source,
        );
        block_dec.for_each_entries(&mut |entry, reader| {
            sevenz_process_entry(entry, reader, display_prefix, size_limit, cfg, callback)
        })?;
    }

    // Emit entries for files that have no associated block (empty files / dirs
    // that appear in the file list but have no data stream in the archive).
    for (file_idx, block_opt) in archive.stream_map.file_block_index.iter().enumerate() {
        if block_opt.is_none() {
            let entry = &archive.files[file_idx];
            if !entry.is_directory() {
                let empty: &mut dyn Read = &mut ([0u8; 0].as_slice());
                sevenz_process_entry(entry, empty, display_prefix, size_limit, cfg, callback)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
            }
        }
    }

    Ok(())
}

/// Extract a single-file compressed archive (bare .gz, .bz2, .xz).
/// Decompresses up to `cfg.max_size_kb` bytes and indexes the inner content.
fn single_compressed<R: Read>(reader: R, path: &Path, cfg: &ExtractorConfig) -> Result<MemberBatch> {
    let inner_name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();

    let size_limit = cfg.max_size_kb * 1024;
    let mut bytes = Vec::new();
    reader.take(size_limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > size_limit {
        return Ok(MemberBatch {
            lines: make_filename_line(&inner_name),
            content_hash: None,
            skip_reason: Some(format!("too large to index (limit {})", fmt_size(size_limit as u64))),
        });
    }

    let content_hash = if bytes.is_empty() { None } else { Some(blake3::hash(&bytes).to_hex().to_string()) };
    Ok(MemberBatch {
        lines: extract_member_bytes(bytes, &inner_name, path.to_str().unwrap_or(""), cfg),
        content_hash,
        skip_reason: None,
    })
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
    callback(MemberBatch { lines: make_filename_line(outer_name), content_hash: None, skip_reason: None });

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
    let mut prefixed = |inner_batch: MemberBatch| {
        let p: Vec<IndexLine> = inner_batch.lines
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
        callback(MemberBatch { lines: p, content_hash: inner_batch.content_hash, skip_reason: inner_batch.skip_reason });
    };

    // Use `reader` as `&mut dyn Read` throughout so that tar_streaming<GzDecoder<&mut dyn Read>>
    // is always the same monomorphisation regardless of nesting depth.
    let result: Result<()> = match kind {
        // ── Tar variants: stream directly, zero extra memory ─────────────
        ArchiveKind::TarGz  => tar_streaming(tar::Archive::new(GzDecoder::new(reader)), outer_name, &inner_cfg, &mut prefixed),
        ArchiveKind::TarBz2 => tar_streaming(tar::Archive::new(BzDecoder::new(reader)), outer_name, &inner_cfg, &mut prefixed),
        ArchiveKind::TarXz  => tar_streaming(tar::Archive::new(XzDecoder::new(reader)), outer_name, &inner_cfg, &mut prefixed),
        ArchiveKind::Tar    => tar_streaming(tar::Archive::new(reader), outer_name, &inner_cfg, &mut prefixed),

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
        return zip_from_archive(archive, outer_name, cfg, callback);
    }

    let archive = zip::ZipArchive::new(Cursor::new(bytes)).context("opening nested zip")?;
    zip_from_archive(archive, outer_name, cfg, callback)
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
    sevenz_streaming(tmp.path(), outer_name, cfg, callback)
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

/// Format a byte count as a human-readable size string.
fn fmt_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{} MB", bytes / (1024 * 1024))
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}

/// Extract an archive member from raw bytes.
///
/// Single-file compressed formats (.gz/.bz2/.xz) are decompressed inline and
/// dispatched via `find_extract_dispatch`.  All other non-archive formats are
/// dispatched directly.  Multi-file archives are NOT handled here — the
/// caller routes those through `handle_nested_archive` before reaching
/// this function.
fn extract_member_bytes(bytes: Vec<u8>, entry_name: &str, display_prefix: &str, cfg: &ExtractorConfig) -> Vec<IndexLine> {
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
                    let display_name = format!("{display_prefix}::{inner_name}");
                    let content_lines = find_extract_dispatch::dispatch_from_bytes(&inner_bytes, &display_name, cfg);
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
    let display_name = format!("{display_prefix}::{entry_name}");
    let content_lines = find_extract_dispatch::dispatch_from_bytes(&bytes, &display_name, cfg);
    let with_path = content_lines.into_iter().map(|mut l| {
        l.archive_path = Some(entry_name.to_string());
        l
    });
    lines.extend(with_path);
    lines
}
