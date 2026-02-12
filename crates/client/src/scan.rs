use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use tracing::{info, warn};
use walkdir::WalkDir;

use find_common::{
    api::{DeleteRequest, IndexFile, UpsertRequest},
    config::ScanConfig,
    extract,
};

use crate::api::ApiClient;

const BATCH_SIZE: usize = 200;
const BATCH_BYTES: usize = 8 * 1024 * 1024; // 8 MB

pub async fn run_scan(
    api: &ApiClient,
    source_name: &str,
    paths: &[String],
    scan: &ScanConfig,
    full: bool,
) -> Result<()> {
    // Build exclusion GlobSet once.
    let excludes = build_globset(&scan.exclude)?;

    // Fetch what the server already knows about this source.
    info!("fetching existing file list from server...");
    let server_files: HashMap<String, i64> = api
        .list_files(source_name)
        .await?
        .into_iter()
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
    let mut batch: Vec<IndexFile> = Vec::with_capacity(BATCH_SIZE);
    let mut batch_bytes: usize = 0;

    for abs_path in &to_index {
        let rel_path = relative_path(abs_path, paths);
        let mtime = mtime_of(abs_path).unwrap_or(0);
        let size = size_of(abs_path).unwrap_or(0);
        let kind = extract::detect_kind(abs_path).to_string();

        let lines = match extract::extract(abs_path, scan.max_file_size_kb) {
            Ok(l) => l,
            Err(e) => {
                warn!("extract {}: {e}", abs_path.display());
                continue;
            }
        };

        // Skip files that yielded no content (binary, too large, etc.)
        if lines.is_empty() {
            continue;
        }

        let file_bytes: usize = lines.iter().map(|l| l.content.len()).sum();
        batch_bytes += file_bytes;
        batch.push(IndexFile {
            path: rel_path,
            mtime,
            size,
            kind,
            lines,
        });

        if batch.len() >= BATCH_SIZE || batch_bytes >= BATCH_BYTES {
            submit_batch(api, source_name, &mut batch).await?;
            batch_bytes = 0;
        }
    }
    if !batch.is_empty() {
        submit_batch(api, source_name, &mut batch).await?;
    }

    // Delete removed files.
    if !to_delete.is_empty() {
        info!("deleting {} removed files", to_delete.len());
        api.delete_files(&DeleteRequest {
            source: source_name.to_string(),
            paths: to_delete,
        })
        .await?;
    }

    // Record completion timestamp.
    let now = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    api.scan_complete(source_name, now).await?;

    info!("scan complete");
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
    let max_bytes = scan.max_file_size_kb * 1024;
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
                Err(e) => { warn!("walk error: {e}"); continue; }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            // Skip oversized files
            if let Ok(meta) = entry.metadata() {
                if meta.len() > max_bytes {
                    continue;
                }
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


async fn submit_batch(
    api: &ApiClient,
    source_name: &str,
    batch: &mut Vec<IndexFile>,
) -> Result<()> {
    let files = std::mem::take(batch);
    info!("submitting batch of {} files", files.len());
    api.upsert_files(&UpsertRequest {
        source: source_name.to_string(),
        files,
    })
    .await
}
