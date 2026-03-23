mod helpers;
use helpers::TestEnv;

use std::time::Duration;

use find_common::config::{ExtractorEntry, ExternalExtractorConfig, ExternalExtractorMode};
use find_client::watch::{WatchOptions, run_watch};

/// Start the watcher with a given config and wait for it to finish
/// registering inotify watches before returning.
/// Returns a `JoinHandle` — call `handle.abort()` at the end of the test.
async fn start_watcher_with_config(config: find_common::config::ClientConfig) -> tokio::task::JoinHandle<()> {
    let opts = WatchOptions {
        config_path: String::new(),
        scan_now: false,
    };
    let handle = tokio::spawn(async move {
        let _ = run_watch(&config, &opts).await;
    });
    // Give the watcher time to complete startup (server version check +
    // walk to register inotify watches) before the test touches the filesystem.
    tokio::time::sleep(Duration::from_millis(500)).await;
    handle
}

/// Start the watcher with the default test config.
async fn start_watcher(env: &TestEnv) -> tokio::task::JoinHandle<()> {
    start_watcher_with_config(env.client_config()).await
}

/// Wait a short period for filesystem events to propagate and be processed.
async fn settle(env: &TestEnv) {
    // Give inotify time to fire and the server worker time to process.
    tokio::time::sleep(Duration::from_millis(200)).await;
    env.server.wait_for_idle().await;
}

// ── W1 — New file is indexed ──────────────────────────────────────────────────

#[ignore]
#[tokio::test]
async fn w1_new_file_is_indexed() {
    let env = TestEnv::new().await;
    let handle = start_watcher(&env).await;

    env.write_file("new.txt", "watch_created_xyz unique content");
    settle(&env).await;

    let results = env.search("watch_created_xyz").await;
    assert!(
        !results.is_empty(),
        "watch_created_xyz not found after creating new file"
    );

    handle.abort();
}

// ── W2 — Modified file is re-indexed ─────────────────────────────────────────

#[ignore]
#[tokio::test]
async fn w2_modified_file_is_reindexed() {
    let env = TestEnv::new().await;

    // Establish baseline via scan.
    let path = env.write_file("mutable.txt", "version_watch_a content");
    env.run_scan().await;

    let handle = start_watcher(&env).await;

    // Overwrite + bump mtime.
    let new_mtime = std::time::SystemTime::now() + std::time::Duration::from_secs(2);
    filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(new_mtime))
        .expect("set mtime");
    std::fs::write(&path, "version_watch_b content").expect("overwrite");
    settle(&env).await;

    assert!(
        !env.search("version_watch_b").await.is_empty(),
        "version_watch_b not found"
    );
    assert!(
        env.search("version_watch_a").await.is_empty(),
        "version_watch_a still present"
    );

    handle.abort();
}

// ── W3 — Deleted file is removed ─────────────────────────────────────────────

#[ignore]
#[tokio::test]
async fn w3_deleted_file_is_removed() {
    let env = TestEnv::new().await;

    env.write_file("ephemeral.txt", "ephemeral_watch_content_zzz");
    env.run_scan().await;
    assert!(!env.search("ephemeral_watch_content_zzz").await.is_empty());

    let handle = start_watcher(&env).await;
    env.remove_file("ephemeral.txt");
    settle(&env).await;

    assert!(
        env.search("ephemeral_watch_content_zzz").await.is_empty(),
        "ephemeral_watch_content_zzz still searchable after deletion"
    );

    handle.abort();
}

// ── W4 — Renamed file updates index ──────────────────────────────────────────

#[ignore]
#[tokio::test]
async fn w4_renamed_file_updates_index() {
    let env = TestEnv::new().await;

    env.write_file("old_name.txt", "rename_watch_content_yyy");
    env.run_scan().await;

    let handle = start_watcher(&env).await;

    let old_path = env.source_dir.path().join("old_name.txt");
    let new_path = env.source_dir.path().join("new_name.txt");
    std::fs::rename(&old_path, &new_path).expect("rename");
    settle(&env).await;

    let files = env.list_files().await;
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert!(
        !paths.contains(&"old_name.txt"),
        "old_name.txt still in index after rename"
    );
    assert!(
        paths.contains(&"new_name.txt"),
        "new_name.txt not in index after rename: {paths:?}"
    );

    handle.abort();
}

// ── W6 — File created empty then updated with content is fully indexed ────────
//
// Regression test for a bug where find-watch never computed the blake3 hash,
// so content lines were inserted into FTS5 (making the file searchable) but the
// raw bytes were never stored in the blob store — leaving the file viewer empty.

#[ignore]
#[tokio::test]
async fn w6_empty_file_then_content_is_stored() {
    let env = TestEnv::new().await;
    let handle = start_watcher(&env).await;

    // 1. Create the file empty.
    let path = env.write_file("watch_content.txt", "");
    settle(&env).await;

    // File should be indexed but have no searchable content yet.
    assert!(
        env.search("w6_unique_phrase_content").await.is_empty(),
        "content found before it was written"
    );

    // 2. Write content (bump mtime so the watcher sees a change).
    let new_mtime = std::time::SystemTime::now() + std::time::Duration::from_secs(2);
    filetime::set_file_mtime(&path, filetime::FileTime::from_system_time(new_mtime))
        .expect("set mtime");
    std::fs::write(&path, "w6_unique_phrase_content").expect("write content");
    settle(&env).await;

    // 3. Content must be searchable.
    assert!(
        !env.search("w6_unique_phrase_content").await.is_empty(),
        "content not found after writing"
    );

    // 4. Content must be retrievable from the blob store (file viewer).
    let lines = env.get_file_lines("watch_content.txt").await;
    assert!(
        lines.iter().any(|l| l.contains("w6_unique_phrase_content")),
        "content not in stored blob after update; lines={lines:?}"
    );

    handle.abort();
}

// ── W5 — External extractor honoured by watch ────────────────────────────────

#[ignore]
#[tokio::test]
async fn w5_external_extractor_honoured_by_watch() {
    let env = TestEnv::new().await;
    let fixtures = helpers::fixtures_dir();
    let extractor_bin = std::path::Path::new(&fixtures)
        .join("find-extract-nd1")
        .to_string_lossy()
        .to_string();

    let mut config = env.client_config_with(|_watch| {
        // watch config doesn't carry extractor overrides; those live in scan config
    });
    config.scan.extractors.insert(
        "nd1".to_string(),
        ExtractorEntry::External(ExternalExtractorConfig {
            mode: ExternalExtractorMode::TempDir,
            bin: extractor_bin,
            args: vec!["{file}".to_string(), "{dir}".to_string()],
        }),
    );

    let handle = start_watcher_with_config(config).await;

    let fixture_nd1 = std::path::Path::new(&fixtures).join("test.nd1");
    let nd1_bytes = std::fs::read(&fixture_nd1).expect("read test.nd1");
    env.write_file_bytes("test.nd1", &nd1_bytes);
    settle(&env).await;

    let files = env.list_files().await;
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert!(
        paths.contains(&"test.nd1::readme.txt"),
        "test.nd1::readme.txt not indexed: {paths:?}"
    );

    handle.abort();
}
