// Post-MVP: inotify/FSEvents watcher using the `notify` crate.
// When a file is created/modified, extract it and PUT to the server.
// When a file is deleted, DELETE from the server.
// Events are debounced (500 ms) to handle editor save storms.

use anyhow::Result;

use find_common::config::ClientConfig;

pub async fn run_watch(_config: &ClientConfig) -> Result<()> {
    // TODO: implement with notify crate
    // 1. Set up notify::RecommendedWatcher on all config.source.paths
    // 2. Channel events into a debounce buffer (tokio::time::sleep reset on each event)
    // 3. On flush:
    //    - CREATE/MODIFY  → extract + PUT /api/v1/files
    //    - REMOVE         → DELETE /api/v1/files
    //    - RENAME(old,new) → DELETE old + extract + PUT new
    anyhow::bail!("find-watch is not yet implemented (post-MVP)")
}
