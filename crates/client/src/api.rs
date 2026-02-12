#![allow(dead_code)] // methods are used by different binaries in this crate

use anyhow::{Context, Result};
use reqwest::Client;

use find_common::api::{
    DeleteRequest, FileRecord, ScanCompleteRequest, SearchResponse, UpsertRequest,
};

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

    /// PUT /api/v1/files  — upsert a batch of files + lines.
    pub async fn upsert_files(&self, req: &UpsertRequest) -> Result<()> {
        self.client
            .put(self.url("/api/v1/files"))
            .bearer_auth(&self.token)
            .json(req)
            .send()
            .await
            .context("PUT /api/v1/files")?
            .error_for_status()
            .context("PUT /api/v1/files status")?;
        Ok(())
    }

    /// DELETE /api/v1/files  — remove files from the index.
    pub async fn delete_files(&self, req: &DeleteRequest) -> Result<()> {
        self.client
            .delete(self.url("/api/v1/files"))
            .bearer_auth(&self.token)
            .json(req)
            .send()
            .await
            .context("DELETE /api/v1/files")?
            .error_for_status()
            .context("DELETE /api/v1/files status")?;
        Ok(())
    }

    /// POST /api/v1/scan-complete  — update last_scan timestamp.
    pub async fn scan_complete(&self, source: &str, timestamp: i64) -> Result<()> {
        self.client
            .post(self.url("/api/v1/scan-complete"))
            .bearer_auth(&self.token)
            .json(&ScanCompleteRequest {
                source: source.to_string(),
                timestamp,
            })
            .send()
            .await
            .context("POST /api/v1/scan-complete")?
            .error_for_status()
            .context("POST /api/v1/scan-complete status")?;
        Ok(())
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
