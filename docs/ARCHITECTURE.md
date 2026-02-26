# find-anything Architecture

## System Overview

find-anything is a two-process system for full-text indexing and search of local files.

```
find-scan ──POST /api/v1/bulk──▶ find-server ──▶ SQLite + ZIP archives
                                      │
                              GET /api/v1/search
                                      │
                               web UI (SvelteKit)
```

| Binary | Role |
|--------|------|
| `find-server` | Receives indexed content, stores it, serves search queries |
| `find-scan`   | Walks the filesystem, extracts content, batches to server |

---

## Crate Structure

```
crates/
├── common/                   # Shared API types, config, fuzzy search
│                             # Deliberately lean: no extractor deps
├── server/                   # HTTP server, SQLite, ZIP archive management
├── client/                   # find-scan binary; dispatches to extractor libs
└── extractors/
    ├── text/                 # Plain text, source code, Markdown + frontmatter
    ├── pdf/                  # PDF text extraction (pdf-extract)
    ├── media/                # Image EXIF, audio tags, video metadata
    ├── html/                 # HTML tag stripping, title/description metadata
    ├── office/               # DOCX, XLSX, PPTX extraction
    ├── epub/                 # EPUB spine + metadata extraction
    ├── pe/                   # PE (Windows executable) metadata
    ├── dispatch/             # Unified bytes-based dispatch — single source of truth
    └── archive/              # ZIP / TAR / GZ / BZ2 / XZ / 7Z + orchestration
```

### Extractor crates

Each extractor is **both a library and a standalone binary**:

- **Library** – linked into `find-scan` for zero-overhead in-process extraction
- **Binary** – standalone CLI for future use by `find-scan-watch` (subprocess mode)

```
find-extract-text    [~2 MB]   gray_matter, serde_yaml, content_inspector
find-extract-pdf     [~10 MB]  pdf-extract
find-extract-media   [~15 MB]  kamadak-exif, id3, metaflac, mp4ameta, audio-video-metadata
find-extract-html    [~3 MB]   scraper (html5ever)
find-extract-office  [~5 MB]   calamine, quick-xml
find-extract-epub    [~3 MB]   quick-xml
find-extract-pe      [~2 MB]   goblin
find-extract-dispatch [~1 MB]  infer + all above extractor libs (unified dispatch)
find-extract-archive  [~6 MB]  zip, tar, flate2, bzip2, xz2, sevenz-rust2
                               + find-extract-dispatch (member delegation via dispatch)
```

Dependency diagram (runtime linkage in `find-scan`):

```
find-common
    ↑
find-extract-{text, pdf, media, html, office, epub, pe}
    ↑
find-extract-dispatch   ← single source of truth for bytes-based dispatch
    ↑               ↑
find-extract-archive   find-client (find-scan)

find-server
  └─ find-common          (no extractors – lean binary)
```

---

## Write Path (Indexing)

```
find-scan → POST /api/v1/bulk (gzip JSON) → inbox/{id}.gz on disk
                                                    │
                                         background worker (polls every 1s)
                                                    │
                                   for each file: remove old chunks from ZIPs
                                                    │
                                   chunk content → append to content_NNNNN.zip
                                                    │
                                   upsert files table + insert lines table + FTS5
```

Key invariants:
- **All DB writes go through the inbox worker** — no route handler writes SQLite directly.
- The bulk route handler only writes a `.gz` file to `data_dir/inbox/` and returns `202 Accepted`.
- The worker processes inbox files sequentially — no concurrent SQLite writes.
- Within a `BulkRequest`, the worker processes **deletes first, then upserts** so renames work correctly.

---

## Content Storage (ZIP Archives)

File content is stored in rotating ZIP archives, not inline in SQLite.

```
data_dir/sources/content/
  0000/content_00001.zip
  0000/content_00002.zip
  ...
  0001/content_01000.zip
  ...
```

- Folder: `content/{archive_num / 1000:04}` (4-digit zero-padded subfolder)
- Archive: `content_{archive_num:05}.zip` (5-digit zero-padded)
- Target: ~10 MB per archive (measured by compressed on-disk size)
- Maximum: 9,999 × 1,000 = 9,999,000 archives (~99.99 TB)

Each file's content is split into ~1 KB chunks:
- Chunk name: `{relative_path}.chunk{N}.txt`
- The `lines` table stores `(chunk_archive, chunk_name, line_offset_in_chunk)`
- No content inline in SQLite — all content lives in ZIPs

