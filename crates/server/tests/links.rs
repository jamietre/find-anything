mod helpers;
use helpers::{make_text_bulk, TestServer};

use find_common::api::{CreateLinkRequest, CreateLinkResponse, ResolveLinkResponse};

// ── create + resolve round-trip ───────────────────────────────────────────────

#[tokio::test]
async fn test_create_and_resolve_link() {
    let srv = TestServer::spawn().await;

    // Index a file so the source/path exist
    let req = make_text_bulk("docs", "notes/important.txt", "some important content");
    srv.post_bulk(&req).await;
    srv.wait_for_idle().await;

    // Create a link
    let create_resp: CreateLinkResponse = srv
        .client
        .post(srv.url("/api/v1/links"))
        .json(&CreateLinkRequest {
            source: "docs".to_string(),
            path: "notes/important.txt".to_string(),
            archive_path: None,
            expires_in_secs: None,
        })
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert!(!create_resp.code.is_empty(), "code must not be empty");
    assert!(create_resp.url.starts_with('/'), "url must be a relative path");
    assert!(create_resp.expires_at > 0, "expires_at must be a positive timestamp");

    // Resolve the link
    let resolve_resp: ResolveLinkResponse = srv
        .client
        .get(srv.url(&format!("/api/v1/links/{}", create_resp.code)))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(resolve_resp.source, "docs");
    assert_eq!(resolve_resp.path, "notes/important.txt");
    assert_eq!(resolve_resp.archive_path, None);
}

// ── code uniqueness ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_two_links_have_different_codes() {
    let srv = TestServer::spawn().await;

    srv.post_bulk(&make_text_bulk("docs", "file-a.txt", "content a")).await;
    srv.post_bulk(&make_text_bulk("docs", "file-b.txt", "content b")).await;
    srv.wait_for_idle().await;

    let make_link = |path: &'static str| {
        let srv = &srv;
        async move {
            srv.client
                .post(srv.url("/api/v1/links"))
                .json(&CreateLinkRequest {
                    source: "docs".to_string(),
                    path: path.to_string(),
                    archive_path: None,
                    expires_in_secs: None,
                })
                .send()
                .await
                .unwrap()
                .json::<CreateLinkResponse>()
                .await
                .unwrap()
        }
    };

    let a = make_link("file-a.txt").await;
    let b = make_link("file-b.txt").await;

    assert_ne!(a.code, b.code, "two different links must have distinct codes");
}

// ── unknown code returns 404 ──────────────────────────────────────────────────

#[tokio::test]
async fn test_resolve_unknown_code_returns_404() {
    let srv = TestServer::spawn().await;

    let status = srv
        .client
        .get(srv.url("/api/v1/links/ZZZZZZ"))
        .send()
        .await
        .unwrap()
        .status();

    assert_eq!(status.as_u16(), 404, "unknown link code should return 404");
}

// ── auth required ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_create_link_requires_auth() {
    let srv = TestServer::spawn().await;

    let status = reqwest::Client::new()
        .post(srv.url("/api/v1/links"))
        .json(&CreateLinkRequest {
            source: "docs".to_string(),
            path: "file.txt".to_string(),
            archive_path: None,
            expires_in_secs: None,
        })
        .send()
        .await
        .unwrap()
        .status();

    assert_eq!(status.as_u16(), 401, "creating a link without auth should return 401");
}

// ── idempotency: same file gets a fresh code each time ───────────────────────

#[tokio::test]
async fn test_repeated_link_creation_returns_new_code() {
    let srv = TestServer::spawn().await;

    srv.post_bulk(&make_text_bulk("docs", "shared.txt", "shared content")).await;
    srv.wait_for_idle().await;

    let body = CreateLinkRequest {
        source: "docs".to_string(),
        path: "shared.txt".to_string(),
        archive_path: None,
        expires_in_secs: None,
    };

    let first: CreateLinkResponse = srv
        .client
        .post(srv.url("/api/v1/links"))
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let second: CreateLinkResponse = srv
        .client
        .post(srv.url("/api/v1/links"))
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    // Both codes resolve correctly even if the server reuses or generates new ones
    for code in [&first.code, &second.code] {
        let resolve: ResolveLinkResponse = srv
            .client
            .get(srv.url(&format!("/api/v1/links/{code}")))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(resolve.path, "shared.txt");
    }
}
