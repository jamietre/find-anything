# Remove Inline Threshold + Fix File Hashing

## Overview

Two related issues with how file content and file identity are tracked:

1. **`inline_threshold_bytes` produces a silent content black hole.** When a file's
   extracted content exceeds the 256-byte inline threshold, the server writes it to
   FTS5 (searchable) but nowhere retrievable — only if phase 2 also runs and the file
   has a `content_hash`. Image/audio/video files currently get `content_hash = None`
   (see issue 2), so their EXIF/tag metadata is permanently unreadable after indexing
   even though it's findable by search.

2. **`content_hash` is not computed for media files.** `scan.rs` skips hashing for
   all `is_binary_ext_path` files (`.jpg`, `.mp3`, `.mp4`, etc.) to avoid blocking on
   locked system files (e.g. live VHDX on Windows). This over-broad skip means images
   and audio files never get a `content_hash`, so phase 2 can't write their content to
   `blobs.db`. The field should also be renamed to `file_hash` to clarify it is a hash
   of the raw binary file bytes — not of the extracted content — so that true binary
   duplicates are detected regardless of what extractors produce.

### Fix

- Remove the `inline_threshold_bytes` config and the `file_content` two-tier storage.
  All content always goes to `blobs.db` via phase 2. The `file_content` table is
  dropped.
- Always compute a streaming blake3 hash of the raw file bytes for every file, except
  extensions that are known to block on open (vmdk, vhdx, qcow2, iso — live system
  images). Media files (jpg, mp3, mp4, etc.) must always be hashed.
- Rename `content_hash` → `file_hash` everywhere (DB column, Rust structs, API
  fields) to make the semantics clear.

## Design Decisions

### Why remove inline storage entirely?

The inline tier was added when the content store was ZIP-based — loading a 10 MB ZIP
to retrieve 2 lines was expensive, so tiny files were kept in the source DB. The
`SqliteContentStore` (blobs.db) replaced ZIPs: `get_lines` is now a single
PK-indexed SQLite range query returning only the rows needed. The performance argument
for inline no longer applies, and the complexity it introduces (two read paths, silent
data loss when threshold misconfigured) outweighs any benefit.

### Why split the binary-ext skip?

The original reason for `is_binary_ext_path` in hashing was to avoid blocking
`File::open` on Windows for live VHDX/VMDK images held open by Hyper-V. Media files
(jpg, png, mp3, mp4) are not in that category — they are always readable. Keeping
them in the skip list was an oversight. We narrow the skip to only truly dangerous
extensions.

### Why rename `content_hash` → `file_hash`?

The current name implies it might be a hash of extracted content (e.g. the EXIF
string), which could collide across different files. It is in fact a streaming blake3
hash of the raw binary file — a "file fingerprint" used for dedup and as the content
store key. The rename makes this unambiguous and prevents future confusion.

### Schema migration

Schema bumps from v13 → v14:
- `file_content` table dropped.
- `files.content_hash` column renamed to `files.file_hash`.
- Data in `file_content` is discarded (not migrated to blobs.db) — files that were
  stored only inline will show `content_unavailable = true` until re-indexed. Running
  `find-scan --upgrade` after upgrading the server re-indexes all files and populates
  `blobs.db` correctly.

`SCANNER_VERSION` is bumped so `--upgrade` triggers re-indexing of all files that
previously had no `file_hash` (all image/audio/video files).

## Implementation

### Phase 1 — Server: remove inline storage

**`crates/server/src/schema_v4.sql`** (new schema)
- Drop `file_content` table.
- Rename `files.content_hash` → `files.file_hash`.

**`crates/server/src/lib.rs`** (schema migration)
- Add migration step from v13 → v14:
  ```sql
  DROP TABLE IF EXISTS file_content;
  ALTER TABLE files RENAME COLUMN content_hash TO file_hash;
  ```
- Bump `SCHEMA_VERSION` to 14.

**`crates/server/src/worker/pipeline.rs`**
- Remove `inline_threshold_bytes` parameter and all `use_inline` branching.
- Always write FTS5 rows (as the current non-inline path does).
- Remove all `file_content` INSERT/DELETE/SELECT.
- Update `files` upsert to use `file_hash` column name.

**`crates/server/src/worker/mod.rs`**
- Remove `inline_threshold_bytes` from `WorkerConfig`.

**`crates/server/src/worker/request.rs`**
- Remove `inline_threshold_bytes` from calls to `process_file_phase1`.

**`crates/server/src/db/mod.rs`**
- Remove `file_content` read path from `read_chunk_for_file` and `read_content_batch`.
- Always go directly to `content_store.get_lines` keyed by `file_hash`.
- Update `content_unavailable`: remove the inline check; a file is unavailable only
  when `file_hash IS NOT NULL AND content_store.contains(&key) == false`.
