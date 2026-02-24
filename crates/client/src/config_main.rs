use anyhow::{Context, Result};
use clap::Parser;

use find_common::config::{default_config_path, parse_client_config};

#[derive(Parser)]
#[command(
    name = "find-config",
    about = "Show the effective client configuration (file values merged with defaults)"
)]
struct Args {
    /// Path to client config file (default: ~/.config/find-anything/client.toml)
    #[arg(long)]
    config: Option<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let config_path = args.config.unwrap_or_else(default_config_path);

    let config_str = std::fs::read_to_string(&config_path)
        .with_context(|| format!("reading config {config_path}"))?;

    // Parse via the shared helper — emits warnings for unknown keys.
    let config = parse_client_config(&config_str)?;

    // Serialize back to TOML so defaults are visible alongside explicit values.
    let effective = toml::to_string_pretty(&config).context("serializing config")?;

    println!("# Effective configuration (file: {config_path})");
    println!("# Values shown include defaults for any fields not set in your file.");
    println!();
    print!("{effective}");

    Ok(())
}
