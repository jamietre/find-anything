mod api;
mod scan;

use anyhow::{Context, Result};
use clap::Parser;

use find_common::config::ClientConfig;

#[derive(Parser)]
#[command(name = "find-scan", about = "Index files and submit to find-anything server")]
struct Args {
    /// Path to client config file
    #[arg(long, default_value = "/etc/find-anything/client.toml")]
    config: String,

    /// Force a full reindex regardless of mtime
    #[arg(long)]
    full: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "find_scan=info".into()),
        )
        .init();

    let args = Args::parse();

    let config_str = std::fs::read_to_string(&args.config)
        .with_context(|| format!("reading config {}", args.config))?;
    let config: ClientConfig =
        toml::from_str(&config_str).context("parsing client config")?;

    let client = api::ApiClient::new(&config.server.url, &config.server.token);

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
