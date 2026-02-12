use std::io::Read;
use std::path::Path;

use crate::api::IndexLine;
use crate::extract::Extractor;

pub struct TextExtractor;

impl Extractor for TextExtractor {
    fn accepts(&self, path: &Path) -> bool {
        // Fast path: known text extensions
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if is_text_ext(ext) {
                return true;
            }
            // Known binary extensions are not text
            if is_binary_ext(ext) {
                return false;
            }
        }
        // Fallback: sniff first 8 KB
        if let Ok(mut f) = std::fs::File::open(path) {
            let mut buf = vec![0u8; 8192];
            if let Ok(n) = f.read(&mut buf) {
                buf.truncate(n);
                return content_inspector::inspect(&buf).is_text();
            }
        }
        false
    }

    fn extract(&self, path: &Path) -> anyhow::Result<Vec<IndexLine>> {
        let content = std::fs::read_to_string(path)?;
        Ok(lines_from_str(&content, None))
    }
}

/// Convert a string to IndexLines (used by text extractor and archive text entries).
pub fn lines_from_str(content: &str, archive_path: Option<String>) -> Vec<IndexLine> {
    content
        .lines()
        .enumerate()
        .map(|(i, line)| IndexLine {
            archive_path: archive_path.clone(),
            line_number: i + 1,
            content: line.to_string(),
        })
        .collect()
}

pub fn is_text_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "rs" | "ts" | "js" | "jsx" | "tsx" | "py" | "rb" | "go" | "java"
        | "c" | "cpp" | "h" | "hpp" | "cs" | "swift" | "kt" | "scala"
        | "r" | "m" | "pl" | "sh" | "bash" | "zsh" | "fish" | "ps1"
        | "lua" | "vim" | "el" | "clj" | "hs" | "ml" | "fs" | "ex"
        | "erl" | "dart" | "jl" | "nim" | "zig" | "s" | "asm"
        | "html" | "htm" | "xml" | "svg" | "md" | "markdown" | "rst"
        | "tex" | "adoc" | "org"
        | "json" | "yaml" | "yml" | "toml" | "ini" | "cfg" | "conf"
        | "env" | "properties" | "plist" | "nix" | "hcl" | "tf"
        | "csv" | "tsv" | "sql" | "graphql" | "gql" | "proto"
        | "txt" | "log" | "diff" | "patch"
        | "lock"  // Cargo.lock, package-lock.json, etc.
    )
}

fn is_binary_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "ico" | "webp" | "heic"
        | "mp3" | "mp4" | "avi" | "mov" | "mkv" | "flac" | "wav" | "ogg"
        | "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx"
        | "zip" | "tar" | "gz" | "bz2" | "xz" | "7z" | "rar"
        | "exe" | "dll" | "so" | "dylib" | "class" | "jar"
        | "o" | "a" | "lib" | "obj" | "wasm"
        | "db" | "sqlite" | "sqlite3"
        | "ttf" | "otf" | "woff" | "woff2"
    )
}
