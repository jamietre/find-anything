mod helpers;
use helpers::TestServer;

use find_common::api::{AppSettingsResponse, SearchResponse, StatsResponse, SourceInfo};

#[tokio::test]
async fn test_get_settings_returns_200() {
    let srv = TestServer::spawn().await;
    let resp = srv
        .client
        .get(srv.url("/api/v1/settings"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 200);
    let body: AppSettingsResponse = resp.json().await.unwrap();
    assert!(!body.version.is_empty());
    assert!(!body.min_client_version.is_empty());
}

#[tokio::test]
async fn test_get_stats_empty_server() {
    let srv = TestServer::spawn().await;
    let resp: StatsResponse = srv
        .client
        .get(srv.url("/api/v1/stats"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(resp.sources.len(), 0);
    assert_eq!(resp.inbox_pending, 0);
}

#[tokio::test]
async fn test_get_sources_empty() {
    let srv = TestServer::spawn().await;
    let resp: Vec<SourceInfo> = srv
        .client
        .get(srv.url("/api/v1/sources"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(resp.is_empty());
}

#[tokio::test]
async fn test_search_missing_q_returns_error() {
    let srv = TestServer::spawn().await;
    let status = srv
        .client
        .get(srv.url("/api/v1/search"))
        .send()
        .await
        .unwrap()
        .status();
    assert!(
        status.as_u16() >= 400,
        "expected error status, got {status}"
    );
}

#[tokio::test]
async fn test_search_empty_server_returns_no_results() {
    let srv = TestServer::spawn().await;
    let resp: SearchResponse = srv
        .client
        .get(srv.url("/api/v1/search?q=anything"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(resp.total, 0);
    assert!(resp.results.is_empty());
}
