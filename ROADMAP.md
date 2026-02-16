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
- **Improved fuzzy scoring** — Exact substring matches get massive score boost
- **FilePath class refactor** — Unified path representation eliminates sync issues between split and composite formats
- **Archive members as first-class files** — Composite `archive.zip::member.txt` paths; each member has its own `file_id`, searchable by name, browsable in the tree

### ✅ Video Metadata Extraction (v0.1.4)
- **Video metadata indexing** — Format type, resolution, duration from MP4, MKV, WebM, AVI, MOV and more

### ✅ Word Wrap Toggle & UX (v0.1.5)
- **Word wrap toggle** — Toolbar button with localStorage persistence
- **Source selector dropdown** — Replaced pill-based filter with compact, scalable dropdown

### ✅ Archive Subfolder Organization (v0.1.6)
- **Thousands-based subfolders** — `sources/content/NNNN/` structure; up to ~99.99 TB capacity
- **Source selector** — Dropdown with checkboxes replaces pills; scales to many sources

### ✅ Markdown Frontmatter Extraction (v0.1.7)
- **YAML frontmatter** — Title, author, tags, and arbitrary fields indexed as `[FRONTMATTER:key] value`
- **Graceful degradation** — Malformed or missing frontmatter doesn't prevent content indexing
- **Nested structures** — Nested YAML objects serialized to searchable strings

### ✅ Extractor Architecture Refactor (v0.1.8)
- **Standalone extractor binaries** — `find-extract-text`, `find-extract-pdf`, `find-extract-media`, `find-extract-archive` as independent binaries with JSON output
- **Shared library crates** — Each extractor is also a library crate consumed by `find-scan` directly
- **Clean separation** — Extractor logic isolated from client logic; each binary can be tested independently

### ✅ Incremental File Watcher (v0.1.9)
- **`find-watch` daemon** — Monitors source paths with `notify` (inotify/FSEvents/ReadDirectoryChanges); pushes single-file updates via `POST /api/v1/bulk`
- **Debounce loop** — Configurable debounce window (default 500ms) collapses rapid events before processing
- **Event accumulation** — Create/Modify → Update; Remove → Delete; Update→Delete = Delete; Delete→Update = Update
- **Rename handling** — Both sides of a rename handled correctly after debounce
- **Subprocess extraction** — Spawns appropriate `find-extract-*` binary per file type; resolves binary next to executable, then PATH
- **Systemd unit files** — User-mode (`~/.config/systemd/user/`) and system-mode (`/etc/systemd/system/`) units with installation README

### ✅ Investigations
- **Archive Index Compression** — FTS5 trigram index is inherently ~3x text size; current architecture is optimal. No changes needed.
- **Audio Metadata Consolidation** — `audio-video-metadata` crate lacks rich music tags; current per-format extractors kept.

---

## Near-term Priorities

### 🔴 GitHub CI & Release Infrastructure
**Status:** Not started — highest priority gate for quality and adoption

A project people can't easily install won't get used. Before pushing for wider
adoption, establish a solid release pipeline:

- **GitHub Actions CI** — On every push/PR: `cargo test`, `cargo clippy -- -D warnings`, `cargo build --release`. Fail fast on regressions.
- **Pre-built binary releases** — GitHub Releases workflow triggered on version tags. Build Linux (x86_64, aarch64) and macOS (x86_64, aarch64) binaries. Upload as release assets.
- **Install script** — `curl -fsSL https://raw.githubusercontent.com/.../install.sh | sh` that downloads the right binary for the current platform and drops it in `~/.local/bin` (or `/usr/local/bin` with sudo).
- **Docker image** — `find-server` as a minimal Docker image (distroless or Alpine). `docker run -v data:/data find-server` just works.
- **Docker Compose** — `docker-compose.yml` for running server + an initial scan container side-by-side.

**Plans to create:** `017-ci-release-pipeline.md`

---

### 🔴 Additional Format Support
**Status:** Not started — high user value, builds on extractor architecture

The extractor crate model (plan 015) makes adding formats straightforward. Add
each as a new extractor binary + library crate following the established pattern.

#### HTML (improved)
**Current behavior:** HTML files are treated as text — tags and attributes are
indexed verbatim alongside content, adding noise to results.

**Target:** Strip all tags; index visible text only. Preserve `<title>`,
`<meta name="description">`, `<h1>`–`<h6>` as structured metadata lines.
Crate: `scraper` or `html5ever` (both pure Rust, no system deps).

**Plan to create:** `018-html-extractor.md`

#### Office Documents (DOCX, XLSX, PPTX)
**High value** — most knowledge workers have large collections of Office files.
All three formats are ZIP-based XML, so no native libs or system deps required.

