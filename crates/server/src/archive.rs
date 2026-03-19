/// Transition shim — re-exports `SharedArchiveState`, `ArchiveManager`, and
/// related types from `find-content-store` while the server is being ported
/// to use `ContentStore` directly.  This module will be deleted in step 15.
#[allow(unused_imports)]
pub use find_content_store::_internal::{
    ArchiveManager, ChunkRange, ChunkRef,
    SharedArchiveState, chunk_lines,
};
