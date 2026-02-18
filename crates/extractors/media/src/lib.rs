use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use find_common::api::IndexLine;
use find_common::config::ExtractorConfig;
use audio_video_metadata::{get_format_from_file, Metadata};
use id3::TagLike;

/// Extract metadata from media files (images, audio, video).
///
/// Supports:
/// - Images: EXIF metadata (JPEG, TIFF, HEIC, RAW formats)
/// - Audio: ID3/Vorbis/M4A tags (MP3, FLAC, M4A, AAC)
/// - Video: Format, resolution, duration (MP4, MKV, WebM, etc.)
///
/// # Arguments
/// * `path` - Path to the media file
/// * `_max_size_kb` - Maximum file size in KB (currently unused)
///
/// # Returns
/// Vector of IndexLine objects with metadata at line_number=0
pub fn extract(path: &Path, _cfg: &ExtractorConfig) -> anyhow::Result<Vec<IndexLine>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Dispatch to appropriate extractor based on extension
    if is_image_ext(&ext) {
        extract_image(path)
    } else if is_audio_ext(&ext) {
        extract_audio(path)
    } else if is_video_ext(&ext) {
        extract_video(path)
    } else {
        Ok(vec![])
    }
}

/// Check if a file is a media file based on extension.
pub fn accepts(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext = ext.to_lowercase();
        is_image_ext(&ext) || is_audio_ext(&ext) || is_video_ext(&ext)
    } else {
        false
    }
}

// ============================================================================
// IMAGE EXTRACTION
// ============================================================================

fn extract_image(path: &Path) -> anyhow::Result<Vec<IndexLine>> {
    let file = File::open(path)?;
    let mut bufreader = BufReader::new(file);

    match exif::Reader::new().read_from_container(&mut bufreader) {
        Ok(exif) => {
            let mut lines = Vec::new();

            // Extract all EXIF fields
            for field in exif.fields() {
                let tag = field.tag.to_string();
                let value = field.display_value().to_string();

                // Skip empty or binary values
                if !value.is_empty() && !value.starts_with("[") {
                    lines.push(IndexLine {
                        archive_path: None,
                        line_number: 0,
                        content: format!("[EXIF:{}] {}", tag, value),
                    });
                }
            }

            Ok(lines)
        }
        Err(_) => {
            // Many images don't have EXIF data, or we can't read it
            // This is normal, just return empty results
            Ok(vec![])
        }
    }
}

pub fn is_image_ext(ext: &str) -> bool {
    matches!(
        ext,
        "jpg" | "jpeg" | "tiff" | "tif" | "heic" | "heif" | "webp"
        | "png" | "cr2" | "cr3" | "nef" | "arw" | "orf" | "rw2"
    )
}

// ============================================================================
// AUDIO EXTRACTION
// ============================================================================

fn extract_audio(path: &Path) -> anyhow::Result<Vec<IndexLine>> {
    let ext = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "mp3" => extract_mp3_tags(path),
        "flac" => extract_flac_tags(path),
        "m4a" | "aac" => extract_mp4_tags(path),
        _ => Ok(vec![]),  // Unsupported format
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
        line_number: 0,
        content: format!("[TAG:{}] {}", key, value),
    }
}

pub fn is_audio_ext(ext: &str) -> bool {
    matches!(
        ext,
        "mp3" | "flac" | "ogg" | "m4a" | "aac" | "opus" | "wav"
    )
}

// ============================================================================
// VIDEO EXTRACTION
// ============================================================================

fn extract_video(path: &Path) -> anyhow::Result<Vec<IndexLine>> {
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
        ext,
        "mp4" | "m4v" | "mkv" | "webm" | "ogv" | "ogg" | "avi" | "mov" | "wmv" | "flv" | "mpg" | "mpeg" | "3gp"
    )
}
