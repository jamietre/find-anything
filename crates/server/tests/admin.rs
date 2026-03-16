mod helpers;
use helpers::{make_text_bulk, TestServer};

use find_common::api::{SearchResponse, StatsResponse};

// ── delete_source ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_delete_source_removes_files_from_search() {
    let srv = TestServer::spawn().await;

    // Index a file with a unique term
    let req = make_text_bulk("to-delete", "doc.txt", "quixotically unique term");
    srv.post_bulk(&req).await;
    srv.wait_for_idle().await;

    // Confirm it is searchable
    let before: SearchResponse = srv
        .client
        .get(srv.url("/api/v1/search?q=quixotically&source=to-delete"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(before.total >= 1, "expected file to be findable before source deletion");

    // Delete the source
    let status = srv
        .client
        .delete(srv.url("/api/v1/admin/source?source=to-delete"))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(status.as_u16(), 200, "delete_source should return 200");

    // The source should no longer appear in stats
    let stats: StatsResponse = srv
        .client
        .get(srv.url("/api/v1/stats"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        !stats.sources.iter().any(|s| s.name == "to-delete"),
        "deleted source must not appear in stats"
    );
}

#[tokio::test]
async fn test_delete_nonexistent_source_returns_404() {
    let srv = TestServer::spawn().await;

    let status = srv
        .client
        .delete(srv.url("/api/v1/admin/source?source=no-such-source"))
        .send()
        .await
        .unwrap()
        .status();

    assert_eq!(status.as_u16(), 404, "deleting a non-existent source should return 404");
}

#[tokio::test]
async fn test_delete_source_requires_auth() {
    let srv = TestServer::spawn().await;

    // Index a source first so the path exists
    srv.post_bulk(&make_text_bulk("protected", "file.txt", "content")).await;
    srv.wait_for_idle().await;

    let status = reqwest::Client::new()
        .delete(srv.url("/api/v1/admin/source?source=protected"))
        .send()
        .await
        .unwrap()
        .status();

    assert_eq!(status.as_u16(), 401, "delete_source without auth should return 401");
}

#[tokio::test]
async fn test_delete_source_other_sources_unaffected() {
    let srv = TestServer::spawn().await;

    srv.post_bulk(&make_text_bulk("keep", "keep.txt", "keep this content")).await;
    srv.post_bulk(&make_text_bulk("drop", "drop.txt", "drop this content")).await;
    srv.wait_for_idle().await;

    // Delete only the "drop" source
    srv.client
        .delete(srv.url("/api/v1/admin/source?source=drop"))
        .send()
        .await
        .unwrap();

    // "keep" source should still be searchable
    let resp: SearchResponse = srv
        .client
        .get(srv.url("/api/v1/search?q=keep+this+content&source=keep"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(resp.total >= 1, "deleting one source must not affect others");
}

// ── compact ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_compact_dry_run_returns_200() {
    let srv = TestServer::spawn().await;

    // Index some content so there are archives to scan
    srv.post_bulk(&make_text_bulk("docs", "file.txt", "compaction test content")).await;
    srv.wait_for_idle().await;

    let status = srv
        .client
        .post(srv.url("/api/v1/admin/compact?dry_run=true"))
        .send()
        .await
        .unwrap()
        .status();

    assert_eq!(status.as_u16(), 200, "compact dry_run should return 200");
}

#[tokio::test]
async fn test_compact_requires_auth() {
    let srv = TestServer::spawn().await;

    let status = reqwest::Client::new()
        .post(srv.url("/api/v1/admin/compact"))
        .send()
        .await
        .unwrap()
        .status();

    assert_eq!(status.as_u16(), 401, "compact without auth should return 401");
}

#[tokio::test]
async fn test_compact_on_empty_server_returns_200() {
    let srv = TestServer::spawn().await;

    // No data indexed — compact should still succeed gracefully
    let status = srv
        .client
        .post(srv.url("/api/v1/admin/compact"))
        .send()
        .await
        .unwrap()
        .status();

    assert_eq!(status.as_u16(), 200, "compact on empty server should return 200");
}

// ── inbox pause / resume ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_pause_and_resume_inbox() {
    let srv = TestServer::spawn().await;

    let pause_status = srv
        .client
        .post(srv.url("/api/v1/admin/inbox/pause"))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(pause_status.as_u16(), 200, "pause should return 200");

    let resume_status = srv
        .client
        .post(srv.url("/api/v1/admin/inbox/resume"))
        .send()
        .await
        .unwrap()
        .status();
    assert_eq!(resume_status.as_u16(), 200, "resume should return 200");
}
