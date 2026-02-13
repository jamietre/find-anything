#![allow(dead_code)] // methods are used by different binaries in this crate

use anyhow::{Context, Result};
use flate2::{write::GzEncoder, Compression};
use reqwest::Client;
use std::io::Write;

use find_common::api::{BulkRequest, ContextResponse, FileRecord, SearchResponse};

pub struct ApiClient {
    client: Client,
    base_url: String,
    token: String,
}

impl ApiClient {
    pub fn new(base_url: &str, token: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// GET /api/v1/files?source=<name>  — returns existing (path, mtime) list.
    pub async fn list_files(&self, source: &str) -> Result<Vec<FileRecord>> {
        let resp = self
            .client
            .get(self.url("/api/v1/files"))
            .query(&[("source", source)])
            .bearer_auth(&self.token)
            .send()
            .await
            .context("GET /api/v1/files")?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(vec![]);
        }
        resp.error_for_status()
            .context("GET /api/v1/files status")?
            .json::<Vec<FileRecord>>()
            .await
            .context("parsing file list")
    }

    /// POST /api/v1/bulk  — upserts, deletions, and scan-complete in one request (gzip-compressed).
    pub async fn bulk(&self, req: &BulkRequest) -> Result<()> {
        let json = serde_json::to_vec(req).context("serialising bulk request")?;
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&json).context("compressing bulk request")?;
        let compressed = encoder.finish().context("finishing gzip stream")?;

        let resp = self.client
            .post(self.url("/api/v1/bulk"))
            .bearer_auth(&self.token)
            .header("Content-Encoding", "gzip")
            .header("Content-Type", "application/json")
            .body(compressed)
            .send()
            .await
            .context("POST /api/v1/bulk")?;

        let status = resp.status();
        if status == reqwest::StatusCode::ACCEPTED || status.is_success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!("POST /api/v1/bulk: unexpected status {status}"))
        }
    }

    /// GET /api/v1/context
    pub async fn context(
        &self,
        source: &str,
        path: &str,
        archive_path: Option<&str>,
        line: usize,
        window: usize,
    ) -> Result<ContextResponse> {
        let mut req = self
            .client
            .get(self.url("/api/v1/context"))
            .bearer_auth(&self.token)
            .query(&[
                ("source", source),
                ("path", path),
                ("line", &line.to_string()),
                ("window", &window.to_string()),
            ]);
        if let Some(ap) = archive_path {
            req = req.query(&[("archive_path", ap)]);
        }
        req.send()
            .await
            .context("GET /api/v1/context")?
            .error_for_status()
            .context("context status")?
            .json::<ContextResponse>()
            .await
            .context("parsing context response")
    }

    /// GET /api/v1/search
    pub async fn search(
        &self,
        query: &str,
        mode: &str,
        sources: &[String],
        limit: usize,
        offset: usize,
    ) -> Result<SearchResponse> {
        let mut req = self
            .client
            .get(self.url("/api/v1/search"))
            .bearer_auth(&self.token)
            .query(&[
                ("q", query),
                ("mode", mode),
                ("limit", &limit.to_string()),
                ("offset", &offset.to_string()),
            ]);
        for s in sources {
            req = req.query(&[("source", s.as_str())]);
        }
        req.send()
            .await
            .context("GET /api/v1/search")?
            .error_for_status()
            .context("search status")?
            .json::<SearchResponse>()
            .await
            .context("parsing search response")
    }
}
