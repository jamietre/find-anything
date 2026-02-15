use std::path::Path;
use std::process;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: find-extract-archive <file-path> [max-size-kb]");
        eprintln!();
        eprintln!("Extracts content from archive files and outputs JSON.");
        eprintln!();
        eprintln!("Supported formats:");
        eprintln!("  - ZIP archives");
        eprintln!("  - TAR archives (including .tar.gz, .tgz)");
        eprintln!("  - 7Z archives");
        eprintln!();
        eprintln!("For each archive member:");
        eprintln!("  - Indexes the filename");
        eprintln!("  - Extracts text content from text files");
        process::exit(1);
    }

    let path = Path::new(&args[1]);
    let max_size_kb = if args.len() > 2 {
        args[2].parse().unwrap_or(10240)
    } else {
        10240 // 10 MB default
    };

    match find_extract_archive::extract(path, max_size_kb) {
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
