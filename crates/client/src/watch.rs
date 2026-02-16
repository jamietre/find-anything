use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tracing::{info, warn};

use find_common::{
    api::{detect_kind_from_ext, BulkRequest, IndexLine},
    config::{ClientConfig, SourceConfig},
};

use crate::api::ApiClient;
use crate::batch::build_index_files;

/// (root_path, source_name, root_str)
type SourceMap = Vec<(PathBuf, String, String)>;

/// What to do with a path after debounce.
#[derive(Debug)]
enum AccumulatedKind {
    Update,
    Delete,
}

pub async fn run_watch(config: &ClientConfig) -> Result<()> {
    let api = ApiClient::new(&config.server.url, &config.server.token);
    let source_map = build_source_map(&config.sources);

    if source_map.is_empty() {
        anyhow::bail!("no source paths configured");
    }

    info!("find-watch starting — watching {} source(s):", config.sources.len());
    for src in &config.sources {
        info!("  source {:?}: {:?}", src.name, src.paths);
    }

    let excludes = build_globset(&config.scan.exclude)?;
    let debounce_ms = config.watch.debounce_ms;

    // Channel: notify (blocking thread) → tokio event loop.
    let (tx, mut rx) = mpsc::channel::<notify::Result<Event>>(1000);

    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.blocking_send(res);
        },
        notify::Config::default(),
    )?;

    for (root, _, _) in &source_map {
        watcher.watch(root, RecursiveMode::Recursive)?;
        info!("watching {:?}", root);
    }

    // Debounce accumulator: path → what to do.
    let mut pending: HashMap<PathBuf, AccumulatedKind> = HashMap::new();

    loop {
        // Wait for the first event (or a timeout to flush pending).
        let timeout_dur = tokio::time::Duration::from_millis(debounce_ms);

        let got_event = if pending.is_empty() {
            // Nothing pending — block indefinitely.
            match rx.recv().await {
                Some(ev) => { accumulate(&mut pending, ev); true }
                None => break, // channel closed
            }
        } else {
            // Events pending — wait up to debounce_ms for another.
            match tokio::time::timeout(timeout_dur, rx.recv()).await {
                Ok(Some(ev)) => { accumulate(&mut pending, ev); true }
                Ok(None)     => break, // channel closed
                Err(_)       => false, // timeout — time to flush
            }
        };

        if got_event {
            // Drain any immediately-available events (non-blocking).
            loop {
                match rx.try_recv() {
                    Ok(ev) => accumulate(&mut pending, ev),
                    Err(_) => break,
                }
            }
            // Reset debounce window: go back to the top of the loop.
            // The pending block will now wait debounce_ms again.
            continue;
        }

        // Flush accumulated events.
        let batch = std::mem::take(&mut pending);
        for (abs_path, kind) in batch {
            // Skip paths that contain '::' — those are archive member paths
            // managed server-side, not real filesystem paths.
            let path_str = abs_path.to_string_lossy();
            if path_str.contains("::") {
                continue;
            }

            // Find which source this file belongs to.
            let Some((source_name, rel_path)) = find_source(&abs_path, &source_map) else {
                continue;
            };

            // Apply exclusion globs.
            if is_excluded(&abs_path, &source_map, &excludes) {
                continue;
            }

            let source_cfg = config.sources.iter().find(|s| s.name == source_name);
            let base_url = source_cfg.and_then(|s| s.base_url.as_deref());

            match kind {
                AccumulatedKind::Update => {
                    // Only process if it exists and is a regular file.
                    if !abs_path.is_file() {
                        continue;
                    }
                    if let Err(e) = handle_update(
                        &api,
                        &source_name,
                        &abs_path,
                        &rel_path,
                        base_url,
                        config,
                    )
                    .await
                    {
                        warn!("update {}: {e}", abs_path.display());
                    }
                }
                AccumulatedKind::Delete => {
                    if let Err(e) = handle_delete(&api, &source_name, &rel_path, base_url).await {
                        warn!("delete {}: {e}", abs_path.display());
                    }
                }
            }
        }
    }

    Ok(())
}

// ── Source map ────────────────────────────────────────────────────────────────

fn build_source_map(sources: &[SourceConfig]) -> SourceMap {
    let mut map = Vec::new();
    for src in sources {
        for root_str in &src.paths {
            let root = PathBuf::from(root_str);
            map.push((root, src.name.clone(), root_str.clone()));
        }
    }
    map
}

/// Return (source_name, rel_path) for a given absolute path.
/// Picks the most-specific (longest) matching root.
fn find_source<'a>(path: &Path, map: &'a SourceMap) -> Option<(String, String)> {
    let mut best: Option<(&PathBuf, &String, &String)> = None;
    for (root, name, root_str) in map {
        if path.starts_with(root) {
            if best.map_or(true, |(b, _, _)| root.as_os_str().len() > b.as_os_str().len()) {
                best = Some((root, name, root_str));
            }
        }
    }
    best.map(|(root, name, _)| {
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        (name.clone(), rel)
    })
}

// ── Exclusion ─────────────────────────────────────────────────────────────────

