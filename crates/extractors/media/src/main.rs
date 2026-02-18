use std::path::Path;
use std::process;
use find_common::config::ExtractorConfig;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: find-extract-media <file-path>");
        eprintln!();
        eprintln!("Extracts metadata from media files and outputs JSON.");
        eprintln!();
        eprintln!("Supported formats:");
        eprintln!("  Images: JPEG, TIFF, HEIC, PNG, RAW (EXIF metadata)");
        eprintln!("  Audio: MP3, FLAC, M4A, AAC (ID3/Vorbis tags)");
        eprintln!("  Video: MP4, MKV, WebM, AVI, MOV (format/resolution/duration)");
        process::exit(1);
    }

    let path = Path::new(&args[1]);
    let cfg = ExtractorConfig {
        max_size_kb: args.get(2).and_then(|s| s.parse().ok()).unwrap_or(102400),
        max_depth: 10,
        max_line_length: 120,
    };

    match find_extract_media::extract(path, &cfg) {
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
            eprintln!("Error extracting media from {}: {}", path.display(), e);
            process::exit(1);
        }
    }
}
