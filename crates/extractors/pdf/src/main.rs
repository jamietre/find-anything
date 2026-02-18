use std::path::Path;
use std::process;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: find-extract-pdf <file-path>");
        eprintln!();
        eprintln!("Extracts text content from PDF files and outputs JSON.");
        process::exit(1);
    }

    let path = Path::new(&args[1]);
    let max_size_kb = if args.len() > 2 {
        args[2].parse().unwrap_or(102400)
    } else {
        102400 // 100 MB default for PDFs
    };
    let max_line_length = if args.len() > 3 {
        args[3].parse().unwrap_or(120)
    } else {
        120
    };

    match find_extract_pdf::extract(path, max_size_kb, max_line_length) {
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
            eprintln!("Error extracting PDF from {}: {}", path.display(), e);
            process::exit(1);
        }
    }
}