fn build_globset(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pat in patterns {
        builder.add(Glob::new(pat)?);
        if let Some(dir_pat) = pat.strip_suffix("/**") {
            builder.add(Glob::new(dir_pat)?);
        }
    }
    Ok(builder.build()?)
}

fn is_excluded(abs_path: &Path, source_map: &SourceMap, excludes: &GlobSet) -> bool {
    // Find the root for this path and check relative path against excludes.
    for (root, _, _) in source_map {
        if let Ok(rel) = abs_path.strip_prefix(root) {
            if excludes.is_match(rel) {
                return true;
            }
        }
    }
    false
}

// ── Event accumulation ────────────────────────────────────────────────────────

fn accumulate(pending: &mut HashMap<PathBuf, AccumulatedKind>, res: notify::Result<Event>) {
    let event = match res {
        Ok(e) => e,
        Err(e) => { warn!("watch error: {e}"); return; }
    };

    for path in event.paths {
        let new_kind = match &event.kind {
            EventKind::Create(_) => AccumulatedKind::Update,
            EventKind::Modify(notify::event::ModifyKind::Data(_)) => AccumulatedKind::Update,
            EventKind::Modify(notify::event::ModifyKind::Name(_)) => {
                // Renames: notify sends From path as Remove-like and To path as Create-like,
                // but both arrive as Modify(Name). We treat each independently:
                // if the file now exists → Update, otherwise → Delete.
                if path.exists() {
                    AccumulatedKind::Update
                } else {
                    AccumulatedKind::Delete
                }
            }
            EventKind::Remove(_) => AccumulatedKind::Delete,
            // Ignore access, metadata-only modify, other events.
            _ => continue,
        };

        match pending.entry(path) {
            Entry::Occupied(mut occ) => {
                // Collapse: Update→Delete = Delete, Delete→Update = Update.
                let existing = occ.get_mut();
                *existing = match (&*existing, &new_kind) {
                    (AccumulatedKind::Update, AccumulatedKind::Delete) => AccumulatedKind::Delete,
                    (AccumulatedKind::Delete, AccumulatedKind::Update) => AccumulatedKind::Update,
                    _ => new_kind,
                };
            }
            Entry::Vacant(vac) => {
                vac.insert(new_kind);
            }
        }
    }
}

// ── File handling ─────────────────────────────────────────────────────────────

async fn handle_update(
    api: &ApiClient,
    source_name: &str,
    abs_path: &Path,
    rel_path: &str,
    base_url: Option<&str>,
    config: &ClientConfig,
) -> Result<()> {
    info!("update: {}", rel_path);

    let lines = extract_via_subprocess(abs_path, config).await;

    let mtime = mtime_of(abs_path).unwrap_or(0);
    let size = size_of(abs_path).unwrap_or(0);
    let ext = abs_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let kind = detect_kind_from_ext(ext).to_string();

    let mut files = build_index_files(rel_path.to_string(), mtime, size, kind, lines);

    api.bulk(&BulkRequest {
        source: source_name.to_string(),
        files: std::mem::take(&mut files),
        delete_paths: vec![],
        base_url: base_url.map(|s| s.to_string()),
        scan_timestamp: None,
    })
    .await
}

async fn handle_delete(
    api: &ApiClient,
    source_name: &str,
    rel_path: &str,
    base_url: Option<&str>,
) -> Result<()> {
    info!("delete: {}", rel_path);

    api.bulk(&BulkRequest {
        source: source_name.to_string(),
        files: vec![],
        delete_paths: vec![rel_path.to_string()],
        base_url: base_url.map(|s| s.to_string()),
        scan_timestamp: None,
    })
    .await
}

// ── Subprocess extraction ─────────────────────────────────────────────────────

async fn extract_via_subprocess(abs_path: &Path, config: &ClientConfig) -> Vec<IndexLine> {
    let binary = extractor_binary_for(abs_path, &config.watch.extractor_dir);
    let max_size_kb = config.scan.max_file_size_kb.to_string();
    let max_depth = config.scan.archives.max_depth.to_string();

    let ext = abs_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let is_archive = matches!(
        ext.as_str(),
        "zip" | "tar" | "gz" | "bz2" | "xz" | "tgz" | "tbz2" | "txz" | "7z"
    );

    let mut cmd = tokio::process::Command::new(&binary);
    cmd.arg(abs_path).arg(&max_size_kb);
    if is_archive {
        cmd.arg(&max_depth);
    }

    match cmd.output().await {
        Ok(out) if out.status.success() => {
            serde_json::from_slice::<Vec<IndexLine>>(&out.stdout).unwrap_or_default()
        }
        Ok(out) => {
            warn!(
                "extractor {} exited {:?} for {}",
                binary,
                out.status.code(),
                abs_path.display()
            );
            vec![]
        }
        Err(e) => {
            warn!("failed to run extractor {}: {e}", binary);
            vec![]
        }
    }
}

fn extractor_binary_for(path: &Path, extractor_dir: &Option<String>) -> String {
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
        _ => "find-extract-text",
    };

    // Resolution order:
    // 1. config.watch.extractor_dir / name
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

// ── Filesystem helpers ────────────────────────────────────────────────────────

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
