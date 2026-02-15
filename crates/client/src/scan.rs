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
};

use crate::extract;

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
    let mut batch: Vec<IndexFile> = Vec::with_capacity(BATCH_SIZE);
    let mut batch_bytes: usize = 0;

    let max_archive_depth = scan.archives.max_depth;

    for abs_path in &to_index {
        let rel_path = relative_path(abs_path, paths);
        let mtime = mtime_of(abs_path).unwrap_or(0);
        let size = size_of(abs_path).unwrap_or(0);
        let kind = extract::detect_kind(abs_path).to_string();

        let lines = match extract::extract(abs_path, scan.max_file_size_kb, max_archive_depth) {
            Ok(l) => l,
            Err(e) => {
                warn!("extract {}: {e}", abs_path.display());
                vec![]
            }
        };

        // Group lines by archive_path. For non-archive files all archive_paths are None.
        // For archive files, each distinct archive_path becomes a separate IndexFile.
        let index_files = build_index_files(rel_path, mtime, size, kind, lines);

        for file in index_files {
            let file_bytes: usize = file.lines.iter().map(|l| l.content.len()).sum();
            batch_bytes += file_bytes;
            batch.push(file);

            if batch.len() >= BATCH_SIZE || batch_bytes >= BATCH_BYTES {
                submit_batch(api, source_name, base_url, &mut batch, vec![], None).await?;
                batch_bytes = 0;
            }
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

/// Convert extracted lines for one filesystem file into one or more IndexFiles.
///
/// For non-archive files: one IndexFile with path = rel_path.
/// For archive files: one IndexFile per distinct archive member (archive_path on the lines),
/// each with a composite path "rel_path::member_path". The outer archive file itself also
/// gets its own IndexFile so it's searchable by name.
fn build_index_files(
    rel_path: String,
    mtime: i64,
    size: i64,
    kind: String,
    lines: Vec<IndexLine>,
) -> Vec<IndexFile> {
    let has_archive_members = lines.iter().any(|l| l.archive_path.is_some());

    if !has_archive_members {
        // Non-archive (or archive with no extractable text members): single IndexFile.
        let mut all_lines = lines;
        // Always index the relative path so the file is findable by name.
        all_lines.push(IndexLine {
            archive_path: None,
            line_number: 0,
            content: rel_path.clone(),
        });
        return vec![IndexFile { path: rel_path, mtime, size, kind, lines: all_lines }];
    }

    // Group by archive_path.
    let mut member_groups: HashMap<String, Vec<IndexLine>> = HashMap::new();
    let mut outer_extra: Vec<IndexLine> = Vec::new();

    for line in lines {
        match line.archive_path.clone() {
            None => outer_extra.push(line),
            Some(member) => member_groups.entry(member).or_default().push(line),
        }
    }

    let mut result = Vec::new();

    // Outer file: searchable by path name.
    let mut outer_lines = outer_extra;
    outer_lines.push(IndexLine {
        archive_path: None,
        line_number: 0,
        content: rel_path.clone(),
    });
    result.push(IndexFile {
        path: rel_path.clone(),
        mtime,
        size,
        kind: kind.clone(),
        lines: outer_lines,
    });

    // One IndexFile per archive member, with composite path "zip::member".
    for (member, mut content_lines) in member_groups {
        let composite_path = format!("{}::{}", rel_path, member);
        // Strip archive_path from individual lines (redundant now that path is composite).
        for l in &mut content_lines {
            l.archive_path = None;
        }
        // Add a line_number=0 entry so the member is findable by name.
        content_lines.push(IndexLine {
            archive_path: None,
            line_number: 0,
            content: composite_path.clone(),
        });
        // Detect the member's actual kind from its filename, not the outer archive's kind.
        let member_kind = extract::detect_kind(std::path::Path::new(&member)).to_string();
        result.push(IndexFile {
            path: composite_path,
            mtime,
            size,
            kind: member_kind,
            lines: content_lines,
        });
    }

    result
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