---

## Read Path (Search)

```
GET /api/v1/search → FTS5 query → candidate rows (chunk_archive, chunk_name, line_offset)
                   → read chunk from ZIP (cached per request)
                   → return matched lines + snippets
```

Context retrieval (`/api/v1/context`, `/api/v1/file`) reads chunks the same way, with a
per-request `HashMap` cache to avoid re-reading the same chunk file twice.

---

## Archive Members as First-Class Files

Archive members use **composite paths** with `::` as separator:

```
taxes/w2.zip::wages.pdf          (member of a ZIP)
data.tar.gz::report.txt          (member of a tarball)
outer.zip::inner.zip::file.txt   (nested archives)
```

- Each member has its own `file_id` in the `files` table
- `::` is reserved — cannot appear in regular file paths
- Members get their `kind` detected from their own filename (not the outer archive)
- Deletion: `DELETE FROM files WHERE path = 'x' OR path LIKE 'x::%'` cleans up members
- Re-indexing: server deletes `path LIKE 'archive::%'` members before re-inserting
- Client filters `::` paths from deletion detection (outer files only)

**Tree browsing**: `GET /api/v1/tree?prefix=archive.zip::` lists archive members.
Archive files (`kind="archive"`) expand in the tree like directories.

---

## Archive Extractor: Member Delegation

The archive extractor acts as an **orchestrator**: it decompresses archive members one
at a time and delegates each member's bytes to `find-extract-dispatch`, which applies
the same priority-ordered extraction pipeline used for regular files.

```
archive.zip
  ├── report.pdf   → dispatch_from_bytes() → find_extract_pdf
  ├── notes.txt    → dispatch_from_bytes() → find_extract_text
  ├── photo.jpg    → dispatch_from_bytes() → find_extract_media
  ├── document.docx→ dispatch_from_bytes() → find_extract_office   ← same as regular files
  ├── page.html    → dispatch_from_bytes() → find_extract_html      ← same as regular files
  ├── nested.zip   → recursive extraction (in-memory via Cursor)
  ├── data.log.gz  → decompress in-memory, dispatch as text
  └── qjs (ELF)   → dispatch_from_bytes() → MIME fallback → [FILE:mime] application/x-elf
```

**Dispatch priority order** (identical for archive members and regular files):
PDF → Media → HTML → Office → EPUB → PE → Text → MIME fallback

**MIME fallback**: For unrecognised binary content, dispatch emits a `line_number=0` line
`[FILE:mime] <mime>` (e.g. `application/x-elf`). The caller uses this to set the file's
`kind` accurately instead of falling back to `"unknown"`.

**Supported archive formats**: ZIP, TAR, TAR.GZ, TAR.BZ2, TAR.XZ, GZ, BZ2, XZ, 7Z

**Depth limiting**: Controlled by `scan.archives.max_depth` (default: 10). When exceeded,
only the filename is indexed and a warning is logged.

---

## Content Extraction: Memory Model

**All extraction is fully in-memory.** There is no true byte-level streaming.

| Path | Code | Memory behaviour |
|------|------|-----------------|
| Regular file | `dispatch_from_path` → `std::fs::read(path)` | Whole file buffered into `Vec<u8>` |
| Archive member | `read_to_end(&mut bytes)` inside extractor loop | Whole member buffered into `Vec<u8>` |
| Nested archive | Recursive `find_extract_archive::extract_from_bytes` | Member bytes already in `Vec<u8>` |

"Streaming" in the current architecture means **iterating archive members one at a
time** — while one member is being extracted, the rest of the archive has not been
read. Each individual member is still fully buffered.

### Memory cap (`max_size_kb`)

`ExtractorConfig::max_size_kb` (derived from `scan.archives.max_member_size_mb` in
`scan.toml`) is the per-member memory limit:

- **Regular files**: skipped entirely if `fs::metadata().len() > max_size_kb * 1024`
  (checked in `dispatch_from_path` before reading).
- **Archive members**: guarded by `take(max_size_kb * 1024 + 1)` as a hard cap on
  the actual `read_to_end` call, independent of what the archive header reports.
  If a member exceeds the limit, only its filename line is indexed and the
  remainder of the member stream is drained to keep the decompressor in sync.

The `take()` guard is the critical safeguard against OOM. Some archive formats
(notably solid 7z blocks) report `entry.size() = 0` for all entries, so a
pre-read size check alone cannot prevent allocating the full decompressed member.

