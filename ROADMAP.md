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

### ✅ GitHub CI & Release Pipeline (v0.2.0)

- **GitHub Actions CI** — `cargo test --workspace` + `cargo clippy -- -D warnings` + web type-check on every push/PR
- **Binary release matrix** — Linux x86_64, Linux aarch64 (native ARM runner), macOS arm64, macOS x86_64 — builds all 8 binaries into platform tarballs
- **GitHub Releases** — Automated release creation on `v*.*.*` tags via `softprops/action-gh-release`
- **Install script** — `curl -fsSL .../install.sh | sh` auto-detects platform, fetches latest release, extracts to `~/.local/bin`
- **Docker** — Multi-stage `find-server` image (rust:slim builder → debian:bookworm-slim runtime), `docker-compose.yml` with data volume
- **`server.toml.example`** — Annotated config template for Docker users

### ✅ Format Extractors: HTML, Office, EPUB (v0.2.1)

- **`find-extract-html`** — Strips tags via `scraper` (html5ever); extracts `[HTML:title]`/`[HTML:description]` metadata, visible paragraph/heading/list text; skips nav/header/footer/script/style
- **`find-extract-office`** — DOCX (zip+quick-xml, `<w:t>/<w:p>` paragraphs, `dc:title`/`dc:creator` metadata), XLSX/XLS/XLSM (calamine rows, sheet metadata), PPTX (zip+quick-xml, `<a:t>/<a:p>`, per-slide metadata)
- **`find-extract-epub`** — Parses `META-INF/container.xml` → OPF → spine → XHTML text walk; indexes `[EPUB:title/creator/publisher/language]` metadata
- **New `"document"` kind** — Added to `detect_kind_from_ext` for docx/xlsx/xls/xlsm/pptx/epub

### ✅ Windows Support (v0.2.2)

- **Windows build pipeline** — Native x86_64-pc-windows-msvc builds via GitHub Actions `windows-latest` runner; ZIP artifacts with all binaries
- **`find-watch` Windows Service** — Self-installing via `windows-service` crate; `install`/`uninstall`/`service-run` subcommands; integrates with Windows Service Control Manager
- **`find-tray` system tray app** — Windows-only GUI using `tray-icon` crate; polls service status and server API; provides Run Full Scan, Start/Stop Watcher, Open Config, and Quit actions
- **PowerShell automation** — `install-windows.ps1` downloads latest release from GitHub, extracts to `%LOCALAPPDATA%`, creates config template, installs service; `uninstall-windows.ps1` removes service and cleans up
- **Auto-start integration** — Tray app registered in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` during service installation
- **Comprehensive documentation** — `docs/windows/README.md` with quick start, service management, troubleshooting, and Windows-specific differences

### ✅ Search UX, Infinite Scroll & Frontend Refactor (v0.2.3)

- **Debounced search with live feedback** — 500ms debounce; old results stay visible and blurred while new search is in-flight; no flash on transition
- **Infinite scroll** — Window scroll listener preemptively loads next 50 results when within 600px of bottom; paginated batches deduplicated by `source:path:line_number` to handle overlapping pages
- **Lazy context loading** — `IntersectionObserver` per result card fetches context only when it scrolls into view; placeholder shown until loaded; avoids burst of N requests on page load
- **Page-scroll architecture** — Natural page scroll (no inner scroll container); sticky topbar; `ResultList` is a pure display component
- **Markdown rendering in file viewer** — `marked` renders `.md` files as HTML with a toolbar toggle between rendered and raw views
- **Command palette** — Ctrl+P opens a file-search palette across all indexed sources
- **Frontend component refactor** — Extracted `SearchView`, `FileView`, `appState` modules; coordinator pattern with all state in `+page.svelte`
- **Context API refactored** — `ContextResponse` returns `{start, match_index, lines[], kind}`; server routes split into `routes/` submodule (search, context, file, tree, bulk)

### ✅ Investigations

- **Archive Index Compression** — FTS5 trigram index is inherently ~3x text size; current architecture is optimal. No changes needed.
- **Audio Metadata Consolidation** — `audio-video-metadata` crate lacks rich music tags; current per-format extractors kept.

### ✅ `find-admin` — General-Purpose Admin Utility (v0.2.6)

Replaced `find-config` with a unified `find-admin` binary covering all administrative and diagnostic tasks:

- `find-admin config` — show effective client config with defaults filled in (replaces `find-config`)
- `find-admin stats` — print per-source file counts, sizes, and last-scan age from the server
- `find-admin sources` — list indexed sources and their base URLs
- `find-admin check` — validate connectivity, bearer-token auth, and source count with colored ✓/✗ output
- `find-admin inbox` — list pending and failed inbox files with size and age
- `find-admin inbox-clear [--failed|--all] [--yes]` — delete inbox files with optional confirmation
- `find-admin inbox-retry [--yes]` — move failed files back to pending for retry

New server endpoints: `GET /api/v1/admin/inbox`, `DELETE /api/v1/admin/inbox?target=pending|failed|all`,
`POST /api/v1/admin/inbox/retry`. All admin endpoints use the same bearer-token auth as all other routes;
RBAC is planned for a future release.

---

## Near-term Priorities

### 🔴 Bug: Scan Should Delete Before Adding

When `find-scan` processes a batch, files to be removed and files to be added/updated
are submitted in the same `BulkRequest`. The server worker already processes deletes
before upserts within a single request. However, when a scan spans multiple batches,
a deletion in a later batch may arrive after an addition from an earlier batch —
meaning a renamed file's new path could be indexed before the old path is cleaned up,
or a re-indexed file could briefly have duplicate entries.

Fix: ensure that when building batches, all deletions are flushed (and confirmed by
the server) before the first addition batch is sent. This makes the scan
delete-first at the batch boundary level, not just within a single bulk request.

---

### 🔴 File Serving & Share URL Mapping (High Priority)

Map source names to base URLs in `server.toml` and expose a server endpoint that
retrieves and serves the actual file bytes, enabling the UI and API clients to open or
download any indexed file directly.

- **`[sources.<name>]` config block** — Each source can have an optional
  `share_url_root` that is a file path or URL prefix (e.g. `file:///mnt/nas/docs`,
  `smb://server/share`, `https://files.example.com/`). The server uses this to
  construct a full URL for any file in that source.
