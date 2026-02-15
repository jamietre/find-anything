# Roadmap

This document tracks the development roadmap for find-anything, from completed features to future ideas.

---

## Recently Completed

### ✅ Core Search & Indexing (v0.1)
- Full-text search with FTS5 trigram indexing
- Fuzzy, exact, and regex search modes
- Multi-source support (one client can manage multiple named sources)
- Archive content indexing (zip, tar, tar.gz, tar.bz2, tar.xz, 7z)
- Incremental scanning based on mtime
- File exclusion patterns (gitignore-style globs)
- Streaming text extraction (memory-efficient for large files)

### ✅ Rich Content Extraction (v0.2)
- **PDF text extraction** — Extract and index text from PDF files
- **Image EXIF metadata** — Index camera make/model, GPS, dates, descriptions
- **Audio metadata** — Index MP3 (ID3), FLAC (Vorbis), M4A tags

### ✅ Web UI (v0.3)
- SvelteKit-based web interface
- Live fuzzy search with syntax highlighting
- File preview and context display
- Source filtering
- Development tooling (mise, pnpm, corepack)

### ✅ Advanced Features (v0.4)
- **Resource base URLs** — Hyperlinkable search results (file://, http://, smb://)
- **Smart context retrieval** — File-type-aware context (metadata for images/audio, paragraph extracts for PDFs)

### ✅ ZIP Content Storage & Async Indexing (v0.1.1)
- **ZIP-backed content storage** — File content stored in rotating 10MB ZIP archives, separate from SQLite FTS index
- **Async inbox processing** — Client gzip-compresses and submits batches; server worker polls and processes asynchronously
- **Schema v2** — Contentless FTS5 index; `lines` table stores chunk references instead of inline content
- **Filename indexing** — Every file indexed by its path so all files are findable by name regardless of content type
- **Auto-migration** — Detects and drops v1 schema on startup, prompting re-scan

### ✅ Directory Tree Explorer (v0.1.2)
- **`GET /api/v1/tree` endpoint** — Prefix-based directory listing using range-scan SQL; returns virtual directory nodes grouped server-side
- **Directory tree sidebar** — Collapsible tree with lazy loading per directory; auto-expands ancestors of the active file
- **Breadcrumb navigation** — Clickable path segments at the top of the detail panel; clicking a directory switches to directory listing view
- **Directory listing view** — Table view of directory contents (name, kind, size, modified date)
- **Atomic archive deletion** — File deletion keeps the SQLite transaction open until ZIP rewrite succeeds; rolls back on failure

### ✅ Archive Navigation & Path Refactoring (v0.1.3)
- **Archive node highlighting** — Clicking nested archive members now correctly highlights the actual file, not the outermost archive
- **Split click behavior** — Archive tree nodes: arrow toggles expansion, name opens/highlights node
- **Improved fuzzy scoring** — Exact substring matches get massive score boost; searching "inner.zip" now correctly ranks files containing that string at top
- **FilePath class refactor** — Unified path representation eliminates sync issues between split (path+archivePath) and composite (path::member) formats
- **Consistent archive behavior** — Ctrl+P and clicking archive nodes both expand to one level and show contents

### ✅ Video Metadata Extraction (v0.1.4)
- **Video metadata indexing** — Extract and index technical metadata from video files
- **audio-video-metadata crate** — Lightweight dependency for format detection (no heavy ffmpeg binding)
- **Metadata extracted:** Format type, resolution (width×height), duration (minutes:seconds)
- **Supported formats:** MP4, M4V, MKV, WebM, OGV, OGG, AVI, MOV, WMV, FLV, MPG, MPEG, 3GP
- **Metadata format:** `[VIDEO:key] value` (matching audio/image pattern)
- **Kind detection:** Video files now return "video" from `detect_kind()`

### ✅ Word Wrap Toggle (v0.1.5)
- **Word wrap toggle button** — Toolbar button in FileViewer to toggle word wrapping on/off
- **CSS switching** — Dynamically applies `white-space: pre-wrap` when enabled, `white-space: pre` when disabled
- **Persistent preference** — Word wrap state saved to localStorage via user profile
- **Syntax preservation** — Line numbers and syntax highlighting preserved when wrapped
- **Default behavior** — Defaults to off (no wrap) to preserve current code viewing experience

### ✅ Archive Subfolder Organization (v0.1.6)
- **Thousands-based subfolders** — Archives organized as `sources/content/NNNN/` where NNNN = 4-digit zero-padded archive_num / 1000
- **Scalable structure** — Each subfolder contains up to 1000 archives (e.g., `content/0000/` → archives 0-999, `content/0001/` → archives 1000-1999)
- **High capacity** — Supports up to 9,999,000 archives (~99.99 TB compressed content)
- **Automatic folder creation** — Subfolders created as needed when archives rotate
- **Updated path resolution** — `archive_path_for_number()` calculates proper subfolder paths
- **Subfolder scanning** — Metrics and archive discovery scan across all subfolders
- **Breaking change** — Requires re-indexing; old flat archive structure not backward compatible

### ✅ Investigations

**Archive Index Compression** (2026-02-14)
- Investigated FTS5 trigram index storage and compression options
- Key finding: FTS5 index naturally ~3x original text size (inherent to trigram indexing)
- Current architecture is optimal: contentless FTS5 + ZIP content storage
- FTS5 already uses built-in page-level compression (not user-configurable)
- Increasing ZIP compression level (6→9) would have minimal benefit (~5-10% reduction)
- **Conclusion:** No changes needed - current implementation is well-balanced
- See: `docs/investigations/001-archive-index-compression.md`

**Audio Metadata Consolidation** (2026-02-14)
- Investigated whether `audio-video-metadata` crate could replace current audio extractors
- Current extractors: id3 (MP3), metaflac (FLAC), mp4ameta (M4A/MP4)
- AudioMetadata struct only provides: format, duration, audio (single optional string)
- Missing: title, artist, album, year, genre, comments (rich music tag data)
- Different purpose: audio-video-metadata is for technical A/V metadata, not music library tags
- **Conclusion:** Keep current audio extractors - they are purpose-built and complementary, not redundant
- See: `docs/investigations/002-audio-metadata-consolidation.md`

---

## Near-term (Next 3-6 months)

### Real-time File Watching
**Status:** Scaffolded, needs implementation

- `find-watch` daemon for real-time incremental indexing
- Platform-specific implementations:
  - Linux: inotify
  - macOS: FSEvents
  - Windows: ReadDirectoryChangesW
- Event debouncing (500ms) to handle editor save storms
- Efficient event → API mapping (CREATE/MODIFY/DELETE/RENAME)

**Plan:** Create `004-realtime-watch.md`

### Systemd Unit Files
**Status:** Not started

- Ready-to-install `.service` and `.timer` files
- Package in `deploy/systemd/` directory
- Installation script with user/group setup
- Logging configuration

### Web UI Enhancements - Phase 1
**Status:** In progress

High-priority UI improvements:
- Clickable resource URLs (uses base_url feature)
- Keyboard shortcuts for navigation
- Dark mode / theme switcher
- Search history (localStorage)
- File type icons and better result formatting

---

## Medium-term (6-12 months)

### Windows Native Client
**Status:** Design phase

Full Windows support with native tooling:
- Windows Service wrapper for `find-scan` and `find-watch`
- Task Scheduler integration for periodic scans
- System tray icon with status indicator
- MSI installer
- PowerShell setup scripts
- Windows-specific path handling

**Goal:** First-class Windows experience matching Linux/macOS

### Search Ranking Improvements
**Status:** Research phase

Enhance result quality:
- Recency bias (recently modified files rank higher)
- Result deduplication across sources (same content hash)
- Frequency scoring (commonly accessed files)
- Custom ranking per file type (e.g., prioritize code over archives)

### Web UI Enhancements - Phase 2
**Status:** Backlog

Advanced features:
- Advanced search filters (file type, date range, size)
- Search result export (JSON, CSV)
- Saved searches
- Search query builder UI
- Pagination or infinite scroll
- Multi-select for batch operations

---

## Long-term (12+ months)

### OCR Support
**Status:** Post-MVP

Optional OCR for images and scanned PDFs:
- Opt-in via `ocr = true` in config
- Requires `tesseract` in PATH
- Images run through tesseract
- Scanned PDFs (no embedded text) fall back to page-render + OCR
- Background processing with concurrency limits
- Content hash caching to avoid re-OCR

**Considerations:**
- Expensive operation (seconds per page)
- Large disk space for cache
- Quality vs performance trade-offs

### Multi-user & Authentication
**Status:** Design needed

Move beyond single shared token:
- Per-user accounts and authentication
- Token rotation and expiry
- Role-based access control (read-only, admin)
- User-specific saved searches and preferences (cloud profile sync)
- Audit logging

### Advanced Integrations

**Webhook notifications:**
- POST to URL when new content matches saved pattern
- Real-time alerts for monitoring

**Index export:**
- `find-server export --source <name> --format json`
- Backup and migration support

**Plugin system:**
- Custom extractors for proprietary formats
- User-defined processing pipelines
- Language-specific analyzers

---

## Ideas & Future Enhancements

These are less structured ideas that may evolve into formal features.

### Web UI Ideas
- [x] Folder path browsing
- [ ] The first line of a file is the same as the path, which we already show above it
- [ ] Wrap lines by default; but have an option in the settings menu to toggle this
- [ ] Show file metadata in detail view (create/edit time)
- [ ] Sources visibility. Pills don't actually work right now; is this a good ux? What if there are a large number of sources?
Consider other UI: dropdown list with checkboxes. A source explorer of some kind. Looking for ideas here.
- [ ] Search suggestions / autocomplete
- [ ] Recent searches dropdown
- [ ] Command palette (Cmd+K style)
- [ ] Regex helper / tester UI
- [ ] Result grouping by file type or source
- [ ] Timeline view for date-based results
- [ ] Graph visualization of file relationships

### Advanced URI Handling

- [x] Base URL configuration (implemented)
- [ ] Custom URI scheme (`find://source/path:line`)
- [ ] Protocol handler for opening files in local editor
- [ ] Deep linking from external tools (IDE, chat apps)
- [x] Line number anchors (`#L42`)
- [ ] Archive entry URLs (`archive.zip#!/entry.txt`)
- [ ] Template-based URL construction

### Performance & Scalability

- [x] Archive subfolder organization — Completed in v0.1.6
- [ ] Distributed indexing (multiple scan clients per source)
- [ ] Database partitioning for large sources (>100GB)
- [ ] Elasticsearch backend option (alternative to SQLite)
- [ ] Read replicas for search load balancing
- [ ] Index compression strategies
- [ ] Incremental FTS5 rebuilds

### Additional Content Types
- [x] Video metadata (MP4, MKV duration, resolution, codecs) — completed v0.1.4
- [ ] Office documents (DOCX, XLSX, PPTX via external tools)
- [ ] Markdown frontmatter extraction
- [ ] Code symbol indexing (functions, classes, imports)
- [ ] Email (mbox, PST) indexing
- [ ] Database dumps (SQL, JSON schemas)

### Search Features

- [ ] Fuzzy file path matching (like fuzzy finder)
- [ ] Boolean operators (AND, OR, NOT)
- [ ] Field-specific search (`path:src author:john`)
- [ ] Proximity search (words within N words of each other)
- [ ] Phonetic search
- [ ] Language-specific analyzers (stemming, synonyms)

### Operations & Monitoring

- [ ] Index statistics dashboard (size, file count, growth rate)
- [ ] Health checks and alerting
- [ ] Performance metrics (query latency percentiles)
- [ ] Slow query logging
- [ ] Database vacuuming automation
- [ ] Backup and restore utilities
- [ ] Source priority and quotas

### Developer Tools

- [ ] GraphQL API (alternative to REST)
- [ ] Python client library
- [ ] JavaScript client library
- [ ] VS Code extension
- [ ] CLI autocomplete (bash, zsh, fish)
- [ ] Docker compose for easy deployment

### Integration Ideas

- [ ] Slack/Discord bot for search
- [ ] Browser extension for web archiving
- [ ] Git commit message indexing
- [ ] Issue tracker integration (GitHub, Jira)
- [ ] Calendar event indexing
- [ ] Chat history indexing (Slack export, Discord)

### Miscellaneous

- [ ] Generate some sample data so we can spin up a server using no personal data, and take some screenshots for the readme
---

## Contributing Ideas

Have an idea not listed here? Consider:
1. **Quick wins** → Open an issue or PR
2. **Substantial features** → Discuss in an issue first
3. **Major changes** → Create a plan in `docs/plans/NNN-feature-name.md`

See `CLAUDE.md` for project conventions and planning guidelines.
