mod helpers;
use helpers::TestServer;

use find_common::api::SearchResponse;

#[tokio::test]
async fn test_search_unknown_source_returns_empty() {
    let srv = TestServer::spawn().await;

    // An unknown source returns empty results, not 404
    let resp: SearchResponse = srv
        .client
        .get(srv.url("/api/v1/search?q=anything&source=nonexistent-source"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(resp.total, 0);
    assert!(resp.results.is_empty());
}

#[tokio::test]
async fn test_context_for_nonexistent_source_returns_error() {
    let srv = TestServer::spawn().await;

    let status = srv
        .client
        .get(srv.url("/api/v1/context?source=no-such-source&path=no-such-file.txt&line=1"))
        .send()
        .await
        .unwrap()
        .status();

    assert!(
        status.as_u16() >= 400,
        "expected error status for nonexistent source, got {status}"
    );
}

#[tokio::test]
async fn test_bulk_without_gzip_returns_error() {
    let srv = TestServer::spawn().await;

    // POST plain JSON without Content-Encoding: gzip
    let status = srv
        .client
        .post(srv.url("/api/v1/bulk"))
        .header("Content-Type", "application/json")
        .body(r#"{"source":"docs","files":[]}"#)
        .send()
        .await
        .unwrap()
        .status();

    // Server should reject this (currently returns 500 since gzip decode fails)
    assert!(
        status.as_u16() >= 400,
        "expected error status for non-gzip bulk, got {status}"
    );
}

#[tokio::test]
async fn test_wrong_token_returns_401() {
    let srv = TestServer::spawn().await;

    // Use a plain client without the auth header
    let no_auth_client = reqwest::Client::new();

    let status = no_auth_client
        .get(srv.url("/api/v1/stats"))
        .header("Authorization", "Bearer wrong-token")
        .send()
        .await
        .unwrap()
        .status();

    assert_eq!(status.as_u16(), 401, "expected 401 for wrong token");
}

#[tokio::test]
async fn test_no_token_returns_401() {
    let srv = TestServer::spawn().await;

    let no_auth_client = reqwest::Client::new();

    let status = no_auth_client
        .get(srv.url("/api/v1/stats"))
        .send()
        .await
        .unwrap()
        .status();

    assert_eq!(status.as_u16(), 401, "expected 401 with no token");
}