- **`GET /api/v1/file-content?source=X&path=Y`** — Streams the actual file bytes
  from the server's local filesystem. Authenticated (bearer token required). Supports
  `Content-Type` detection via `mime_guess`. Respects `Range` headers for large files /
  media streaming. Returns 404 if the file is not on the server's local filesystem.
- **UI integration** — "Open" / "Download" button in the detail panel that hits this
  endpoint; the browser receives the raw file rather than extracted text.
- **Archive member serving** — For composite paths (`archive.zip::member.txt`), extract
  and stream the specific member from the ZIP rather than the whole archive.

---

### 🟡 Memory-Safe Archive Extraction (Streaming)

Currently all extraction is **fully in-memory**: `dispatch_from_path` calls
`std::fs::read()` and archive members use `read_to_end()`. "Streaming" in the
current code means iterating one archive member at a time, not true byte-level
streaming — each individual member is still fully buffered into a `Vec<u8>`.

**Partial fix applied**: All three archive extractors (ZIP, TAR, 7z) now use
`take(size_limit + 1)` as a hard memory bound on reads, preventing OOM when an
archive's size header reports 0 (a known issue with solid 7z blocks where
`entry.size()` is set to 0 for all entries in the block).

The longer-term improvement is to have extractors accept **either a stream or a
byte slice** so large members can be indexed without holding the full content in RAM:

- **Extractor API** — each extractor's `extract_from_*` accepts `impl Read` in
  addition to `&[u8]`; the bytes path remains for callers that already have the
  buffer (e.g. nested archive recursion)
- **Streaming text extraction** — pipe member bytes directly into the line iterator
  without buffering the whole member; only the current line needs to be in memory
- **Temp-file fallback** — for extractors that require a seekable file (PDF, Office
  docs), write the member to a `NamedTempFile` and pass the path; clean up after
- **Benchmark** — measure peak RSS during extraction of a large tar.gz with big
  members before and after to confirm the improvement

---

### ✅ Improve 7z Archive Compatibility (v0.2.6)

Replaced `sevenz-rust` with `sevenz-rust2` (v0.20), which supports all major 7z
codecs (LZMA, LZMA2, BZIP2, DEFLATE, PPMD, LZ4, ZSTD, COPY). This fixes
widespread `ChecksumVerificationFailed` errors on real-world archives and is 50%
faster on LZMA2. Binary member failures (images, video, audio) in ZIP, TAR, and
7z archives now log at DEBUG instead of WARN.

A potential future enhancement remains: opt-in shelling out to system `7z` for
any archives that still fail (e.g. encrypted or very exotic codecs).

---

### 🟡 Archive Extractor Test Coverage

Add automated tests for the archive extractor using fixture files checked into the
repo:

- **7z fixture** — a small `.7z` file containing text files, dotfiles (no extension),
  `.cmd`/`.bat`/`.vbs` scripts, and a nested zip — verifying `accepts_bytes` content
  sniffing, extension whitelist, and nested extraction
- **Zip fixture** — covering corrupt/unreadable entries (verify skip-and-continue
  behaviour), oversized members (verify size pre-check), and members with no extension
- **Tar.gz fixture** — covering the same member-level scenarios
- **Unit tests for `is_text_ext` / `accepts_bytes`** — table-driven tests covering
  each extension category and the content-sniff fallback for extensionless files

---

### 🟡 Installation & End-User Experience

**Status:** Partially done (systemd units, install script, Docker in v0.2.0)

Beyond the release pipeline, the getting-started experience needs polish:

- **README quickstart** — Rewrite README with a 5-minute getting-started guide: install binary → write minimal config → run `find-scan` → run `find-watch` → open UI.
- **Config validator** — `find-scan --check-config` that validates the TOML, checks server connectivity, and prints a human-readable summary of sources and settings.
- **Scan progress output** — Show a progress bar or per-source summary during `find-scan` so users know it's working on large directories.
- **`find-watch --status`** — Query the running watcher (via a unix socket or pidfile) for its current state: sources watched, events processed, last update.

