use std::path::Path;

use tracing::{debug, error, info, warn};

use find_common::{api::IndexLine, config::ScanConfig};
use find_extract_archive::MemberBatch;

/// Extract content from any file via the appropriate subprocess.
///
/// For archive files, parses `Vec<MemberBatch>` from the binary and flattens to
/// `Vec<IndexLine>` so the caller receives a flat list identical to the pre-subprocess
/// result.  For all other formats, parses `Vec<IndexLine>` directly.
///
/// On subprocess failure or parse error, returns an empty vec (the error is
/// logged as a warning so the scan can continue with other files).
pub async fn extract_via_subprocess(
    abs_path: &Path,
    scan: &ScanConfig,
    extractor_dir: &Option<String>,
) -> Vec<IndexLine> {
    let binary = extractor_binary_for(abs_path, extractor_dir);
    let max_size_kb = (scan.max_file_size_mb * 1024).to_string();
    let max_depth = scan.archives.max_depth.to_string();
    let max_line_length = scan.max_line_length.to_string();

    let ext = abs_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let is_archive = find_extract_archive::is_archive_ext(&ext);
    let is_pdf = ext == "pdf";

    let mut cmd = tokio::process::Command::new(&binary);
    cmd.arg(abs_path).arg(&max_size_kb);
    if is_archive {
        // find-extract-archive: <path> [max-size-kb] [max-depth] [max-line-length]
        cmd.arg(&max_depth).arg(&max_line_length);
    } else if is_pdf {
        // find-extract-pdf: <path> [max-size-kb] [max-line-length]
        cmd.arg(&max_line_length);
    }

    match cmd.output().await {
        Ok(out) => {
            relay_subprocess_logs(&out.stderr);
            if out.status.success() {
                if is_archive {
                    let batches: Vec<MemberBatch> =
                        serde_json::from_slice(&out.stdout).unwrap_or_default();
                    batches.into_iter().flat_map(|b| b.lines).collect()
                } else {
                    serde_json::from_slice::<Vec<IndexLine>>(&out.stdout).unwrap_or_default()
                }
            } else {
                warn!(
                    "extractor {} exited {:?} for {}",
                    binary,
                    out.status.code(),
                    abs_path.display()
                );
                vec![]
            }
        }
        Err(e) => {
            warn!("failed to run extractor {}: {e:#}", binary);
            vec![]
        }
    }
}

/// Extract content from an archive file via subprocess, returning the full
/// `Vec<MemberBatch>` with content hashes and skip reasons intact.
///
/// Used by `find-scan`'s archive path, which needs per-member metadata for
/// deduplication and failure reporting.
#[allow(dead_code)]
pub async fn extract_archive_via_subprocess(
    abs_path: &Path,
    scan: &ScanConfig,
    extractor_dir: &Option<String>,
) -> Vec<MemberBatch> {
    let binary = extractor_binary_for(abs_path, extractor_dir);
    let max_size_kb = (scan.max_file_size_mb * 1024).to_string();
    let max_depth = scan.archives.max_depth.to_string();
    let max_line_length = scan.max_line_length.to_string();

    let mut cmd = tokio::process::Command::new(&binary);
    cmd.arg(abs_path)
        .arg(&max_size_kb)
        .arg(&max_depth)
        .arg(&max_line_length);

    match cmd.output().await {
        Ok(out) => {
            relay_subprocess_logs(&out.stderr);
            if out.status.success() {
                serde_json::from_slice::<Vec<MemberBatch>>(&out.stdout).unwrap_or_default()
            } else {
                warn!(
                    "extractor {} exited {:?} for {}",
                    binary,
                    out.status.code(),
                    abs_path.display()
                );
                vec![]
            }
        }
        Err(e) => {
            warn!("failed to run extractor {}: {e:#}", binary);
            vec![]
        }
    }
}

/// Re-emit subprocess stderr lines through our tracing subscriber so they
/// appear in the parent process output at the correct level and pass through
/// the same log-ignore filters as in-process events.
///
/// tracing-subscriber fmt (no time, no ANSI) formats lines as:
///   `{LEVEL} {target}: {message}`
/// We parse the level prefix and re-emit accordingly.
pub fn relay_subprocess_logs(stderr: &[u8]) {
    let text = String::from_utf8_lossy(stderr);
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Parse the level prefix emitted by tracing-subscriber fmt.
        // Typical format: "WARN target: message" or "ERROR target: message".
        let rest = line.trim_start_matches(|c: char| !c.is_alphanumeric());
        if let Some(msg) = rest.strip_prefix("ERROR ") {
            error!(target: "subprocess", "{msg}");
        } else if let Some(msg) = rest.strip_prefix("WARN ") {
            warn!(target: "subprocess", "{msg}");
        } else if let Some(msg) = rest.strip_prefix("INFO ") {
            info!(target: "subprocess", "{msg}");
        } else if let Some(msg) = rest.strip_prefix("DEBUG ") {
            debug!(target: "subprocess", "{msg}");
        } else if let Some(msg) = rest.strip_prefix("TRACE ") {
            debug!(target: "subprocess", "{msg}");
        } else {
            // Unknown format — emit as warn so it's not silently dropped.
            warn!(target: "subprocess", "{line}");
        }
    }
}

pub fn extractor_binary_for(path: &Path, extractor_dir: &Option<String>) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let name = match ext.as_str() {
        "zip" | "tar" | "gz" | "bz2" | "xz" | "tgz" | "tbz2" | "txz" | "7z" => {
            "find-extract-archive"
        }
        "pdf" => "find-extract-pdf",
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "ico" | "webp" | "heic"
        | "tiff" | "tif" | "raw" | "cr2" | "nef" | "arw"
        | "mp3" | "flac" | "ogg" | "m4a" | "aac" | "wav" | "wma" | "opus"
        | "mp4" | "mkv" | "avi" | "mov" | "wmv" | "webm" | "m4v" | "flv" => {
            "find-extract-media"
        }
        "html" | "htm" | "xhtml" => "find-extract-html",
        "docx" | "xlsx" | "xls" | "xlsm" | "pptx" => "find-extract-office",
        "epub" => "find-extract-epub",
        _ => "find-extract-text",
    };

    // Resolution order:
    // 1. configured extractor_dir / name
    // 2. same dir as current executable / name
    // 3. name (rely on PATH)
    if let Some(dir) = extractor_dir {
        return format!("{}/{}", dir, name);
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(name);
            if candidate.exists() {
                return candidate.to_string_lossy().to_string();
            }
        }
    }

    name.to_string()
}
