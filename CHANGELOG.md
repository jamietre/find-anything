# Changelog

All notable changes to find-anything are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html)

---

## [Unreleased]

### Added
- **Per-directory indexing control** — place a `.noindex` file in any directory to exclude it and all descendants from indexing; place a `.index` TOML file to override scan settings for a subtree (`exclude`, `max_file_size_mb`, `include_hidden`, `follow_symlinks`, `archives.enabled`, `archives.max_depth`, `max_line_length`); `exclude` is additive (appended to parent list), all other fields replace; both marker filenames are configurable via `scan.noindex_file` / `scan.index_file` in `client.toml`; control files themselves are never indexed; overrides are applied in `find-scan` (with per-directory caching) and `find-watch` (per-event, no cache needed)
- **Worker status in stats** — `GET /api/v1/stats` now returns a `worker_status` field (`idle` or `processing` with `source` and `file`); `find-admin stats` prints a `Worker:` line showing `idle` or `● processing source/file`; the web Stats panel shows a pulsing dot and filename in the metrics strip while indexing is active

### Fixed
- **Filesystem walk performance** — removed an `exists()` syscall per directory during the `.noindex` check in `walk_paths`; the marker file is now detected by filename in the walk loop body (zero extra syscalls in the common case), and any collected files under a `.noindex` directory are pruned in a single pass after the walk; added 5-second progress logging (`walking filesystem… N files found so far`) and a completion log line
- **`config/update.sh` path resolution** — script now uses `$(dirname "$0")` so it works when called from any working directory; also added `set -euo pipefail` to stop on first failure

### Fixed
- **PDF ZapfDingbats panic** — forked `pdf-extract` as `jamietre/pdf-extract` and fixed four `unwrap()` panics on unknown glyph names; the critical fix is in the core-font with-encoding path, which now tries both the Adobe Glyph List and the ZapfDingbats table before skipping silently; the other three sites use `unwrap_or(0)`; also replaced the permanently-silent `dlog!` macro with `log::debug!` so debug output is available to consumers that initialise a logger
- **PDF extraction hardened against malformed documents** — ~35 additional bad-input panic sites in the forked `pdf-extract` replaced with `warn!` + safe fallback so as much text as possible is returned even from broken PDFs; areas covered: UTF-16 decode (lossy instead of panic), unknown font encoding names (fall back to PDFDocEncoding), Type1/CFF font parse failures (skip with warning), Differences array unexpected types (skip entry), missing encoding/unicode fallback in `decode_char` (return empty string), Type0/CID font missing DescendantFonts or Encoding (fall back to Identity-H), ToUnicode CMap parse failures (skip map), malformed colorspace arrays (fall back to DeviceRGB), content stream decode failure (skip page), and bad operator operands (skip operator); `show_text` no longer panics when no font is selected

### Added
- **`mise run clippy` task** — runs `cargo clippy --workspace -- -D warnings` matching the CI check; CLAUDE.md updated to require clippy passes before committing Rust changes
- **`mise run build-arm` task** — cross-compiles all binaries for ARM7 (armv7-unknown-linux-gnueabihf) using `cross`, matching the CI release build and avoiding glibc version mismatches on NAS deployments
- **`mise run build-server` task** — builds web UI then compiles all binaries for x86_64 release
- **`DEVELOPMENT.md`** — new developer guide covering prerequisites, mise tasks, native and ARM7 build instructions (`cross` usage explained), linting, CI/release matrix, and project structure
- **Expanded default excludes** — added OS/platform-specific patterns: Synology (`#recycle`, `@eaDir`, `#snapshot`), Windows (`$RECYCLE.BIN`, `System Volume Information`), macOS (`__MACOSX`, `.Spotlight-V100`, `.Trashes`, `.fseventsd`), Linux (`lost+found`), and VCS (`svn`, `.hg`)
- **Full paths in extraction error messages** — PDF and other extraction errors now log the full file path (e.g. `/data/archive.zip::Contract.pdf`) instead of just the filename, making it easier to locate the problematic file

### Fixed
- **Clippy warnings** — fixed three clippy lints that were failing CI: `single_component_path_imports` in `scan.rs`, `collapsible_if` in `routes/admin.rs`, `collapsible_else_if` in `admin_main.rs`

