use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use tracing::{info, warn};
use walkdir::WalkDir;

use find_common::{
    api::{IndexFile, IndexLine, IndexingFailure},
    config::{ExtractorConfig, ScanConfig},
};

use crate::api::ApiClient;
use crate::batch::{build_index_files, build_member_index_files, submit_batch};
use crate::extract;

use find_extract_archive;

const BATCH_SIZE: usize = 200;
const BATCH_BYTES: usize = 8 * 1024 * 1024; // 8 MB
const MAX_FAILURES_PER_BATCH: usize = 100;
const MAX_ERROR_LEN: usize = 500;

pub async fn run_scan(
    api: &ApiClient,
    source_name: &str,
    paths: &[String],
    scan: &ScanConfig,
    base_url: Option<&str>,
    full: bool,
) -> Result<()> {
    // Build exclusion GlobSet once.
    let excludes = build_globset(&scan.exclude)?;

    // Fetch what the server already knows about this source.
    // Only consider outer files (no "::" in path) for deletion/mtime comparison;
    // inner archive members are managed server-side.
    info!("fetching existing file list from server...");
    let server_files: HashMap<String, i64> = api
        .list_files(source_name)
        .await?
        .into_iter()
        .filter(|f| !f.path.contains("::"))
        .map(|f| (f.path, f.mtime))
        .collect();

    // Walk all configured paths and build the local file map.
    info!("walking filesystem...");
    let local_files = walk_paths(paths, scan, &excludes);

    // Compute sets.
    let server_paths: HashSet<&str> = server_files.keys().map(|s| s.as_str()).collect();
    let local_paths: HashSet<&str> = local_files.keys().map(|s| s.as_str()).collect();

    let to_delete: Vec<String> = server_paths
        .difference(&local_paths)
        .map(|s| s.to_string())
        .collect();

    let to_index: Vec<&PathBuf> = local_files
        .iter()
        .filter(|(rel_path, abs_path)| {
            if full {
                return true;
            }
            let server_mtime = server_files.get(*rel_path).copied();
            let local_mtime  = mtime_of(abs_path).unwrap_or(0);
            match server_mtime {
                None     => true,              // new file not yet in index
                Some(sm) => local_mtime > sm,  // file modified since last index
            }
        })
        .map(|(_, abs)| abs)
        .collect();

    info!(
        "{} files to index, {} to delete",
        to_index.len(),
        to_delete.len()
    );

    // Index in batches.
    let total = to_index.len();
    let mut completed: usize = 0;
    let mut batch: Vec<IndexFile> = Vec::with_capacity(BATCH_SIZE);
    let mut batch_bytes: usize = 0;
    let mut failures: Vec<IndexingFailure> = Vec::new();

    let cfg = ExtractorConfig::from_scan(scan);

    for abs_path in &to_index {
        let rel_path = relative_path(abs_path, paths);
        let mtime = mtime_of(abs_path).unwrap_or(0);
        let size = size_of(abs_path).unwrap_or(0);
        let kind = extract::detect_kind(abs_path).to_string();
        let t0 = std::time::Instant::now();

        completed += 1;

        if find_extract_archive::accepts(abs_path) {
            // ── Streaming archive extraction ─────────────────────────────────
            // Members are processed one at a time via a bounded channel so that
            // lines are freed after each member is converted to an IndexFile,
            // rather than holding the entire archive's content in memory.
            info!("extracting archive {rel_path} ({completed}/{total})");

            // Submit the outer archive file first, before any member batches.
            // The server deletes stale inner members when it sees the outer file,
            // so it must arrive before member batches — not after them.
            let outer_file = IndexFile {
                path: rel_path.clone(),
                mtime,
                size,
                kind,
                lines: vec![IndexLine { archive_path: None, line_number: 0, content: rel_path.clone() }],
                extract_ms: None,
            };
            batch.push(outer_file);
            submit_batch(api, source_name, base_url, &mut batch, &mut failures, vec![], None).await?;
            batch_bytes = 0;

            let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<IndexLine>>(16);
            let abs_clone: std::path::PathBuf = (*abs_path).clone(); // owned — required by spawn_blocking 'static
            let cfg_clone = cfg;

            let extract_task = tokio::task::spawn_blocking(move || {
                find_extract_archive::extract_streaming(&abs_clone, &cfg_clone, &mut |member_lines| {
                    // blocking_send provides backpressure; ignore errors (scan cancelled).
                    let _ = tx.blocking_send(member_lines);
                })
            });

            let mut members_submitted: usize = 0;
            while let Some(member_lines) = rx.recv().await {
                // Apply exclude patterns to archive members.
                // archive_path may be "inner.zip::path/to/file.js" for nested archives;
                // take the last segment (actual file path) for glob matching.
                if let Some(ap) = member_lines.first().and_then(|l| l.archive_path.as_deref()) {
                    let file_path = ap.rsplit("::").next().unwrap_or(ap);
                    if excludes.is_match(file_path) {
                        continue;
                    }
                }
                for file in build_member_index_files(&rel_path, mtime, size, member_lines) {
                    let file_bytes: usize = file.lines.iter().map(|l| l.content.len()).sum();
                    batch_bytes += file_bytes;
                    members_submitted += 1;
                    batch.push(file);
                    if batch.len() >= BATCH_SIZE || batch_bytes >= BATCH_BYTES {
                        info!("submitting batch — extracting {rel_path} ({} members, {} total)", batch.len(), members_submitted);
                        submit_batch(api, source_name, base_url, &mut batch, &mut failures, vec![], None).await?;
                        batch_bytes = 0;
                    }
                }
            }

            match extract_task.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    let msg = format!("{e:#}");
                    let truncated = truncate_error(&msg, MAX_ERROR_LEN);
                    warn!("extract {}: {}", abs_path.display(), truncated);
                    if failures.len() < MAX_FAILURES_PER_BATCH {
                        failures.push(IndexingFailure { path: rel_path.clone(), error: truncated });
                    }
                }
                Err(e) => warn!("extract task panicked for {}: {e}", abs_path.display()),
            }
        } else {
            // ── Non-archive extraction ───────────────────────────────────────
            // dispatch_from_path handles MIME detection internally: it emits a
            // [FILE:mime] line when no extractor matched the bytes, so we check
            // for that line below to update the kind accordingly.
            let lines = match extract::extract(abs_path, &cfg) {
                Ok(l) => l,
                Err(e) => {
                    let msg = format!("{e:#}");
                    let truncated = truncate_error(&msg, MAX_ERROR_LEN);
                    warn!("extract {}: {}", abs_path.display(), truncated);
                    if failures.len() < MAX_FAILURES_PER_BATCH {
                        failures.push(IndexingFailure { path: rel_path.clone(), error: truncated });
                    }
                    vec![]
                }
            };
            // Refine "unknown" or "text" kind using extracted content:
            // - A [FILE:mime] line emitted by dispatch means binary → use mime_to_kind.
            // - Text content lines (line_number > 0) present → promote to "text".
            // - Neither → keep as-is (archive members use "unknown" when unrecognised).
            let kind = if kind == "text" || kind == "unknown" {
                if let Some(mime_line) = lines.iter().find(|l| l.line_number == 0 && l.content.starts_with("[FILE:mime] ")) {
                    let mime = &mime_line.content["[FILE:mime] ".len()..];
                    find_extract_dispatch::mime_to_kind(mime).to_string()
                } else if lines.iter().any(|l| l.line_number > 0) {
                    "text".to_string()
                } else {
                    kind
                }
            } else {
                kind
            };
            let extract_ms = t0.elapsed().as_millis() as u64;
            let mut index_files = build_index_files(rel_path, mtime, size, kind, lines);
            if let Some(f) = index_files.first_mut() {
                f.extract_ms = Some(extract_ms);
            }
            for file in index_files {
                let file_bytes: usize = file.lines.iter().map(|l| l.content.len()).sum();
                batch_bytes += file_bytes;
                batch.push(file);
                if batch.len() >= BATCH_SIZE || batch_bytes >= BATCH_BYTES {
                    info!("submitting batch — {completed}/{total} files completed");
                    submit_batch(api, source_name, base_url, &mut batch, &mut failures, vec![], None).await?;
                    batch_bytes = 0;
                }
            }
        }

        if batch.len() >= BATCH_SIZE || batch_bytes >= BATCH_BYTES {
            info!("submitting batch — {completed}/{total} files completed");
            submit_batch(api, source_name, base_url, &mut batch, &mut failures, vec![], None).await?;
            batch_bytes = 0;
        }
    }

    // Final batch: remaining files + all deletes + scan-complete timestamp.
    let now = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    if !to_delete.is_empty() {
        info!("deleting {} removed files", to_delete.len());
    }
    submit_batch(api, source_name, base_url, &mut batch, &mut failures, to_delete, Some(now)).await?;

    info!("scan complete — {total} files indexed");
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn build_globset(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pat in patterns {
        builder.add(Glob::new(pat)?);
        // For patterns like **/node_modules/**, also add **/node_modules so that
        // the directory entry itself is excluded and walkdir won't descend into it.
        if let Some(dir_pat) = pat.strip_suffix("/**") {
            builder.add(Glob::new(dir_pat)?);
        }
    }
    Ok(builder.build()?)
}