### Future: streaming extractor API

The intended long-term improvement is to allow extractors to accept `impl Read`
in addition to `&[u8]`, so large members can be piped through without buffering.
See the Roadmap section "Memory-Safe Archive Extraction (Streaming)".

---

## Extractor Binary Protocol

Each extractor binary can be invoked standalone (for future use by `find-scan-watch`):

```bash
find-extract-text   [--max-size-kb N] <file-path>   → JSON array of IndexLine
find-extract-pdf    [--max-size-kb N] <file-path>   → JSON array of IndexLine
find-extract-media  [--max-size-kb N] <file-path>   → JSON array of IndexLine
find-extract-archive <file-path> [max-size-kb] [max-depth]  → JSON array of IndexLine
```

**IndexLine** fields:
- `line_number` — 0 = filename/metadata; 1+ = content lines
- `content` — text content of the line
- `archive_path` — member path within archive (None for regular files)

---

## Directory Tree

`GET /api/v1/tree?source=X&prefix=foo/bar/` uses a **range-scan** on the `files` table:

```sql
WHERE path >= 'foo/bar/' AND path < 'foo/bar0'
```

`prefix_bump` increments the last byte of the prefix to get the upper bound.
Results are grouped server-side into virtual directory nodes and file nodes.
Only immediate children are returned; the UI lazy-loads subdirectories on expand.

---

## Server Routes

The server's HTTP handlers live in `crates/server/src/routes/`, split by concern:

| File | Endpoints |
|------|-----------|
| `routes/mod.rs` | Shared helpers (`check_auth`, `source_db_path`, `compact_lines`); `GET /api/v1/metrics` |
| `routes/search.rs` | `GET /api/v1/search` — fuzzy / exact / regex modes, multi-source parallel query |
| `routes/context.rs` | `GET /api/v1/context`, `POST /api/v1/context-batch`; returns `{start, match_index, lines[], kind}` |
| `routes/file.rs` | `GET /api/v1/file`, `GET /api/v1/files` |
| `routes/tree.rs` | `GET /api/v1/sources`, `GET /api/v1/tree` |
| `routes/bulk.rs` | `POST /api/v1/bulk` — writes gzip to inbox, returns 202 immediately |

`check_auth` and `source_db_path` are `pub(super)` so only submodules can call them.

---

## Web UI Structure

The SvelteKit frontend (`web/src/`) follows a coordinator + view component pattern:

```
routes/+page.svelte     — thin coordinator: owns all state, no layout code
lib/
  appState.ts           — pure functions: buildUrl(), restoreFromParams(), AppState type
  SearchView.svelte     — search topbar + ResultList + error display
  FileView.svelte       — file topbar + sidebar (DirectoryTree) + viewer panel
  ResultList.svelte     — pure display component; renders result cards, no scroll logic
  SearchResult.svelte   — single result card with lazy-loaded context lines
  FileViewer.svelte     — full file display (text, markdown, binary, image, PDF)
  api.ts                — typed fetch wrappers for all server endpoints
```

**State management**: All mutable state (query, results, file path, view mode, etc.) lives
in `+page.svelte`. Child components receive props and emit typed Svelte events upward.

**Page scroll architecture**: The page scrolls naturally (no inner scroll container in
`ResultList`). The search topbar is `position: sticky; top: 0`. A `window` scroll listener
in `+page.svelte` calls `triggerLoad()` when within 600 px of the bottom.

**Pagination**: `+page.svelte` fetches the next batch (limit 50, offset = current length)
and deduplicates by `source:path:line_number` before appending — the search API can return
overlapping results across page boundaries. A new search resets `results` and scrolls to
top. No virtual DOM recycling — plain `{#each}` with dedup is adequate for these batch sizes.

**Context lines**: `SearchResult` fetches context lazily via `IntersectionObserver` — only
when the card scrolls into view — to avoid a burst of N requests on initial load. A
placeholder bar is shown until loaded. Falls back silently to the `snippet` field if the
request fails or returns empty lines. The `ContextResponse` returns `{start, match_index,
lines: string[], kind}` where `start` is the first line number and `match_index` locates
the matched line within the window (null if the match falls in a sparse gap).

**URL / history**: `buildUrl` encodes `q`, `mode`, `source[]`, `path`, and `panelMode`
into query params. `restoreFromParams` reconstructs `AppState` from `URLSearchParams`.
`history.pushState` / `replaceState` are called directly in `+page.svelte`.

