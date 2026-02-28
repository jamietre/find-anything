use std::path::Path;
use std::process;
use find_common::config::ExtractorConfig;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "warn".into()))
        .with(tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .without_time()
            .with_ansi(false))
        .init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: find-extract-archive <file-path> [max-size-kb] [max-depth] [max-line-length]");
        eprintln!();
        eprintln!("Extracts content from archive files and outputs JSON.");
        eprintln!();
        eprintln!("Supported formats:");
        eprintln!("  - ZIP archives (.zip)");
        eprintln!("  - TAR archives (.tar, .tar.gz, .tgz, .tar.bz2, .tbz2, .tar.xz, .txz)");
        eprintln!("  - Single-file compressed (.gz, .bz2, .xz)");
        eprintln!("  - 7Z archives (.7z)");
        eprintln!();
        eprintln!("For each archive member:");
        eprintln!("  - Indexes the filename");
        eprintln!("  - Text files: extracts line content");
        eprintln!("  - PDF files: extracts text");
        eprintln!("  - Media files: extracts metadata");
        eprintln!("  - Nested archives: recursively extracts up to max-depth");
        process::exit(1);
    }

    let path = Path::new(&args[1]);
    let cfg = ExtractorConfig {
        max_size_kb: args.get(2).and_then(|s| s.parse().ok()).unwrap_or(10240),
        max_depth: args.get(3).and_then(|s| s.parse().ok()).unwrap_or(10),
        max_line_length: args.get(4).and_then(|s| s.parse().ok()).unwrap_or(120),
        ..Default::default()
    };

    let mut batches: Vec<find_extract_archive::MemberBatch> = Vec::new();
    match find_extract_archive::extract_streaming(path, &cfg, &mut |batch| {
        batches.push(batch);
    }) {
        Ok(()) => {
            match serde_json::to_string_pretty(&batches) {
                Ok(json) => {
                    println!("{}", json);
                    process::exit(0);
                }
                Err(e) => {
                    eprintln!("Error serializing to JSON: {}", e);
                    process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Error extracting archive from {}: {}", path.display(), e);
            process::exit(1);
        }
    }
}
