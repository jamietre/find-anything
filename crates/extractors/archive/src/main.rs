use std::path::Path;
use std::process;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: find-extract-archive <file-path> [max-size-kb] [max-depth]");
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
    let max_size_kb = if args.len() > 2 {
        args[2].parse().unwrap_or(10240)
    } else {
        10240 // 10 MB default
    };
    let max_depth = if args.len() > 3 {
        args[3].parse().unwrap_or(10)
    } else {
        10
    };
    let max_line_length = if args.len() > 4 {
        args[4].parse().unwrap_or(120)
    } else {
        120
    };

    match find_extract_archive::extract(path, max_size_kb, max_depth, max_line_length) {
        Ok(lines) => {
            match serde_json::to_string_pretty(&lines) {
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
