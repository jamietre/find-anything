// Moved from find-server crates/server/src/archive.rs — ArchiveManager.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use anyhow::{Context, Result};
use zip::{CompressionMethod, ZipArchive, ZipWriter};
use zip::write::{FullFileOptions, SimpleFileOptions};

use super::chunk::Chunk;
use super::shared::{parse_archive_number, SharedArchiveState};

const TARGET_ARCHIVE_SIZE: usize = 10 * 1024 * 1024; // 10 MB

/// Reference to a chunk stored in an archive.
#[derive(Debug, Clone)]
pub struct ChunkRef {
    pub archive_name: String,
    pub chunk_name: String,
}

/// Per-worker archive manager.
pub struct ArchiveManager {
    state: Arc<SharedArchiveState>,
    current_archive_num: Option<u32>,
    read_cache: RefCell<HashMap<PathBuf, Vec<u8>>>,
}

impl ArchiveManager {
    pub fn new(state: Arc<SharedArchiveState>) -> Self {
        Self {
            state,
            current_archive_num: None,
            read_cache: RefCell::new(HashMap::new()),
        }
    }

    /// Create an `ArchiveManager` for **read-only** use (e.g. route handlers).
    pub fn new_for_reading(data_dir: PathBuf) -> Self {
        use std::sync::atomic::{AtomicU32, AtomicU64};
        use std::sync::Mutex;
        let state = Arc::new(SharedArchiveState {
            data_dir,
            next_archive_num:   AtomicU32::new(0),
            total_archives:     AtomicU64::new(0),
            archive_size_bytes: AtomicU64::new(0),
            rewrite_locks:      Mutex::new(HashMap::new()),
            source_locks:       Mutex::new(HashMap::new()),
        });
        Self {
            state,
            current_archive_num: None,
            read_cache: RefCell::new(HashMap::new()),
        }
    }

    /// Append chunks to archives, creating new ones as needed.
    pub fn append_chunks(&mut self, chunks: Vec<Chunk>) -> Result<Vec<ChunkRef>> {
        let mut refs = Vec::new();
        for chunk in chunks {
            let archive_path = self.current_archive_path()?;
            let chunk_name = format!("{}.{}", chunk.block_id, chunk.chunk_number);
            self.append_to_zip_with_comment(&archive_path, &chunk_name, chunk.content.as_bytes(), "")?;
            refs.push(ChunkRef {
                archive_name: archive_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string(),
                chunk_name,
            });
            let on_disk = std::fs::metadata(&archive_path)
                .map(|m| m.len() as usize)
                .unwrap_or(0);
            if on_disk >= TARGET_ARCHIVE_SIZE {
                self.current_archive_num = None;
            }
        }
        Ok(refs)
    }

    /// Append a raw chunk (arbitrary name) to the current archive.
    /// Used by `ZipContentStore::put` with key-prefix naming.
    pub fn append_raw(&mut self, chunk_name: &str, content: &[u8]) -> Result<ChunkRef> {
        let archive_path = self.current_archive_path()?;
        self.append_to_zip_with_comment(&archive_path, chunk_name, content, "")?;
        let archive_name = archive_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let on_disk = std::fs::metadata(&archive_path)
            .map(|m| m.len() as usize)
            .unwrap_or(0);
        if on_disk >= TARGET_ARCHIVE_SIZE {
            self.current_archive_num = None;
        }
        Ok(ChunkRef { archive_name, chunk_name: chunk_name.to_string() })
    }

    /// Remove chunks from archives by rewriting affected ZIPs.
    pub fn remove_chunks(&self, refs: Vec<ChunkRef>) -> Result<u64> {
        let mut by_archive: HashMap<String, HashSet<String>> = HashMap::new();
        for chunk_ref in refs {
            by_archive
                .entry(chunk_ref.archive_name)
                .or_default()
                .insert(chunk_ref.chunk_name);
        }
        let mut bytes_freed: u64 = 0;
        for (archive_name, chunks_to_remove) in by_archive {
            let archive_path = if let Some(num) = parse_archive_number(&archive_name) {
                self.state.archive_path_for_number(num as u32)
            } else {
                self.state.sources_dir().join(&archive_name)
            };
            if archive_path.exists() {
                let valid = File::open(&archive_path)
                    .ok()
                    .and_then(|f| ZipArchive::new(f).ok())
                    .is_some();
                if valid {
                    let lock = self.state.rewrite_lock_for(&archive_path);
                    let _guard = lock.lock().unwrap();
                    bytes_freed += self.rewrite_archive(&archive_path, &chunks_to_remove)?;
                } else {
                    tracing::warn!(
                        "skipping corrupt archive during chunk removal: {}",
                        archive_path.display()
                    );
                }
            }
        }
        Ok(bytes_freed)
    }

