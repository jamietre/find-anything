use std::path::Path;
use std::process;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: find-extract-epub <file-path>");
        eprintln!();
        eprintln!("Extracts text from EPUB ebooks and outputs JSON.");
        eprintln!();
        eprintln!("Supported formats: .epub");
        process::exit(1);
    }

    let path = Path::new(&args[1]);

    let max_size_kb = if args.len() > 2 {
        args[2].parse().unwrap_or(10240)
    } else {
        10240 // 10 MB default
    };

    match find_extract_epub::extract(path, max_size_kb) {
        Ok(lines) => match serde_json::to_string_pretty(&lines) {
            Ok(json) => {
                println!("{}", json);
                process::exit(0);
            }
            Err(e) => {
                eprintln!("Error serializing to JSON: {}", e);
                process::exit(1);
            }
        },
        Err(e) => {
            eprintln!("Error extracting from {}: {}", path.display(), e);
            process::exit(1);
        }
    }
}
