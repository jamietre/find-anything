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
    routing::{get, post},
    Router,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use find_common::config::ServerAppConfig;

pub struct AppState {
    pub config: ServerAppConfig,
    pub data_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "find_server=info,tower_http=info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/etc/find-anything/server.toml".into());

    let config_str = std::fs::read_to_string(&config_path)
        .with_context(|| format!("reading config: {config_path}"))?;
    let config: ServerAppConfig = toml::from_str(&config_str)
        .context("parsing server config")?;

    let data_dir = PathBuf::from(&config.server.data_dir);
    std::fs::create_dir_all(data_dir.join("sources"))
        .context("creating sources directory")?;
    std::fs::create_dir_all(data_dir.join("inbox").join("failed"))
        .context("creating inbox directory")?;

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
        .route("/api/v1/tree",           get(routes::list_dir))
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .with_context(|| format!("binding to {bind}"))?;

    tracing::info!("listening on {bind}");
    axum::serve(listener, app).await.context("server error")?;

    Ok(())
}