---

## Medium-term

### Performance

- allow passing multiple files to extractors to avoid loading plugin repeatedly when processeing long lists of files
- what does current arch do in these situations? worth doing?

### Search Quality Improvements

- Recency bias (recently modified files rank higher)
- Result deduplication across sources
- Advanced filters in UI (file type, date range, size)
- Boolean operators (AND, OR, NOT) in query syntax "advanced search"

### Web UI Phase 2

- Allow showing tree directly from the main page, e.g. without a search, same as if a search had already occurred. (UX ideas?)
- Allow clicking on file path segments to navigate to that area in the left nav
- Search suggestions / autocomplete
- Recent searches dropdown
- Search result export (JSON, CSV)
- Advanced search filter UI

---

## Long-term

### OCR Support

Optional OCR for images and scanned PDFs via `tesseract` in PATH. Expensive
operation; opt-in via `ocr = true` in config. Background processing with
content-hash caching to avoid re-OCR.

### Multi-user & Authentication

- Per-user accounts, token rotation, role-based access control (read-only/admin),
  audit logging.
- Encryption of data archives (and index?)

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
- [x] Command palette (Ctrl+P) — v0.2.3
- [ ] Regex helper / tester UI
- [ ] Result grouping by file type or source
- [ ] Show images inlne when possible if remote-url works
- [ ] In stats dashboard, show actual size of database and archive files

### Additional Content Types

- [x] PDF text extraction
- [x] Image EXIF metadata
- [x] Audio metadata (MP3, FLAC, M4A)
- [x] Video metadata (MP4, MKV, WebM, etc.) — v0.1.4
- [x] Markdown frontmatter extraction — v0.1.7
- [x] HTML — improved (strip tags, text-only) — v0.2.1
- [x] DOCX, XLSX, PPTX — v0.2.1
- [x] EPUB — v0.2.1
- [ ] Image AI analysis
- [ ] Code symbol indexing (functions, classes, imports)
- [ ] Email (mbox, PST) indexing

### Extractor Log Verbosity

- [ ] **Suppress pdf-extract noise by default** — the hardened fork now emits `warn!`
  for every unrecognised font encoding, colorspace, or glyph name; on a large corpus
  this is very chatty. By default, `find-client` (and `find-watch`) should suppress
  `WARN` and below from the `pdf_extract` crate (or from all extractor sub-processes).
  Options: set `RUST_LOG` when spawning extractor processes; or filter by crate path
  in the `env_logger` / `tracing` initialisation in `find-client`.
- [ ] **Per-extractor log level config** — add an optional `scan.extractor_log_level`
  (or per-extractor overrides) so operators can dial up verbosity for debugging a
  specific extractor without flooding logs from others; default should be `error` or
  `off` for third-party library crates

### Indexing Control

- [x] **`.noindex` / `.index` per-directory control** — `.noindex` marker skips a directory
  and all descendants; `.index` TOML file overrides scan settings for a subtree (excludes,
  size limit, hidden files, archive depth, etc.); both filenames configurable via
  `scan.noindex_file` / `scan.index_file`

### Performance & Scalability

- [x] Archive subfolder organization (v0.1.6)
- [x] FTS5 contentless index + ZIP content storage
- [ ] Distributed indexing (multiple scan clients per source)
- [ ] Database partitioning for large sources (>100GB)
- [ ] Incremental FTS5 rebuilds
- [ ] **Optimize file-list transfer for large sources** — at scan start, `find-scan`
  fetches the full server file list via `GET /api/v1/files` to detect deletions and
  changed mtimes. The response is held in memory as a `HashMap<String, i64>` alongside
  the local `HashMap<String, PathBuf>` built by the filesystem walk. At ~140 bytes/entry
  for the server map and ~200 bytes/entry for the local map, 1 M files costs roughly
  340 MB peak; 10 M files ~3.4 GB. At current NAS scale (~23 K files, ~8 MB total) this
  is negligible. Two improvements make sense if the source grows significantly:
  (1) **Drop `kind` from `FileRecord`** — the client discards it immediately; removing
  it from the API response and the `SELECT` saves ~15–20% of payload and parse cost for
  free. (2) **Server-side diff** — instead of sending the full file list to the client,
  the client posts a compact `path → mtime` map and the server returns only the paths
  to delete and those needing re-indexing; this eliminates both client-side HashMaps
  and the full JSON body entirely, reducing peak client memory from O(n) to O(batch).
  The server-side diff is a non-trivial API change (new endpoint, server reads the local
  map from the request body) so is deferred until there is a concrete need.

### Operations & Monitoring

- [ ] Track stats on time to index each file, and report on them
- [ ] Index statistics dashboard
- [ ] Health check endpoint
- [ ] Slow query logging
- [ ] Database vacuuming automation
- [ ] Backup and restore utilities

### Developer Tools

- [x] Docker Compose — v0.2.0
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
