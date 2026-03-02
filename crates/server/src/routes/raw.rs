use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use tokio::fs::File;
use tokio_util::io::ReaderStream;

use crate::AppState;

use super::check_auth;

#[derive(Deserialize)]
pub struct RawParams {
    source: String,
    path: String,
    /// When `convert=png`, the server decodes the file with the `image` crate
    /// and re-encodes it as PNG. Useful for formats browsers cannot display
    /// natively (e.g. TIFF).
    convert: Option<String>,
}

/// GET /api/v1/raw?source=<name>&path=<relative_path>[&convert=png]
///
/// Streams the original file from the source's configured filesystem root.
/// Requires the source to have a `path` configured in `[sources.<name>]`.
pub async fn get_raw(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Query(params): Query<RawParams>,
) -> Response {
    if let Err(s) = check_auth(&state, &headers) {
        return s.into_response();
    }

    // Reject archive member paths.
    if params.path.contains("::") {
        return StatusCode::BAD_REQUEST.into_response();
    }

    // Reject paths that start with '/' or contain '..' components.
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

    // Look up the source's configured filesystem root.
    let source_root_str = match state
        .config
        .sources
        .get(&params.source)
        .and_then(|sc| sc.path.as_deref())
    {
        Some(p) => p.to_owned(),
        None => return StatusCode::NOT_FOUND.into_response(),
    };

    let source_root = std::path::Path::new(&source_root_str);
    let full_path = source_root.join(&params.path);

    // Canonicalize both paths and confirm the file is still inside the root.
    let canonical_root = match source_root.canonicalize() {
        Ok(p) => p,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    let canonical_full = match full_path.canonicalize() {
        Ok(p) => p,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };
    if !canonical_full.starts_with(&canonical_root) {
        return StatusCode::BAD_REQUEST.into_response();
    }

    // If convert=png is requested, decode the image and re-encode as PNG.
    // Build Content-Disposition with the real filename so browser PDF/image
    // viewers show the actual name rather than "raw" (the endpoint path).
    let display_filename = canonical_full
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    // For convert=png the extension changes, so use the stem + ".png".
    let png_filename = canonical_full
        .file_stem()
        .and_then(|n| n.to_str())
        .map(|stem| format!("{stem}.png"))
        .unwrap_or_else(|| "file.png".to_string());
    // Sanitize: strip any double-quotes to avoid breaking the header value.
    let safe_name = display_filename.replace('"', "");
    let safe_png_name = png_filename.replace('"', "");
    let disposition = format!("inline; filename=\"{safe_name}\"");
    let png_disposition = format!("inline; filename=\"{safe_png_name}\"");

    if params.convert.as_deref() == Some("png") {
        let bytes = match tokio::fs::read(&canonical_full).await {
            Ok(b) => b,
            Err(_) => return StatusCode::NOT_FOUND.into_response(),
        };
        let png_bytes = match tokio::task::spawn_blocking(move || -> Result<Vec<u8>, ()> {
            let img = image::load_from_memory(&bytes).map_err(|_| ())?;
            let mut out = Vec::new();
            img.write_to(
                &mut std::io::Cursor::new(&mut out),
                image::ImageFormat::Png,
            )
            .map_err(|_| ())?;
            Ok(out)
        })
        .await
        {
            Ok(Ok(b)) => b,
            _ => return StatusCode::UNPROCESSABLE_ENTITY.into_response(),
        };
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/png")
            .header(header::CONTENT_DISPOSITION, png_disposition)
            .body(Body::from(png_bytes))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    // Default: stream the file as-is.
    let file = match File::open(&canonical_full).await {
        Ok(f) => f,
        Err(_) => return StatusCode::NOT_FOUND.into_response(),
    };

    let mime = mime_guess::from_path(&canonical_full).first_or_octet_stream();
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.essence_str())
        .header(header::CONTENT_DISPOSITION, disposition)
        .body(body)
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
