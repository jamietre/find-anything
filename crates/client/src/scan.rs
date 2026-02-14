use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use tracing::{info, warn};
use walkdir::WalkDir;

use find_common::{
    api::{BulkRequest, IndexFile, IndexLine},
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
    base_url: Option<&str>,
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

        let mut lines = match extract::extract(abs_path, scan.max_file_size_kb) {
            Ok(l) => l,
            Err(e) => {
                warn!("extract {}: {e}", abs_path.display());
                vec![]
            }
        };

        // Always index the relative path (line_number 0) so every file is
        // findable by name/path even if it has no other extractable content.
        lines.push(IndexLine {
            archive_path: None,
            line_number: 0,
            content: rel_path.clone(),
        });

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
            submit_batch(api, source_name, base_url, &mut batch, vec![], None).await?;
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
    submit_batch(api, source_name, base_url, &mut batch, to_delete, Some(now)).await?;

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
    base_url: Option<&str>,
    batch: &mut Vec<IndexFile>,
    delete_paths: Vec<String>,
    scan_timestamp: Option<i64>,
) -> Result<()> {
    let files = std::mem::take(batch);
    if !files.is_empty() {
        info!("submitting batch of {} files", files.len());
    }
    api.bulk(&BulkRequest {
        source: source_name.to_string(),
        files,
        delete_paths,
        base_url: base_url.map(|s| s.to_string()),
        scan_timestamp,
    })
    .await
}