### Added
- **File viewer metadata panel** — `line_number=0` entries that carry file metadata (EXIF tags, ID3 tags, etc.) are now shown in a dedicated panel above the code area, without line numbers; the file's own path line is omitted entirely since it is already displayed in the path bar
- **Search result filename/metadata match display** — results matched by filename or metadata (`line_number=0`) no longer display `:0` in the result header and show the matched snippet directly without a line number column; context is not fetched for these results
- **`mise dev` full dev environment** — `mise dev` now starts both the Rust API server (via cargo-watch) and the Vite dev server together, giving live reload for both Rust and Svelte/TypeScript changes
- **File viewer code table layout** — fixed table column widths so the code column claims all available horizontal space; previously `table-layout: auto` distributed spare width across all columns, making the line-number column much wider than needed and pushing code content toward the centre
- **Unified extraction dispatch** (`find-extract-dispatch` crate) — new crate that is the single source of truth for bytes-based content extraction; both the archive extractor and `find-client` now route all non-archive content through the same full pipeline (PDF → media → HTML → office → EPUB → PE → text → MIME fallback); archive members gain HTML, Office document, EPUB, and PE extraction that was previously only applied to regular files; eliminates a class of bugs where features added to the regular-file path were not reflected in archive-member extraction
- **Indexing error reporting** — extraction failures are now tracked end-to-end: the client reports them in each bulk upload, the server stores them in a new `indexing_errors` table (schema v4), and the UI surfaces them in a new **Errors** panel in Settings; the file detail view shows an amber warning banner when a file had an extraction error; the Stats panel shows an error count badge per source
- **`find-admin` binary** — unified administrative utility replacing `find-config`; subcommands: `config`, `stats`, `sources`, `check`, `inbox`, `inbox-clear`, `inbox-retry`
- **Admin inbox endpoints** — `GET /api/v1/admin/inbox` (list pending/failed files), `DELETE /api/v1/admin/inbox?target=pending|failed|all`, `POST /api/v1/admin/inbox/retry`; all require bearer-token auth
- **Disk usage stats** — statistics dashboard now shows SQLite DB size and ZIP archive size
- **`find-server --config` flag** — `find-server` now uses `--config <PATH>` (consistent with `find-scan`, `find-watch`, and `find-anything`); the flag defaults to `$XDG_CONFIG_HOME/find-anything/server.toml`, `/etc/find-anything/server.toml` when running as root, or `~/.config/find-anything/server.toml` otherwise; overridable with `FIND_ANYTHING_SERVER_CONFIG`
- **CLI reference** — new `docs/cli.md` with comprehensive documentation for all binaries: `find-server`, `find-scan`, `find-watch`, `find-anything`, `find-admin` (all subcommands), full config references, and extractor binary table
- **Startup schema check** — `find-server` now validates the schema version of every existing source database at startup and exits with a clear error if any are incompatible, rather than failing on the first query
- **`find-admin inbox-show <name>`** — new subcommand that decodes and summarises a named inbox item (by filename, with or without `.gz`); searches the pending queue first, then failed; marks the result `[FAILED]` if found in the failed queue; accepts `--json` for raw output; implemented via a new `GET /api/v1/admin/inbox/show?name=<name>` endpoint
- **Exclude patterns applied to archive members** — `scan.exclude` globs (e.g. `**/node_modules/**`, `**/target/**`) now filter archive members in the same way they filter filesystem paths; previously, archives containing excluded directories (such as Lambda deployment ZIPs with bundled `node_modules`) would index all their members regardless of the exclude config

### Removed
- **`find-config` binary** — replaced by `find-admin config`

