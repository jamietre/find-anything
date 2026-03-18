use std::path::Path;

use serde::Deserialize;
use tracing::{debug, error, info, warn};

use crate::api::IndexLine;
use crate::config::ExtractorConfig;

/// Minimal deserialization target for archive subprocess output.
/// We only need the `lines` field from `MemberBatch` to avoid a circular
/// dependency on `find-extract-archive`.
#[derive(Deserialize)]
struct BatchLines {
    lines: Vec<IndexLine>,
}

/// Extract content from any file via the appropriate subprocess, returning a
/// flat `Vec<IndexLine>`.
///
/// For archive files the subprocess outputs `Vec<MemberBatch>` (each with a
/// `lines` field); these are flattened into a single vec.  For all other
/// formats the subprocess outputs `Vec<IndexLine>` directly.
///
/// Returns an empty vec on subprocess failure (error is already logged).
pub async fn extract_lines_via_subprocess(
    abs_path: &Path,
    cfg: &ExtractorConfig,
    extractor_dir: &Option<String>,
) -> Vec<IndexLine> {
    let binary = extractor_binary_for(abs_path, extractor_dir);
    let max_content_kb = (cfg.max_content_kb).to_string();
    let max_depth = cfg.max_depth.to_string();
    let max_line_length = cfg.max_line_length.to_string();

    let ext = abs_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Detect archive and pdf by extension to select the right argument layout.
    let is_archive = matches!(
        ext.as_str(),
        "zip" | "tar" | "gz" | "bz2" | "xz" | "tgz" | "tbz2" | "txz" | "7z"
    );
    let is_pdf = ext == "pdf";

    let mut cmd = tokio::process::Command::new(&binary);
    cmd.arg(abs_path).arg(&max_content_kb);
    if is_archive {
        // find-extract-archive: <path> [max-content-kb] [max-depth] [max-line-length]
        cmd.arg(&max_depth).arg(&max_line_length);
    } else if is_pdf {
        // find-extract-pdf: <path> [max-content-kb] [max-line-length]
        cmd.arg(&max_line_length);
    }

    match cmd.output().await {
        Ok(out) => {
            relay_subprocess_logs(&out.stderr, &abs_path.to_string_lossy());
            if out.status.success() {
                if is_archive {
                    // Parse Vec<MemberBatch> minimally — only extract `lines`.
                    let batches: Vec<BatchLines> =
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

/// Resolve the name of the extractor subprocess binary for a given file path.
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

/// Re-emit subprocess stderr lines through our tracing subscriber so they
/// appear in the parent process output at the correct level and pass through
/// the same log-ignore filters as in-process events.
///
/// `file` is the path of the file being extracted — included in every log
/// line so errors can be traced back to the source file.
///
/// tracing-subscriber fmt (no time, no ANSI) formats lines as:
///   `{LEVEL} {target}: {message}`
/// We parse the level prefix and re-emit accordingly.
pub fn relay_subprocess_logs(stderr: &[u8], file: &str) {
    let text = std::string::String::from_utf8_lossy(stderr);
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Parse the level prefix emitted by tracing-subscriber fmt.
        // Typical format: "WARN target: message" or "ERROR target: message".
        let rest = line.trim_start_matches(|c: char| !c.is_alphanumeric());
        if let Some(msg) = rest.strip_prefix("ERROR ") {
            error!(target: "subprocess", file, "{msg}");
        } else if let Some(msg) = rest.strip_prefix("WARN ") {
            warn!(target: "subprocess", file, "{msg}");
        } else if let Some(msg) = rest.strip_prefix("INFO ") {
            info!(target: "subprocess", file, "{msg}");
        } else if let Some(msg) = rest.strip_prefix("DEBUG ") {
            debug!(target: "subprocess", file, "{msg}");
        } else if let Some(msg) = rest.strip_prefix("TRACE ") {
            debug!(target: "subprocess", file, "{msg}");
        } else {
            // Unknown format — emit as warn so it's not silently dropped.
            warn!(target: "subprocess", file, "{line}");
        }
    }
}
