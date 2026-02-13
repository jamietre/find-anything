use std::path::Path;
use crate::api::IndexLine;
use crate::extract::Extractor;
use id3::TagLike;

pub struct AudioExtractor;

impl Extractor for AudioExtractor {
    fn accepts(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| is_audio_ext(e))
            .unwrap_or(false)
    }

    fn extract(&self, path: &Path) -> anyhow::Result<Vec<IndexLine>> {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match ext.as_str() {
            "mp3" => extract_mp3_tags(path),
            "flac" => extract_flac_tags(path),
            "m4a" | "mp4" | "aac" => extract_mp4_tags(path),
            _ => Ok(vec![]),  // Unsupported format
        }
    }
}

fn extract_mp3_tags(path: &Path) -> anyhow::Result<Vec<IndexLine>> {
    match id3::Tag::read_from_path(path) {
        Ok(tag) => {
            let mut lines = Vec::new();

            if let Some(title) = tag.title() {
                lines.push(make_tag_line("title", title));
            }
            if let Some(artist) = tag.artist() {
                lines.push(make_tag_line("artist", artist));
            }
            if let Some(album) = tag.album() {
                lines.push(make_tag_line("album", album));
            }
            if let Some(year) = tag.year() {
                lines.push(make_tag_line("year", &year.to_string()));
            }
            if let Some(genre) = tag.genre() {
                lines.push(make_tag_line("genre", genre));
            }
            for comment in tag.comments() {
                let text = &comment.text;
                if !text.is_empty() {
                    lines.push(make_tag_line("comment", text));
                }
            }

            Ok(lines)
        }
        Err(_) => {
            // File may not have ID3 tags or tags may be unreadable
            Ok(vec![])
        }
    }
}

fn extract_flac_tags(path: &Path) -> anyhow::Result<Vec<IndexLine>> {
    match metaflac::Tag::read_from_path(path) {
        Ok(tag) => {
            let mut lines = Vec::new();
            let vorbis = tag.vorbis_comments();

            if let Some(vorbis) = vorbis {
                for (key, values) in vorbis.comments.iter() {
                    for value in values {
                        if !value.is_empty() {
                            lines.push(make_tag_line(key, value));
                        }
                    }
                }
            }

            Ok(lines)
        }
        Err(_) => {
            // File may not have FLAC tags or tags may be unreadable
            Ok(vec![])
        }
    }
}

fn extract_mp4_tags(path: &Path) -> anyhow::Result<Vec<IndexLine>> {
    match mp4ameta::Tag::read_from_path(path) {
        Ok(tag) => {
            let mut lines = Vec::new();

            if let Some(title) = tag.title() {
                lines.push(make_tag_line("title", title));
            }
            if let Some(artist) = tag.artist() {
                lines.push(make_tag_line("artist", artist));
            }
            if let Some(album) = tag.album() {
                lines.push(make_tag_line("album", album));
            }
            if let Some(year) = tag.year() {
                lines.push(make_tag_line("year", year));
            }
            if let Some(genre) = tag.genre() {
                lines.push(make_tag_line("genre", genre));
            }
            if let Some(comment) = tag.comment() {
                if !comment.is_empty() {
                    lines.push(make_tag_line("comment", comment));
                }
            }

            Ok(lines)
        }
        Err(_) => {
            // File may not have MP4 tags or tags may be unreadable
            Ok(vec![])
        }
    }
}

fn make_tag_line(key: &str, value: &str) -> IndexLine {
    IndexLine {
        archive_path: None,
        line_number: 0,  // Metadata has no line concept
        content: format!("[TAG:{}] {}", key, value),
    }
}

pub fn is_audio_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "mp3" | "flac" | "ogg" | "m4a" | "aac" | "mp4" | "opus" | "wav"
    )
}
