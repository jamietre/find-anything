mod archive;
mod db;
mod fuzzy;
mod routes;
mod worker;

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum::{
    extract::DefaultBodyLimit,
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use find_common::config::{parse_server_config, ServerAppConfig};

// ── Embedded web UI ────────────────────────────────────────────────────────────
// In release builds, all files under web/build/ are compiled into the binary.
// In debug builds (no `debug-embed` feature), they are read from disk at runtime.

#[derive(rust_embed::RustEmbed)]
#[folder = "../../web/build/"]
struct WebAssets;

async fn serve_static(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match WebAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                [(header::CONTENT_TYPE, mime.essence_str())],
                content.data,
            )
                .into_response()
        }
        None => {
            // SPA fallback — serve index.html for any unknown path so the
            // SvelteKit client-side router can handle it.
            match WebAssets::get("index.html") {
                Some(content) => (
                    [(header::CONTENT_TYPE, "text/html")],
                    content.data,
                )
                    .into_response(),
                None => StatusCode::NOT_FOUND.into_response(),
            }
        }
    }
}

pub struct AppState {
    pub config: ServerAppConfig,
    pub data_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "warn,find_server=info,tower_http=info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/etc/find-anything/server.toml".into());

    let config_str = std::fs::read_to_string(&config_path)
        .with_context(|| format!("reading config: {config_path}"))?;
    let config = parse_server_config(&config_str)?;

    let data_dir = PathBuf::from(&config.server.data_dir);
    std::fs::create_dir_all(data_dir.join("sources"))
        .context("creating sources directory")?;
    std::fs::create_dir_all(data_dir.join("inbox").join("failed"))
        .context("creating inbox directory")?;

    // Fail fast if any existing source DB has an incompatible schema.
    db::check_all_sources(&data_dir.join("sources"))
        .context("schema version check failed — delete the listed database(s) and re-run `find-scan --full`")?;

    let bind = config.server.bind.clone();
    let state = Arc::new(AppState { config, data_dir: data_dir.clone() });

    // Spawn the async inbox worker.
    let worker_data_dir = data_dir.clone();
    tokio::spawn(async move {
        if let Err(e) = worker::start_inbox_worker(worker_data_dir).await {
            tracing::error!("Inbox worker failed: {e}");
        }
    });

    let app = Router::new()
        .route("/api/v1/sources",        get(routes::list_sources))
        .route("/api/v1/file",           get(routes::get_file))
        .route("/api/v1/files",          get(routes::list_files))
        .route("/api/v1/bulk",           post(routes::bulk))
        .route("/api/v1/search",         get(routes::search))
        .route("/api/v1/context",        get(routes::get_context))
        .route("/api/v1/context-batch",  post(routes::context_batch))
        .route("/api/v1/settings",       get(routes::get_settings))
        .route("/api/v1/metrics",        get(routes::get_metrics))
        .route("/api/v1/stats",          get(routes::get_stats))
        .route("/api/v1/errors",         get(routes::get_errors))
        .route("/api/v1/tree",           get(routes::list_dir))
        .route("/api/v1/admin/inbox",       get(routes::inbox_status).delete(routes::inbox_clear))
        .route("/api/v1/admin/inbox/retry", post(routes::inbox_retry))
        .fallback(serve_static)
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .with_context(|| format!("binding to {bind}"))?;

    tracing::info!("listening on {bind}");
    axum::serve(listener, app).await.context("server error")?;

    Ok(())
}