### Fixed
- **`find-admin`/`find-scan` config not found when running as root** — client tools now look for `/etc/find-anything/client.toml` when running as root (UID 0) before falling back to `~/.config/find-anything/client.toml`; matches the existing behaviour for `find-server` and aligns with the system-mode install layout where `client.toml` is placed in `/etc/find-anything/`
- **Empty `sources` list rejected at parse time** — `[[sources]]` is now optional in `client.toml`; a config with only `[server]` is valid; `find-scan` exits cleanly with a log message when no sources are configured, allowing a minimal server-side config to be used by admin tools without scan configuration
- **Archive OOM on solid 7z blocks** — `entry.size()` in sevenz-rust2 returns 0 for entries in solid blocks, bypassing the pre-read size guard and allowing unbounded allocation; all three archive extractors (ZIP, TAR, 7z) now use `take(size_limit + 1)` as a hard memory cap on the actual read, independent of the header-reported size; oversized members index their filename only and the stream is drained to maintain decompressor integrity
- **Archive members misidentified as `text`** — files inside archives with unknown or binary extensions (e.g. ELF executables, `.deb` packages, files with no extension) were previously labelled `text`; dispatch now always emits a `[FILE:mime]` line for binary content using `infer` with an `application/octet-stream` fallback; `detect_kind_from_ext` now returns `"unknown"` for unrecognised extensions instead of `"text"`; the scan pipeline promotes `"unknown"` to `"text"` only when content inspection confirms the bytes are text
- **`mise dev` Ctrl+C not stopping server** — hitting Ctrl+C left `find-server` running and caused `address already in use` on the next start; `cargo-watch` is now launched via `setsid` so it leads its own process group, and the trap sends `SIGTERM` to the entire group (cargo-watch + cargo run + find-server) rather than just the top-level process
- **Streaming archive extraction** — archive members are now processed one at a time via a bounded channel; lines for each member are freed after the batch is submitted, keeping memory usage proportional to one member rather than the whole archive; nested ZIP archives that fit within `max_temp_file_mb` are extracted in-memory (no disk I/O), larger ones spill to a temp file, and nested 7z archives always use a temp file (required by the 7z API); nested TAR variants are streamed directly with zero extra allocation
- **Archive scan progress** — `find-scan` now logs `extracting archive <name> (N/M)` when it begins processing each archive, so long-running extractions are visible rather than appearing stuck at `0/M files completed`
- **Archive batch progress log** — the mid-archive batch submission log now shows per-batch member count alongside the cumulative total (e.g. `102 members, 302 total`), making it clear when the 8 MB byte limit (rather than the 200-item count limit) triggered the flush
- **`include_hidden` applied to archive members** — archive members whose path contains a hidden component (a segment starting with `.`) are now filtered according to the `include_hidden` config setting, consistent with how the filesystem walker filters hidden files and directories
- **Corrupt nested archive log noise** — "Could not find EOCD" and similar errors for unreadable nested archives are now logged at DEBUG instead of WARN; the outer member filename is still indexed regardless
- **`mise inbox` / `inbox-clear` tasks** — fixed missing `--` separator causing `--config` to be parsed by `cargo run` instead of the binary; added both tasks to `.mise.toml`
- **Archive member line_number=0 duplicate** — archive members were being indexed with two `line_number=0` entries: one from the extractor (containing only the member filename) and one added by the batch builder (containing the full composite path); the extractor's version is now discarded, leaving exactly one path line per member
- **Content archive corruption recovery** — if the most recent content ZIP was left incomplete by a server crash (missing EOCD), the server previously failed every subsequent inbox request that tried to append to it; it now detects the corrupt archive on startup and skips to a new file instead
- **Multi-source search query** — searching with more than one source selected produced `Failed to deserialize query string: duplicate field 'source'`; the search route now parses repeated `?source=a&source=b` params correctly using `form_urlencoded` rather than `serde_urlencoded`
- **7z solid archive CRC failures** — files in a solid 7z block that were skipped due to the `max_file_size_mb` limit were not having their bytes drained from the decompressor stream; this left the stream at the wrong offset, causing every subsequent file in the block to read corrupt data and fail CRC verification; the reader is now always drained on size-limit skips
- **7z archive compatibility** — replaced `sevenz-rust` with `sevenz-rust2` (v0.20); adds support for LZMA, BZIP2, DEFLATE, PPMD, LZ4, ZSTD codecs inside 7z archives, fixing widespread `ChecksumVerificationFailed` errors on real-world archives; 50% faster decompression on LZMA2 archives
- **Archive log noise** — read failures for binary members (images, video, audio) inside ZIP, TAR, and 7z archives are now logged at DEBUG instead of WARN
- **Logging** — unknown config key warnings now always appear; default log filter changed to `warn,<crate>=info` so warnings from all crates (including `find-common`) are visible; `find-config` and `find-anything` now initialize a tracing subscriber so they emit warnings too
- **Schema version check** — `find-server` now detects incompatible (pre-chunk) SQLite databases on startup and prints a clear error with instructions to delete and rebuild, instead of crashing with a cryptic SQL error
- **Archive content extraction** — fixed a bug where any archive member whose file extension was not in the known-text whitelist (dotfiles, `.cmd`, `.bat`, `.vbs`, `.ahk`, `.reg`, `.code-workspace`, `.gitignore`, etc.) had its content silently skipped; content sniffing now operates on in-memory bytes rather than attempting to open a non-existent on-disk path
- **Text extension whitelist** — added Windows script formats (`.cmd`, `.bat`, `.vbs`, `.ahk`, `.au3`, `.reg`), editor/IDE project files (`.code-workspace`, `.editorconfig`), and common dotfile names (`.gitignore`, `.gitattributes`, `.gitmodules`, `.dockerignore`) as recognised text types
- **Archive resilience** — ZIP and TAR extractors now skip corrupt or unreadable entries with a warning and continue processing the rest of the archive, rather than aborting on the first error; 7z read errors are now logged with the entry name instead of silently discarded
- **Archive size limit** — archive container files (`.zip`, `.tar.gz`, `.7z`, etc.) are now exempt from the whole-file `max_file_size_mb` check; the per-member size limit inside the extractor still applies, so individual oversized members are skipped while the rest of the archive is processed
- **Archive memory safety** — ZIP, TAR, and 7z extractors now check each entry's uncompressed size header before reading into memory; oversized members are skipped without allocating, preventing OOM on archives containing very large individual files
- **Error chain logging** — extraction failures in `find-scan` now use `{:#}` formatting to print the full anyhow error chain (e.g. `opening zip: invalid Zip archive: …`) rather than just the outermost context string
- **Tree infinite-nesting bug** — expanding a subdirectory inside an archive (e.g. `archive.7z → settings/`) no longer produces an infinite cascade of empty arrow nodes; archive virtual directory entries now carry a trailing `/` in their path so the server correctly strips the prefix on the next `listDir` call