---

## Snippet Retrieval

The `snippet` field in search results is **not stored in SQLite**. It is read live from
ZIP archives at query time:

1. FTS5 trigram index matches the query → returns `rowid`s (no text stored, `content=''`)
2. Join to `lines` table → gets `(chunk_archive, chunk_name, line_offset_in_chunk)`
3. Read chunk text from ZIP → index into lines by offset → that string is the snippet

A per-request `HashMap` cache avoids re-reading the same chunk for multiple results.

**Implication**: For files with very long lines (e.g., PDFs with no line breaks), the
snippet can be very large because there is no truncation in the pipeline. The full line
content is returned verbatim in the JSON response.

---

## Key Files

| File | Purpose |
|------|---------|
| `crates/common/src/api.rs` | All HTTP request/response types |
| `crates/common/src/config.rs` | Client + server config structs |
| `crates/extractors/text/src/lib.rs` | Text + Markdown frontmatter extraction |
| `crates/extractors/pdf/src/lib.rs` | PDF extraction (with catch_unwind) |
| `crates/extractors/media/src/lib.rs` | Image EXIF, audio tags, video metadata |
| `crates/extractors/html/src/lib.rs` | HTML text extraction + metadata |
| `crates/extractors/office/src/lib.rs` | DOCX / XLSX / PPTX extraction |
| `crates/extractors/epub/src/lib.rs` | EPUB spine + metadata extraction |
| `crates/extractors/pe/src/lib.rs` | PE (Windows executable) metadata |
| `crates/extractors/dispatch/src/lib.rs` | Unified bytes-based dispatch + `mime_to_kind` |
| `crates/extractors/archive/src/lib.rs` | Archive format iteration + orchestration |
| `crates/client/src/extract.rs` | Top-level dispatcher: archive vs. dispatch_from_path |
| `crates/client/src/scan.rs` | Filesystem walk, batch building, submission |
| `crates/server/src/worker.rs` | Inbox polling loop + BulkRequest processing |
| `crates/server/src/archive.rs` | ZIP archive management + chunk_lines() |
| `crates/server/src/db.rs` | All SQLite operations |
| `crates/server/src/routes/` | HTTP route handlers (see Server Routes above) |
| `crates/server/src/schema_v2.sql` | DB schema |
| `web/src/lib/api.ts` | TypeScript API client |
| `web/src/lib/appState.ts` | URL serialisation + AppState type |
| `web/src/routes/+page.svelte` | Main page — coordinator, owns all state |
| `web/src/lib/SearchView.svelte` | Search topbar + result list |
| `web/src/lib/FileView.svelte` | File topbar + sidebar + viewer panel |

---

## Key Invariants

- **`line_number = 0`** is always the file's relative path, indexed so every file is
  findable by name even if content extraction yields nothing.
- **FTS5 index is contentless** (`content=''`); content lives only in ZIPs. FTS5 is
  populated manually by the worker at insert time. The `lines` table stores only
  `(chunk_archive, chunk_name, line_offset_in_chunk)` — no content column in SQLite.
- **`archive_path` on `IndexLine`** is deprecated (schema v3) — composite paths in
  `files.path` replaced it. For backward compatibility, API endpoints still accept an
  `archive_path` query param.
- **The `files` table is per-source** — one SQLite DB per source name, stored at
  `data_dir/sources/{source}.db`. ZIP archives are shared across sources.
- **PDF extraction** wraps `pdf-extract` in `std::panic::catch_unwind` because the
  library panics on malformed PDFs rather than returning errors.

---

## Plan 015 Status: Extractor Architecture Refactor

Phase 1 is **complete**:

| Goal | Status |
|------|--------|
| Extractor crates created (`text`, `pdf`, `media`, `archive`) | ✅ Done |
| Each extractor is both a library and a CLI binary | ✅ Done |
| `find-scan` links all extractor libraries statically | ✅ Done |
| Archive extractor orchestrates PDF, media, text, nested archives | ✅ Done |
| bz2/xz archive format support | ✅ Done |
| `max_depth` passed through from config to archive extractor | ✅ Done |
| Old extractors removed from `find-common` | ✅ Done |
| `find-common` has zero extractor dependencies (lean server binary) | ✅ Done |

Phase 2 (incremental client `find-scan-watch`) and Phase 3 (subprocess spawning in the
archive extractor) are **not yet implemented**. See `docs/plans/015-extractor-architecture-refactor.md`.
