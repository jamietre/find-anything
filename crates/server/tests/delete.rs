mod helpers;
use helpers::{make_text_bulk, TestServer};

use find_common::api::{BulkRequest, SearchResponse, StatsResponse};

#[tokio::test]
async fn test_bulk_delete_removes_file_from_search() {
    let srv = TestServer::spawn().await;

    // Index a file with a unique word
    let req = make_text_bulk("docs", "delete-me.txt", "uniqueword123 some other content");
    srv.post_bulk(&req).await;
    srv.wait_for_idle().await;

    // Verify it's searchable
    let before: SearchResponse = srv
        .client
        .get(srv.url("/api/v1/search?q=uniqueword123&source=docs"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(before.total >= 1, "expected file to be findable before delete");

    // Delete it
    let delete_req = BulkRequest {
        source: "docs".to_string(),
        files: vec![],
        delete_paths: vec!["delete-me.txt".to_string()],
        scan_timestamp: None,
        indexing_failures: vec![],
        rename_paths: vec![],
    };
    srv.post_bulk(&delete_req).await;
    srv.wait_for_idle().await;

    // Verify it's gone
    let after: SearchResponse = srv
        .client
        .get(srv.url("/api/v1/search?q=uniqueword123&source=docs"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(after.total, 0, "expected no results after delete");
}

#[tokio::test]
async fn test_delete_nonexistent_path_is_safe() {
    let srv = TestServer::spawn().await;

    // Deleting a path that was never indexed should not crash the server
    let delete_req = BulkRequest {
        source: "docs".to_string(),
        files: vec![],
        delete_paths: vec!["nonexistent-file.txt".to_string()],
        scan_timestamp: None,
        indexing_failures: vec![],
        rename_paths: vec![],
    };
    srv.post_bulk(&delete_req).await;
    srv.wait_for_idle().await;

    // Server should still be responsive
    let resp: StatsResponse = srv
        .client
        .get(srv.url("/api/v1/stats"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(resp.inbox_pending, 0);
}
