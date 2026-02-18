use std::io::{self, BufRead};
use std::path::Path;
use find_common::config::ExtractorConfig;

fn main() -> anyhow::Result<()> {
    let cfg = ExtractorConfig {
        max_size_kb: 100 * 1024,
        max_depth: 10,
        max_line_length: 120,
    };

    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let path_str = line?;
        let path = Path::new(&path_str);

        if !find_extract_pe::accepts(path) {
            continue;
        }

        match find_extract_pe::extract(path, &cfg) {
            Ok(lines) => {
                for index_line in lines {
                    println!("{}", serde_json::to_string(&index_line)?);
                }
            }
            Err(e) => {
                eprintln!("Error extracting {}: {}", path_str, e);
            }
        }
    }

    Ok(())
}
