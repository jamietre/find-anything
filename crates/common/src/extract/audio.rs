// Post-MVP: audio tag extraction via id3 / metaflac / mp4ameta crates.
// Stub returns no lines until implemented.

use std::path::Path;
use crate::api::IndexLine;
use crate::extract::Extractor;

pub struct AudioExtractor;

impl Extractor for AudioExtractor {
    fn accepts(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| is_audio_ext(e))
            .unwrap_or(false)
    }

    fn extract(&self, _path: &Path) -> anyhow::Result<Vec<IndexLine>> {
        Ok(vec![])
    }
}

pub fn is_audio_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "mp3" | "flac" | "ogg" | "m4a" | "aac" | "mp4" | "opus" | "wav"
    )
}
