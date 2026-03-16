mod helpers;
use helpers::{make_text_bulk, TestServer};

use find_common::api::SearchResponse;

// ── exact mode ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_exact_mode_matches_substring() {
    let srv = TestServer::spawn().await;
    let req = make_text_bulk("docs", "exact.txt", "the quick brown fox jumps over the lazy dog");
    srv.post_bulk(&req).await;
    srv.wait_for_idle().await;

    let resp: SearchResponse = srv
        .client
        .get(srv.url("/api/v1/search?q=quick+brown+fox&mode=exact&source=docs"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert!(resp.total >= 1, "exact phrase should match");
    assert!(resp.results.iter().any(|r| r.path == "exact.txt"));
}

#[tokio::test]
async fn test_exact_mode_does_not_match_fuzzy_variants() {
    let srv = TestServer::spawn().await;
    // Index a file with "colour" (British spelling)
    let req = make_text_bulk("docs", "spelling.txt", "the colour of the sky is blue");
    srv.post_bulk(&req).await;
    srv.wait_for_idle().await;

    // Exact search for "color" (American spelling) should return no results
    let resp: SearchResponse = srv
        .client
        .get(srv.url("/api/v1/search?q=color&mode=exact&source=docs"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(resp.total, 0, "exact mode must not match fuzzy variants");
}

// ── regex mode ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_regex_mode_matches_pattern() {
    let srv = TestServer::spawn().await;
    // Use literal words that FTS5 can find as a pre-filter, connected by a regex wildcard.
    // regex_to_fts_terms extracts "fatal" and "encountered" as literal FTS5 terms.
    let req = make_text_bulk("docs", "regex.txt", "fatal error encountered at runtime");
    srv.post_bulk(&req).await;
    srv.wait_for_idle().await;

    // Pattern: "fatal" ... "encountered" with anything in between
    let resp: SearchResponse = srv
        .client
        .get(srv.url("/api/v1/search?q=fatal.%2Bencountered&mode=regex&source=docs"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert!(resp.total >= 1, "regex mode should match fatal.+encountered pattern");
    assert!(resp.results.iter().any(|r| r.path == "regex.txt"));
}

#[tokio::test]
async fn test_regex_mode_no_match() {
    let srv = TestServer::spawn().await;
    let req = make_text_bulk("docs", "noregex.txt", "ordinary text with no special codes");
    srv.post_bulk(&req).await;
    srv.wait_for_idle().await;

    // Pattern that shouldn't match
    let resp: SearchResponse = srv
        .client
        .get(srv.url("/api/v1/search?q=%5BXYZ%5D%7B5%7D&mode=regex&source=docs"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(resp.total, 0, "regex mode should return empty for non-matching pattern");
}

// ── file-* modes ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_file_fuzzy_mode_matches_filename() {
    let srv = TestServer::spawn().await;
    // The content is different from the filename — only the filename should match
    let req = make_text_bulk("docs", "invoices/report2024.txt", "unrelated content here");
    srv.post_bulk(&req).await;
    srv.wait_for_idle().await;

    let resp: SearchResponse = srv
        .client
        .get(srv.url("/api/v1/search?q=report2024&mode=file-fuzzy&source=docs"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert!(resp.total >= 1, "file-fuzzy should match filename");
    assert!(resp.results.iter().any(|r| r.path.contains("report2024")));
}

#[tokio::test]
async fn test_file_fuzzy_does_not_match_content_only() {
    let srv = TestServer::spawn().await;
    // The filename does not contain the search term, only the content does
    let req = make_text_bulk("docs", "document.txt", "zymurgy fermentation content");
    srv.post_bulk(&req).await;
    srv.wait_for_idle().await;

    let resp: SearchResponse = srv
        .client
        .get(srv.url("/api/v1/search?q=zymurgy&mode=file-fuzzy&source=docs"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(resp.total, 0, "file-fuzzy should only match filenames, not content");
}

#[tokio::test]
async fn test_file_exact_matches_exact_filename_fragment() {
    let srv = TestServer::spawn().await;
    let req = make_text_bulk("docs", "data/quarterly_report.txt", "some financial content");
    srv.post_bulk(&req).await;
    srv.wait_for_idle().await;

    let resp: SearchResponse = srv
        .client
        .get(srv.url("/api/v1/search?q=quarterly_report&mode=file-exact&source=docs"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert!(resp.total >= 1, "file-exact should match the exact filename fragment");
}

// ── fuzzy (default) mode ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_fuzzy_mode_is_default() {
    let srv = TestServer::spawn().await;
    let req = make_text_bulk("docs", "fuzzy.txt", "documentation about configuration options");
    srv.post_bulk(&req).await;
    srv.wait_for_idle().await;

    // No mode param — should default to fuzzy and find results
    let resp: SearchResponse = srv
        .client
        .get(srv.url("/api/v1/search?q=configuration&source=docs"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert!(resp.total >= 1, "default mode should be fuzzy and find matches");
}