    /// Read chunk content from archive, using a per-instance byte cache.
    pub fn read_chunk(&self, chunk_ref: &ChunkRef) -> Result<String> {
        let archive_path = if let Some(num) = parse_archive_number(&chunk_ref.archive_name) {
            self.state.archive_path_for_number(num as u32)
        } else {
            self.state.sources_dir().join(&chunk_ref.archive_name)
        };

        if !self.read_cache.borrow().contains_key(&archive_path) {
            let bytes = std::fs::read(&archive_path)
                .with_context(|| format!("reading archive {}", archive_path.display()))?;
            self.read_cache.borrow_mut().insert(archive_path.clone(), bytes);
        }

        let cache = self.read_cache.borrow();
        let bytes = cache.get(&archive_path).expect("just inserted");
        let cursor = std::io::Cursor::new(bytes.as_slice());
        let mut zip = ZipArchive::new(cursor)?;

        let mut entry = zip
            .by_name(&chunk_ref.chunk_name)
            .with_context(|| format!("finding chunk {} in archive", chunk_ref.chunk_name))?;

        let mut content = String::new();
        entry.read_to_string(&mut content)?;
        Ok(content)
    }

    fn current_archive_path(&mut self) -> Result<PathBuf> {
        if let Some(num) = self.current_archive_num {
            let path = self.state.archive_path_for_number(num);
            let on_disk = std::fs::metadata(&path).map(|m| m.len() as usize).unwrap_or(0);
            if on_disk < TARGET_ARCHIVE_SIZE {
                return Ok(path);
            }
            self.current_archive_num = None;
        }

        let new_num = self.state.allocate_archive_num();
        let new_path = self.state.archive_path_for_number(new_num);

        if let Some(parent) = new_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = File::create(&new_path)?;
        ZipWriter::new(file).finish()?;

        self.state.total_archives.fetch_add(1, Ordering::Relaxed);
        let initial_size = std::fs::metadata(&new_path).map(|m| m.len()).unwrap_or(0);
        self.state.archive_size_bytes.fetch_add(initial_size, Ordering::Relaxed);

        self.current_archive_num = Some(new_num);
        Ok(new_path)
    }

    fn append_to_zip_with_comment(
        &self,
        archive_path: &Path,
        entry_name: &str,
        content: &[u8],
        comment: &str,
    ) -> Result<()> {
        {
            let file = File::open(archive_path)?;
            let zip = ZipArchive::new(file)?;
            if zip.index_for_name(entry_name).is_some() {
                let to_remove: HashSet<String> = std::iter::once(entry_name.to_string()).collect();
                self.rewrite_archive(archive_path, &to_remove)?;
                tracing::warn!("removed stale chunk {entry_name} before re-appending");
            }
        }

        let size_before = std::fs::metadata(archive_path).map(|m| m.len()).unwrap_or(0);

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(archive_path)?;
        let mut zip = ZipWriter::new_append(file)?;

        let options: FullFileOptions<'_> = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(6))
            .into_full_options()
            .with_file_comment(comment);

        zip.start_file(entry_name, options)?;
        zip.write_all(content)?;
        zip.finish()?;

        let size_after = std::fs::metadata(archive_path).map(|m| m.len()).unwrap_or(0);
        if size_after > size_before {
            self.state
                .archive_size_bytes
                .fetch_add(size_after - size_before, Ordering::Relaxed);
        }

        Ok(())
    }

