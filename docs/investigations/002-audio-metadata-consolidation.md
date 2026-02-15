# Investigation: Audio Metadata Consolidation

**Date**: 2026-02-14
**Context**: After implementing video metadata extraction using `audio-video-metadata` crate, investigate whether this crate can replace our current audio metadata extraction tools.

## Current Audio Extraction

We currently use three specialized crates for audio tag extraction:

```toml
id3          = "1"       # MP3 ID3 tags
metaflac     = "0.2"     # FLAC Vorbis comments
mp4ameta     = "0.11"    # M4A/MP4 tags
```

### Metadata Extracted

- **Title** - Song/track title
- **Artist** - Performing artist
- **Album** - Album name
- **Year** - Release year
- **Genre** - Music genre
- **Comment** - User comments/notes

### Format Coverage

- MP3 via ID3 tags
- FLAC via Vorbis comments
- M4A/MP4/AAC via MP4 metadata atoms

## audio-video-metadata Crate

### AudioMetadata Structure

According to [docs.rs documentation](https://docs.rs/audio-video-metadata/0.1.7/audio_video_metadata/types/struct.AudioMetadata.html):

```rust
pub struct AudioMetadata {
    pub format: AudioType,
    pub duration: Option<Duration>,
    pub audio: Option<String>,
}
```

### Fields Available

1. **format** (`AudioType`) - Audio format type (MP3, OGG, MP4, etc.)
2. **duration** (`Option<Duration>`) - Track duration
3. **audio** (`Option<String>`) - Single optional string field (purpose unclear)

## Comparison Analysis

| Feature | Current Extractors | audio-video-metadata |
|---------|-------------------|---------------------|
| Title | ✅ | ❌ |
| Artist | ✅ | ❌ |
| Album | ✅ | ❌ |
| Year | ✅ | ❌ |
| Genre | ✅ | ❌ |
| Comments | ✅ | ❌ |
| Duration | ❌ | ✅ |
| Format | ✅ (via extension) | ✅ |

## Findings

The `audio-video-metadata` crate is **NOT suitable** for replacing our current audio metadata extractors:

1. **Metadata Scope**: The crate provides only technical metadata (format, duration), not music tags
2. **Limited Fields**: Only 3 fields vs. 6+ rich tag fields from current extractors
3. **Different Purpose**: Designed for A/V technical metadata, not music library management
4. **No Tag Support**: No structured access to ID3, Vorbis, or MP4 metadata atoms

The single `audio: Option<String>` field cannot provide the structured tag data we need.

## Recommendation

**Keep current audio metadata extractors** - do NOT consolidate.

### Rationale

1. **Current extractors are purpose-built** for music tag extraction and provide rich, structured metadata
2. **audio-video-metadata serves a different use case** - it's designed for technical A/V format detection, not music tags
3. **Complementary, not redundant** - video files need duration/format detection (which audio-video-metadata provides), while audio files need tag extraction (which our current tools provide)
4. **No dependency savings** - Removing id3/metaflac/mp4ameta would lose critical functionality

### Future Consideration

If we wanted duration information for audio files (currently not extracted), we could:
- Add duration extraction using audio-video-metadata to supplement (not replace) existing tag extraction
- This would mean both extractors run on audio files: tags from current extractors + duration from audio-video-metadata

However, this adds complexity for marginal benefit, so **not recommended** unless users specifically request audio duration indexing.

## Conclusion

The investigation confirms that:
- ✅ audio-video-metadata is perfect for video metadata extraction (format, resolution, duration)
- ❌ audio-video-metadata cannot replace audio tag extractors
- ✅ Current architecture is optimal: specialized extractors for different metadata types

No changes needed to audio extraction implementation.

## Sources

- [audio-video-metadata crate documentation](https://docs.rs/audio-video-metadata/0.1.7/audio_video_metadata/)
- [AudioMetadata struct definition](https://docs.rs/audio-video-metadata/0.1.7/audio_video_metadata/types/struct.AudioMetadata.html)
- Current codebase: `crates/common/src/extract/audio.rs`
