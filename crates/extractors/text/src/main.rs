use std::path::Path;
use std::process;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: find-extract-text <file-path>");
        eprintln!();
        eprintln!("Extracts text content from files and outputs JSON.");
        eprintln!();
        eprintln!("Supported formats:");
        eprintln!("  - Plain text files");
        eprintln!("  - Source code (Rust, Python, JavaScript, etc.)");
        eprintln!("  - Markdown (with frontmatter extraction)");
        eprintln!("  - Config files (JSON, YAML, TOML, etc.)");
        process::exit(1);
    }

    let path = Path::new(&args[1]);

    // Optional: max size parameter
    let max_size_kb = if args.len() > 2 {
        args[2].parse().unwrap_or(10240)
    } else {
        10240 // 10 MB default
    };

    match find_extract_text::extract(path, max_size_kb) {
        Ok(lines) => {
            // Output JSON to stdout
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
            eprintln!("Error extracting text from {}: {}", path.display(), e);
            process::exit(1);
        }
    }
}
