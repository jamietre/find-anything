use std::path::Path;
use crate::api::IndexLine;
use crate::extract::Extractor;
use audio_video_metadata::{get_format_from_file, Metadata};

pub struct VideoExtractor;

impl Extractor for VideoExtractor {
    fn accepts(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| is_video_ext(e))
            .unwrap_or(false)
    }

    fn extract(&self, path: &Path) -> anyhow::Result<Vec<IndexLine>> {
        extract_video_metadata(path)
    }
}

fn extract_video_metadata(path: &Path) -> anyhow::Result<Vec<IndexLine>> {
    match get_format_from_file(path) {
        Ok(Metadata::Video(m)) => {
            let mut lines = Vec::new();

            // Format type (e.g., "MP4", "WebM", "Ogg")
            lines.push(make_meta_line("format", &format!("{:?}", m.format)));

            // Dimensions (width x height)
            lines.push(make_meta_line(
                "resolution",
                &format!("{}x{}", m.dimensions.width, m.dimensions.height)
            ));

            // Duration from audio track if available
            if let Some(duration) = m.audio.duration {
                let secs = duration.as_secs();
                let mins = secs / 60;
                let secs = secs % 60;
                lines.push(make_meta_line("duration", &format!("{}:{:02}", mins, secs)));
            }

            Ok(lines)
        }
        Ok(Metadata::Audio(_)) => {
            // File was detected as audio, not video - skip
            Ok(vec![])
        }
        Err(_) => {
            // Failed to parse - return empty
            Ok(vec![])
        }
    }
}

fn make_meta_line(key: &str, value: &str) -> IndexLine {
    IndexLine {
        archive_path: None,
        line_number: 0,
        content: format!("[VIDEO:{}] {}", key, value),
    }
}

pub fn is_video_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "mp4" | "m4v" | "mkv" | "webm" | "ogv" | "ogg" | "avi" | "mov" | "wmv" | "flv" | "mpg" | "mpeg" | "3gp"
    )
}
