pub mod bench;
mod key;
mod multi_store;
mod sqlite_store;
mod store;
pub mod zip_store;

pub use key::ContentKey;
pub use multi_store::MultiContentStore;
pub use sqlite_store::SqliteContentStore;
pub use store::{CompactResult, ContentStore};
pub use zip_store::ZipContentStore;

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use find_common::config::{BackendInstanceConfig, BackendType};

/// Open a single content store backend from its config entry.
///
/// `dir` is the data directory for this backend (the caller decides whether
/// to use `data_dir` directly or a per-backend subdirectory).
pub fn open_backend(b: &BackendInstanceConfig, dir: &Path) -> Result<Arc<dyn ContentStore>> {
    match b.backend_type {
        BackendType::Zip => Ok(Arc::new(
            ZipContentStore::open(dir)
                .map_err(|e| anyhow::anyhow!("opening zip store '{}': {e:#}", b.name))?,
        )),
        BackendType::Sqlite => Ok(Arc::new(
            SqliteContentStore::open(dir, b.chunk_size_kb, b.max_read_connections, b.compress)
                .map_err(|e| anyhow::anyhow!("opening sqlite store '{}': {e:#}", b.name))?,
        )),
    }
}

/// Temporarily exported internals used by the `find-server` transition shim.
/// Removed when step 15 is complete.
#[doc(hidden)]
pub mod _internal {
    pub use crate::zip_store::shared::SharedArchiveState;
    pub use crate::zip_store::archive::{ArchiveManager, ChunkRef};
    pub use crate::zip_store::chunk::{chunk_lines, Chunk, ChunkRange, ChunkResult};
}
