mod helpers;
use helpers::{make_text_bulk, TestServer};

use find_common::api::{SearchResponse, SourceInfo};

#[tokio::test]
async fn test_two_sources_are_isolated() {
    let srv = TestServer::spawn().await;

    // Index "alpha" in source-a and "beta" in source-b
    srv.post_bulk(&make_text_bulk("source-a", "file.txt", "alpha content here")).await;
    srv.post_bulk(&make_text_bulk("source-b", "file.txt", "beta content here")).await;
    srv.wait_for_idle().await;

    // alpha is found in source-a
    let resp: SearchResponse = srv
        .client
        .get(srv.url("/api/v1/search?q=alpha&source=source-a"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(resp.total >= 1, "expected alpha in source-a");

    // beta is found in source-b
    let resp: SearchResponse = srv
        .client
        .get(srv.url("/api/v1/search?q=beta&source=source-b"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(resp.total >= 1, "expected beta in source-b");

    // alpha is NOT in source-b
    let resp: SearchResponse = srv
        .client
        .get(srv.url("/api/v1/search?q=alpha&source=source-b"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(resp.total, 0, "alpha should not appear in source-b");
}

#[tokio::test]
async fn test_sources_list_shows_both() {
    let srv = TestServer::spawn().await;

    srv.post_bulk(&make_text_bulk("source-x", "a.txt", "content x")).await;
    srv.post_bulk(&make_text_bulk("source-y", "b.txt", "content y")).await;
    srv.wait_for_idle().await;

    let sources: Vec<SourceInfo> = srv
        .client
        .get(srv.url("/api/v1/sources"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let names: Vec<&str> = sources.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"source-x"), "expected source-x, got {names:?}");
    assert!(names.contains(&"source-y"), "expected source-y, got {names:?}");
}

#[tokio::test]
async fn test_cross_source_search_finds_all() {
    let srv = TestServer::spawn().await;

    srv.post_bulk(&make_text_bulk("src-1", "f1.txt", "sharedterm value one")).await;
    srv.post_bulk(&make_text_bulk("src-2", "f2.txt", "sharedterm value two")).await;
    srv.wait_for_idle().await;

    // No source filter — should find in both
    let resp: SearchResponse = srv
        .client
        .get(srv.url("/api/v1/search?q=sharedterm"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(resp.total >= 2, "expected results from both sources, got {}", resp.total);
}