- **DOCX** — Extract `word/document.xml`; strip XML; index paragraphs as lines. Crate: `docx-rs` (read-only) or parse zip+xml directly.
- **XLSX** — Extract sheet cell values (text + number cells). Crate: `calamine` (pure Rust, excellent).
- **PPTX** — Extract slide text from `ppt/slides/slide*.xml`. Pure zip+xml.

**Plan to create:** `019-office-document-extractor.md`

#### EPUB
**High value** for anyone with an ebook collection. EPUB is a ZIP of XHTML
files; extracting text is the same problem as HTML, just wrapped.

- Parse `content.opf` for spine order and metadata (title, author, publisher)
- Extract text from each XHTML chapter
- Index metadata as `[EPUB:title]` / `[EPUB:author]` lines

**Plan to create:** `020-epub-extractor.md`

---

### 🟡 Installation & End-User Experience
**Status:** Partially done (systemd units added in v0.1.9)

Beyond the release pipeline, the getting-started experience needs polish:

- **README quickstart** — Rewrite README with a 5-minute getting-started guide: install binary → write minimal config → run `find-scan` → run `find-watch` → open UI.
- **Config validator** — `find-scan --check-config` that validates the TOML, checks server connectivity, and prints a human-readable summary of sources and settings.
- **Scan progress output** — Show a progress bar or per-source summary during `find-scan` so users know it's working on large directories.
- **`find-watch --status`** — Query the running watcher (via a unix socket or pidfile) for its current state: sources watched, events processed, last update.

---

## Medium-term

### Search Quality Improvements
- Recency bias (recently modified files rank higher)
- Result deduplication across sources
- Advanced filters in UI (file type, date range, size)
- Boolean operators (AND, OR, NOT) in query syntax

### Web UI Phase 2
- Search suggestions / autocomplete
- Recent searches dropdown
- Command palette (Cmd+K style)
- Search result export (JSON, CSV)
- Advanced search filter UI

### Windows Support
- Windows Service wrapper for `find-watch`
- Task Scheduler integration for periodic scans
- MSI installer / WiX toolset
- PowerShell setup scripts

---

## Long-term

### OCR Support
Optional OCR for images and scanned PDFs via `tesseract` in PATH. Expensive
operation; opt-in via `ocr = true` in config. Background processing with
content-hash caching to avoid re-OCR.

### Multi-user & Authentication
Per-user accounts, token rotation, role-based access control (read-only/admin),
audit logging.

### Advanced Integrations
- Webhook notifications on new matches for saved searches
- Index export (`find-server export --source <name> --format json`)
- VS Code extension
- Plugin system for custom extractors

---

## Ideas & Future Enhancements

### Web UI Ideas
- [x] Folder path browsing
- [x] Sources visibility — dropdown selector (v0.1.6)
- [x] Word wrap toggle (v0.1.5)
- [x] File metadata in detail view (create/edit time)
- [ ] Search suggestions / autocomplete
- [ ] Recent searches dropdown
- [ ] Command palette (Cmd+K style)
- [ ] Regex helper / tester UI
- [ ] Result grouping by file type or source

### Additional Content Types
- [x] PDF text extraction
- [x] Image EXIF metadata
- [x] Audio metadata (MP3, FLAC, M4A)
- [x] Video metadata (MP4, MKV, WebM, etc.) — v0.1.4
- [x] Markdown frontmatter extraction — v0.1.7
- [ ] HTML — improved (strip tags, text-only) — **near-term**
- [ ] DOCX, XLSX, PPTX — **near-term**
- [ ] EPUB — **near-term**
- [ ] Code symbol indexing (functions, classes, imports)
- [ ] Email (mbox, PST) indexing

### Performance & Scalability
- [x] Archive subfolder organization (v0.1.6)
- [x] FTS5 contentless index + ZIP content storage
- [ ] Distributed indexing (multiple scan clients per source)
- [ ] Database partitioning for large sources (>100GB)
- [ ] Incremental FTS5 rebuilds

### Operations & Monitoring
- [ ] Index statistics dashboard
- [ ] Health check endpoint
- [ ] Slow query logging
- [ ] Database vacuuming automation
- [ ] Backup and restore utilities

### Developer Tools
- [ ] Docker Compose — **near-term**
- [ ] CLI autocomplete (bash, zsh, fish)
- [ ] Python / JavaScript client library
- [ ] VS Code extension

---

## Contributing

Have an idea not listed here? Consider:
1. **Quick wins** → Open an issue or PR
2. **Substantial features** → Discuss in an issue first
3. **Major changes** → Create a plan in `docs/plans/NNN-feature-name.md`

See `CLAUDE.md` for project conventions and planning guidelines.
