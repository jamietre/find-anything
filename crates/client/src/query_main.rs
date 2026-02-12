mod api;

use anyhow::{Context, Result};
use clap::Parser;
use colored::Colorize;

use find_common::config::ClientConfig;

#[derive(Parser)]
#[command(name = "find", about = "Search the find-anything index")]
struct Args {
    /// Search pattern
    pattern: String,

    /// Matching mode
    #[arg(long, default_value = "fuzzy")]
    mode: String,

    /// Only search these sources (repeatable)
    #[arg(long = "source")]
    sources: Vec<String>,

    /// Maximum results to show
    #[arg(long, default_value = "50")]
    limit: usize,

    /// Skip first N results
    #[arg(long, default_value = "0")]
    offset: usize,

    /// Suppress color output
    #[arg(long)]
    no_color: bool,

    /// Path to client config file
    #[arg(long, default_value = "/etc/find-anything/client.toml")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    if args.no_color {
        colored::control::set_override(false);
    }

    let config_str = std::fs::read_to_string(&args.config)
        .with_context(|| format!("reading config {}", args.config))?;
    let config: ClientConfig = toml::from_str(&config_str).context("parsing client config")?;

    let client = api::ApiClient::new(&config.server.url, &config.server.token);

    let resp = client
        .search(
            &args.pattern,
            &args.mode,
            &args.sources,
            args.limit,
            args.offset,
        )
        .await?;

    if resp.results.is_empty() {
        eprintln!("no results");
        return Ok(());
    }

    for hit in &resp.results {
        let source_tag = format!("[{}]", hit.source).cyan().to_string();
        let path = match &hit.archive_path {
            Some(inner) => format!("{}::{}", hit.path, inner),
            None => hit.path.clone(),
        };
        let loc = format!("{}:{}", path, hit.line_number).green().to_string();
        let snippet = hit.snippet.trim();
        println!("{} {}  {}", source_tag, loc, snippet);
    }

    eprintln!("({} total)", resp.total);
    Ok(())
}
