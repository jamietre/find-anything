mod api;
mod batch;
mod extract;
mod lazy_header;
mod scan;

use anyhow::{Context, Result};
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

use find_common::config::{default_config_path, parse_client_config};
use find_common::logging::LogIgnoreFilter;

#[derive(Parser)]
#[command(name = "find-scan", about = "Index files and submit to find-anything server")]
struct Args {
    /// Path to client config file (default: /etc/find-anything/client.toml as root, else ~/.config/find-anything/client.toml)
    #[arg(long)]
    config: Option<String>,

    /// Force a full reindex regardless of mtime
    #[arg(long)]
    full: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "warn,find_scan=info".into()))
        .with(lazy_header::FileHeaderLayer)
        .with(tracing_subscriber::fmt::layer().with_filter(LogIgnoreFilter))
        .init();

    let args = Args::parse();

    let config_path = args.config.unwrap_or_else(default_config_path);
    let config_str = std::fs::read_to_string(&config_path)
        .with_context(|| format!("reading config {config_path}"))?;
    let config = parse_client_config(&config_str)?;

    if let Err(e) = find_common::logging::set_ignore_patterns(&config.log.ignore) {
        tracing::warn!("invalid log ignore pattern: {e}");
    }

    let client = api::ApiClient::new(&config.server.url, &config.server.token);

    if config.sources.is_empty() {
        tracing::info!("No sources configured — nothing to scan.");
        return Ok(());
    }

    // Scan all configured sources
    for source in &config.sources {
        tracing::info!("Scanning source: {}", source.name);
        scan::run_scan(
            &client,
            &source.name,
            &source.paths,
            &config.scan,
            source.base_url.as_deref(),
            args.full,
        )
        .await?;
    }

    Ok(())
}