    pub(crate) fn rewrite_archive(
        &self,
        archive_path: &Path,
        chunks_to_remove: &HashSet<String>,
    ) -> Result<u64> {
        let size_before = std::fs::metadata(archive_path).map(|m| m.len()).unwrap_or(0);
        let temp_path = archive_path.with_extension("zip.tmp");

        let result = self.rewrite_archive_inner(archive_path, chunks_to_remove, &temp_path);
        if result.is_err() {
            let _ = std::fs::remove_file(&temp_path);
        }
        let bytes_freed = result?;

        let size_after = std::fs::metadata(archive_path).map(|m| m.len()).unwrap_or(0);
        match size_after.cmp(&size_before) {
            std::cmp::Ordering::Greater => {
                self.state
                    .archive_size_bytes
                    .fetch_add(size_after - size_before, Ordering::Relaxed);
            }
            std::cmp::Ordering::Less => {
                let _ = self.state.archive_size_bytes.fetch_update(
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                    |v| Some(v.saturating_sub(size_before - size_after)),
                );
            }
            std::cmp::Ordering::Equal => {}
        }

        Ok(bytes_freed)
    }

    fn rewrite_archive_inner(
        &self,
        archive_path: &Path,
        chunks_to_remove: &HashSet<String>,
        temp_path: &Path,
    ) -> Result<u64> {
        let file = File::open(archive_path)?;
        let mut old_zip = ZipArchive::new(file)?;

        let temp_file = File::create(temp_path)?;
        let mut new_zip = ZipWriter::new(temp_file);

        let base_options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(6))
            .into_full_options();

        let mut bytes_freed: u64 = 0;
        for i in 0..old_zip.len() {
            let mut entry = old_zip.by_index(i)?;
            let name = entry.name().to_string();
            let comment = entry.comment().to_string();

            if chunks_to_remove.contains(&name) {
                bytes_freed += entry.compressed_size();
            } else {
                let entry_options = base_options.clone().with_file_comment(comment.as_str());
                new_zip.start_file(&name, entry_options)?;
                std::io::copy(&mut entry, &mut new_zip)?;
            }
        }

        new_zip.finish()?;
        drop(old_zip);
        std::fs::rename(temp_path, archive_path)?;
        Ok(bytes_freed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state(dir: &Path) -> Arc<SharedArchiveState> {
        SharedArchiveState::new(dir.to_path_buf()).unwrap()
    }

    fn make_chunk(block_id: i64, chunk_number: usize, content: &str) -> Chunk {
        Chunk {
            block_id,
            chunk_number,
            content: content.to_string(),
        }
    }

    #[test]
    fn append_then_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let state = make_state(dir.path());
        let mut mgr = ArchiveManager::new(Arc::clone(&state));

        let refs = mgr
            .append_chunks(vec![
                make_chunk(1, 0, "hello world\nline 2"),
                make_chunk(2, 0, "another file"),
            ])
            .unwrap();

        assert_eq!(refs.len(), 2);
        assert_eq!(mgr.read_chunk(&refs[0]).unwrap(), "hello world\nline 2");
        assert_eq!(mgr.read_chunk(&refs[1]).unwrap(), "another file");
    }

    #[test]
    fn remove_chunks_deletes_entry() {
        let dir = tempfile::tempdir().unwrap();
        let state = make_state(dir.path());
        let mut mgr = ArchiveManager::new(Arc::clone(&state));

        let refs = mgr
            .append_chunks(vec![
                make_chunk(1, 0, "keep me"),
                make_chunk(2, 0, "delete me"),
            ])
            .unwrap();

        mgr.remove_chunks(vec![refs[1].clone()]).unwrap();

        let reader = ArchiveManager::new_for_reading(dir.path().to_path_buf());
        assert_eq!(reader.read_chunk(&refs[0]).unwrap(), "keep me");
        assert!(
            reader.read_chunk(&refs[1]).is_err(),
            "removed chunk should not be readable"
        );
    }

    #[test]
    fn stale_chunk_append_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let state = make_state(dir.path());
        let mut mgr = ArchiveManager::new(Arc::clone(&state));

        mgr.append_chunks(vec![make_chunk(1, 0, "original")]).unwrap();
        let refs = mgr.append_chunks(vec![make_chunk(1, 0, "updated")]).unwrap();
        assert_eq!(mgr.read_chunk(&refs[0]).unwrap(), "updated");
    }
}
