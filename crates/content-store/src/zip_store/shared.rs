// Moved from find-server crates/server/src/archive.rs — SharedArchiveState.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// State shared across all `ArchiveManager` instances (i.e. all workers).
///
/// - `next_archive_num` is an atomic counter; each worker atomically claims a
///   unique number and owns that archive exclusively for appending.
/// - `rewrite_locks` is a per-archive mutex registry used only during rewrite
///   operations (chunk removal for re-indexing / deletion).
/// - `source_locks` serialises SQLite writes per source during the transition
///   period (removed when archive_batch.rs is fully ported to ContentStore).
pub struct SharedArchiveState {
    pub(crate) data_dir: PathBuf,
    pub(crate) next_archive_num: AtomicU32,
    pub(crate) total_archives: AtomicU64,
    pub(crate) archive_size_bytes: AtomicU64,
    pub(crate) rewrite_locks: Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>,
    /// Per-source write serialisation lock used by the old archive_batch path.
    /// Will be removed once archive_batch is ported to ContentStore.
    pub(crate) source_locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl SharedArchiveState {
    /// Initialise shared state for `data_dir`, scanning the existing content
    /// directory to seed the counter and populate running totals.
    pub fn new(data_dir: PathBuf) -> anyhow::Result<Arc<Self>> {
        let (max_num, total_archives, archive_size_bytes) = Self::scan_archives(&data_dir);
        Ok(Arc::new(Self {
            data_dir,
            next_archive_num:   AtomicU32::new(max_num.saturating_add(1)),
            total_archives:     AtomicU64::new(total_archives),
            archive_size_bytes: AtomicU64::new(archive_size_bytes),
            rewrite_locks:      Mutex::new(HashMap::new()),
            source_locks:       Mutex::new(HashMap::new()),
        }))
    }

    /// Running count of archive ZIP files (updated incrementally).
    pub fn total_archives(&self) -> u64 {
        self.total_archives.load(Ordering::Relaxed)
    }

    /// Running sum of archive ZIP on-disk sizes in bytes (updated incrementally).
    pub fn archive_size_bytes(&self) -> u64 {
        self.archive_size_bytes.load(Ordering::Relaxed)
    }

    fn scan_archives(data_dir: &Path) -> (u32, u64, u64) {
        let content_dir = data_dir.join("sources").join("content");
        let mut max_num = 0u32;
        let mut count = 0u64;
        let mut size_bytes = 0u64;
        if let Ok(entries) = std::fs::read_dir(&content_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    if let Ok(subdir) = std::fs::read_dir(entry.path()) {
                        for file_entry in subdir.flatten() {
                            if let Some(name) = file_entry.file_name().to_str() {
                                if let Some(num) = parse_archive_number(name) {
                                    max_num = max_num.max(num as u32);
                                    count += 1;
                                    size_bytes +=
                                        file_entry.metadata().map(|m| m.len()).unwrap_or(0);
                                }
                            }
                        }
                    }
                }
            }
        }
        (max_num, count, size_bytes)
    }

    /// Atomically claim the next archive number.
    pub fn allocate_archive_num(&self) -> u32 {
        self.next_archive_num.fetch_add(1, Ordering::Relaxed)
    }

    /// Return (or lazily create) the per-archive rewrite lock for `path`.
    pub fn rewrite_lock_for(&self, path: &Path) -> Arc<Mutex<()>> {
        let mut locks = self.rewrite_locks.lock().unwrap();
        locks
            .entry(path.to_path_buf())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Return (or lazily create) the per-source write serialisation lock.
    pub fn source_lock(&self, source: &str) -> Arc<Mutex<()>> {
        let mut locks = self.source_locks.lock().unwrap();
        locks
            .entry(source.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Compute the on-disk path for a given archive number.
    pub fn archive_path_for_number(&self, archive_num: u32) -> PathBuf {
        let n = archive_num as usize;
        let filename = format!("content_{n:05}.zip");
        let subfolder = format!("{:04}", n / 1000);
        self.data_dir
            .join("sources")
            .join("content")
            .join(subfolder)
            .join(filename)
    }

    pub(crate) fn sources_dir(&self) -> PathBuf {
        self.data_dir.join("sources")
    }
}

/// Extract archive number from filename (e.g. "content_00123.zip" → 123).
pub(crate) fn parse_archive_number(filename: &str) -> Option<usize> {
    filename
        .strip_prefix("content_")
        .and_then(|s| s.strip_suffix(".zip"))
        .and_then(|s| s.parse::<usize>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    fn make_state(dir: &Path) -> Arc<SharedArchiveState> {
        SharedArchiveState::new(dir.to_path_buf()).unwrap()
    }

    #[test]
    fn archive_path_for_number_correct_subfolders() {
        let dir = tempfile::tempdir().unwrap();
        let state = make_state(dir.path());

        let p = state.archive_path_for_number(0);
        assert!(p.ends_with("content/0000/content_00000.zip"), "{}", p.display());

        let p = state.archive_path_for_number(999);
        assert!(p.ends_with("content/0000/content_00999.zip"), "{}", p.display());

        let p = state.archive_path_for_number(1000);
        assert!(p.ends_with("content/0001/content_01000.zip"), "{}", p.display());

        let p = state.archive_path_for_number(12345);
        assert!(p.ends_with("content/0012/content_12345.zip"), "{}", p.display());
    }

    #[test]
    fn shared_state_allocates_unique_archive_numbers() {
        let dir = tempfile::tempdir().unwrap();
        let state = make_state(dir.path());
        let n1 = state.allocate_archive_num();
        let n2 = state.allocate_archive_num();
        let n3 = state.allocate_archive_num();
        assert_ne!(n1, n2);
        assert_ne!(n2, n3);
        assert_ne!(n1, n3);
    }

    #[test]
    fn shared_state_seeds_from_existing_archives() {
        let dir = tempfile::tempdir().unwrap();
        let content_dir = dir.path().join("sources").join("content").join("0000");
        std::fs::create_dir_all(&content_dir).unwrap();
        File::create(content_dir.join("content_00001.zip")).unwrap();
        File::create(content_dir.join("content_00005.zip")).unwrap();

        let state = make_state(dir.path());
        let n = state.allocate_archive_num();
        assert_eq!(n, 6);
    }
}
