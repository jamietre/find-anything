# Changelog

All notable changes to find-anything are documented here.

Format: [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)
Versioning: [Semantic Versioning](https://semver.org/spec/v2.0.0.html)

---

## [Unreleased]

### Added
- **`find-admin` binary** — unified administrative utility replacing `find-config`; subcommands: `config`, `stats`, `sources`, `check`, `inbox`, `inbox-clear`, `inbox-retry`
- **Admin inbox endpoints** — `GET /api/v1/admin/inbox` (list pending/failed files), `DELETE /api/v1/admin/inbox?target=pending|failed|all`, `POST /api/v1/admin/inbox/retry`; all require bearer-token auth
- **Disk usage stats** — statistics dashboard now shows SQLite DB size and ZIP archive size

### Removed
- **`find-config` binary** — replaced by `find-admin config`

### Fixed
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
