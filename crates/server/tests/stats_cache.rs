mod helpers;
use helpers::{make_text_bulk, TestServer};

use find_common::api::BulkRequest;

/// ?refresh=true returns the correct file count after indexing.
#[tokio::test]
async fn refresh_returns_correct_file_count() {
    let srv = TestServer::spawn().await;

    srv.post_bulk(&make_text_bulk("src", "hello.txt", "hello world")).await;
    srv.wait_for_idle().await;

    let resp = srv.get_stats_refresh().await;
    let src = resp.sources.iter().find(|s| s.name == "src").expect("source not found");
    assert_eq!(src.total_files, 1);
    assert!(src.total_size > 0);
}

/// Indexing a second file increments total_files in the cache.
#[tokio::test]
async fn incremental_new_file_increments_total_files() {
    let srv = TestServer::spawn().await;

    // Index the first file and refresh the cache so the source entry exists.
    srv.post_bulk(&make_text_bulk("src", "a.txt", "first")).await;
    srv.wait_for_idle().await;
    let initial = srv.get_stats_refresh().await;
    let count_before = initial
        .sources
        .iter()
        .find(|s| s.name == "src")
        .map(|s| s.total_files)
        .unwrap_or(0);

    // Index a second file.
    srv.post_bulk(&make_text_bulk("src", "b.txt", "second")).await;
    srv.wait_for_idle().await;

    // The cache should reflect the incremental update without a refresh.
    let resp = srv.get_stats().await;
    let src = resp.sources.iter().find(|s| s.name == "src").expect("source not found");
    assert_eq!(src.total_files, count_before + 1);
}

/// Deleting a file decrements total_files in the cache.
#[tokio::test]
async fn incremental_delete_decrements_total_files() {
    let srv = TestServer::spawn().await;

    // Index a file and populate the cache.
    srv.post_bulk(&make_text_bulk("src", "file.txt", "content")).await;
    srv.wait_for_idle().await;
    srv.get_stats_refresh().await;

    // Delete the file.
    let del_req = BulkRequest {
        source: "src".to_string(),
        files: vec![],
        delete_paths: vec!["file.txt".to_string()],
        scan_timestamp: None,
        indexing_failures: vec![],
        rename_paths: vec![],
    };
    srv.post_bulk(&del_req).await;
    srv.wait_for_idle().await;

    // Cache should show 0 files without requiring a refresh.
    let resp = srv.get_stats().await;
    let src = resp.sources.iter().find(|s| s.name == "src").expect("source not found");
    assert_eq!(src.total_files, 0);
}

/// Indexing multiple text files updates by_kind incrementally.
#[tokio::test]
async fn incremental_by_kind_is_updated() {
    let srv = TestServer::spawn().await;

    // Index the first file and populate the cache.
    srv.post_bulk(&make_text_bulk("src", "readme.txt", "text content")).await;
    srv.wait_for_idle().await;
    srv.get_stats_refresh().await;

    // Index a second text file.
    srv.post_bulk(&make_text_bulk("src", "other.txt", "more text")).await;
    srv.wait_for_idle().await;

    let resp = srv.get_stats().await;
    let src = resp.sources.iter().find(|s| s.name == "src").expect("source not found");

    // There should be at least one kind entry with count >= 2.
    let text_kind = src.by_kind.values().find(|k| k.count >= 2);
    assert!(text_kind.is_some(), "expected at least 2 files in a kind, got: {:?}", src.by_kind);
}
