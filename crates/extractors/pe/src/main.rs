use std::io::{self, BufRead};
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let stdin = io::stdin();
    for line in stdin.lock().lines() {
        let path_str = line?;
        let path = Path::new(&path_str);

        if !find_extract_pe::accepts(path) {
            continue;
        }

        match find_extract_pe::extract(path, 100 * 1024) {
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