---

## [0.2.5] - 2026-02-24

### Changed
- `max_file_size_kb` renamed to `max_file_size_mb`; default changed from 1 MB to 10 MB
- `find-anything` binary renamed from `find` to avoid conflict with the coreutils `find` command

### Added
- **`find-config`** — new binary that shows the effective client configuration with all defaults filled in; also warns on unknown config keys
- **Unknown config key warnings** — all three client binaries and `find-server` now emit a `WARN` log for any unrecognised TOML keys
- **Default config path** — all client tools now default to `~/.config/find-anything/client.toml`; overridable via `FIND_ANYTHING_CONFIG` env var or `XDG_CONFIG_HOME`
- **About tab** in Settings — shows server version and a "Check for updates" button
- **Scan progress** — `find-scan` now logs `X/Y files completed` on each batch submission
- **armv7 build target** — supports Synology NAS and other 32-bit ARM Linux devices
- **Restart instructions** — install script prints the correct `systemctl restart` command after an upgrade
- **Server connectivity check** — client install script tests the server URL before proceeding

### Fixed
- `find-server` invocation now uses positional config path argument (not `--config` flag)
- Install scripts: all `read` prompts work correctly when piped via `curl | sh`
- systemd detection: check `/run/systemd/system` presence rather than `systemctl --user status`
- Synology DSM: install script falls back to system-level service unit with `sudo mv` instructions

### Removed
- `ocr` config setting (was never implemented)

---

## [0.2.4] - 2024-12-01

### Added
- **Windows Inno Setup installer** — wizard-style installer with server URL/token/directory prompts; writes `client.toml`; registers `find-watch` as a Windows service
- **`install-server.sh`** — dedicated server installer; configures systemd (system or user mode), generates a secure bearer token, writes annotated `server.toml`
- **`install.sh` improvements** — interactive prompts for URL, token, source name, and directories; generates annotated `client.toml`; sets up `find-watch` systemd user service
- **WinGet manifest** — `Outsharked.FindAnything` package with inno and zip installer entries
- **Unified settings page** — sidebar nav with Preferences, Stats, and About tabs

### Changed
- Release pipeline builds web UI and embeds it into `find-server` binary
- `install.sh` and `install-server.sh` split from a single combined script

---

## [0.2.3] - 2024-11-15

### Added
- **Infinite scroll** — preemptively loads next page when near bottom; cross-page deduplication prevents duplicate keys
- **Lazy context loading** — `IntersectionObserver` fetches context only when result card is visible
- **Command palette** — Ctrl+P opens a file-search palette across all indexed sources
- **Markdown rendering** — `.md` files rendered as HTML in the file viewer with raw/rendered toggle
- **Debounced live search** — 500ms debounce; previous results stay visible while new search is in-flight

### Changed
- Frontend refactored into `SearchView`, `FileView`, `appState` coordinator modules
- `ContextResponse` now returns `{start, match_index, lines[], kind}`
- Server routes split into `routes/` submodule (search, context, file, tree, bulk, settings)
- Page-scroll architecture replaces inner scroll container

---

## [0.2.2] - 2024-11-01

