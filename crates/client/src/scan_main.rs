mod api;
mod batch;
mod extract;
mod lazy_header;
mod scan;
mod subprocess;
mod upload;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};

use find_common::config::{default_config_path, parse_client_config};
use find_common::logging::LogIgnoreFilter;
use scan::{ScanOptions, ScanSource};

#[derive(Parser)]
#[command(name = "find-scan", about = "Index files and submit to find-anything server")]
struct Args {
    /// Path to client config file (default: /etc/find-anything/client.toml as root, else ~/.config/find-anything/client.toml)
    #[arg(long)]
    config: Option<String>,

    /// Force a full reindex regardless of mtime
    #[arg(long)]
    full: bool,

    /// Suppress per-file processing logs (only log warnings, errors, and summary)
    #[arg(long)]
    quiet: bool,

    /// Dry run: walk the filesystem and compare with the server's current state,
    /// but do not extract content or submit any changes. Prints a summary of
    /// how many files would be added, modified, unchanged, and deleted.
    /// Cannot be combined with a single-file argument.
    #[arg(long)]
    dry_run: bool,

    /// Scan a single file instead of all configured sources. The file must be
    /// under one of the configured source paths. Mtime checking is skipped —
    /// the file is always (re-)indexed.
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,
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

    let opts = ScanOptions {
        full: args.full,
        quiet: args.quiet,
        dry_run: args.dry_run,
    };

    // Single-file mode: scan one specific file and exit.
    if opts.dry_run && args.file.is_some() {
        anyhow::bail!("--dry-run cannot be combined with a single-file argument");
    }

    if let Some(path) = args.file {
        let abs = std::fs::canonicalize(&path)
            .with_context(|| format!("cannot access {}", path.display()))?;
        anyhow::ensure!(abs.is_file(), "{} is not a file", abs.display());

        // Find the source whose configured path is the longest prefix of `abs`.
        let mut best: Option<(&find_common::config::SourceConfig, PathBuf, PathBuf)> = None;
        for source in &config.sources {
            let root_canon = std::fs::canonicalize(&source.path).unwrap_or_else(|_| PathBuf::from(&source.path));
            if let Ok(rel) = abs.strip_prefix(&root_canon) {
                let longer = best.as_ref()
                    .is_none_or(|(_, rc, _)| root_canon.as_os_str().len() > rc.as_os_str().len());
                if longer {
                    best = Some((source, root_canon, rel.to_path_buf()));
                }
            }
        }
        let (source, _, rel) = best.ok_or_else(|| {
            let paths = config.sources.iter()
                .map(|s| s.path.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::anyhow!(
                "{} is not under any configured source path\nConfigured paths: {paths}",
                abs.display()
            )
        })?;
        let rel_path = scan::normalise_path_sep(&rel.to_string_lossy());

        tracing::info!("Scanning single file: {} (source: {}, rel: {})", abs.display(), source.name, rel_path);
        let scan_source = ScanSource {
            name: &source.name,
            paths: std::slice::from_ref(&source.path),
            base_url: source.base_url.as_deref(),
            include: &source.include,
        };
        scan::scan_single_file(&client, &scan_source, &rel_path, &abs, &config.scan, &opts).await?;
        return Ok(());
    }

    // Scan all configured sources
    for source in &config.sources {
        tracing::info!("Scanning source: {}", source.name);
        let scan_source = ScanSource {
            name: &source.name,
            paths: std::slice::from_ref(&source.path),
            base_url: source.base_url.as_deref(),
            include: &source.include,
        };
        scan::run_scan(&client, &scan_source, &config.scan, &opts).await?;
    }

    Ok(())
}
