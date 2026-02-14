# TODO - High Priority

## 1. Video Metadata Extraction

**Goal:** Extract and index video file metadata (like audio/image already supported)

**Metadata to extract:**
- Duration, resolution, codec, framerate
- Title, artist, album (from container metadata)
- Creation date, camera model (if embedded)

**Implementation approach:**
- Add `video.rs` extractor to `crates/common/src/extract/`
- Use `ffprobe` or Rust crate (e.g., `ffmpeg-next`, `mp4parse`)
- Detect video extensions: `.mp4`, `.mkv`, `.avi`, `.mov`, `.webm`, etc.
- Format metadata as key-value lines (similar to audio/image extractors)

**Files to create/modify:**
- `crates/common/src/extract/video.rs` - New extractor
- `crates/common/src/extract/mod.rs` - Register video extractor
- Update `detect_kind()` to return "video" for video files

**Dependencies to add:**
- Research best Rust crate for video metadata (avoid heavy ffmpeg binding if possible)

---

## 2. Word Wrap in File Viewer

**Current behavior:**
- Long lines overflow horizontally, require scrolling

**Desired behavior:**
- Toggle word wrap on/off
- Preserve syntax highlighting and line numbers when wrapped
- Remember preference per file type or globally

**Implementation:**
- Add word-wrap toggle button to FileViewer toolbar
- CSS: `white-space: pre-wrap` vs. `white-space: pre`
- Store preference in localStorage or profile
- Consider: wrap only for text files, not code?

**Files to modify:**
- `web/src/lib/FileViewer.svelte` - Add toggle button and CSS
- `web/src/lib/profile.ts` - Store wrap preference

---

## 3. Content Archive Subfolder Strategy

**Current problem:**
- All `content_NNNNN.zip` archives in flat `data_dir/sources/` folder
- Large number of files becomes unwieldy (filesystem limits, slow listings)

**Proposed strategies:**

**Option A: Prefix-based subfolders**
```
sources/
  content/
    00/
      content_00001.zip
      content_00099.zip
    01/
      content_00100.zip
      content_00199.zip
```
- Use first 2 digits of archive number as subfolder name
- Max 100 files per subfolder (00-99)

**Option B: Fixed subfolder count**
```
sources/
  content/
    0/  (archives 0000-0999)
    1/  (archives 1000-1999)
    2/  (archives 2000-2999)
```
- Use thousands digit as subfolder
- Simpler logic, still limits files per folder

**Option C: Date-based**
```
sources/
  content/
    2026-02/
      content_00001.zip
      content_00050.zip
```
- Group by month created
- Natural temporal organization
- Requires metadata tracking

**Decision needed:**
- Which strategy is simplest and most future-proof?
- Migration path for existing flat archives?

**Files to modify:**
- `crates/server/src/archive.rs` - Archive path construction
- Add migration function to reorganize existing archives
- Update path resolution in chunk reading

**Backward compatibility:**
- Support reading from both old (flat) and new (subfolder) locations
- Migrate on first write after update?
