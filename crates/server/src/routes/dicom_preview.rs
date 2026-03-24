use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use crate::AppState;
use super::check_auth;

#[derive(Deserialize)]
pub struct DicomPreviewParams {
    source: String,
    path: String,
}

/// GET /api/v1/dicom-preview?source=<name>&path=<relative_path>
///
/// Spawns `find-preview-dicom` against the file and returns the PNG output.
/// Returns 422 when the file cannot be converted (unsupported transfer syntax,
/// corrupt file, or find-preview-dicom not available on this installation).
pub async fn get_dicom_preview(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<DicomPreviewParams>,
) -> Response {
    if let Err(s) = check_auth(&state, &headers) {
        return s.into_response();
    }

    // Validate path components.
    if params.path.starts_with('/') || params.path.starts_with('\\') {
        return StatusCode::BAD_REQUEST.into_response();
    }
    for component in std::path::Path::new(&params.path).components() {
        if matches!(
            component,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        ) {
            return StatusCode::BAD_REQUEST.into_response();
        }
    }

    // Resolve source root.
    let source_root_str = match state
        .config
        .sources
        .get(&params.source)
        .and_then(|sc| sc.path.as_deref())
    {
        Some(p) => p.to_owned(),
        None => {
            tracing::warn!(source = %params.source, "dicom-preview: source not configured or has no path");
            return StatusCode::NOT_FOUND.into_response();
        }
    };

    let source_root = std::path::Path::new(&source_root_str);
    let full_path = source_root.join(&params.path);

    let canonical_root = match source_root.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(source = %params.source, error = %e, "dicom-preview: source root not accessible");
            return StatusCode::NOT_FOUND.into_response();
        }
    };
    let canonical_full = match full_path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(source = %params.source, path = %params.path, error = %e, "dicom-preview: file not found");
            return StatusCode::NOT_FOUND.into_response();
        }
    };
    if !canonical_full.starts_with(&canonical_root) {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let timeout_secs = state.config.scan.dicom_preview_timeout_secs;
    let binary = resolve_preview_binary();

    let result = tokio::task::spawn_blocking(move || {
        run_preview_binary(&binary, &canonical_full, timeout_secs)
    })
    .await
    .unwrap_or_else(|e| Err(format!("task panic: {e}")));

    match result {
        Ok(png_bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/png")
            .header(header::CONTENT_LENGTH, png_bytes.len().to_string())
            .body(Body::from(png_bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
        Err(e) => {
            tracing::warn!(source = %params.source, path = %params.path, error = %e, "dicom-preview: conversion failed");
            StatusCode::UNPROCESSABLE_ENTITY.into_response()
        }
    }
}

fn resolve_preview_binary() -> String {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("find-preview-dicom");
            if candidate.exists() {
                return candidate.to_string_lossy().into_owned();
            }
            if let Some(parent) = dir.parent() {
                let candidate = parent.join("find-preview-dicom");
                if candidate.exists() {
                    return candidate.to_string_lossy().into_owned();
                }
            }
        }
    }
    "find-preview-dicom".to_string()
}

fn run_preview_binary(
    binary: &str,
    path: &std::path::Path,
    timeout_secs: u64,
) -> Result<Vec<u8>, String> {
    use std::process::Command;

    let mut child = Command::new(binary)
        .arg(path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to spawn {binary}: {e}"))?;

    // Drain stdout in a background thread to avoid pipe-buffer deadlock:
    // the child may write more than the OS pipe buffer (typically 64 KB),
    // so it blocks waiting for the parent to read — while the parent is
    // waiting for the child to exit.  Draining concurrently breaks the cycle.
    let stdout_thread = {
        use std::io::Read as _;
        let mut stdout = child.stdout.take().ok_or("no stdout")?;
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            stdout.read_to_end(&mut buf).map(|_| buf)
        })
    };

    // Poll for completion with timeout.
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = stdout_thread.join();
                    return Err(format!("timed out after {timeout_secs}s"));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(format!("wait error: {e}")),
        }
    };

    let png_bytes = stdout_thread
        .join()
        .map_err(|_| "stdout thread panicked".to_string())?
        .map_err(|e| e.to_string())?;

    if status.success() {
        Ok(png_bytes)
    } else {
        let mut stderr_buf = Vec::new();
        if let Some(mut se) = child.stderr.take() {
            use std::io::Read as _;
            let _ = se.read_to_end(&mut stderr_buf);
        }
        Err(format!("exit {:?}: {}", status.code(), String::from_utf8_lossy(&stderr_buf)))
    }
}