### Added
- **Windows support** — native x86_64-pc-windows-msvc builds
- **`find-watch` Windows Service** — self-installing via `windows-service` crate with `install`/`uninstall`/`service-run` subcommands
- **`find-tray` system tray** — Windows tray icon with Run Full Scan, Start/Stop Watcher, Open Config, and Quit actions
- **`install-windows.ps1`** — downloads latest release, extracts to `%LOCALAPPDATA%`, creates config, installs service

---

## [0.2.1] - 2024-10-15

### Added
- **`find-extract-html`** — strips tags, extracts `[HTML:title]`/`[HTML:description]` metadata and visible text
- **`find-extract-office`** — indexes DOCX paragraphs, XLSX/XLS/XLSM rows, PPTX slide text; title/author metadata
- **`find-extract-epub`** — full chapter text; `[EPUB:title/creator/publisher/language]` metadata
- New `"document"` file kind for docx/xlsx/xls/xlsm/pptx/epub

---

## [0.2.0] - 2024-10-01

### Added
- **GitHub Actions CI** — `cargo test`, `cargo clippy`, and web type-check on every push/PR
- **Binary release matrix** — Linux x86_64/aarch64, macOS arm64/x86_64; platform tarballs on GitHub Releases
- **Docker** — multi-stage `find-server` image; `docker-compose.yml` with data volume
- **`install.sh`** — `curl | sh` installer; auto-detects platform, fetches latest release

---

## [0.1.9] - 2024-09-15

### Added
- **`find-watch` daemon** — inotify/FSEvents/ReadDirectoryChanges watcher with configurable debounce
- **Rename handling** — both sides of a rename processed correctly after debounce window
- **Subprocess extraction** — spawns `find-extract-*` binary per file type
- **Systemd unit files** — user-mode and system-mode units with installation docs

---

## [0.1.8] - 2024-09-01

### Changed
- **Extractor architecture refactor** — each extractor is now a standalone binary (`find-extract-text`, `find-extract-pdf`, `find-extract-media`, `find-extract-archive`) and a shared library crate

---

## [0.1.7] - 2024-08-15

### Added
- **Markdown YAML frontmatter** — title, author, tags, and arbitrary fields indexed as `[FRONTMATTER:key] value`

---

## [0.1.6] - 2024-08-01

### Changed
- **Archive subfolder organization** — `sources/content/NNNN/` thousands-based structure; capacity ~99.99 TB

---

## [0.1.5] - 2024-07-15

### Added
- **Word wrap toggle** — toolbar button with localStorage persistence
- **Source selector dropdown** — replaces pill-based filter; scales to many sources

---

## [0.1.4] - 2024-07-01

### Added
- **Video metadata** — format, resolution, duration from MP4, MKV, WebM, AVI, MOV and more

---

## [0.1.3] - 2024-06-15

### Added
- **Archive members as first-class files** — composite `archive.zip::member.txt` paths; each member has its own `file_id`
- **Command palette** — Ctrl+P file search across all indexed sources
- **Improved fuzzy scoring** — exact substring matches get a large score boost

### Changed
- `FilePath` class refactor — unified path representation eliminates sync issues

---

## [0.1.2] - 2024-06-01

### Added
- **`GET /api/v1/tree`** — prefix-based directory listing using range-scan SQL
- **Directory tree sidebar** — collapsible tree with lazy loading
- **Breadcrumb navigation** — clickable path segments; clicking a directory shows directory listing
- **Atomic archive deletion** — SQLite transaction stays open across ZIP rewrite; rolls back on failure

---

## [0.1.1] - 2024-05-15

### Added
- **ZIP-backed content storage** — file content in rotating 10 MB ZIP archives; SQLite holds only metadata and FTS index
- **Async inbox processing** — client submits gzip-compressed batches; server worker polls and processes asynchronously
- **Contentless FTS5 index** — `lines` table stores chunk references; schema v2
- **Auto-migration** — detects and drops v1 schema on startup

---

## [0.1.0] - 2024-05-01

### Added
- Full-text search with FTS5 trigram indexing
- Fuzzy, exact, and regex search modes
- Multi-source support
- Archive content indexing (zip, tar, tar.gz, tar.bz2, tar.xz, 7z)
- Incremental scanning based on mtime
- File exclusion patterns (gitignore-style globs)
- PDF text extraction
- Image EXIF metadata (camera, GPS, dates)
- Audio metadata (ID3, Vorbis, MP4 tags)
- SvelteKit web UI with live search, file preview, and source filtering
