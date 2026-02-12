mod watch;

use anyhow::{Context, Result};
use clap::Parser;

use find_common::config::ClientConfig;

#[derive(Parser)]
#[command(name = "find-watch", about = "Watch filesystem and update index in real-time (post-MVP)")]
struct Args {
    #[arg(long, default_value = "/etc/find-anything/client.toml")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "find_watch=info".into()),
        )
        .init();

    let args = Args::parse();
    let config_str = std::fs::read_to_string(&args.config)
        .with_context(|| format!("reading config {}", args.config))?;
    let config: ClientConfig = toml::from_str(&config_str).context("parsing client config")?;

    watch::run_watch(&config).await
}