- Update rename handler (no longer needs to patch `file_content` content).
- Update all column references `content_hash` → `file_hash`.

**`crates/server/src/db/stats.rs`**
- Update `get_files_pending_content`: remove `file_content` join; pending = files with
  `file_hash IS NOT NULL AND NOT EXISTS (blobs.db key)`.

**`crates/server/src/worker/archive_batch.rs`**
- Remove `is_inline` check (no longer needed — all files go through archive phase).

**`crates/common/src/config.rs`**
- Remove `inline_threshold_bytes` from `ServerConfig`.

**`crates/common/src/defaults_server.toml`**
- Remove `inline_threshold_bytes`.

### Phase 2 — Client: fix hashing

**`crates/extractors/text/src/lib.rs`**
- Split `is_binary_ext` into two functions:
  - `is_binary_ext`: retains only truly-locked-on-open extensions: `vmdk`, `vhdx`,
    `vhd`, `qcow2`, `iso`, `img`, `dmg` (large disk images that may block open on
    Windows).
  - Media formats (jpg, png, mp3, mp4, etc.) are removed from this list — they are
    handled by specialist extractors and must be hashed.
- Update `is_binary_ext_path` accordingly.
- Update `accepts_bytes` if needed (it uses `is_binary_ext` to short-circuit).

**`crates/client/src/scan.rs`**
- In `push_non_archive_files`: remove the `is_binary_ext_path` guard around
  `hash_file`; always call `hash_file` (which handles open failure gracefully via
  `Option`).
- Rename local variable `content_hash` → `file_hash`.
- Update `IndexFile` field: `content_hash` → `file_hash`.

**`crates/common/src/api.rs`**
- Rename `IndexFile.content_hash` → `IndexFile.file_hash`.
- Bump `SCANNER_VERSION` in `find_extract_types` so `--upgrade` triggers re-index of
  all previously un-hashed files.

### Phase 3 — Cleanup

**`crates/server/src/compaction.rs`**
- Update column references if any reference `content_hash`.

**`crates/server/tests/`**
- Update all test helpers that set `inline_threshold_bytes: 0`.
- Remove any tests that specifically test inline vs deferred branching.
- Add a test: image file with EXIF data → content is retrievable via `/file` endpoint
  after both phases complete.

**`CLAUDE.md`**
- Update architecture section to reflect current reality (done alongside this plan).

## Files Changed

| File | Change |
|------|--------|
| `crates/server/src/schema_v4.sql` | Drop `file_content`, rename `content_hash` → `file_hash` |
| `crates/server/src/lib.rs` | Migration v13→v14 |
| `crates/server/src/worker/pipeline.rs` | Remove inline logic entirely |
| `crates/server/src/worker/mod.rs` | Remove `inline_threshold_bytes` from config |
| `crates/server/src/worker/request.rs` | Remove threshold param |
| `crates/server/src/worker/archive_batch.rs` | Remove is_inline check |
| `crates/server/src/db/mod.rs` | Remove `file_content` read path |
| `crates/server/src/db/stats.rs` | Update pending-content count |
| `crates/common/src/config.rs` | Remove `inline_threshold_bytes` |
| `crates/common/src/defaults_server.toml` | Remove `inline_threshold_bytes` |
| `crates/extractors/text/src/lib.rs` | Narrow `is_binary_ext` to truly locked extensions |
| `crates/client/src/scan.rs` | Always hash; rename field |
| `crates/common/src/api.rs` | `content_hash` → `file_hash`; bump `SCANNER_VERSION` |
| `crates/server/tests/` | Update tests |
| `CLAUDE.md` | Update architecture docs |

## Testing

1. Index an image with EXIF data → `/api/v1/file` must return the EXIF in `metadata[]`.
2. Index an audio file with tags → same check.
3. Index two byte-identical files → server reports them as duplicates.
4. Index two files with identical EXIF but different content → NOT duplicates.
5. Large text file (> old 256-byte threshold) → content fully retrievable.
6. Tiny file (< 256 bytes, was previously inline) → content retrievable from blobs.db.
7. Schema migration: existing install with `file_content` rows → migration succeeds,
   rows discarded, files become `content_unavailable` until re-scanned.
8. `find-scan --upgrade` after upgrading server → all previously un-hashed image/audio
   files get re-indexed and become retrievable.

## Breaking Changes

- `inline_threshold_bytes` server config key is removed. Existing `server.toml` files
  with this key will produce a warning (unknown field) but still work.
- `MIN_CLIENT_VERSION` bump required: `IndexFile.content_hash` → `IndexFile.file_hash`
  is a breaking rename in the bulk API. Old clients sending `content_hash` will be
  rejected.
- Schema v14 requires a migration. Files previously stored only inline will lose their
  content until re-scanned (they remain searchable via FTS5).