/// Returns a map of relative_path → absolute_path for all files under `paths`.
fn walk_paths(
    paths: &[String],
    scan: &ScanConfig,
    excludes: &GlobSet,
) -> HashMap<String, PathBuf> {
    let mut map = HashMap::new();

    for root_str in paths {
        let root = Path::new(root_str);
        for entry in WalkDir::new(root)
            .follow_links(scan.follow_symlinks)
            .into_iter()
            .filter_entry(|e| {
                // Hidden files
                if !scan.include_hidden {
                    if let Some(name) = e.file_name().to_str() {
                        if name.starts_with('.') && e.depth() > 0 {
                            return false;
                        }
                    }
                }
                // Exclusion globs (match relative to root)
                if let Ok(rel) = e.path().strip_prefix(root) {
                    if excludes.is_match(rel) {
                        return false;
                    }
                }
                true
            })
        {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => { warn!("walk error: {e:#}"); continue; }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let abs = entry.path().to_path_buf();
            let rel = relative_path(&abs, paths);
            map.insert(rel, abs);
        }
    }
    map
}

fn relative_path(abs: &Path, roots: &[String]) -> String {
    for root in roots {
        if let Ok(rel) = abs.strip_prefix(root) {
            return rel.to_string_lossy().to_string();
        }
    }
    abs.to_string_lossy().to_string()
}

fn mtime_of(path: &Path) -> Option<i64> {
    path.metadata()
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs() as i64)
}

fn size_of(path: &Path) -> Option<i64> {
    path.metadata().ok().map(|m| m.len() as i64)
}

/// Truncate `s` to at most `max` bytes at a UTF-8 char boundary, appending `…` if truncated.
fn truncate_error(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    // Walk back from `max` to find a valid char boundary.
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

