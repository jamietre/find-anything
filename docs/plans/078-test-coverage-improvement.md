# 078 — Test Coverage Improvement

## Overview

Current workspace coverage: **65% lines / 70% functions** (from `mise run coverage`).

Several critical paths are significantly undertested. This plan targets the highest-value
gaps — particularly routes with <50% coverage and complex DB logic with no unit tests.

The goal is not 100% coverage but meaningful coverage of critical paths: search, admin
operations, context/file retrieval, stats, and the core DB query functions.

**Not in scope:** binary `main.rs` entry points (all 0%, not meaningfully testable via
unit tests), `update_check`/`update_apply` (network-dependent), SSE stream body content
(only verify headers and initial response), and `routes/raw.rs` serving content from real
filesystem mounts (requires significant test harness work for uncertain return).

---

## Current State

| File | Lines | Functions | Priority |
|------|-------|-----------|----------|
| `routes/admin.rs` | 26% | 32% | **High** — compact, delete, inbox ops |
| `routes/recent.rs` | 31% | 27% | **High** — no dedicated test file |
| `routes/context.rs` | 32% | 38% | **High** — no dedicated test file |
| `routes/mod.rs` | 44% | 45% | **Medium** — auth helpers, metrics |
| `db/search.rs` | 62% | 68% | **High** — `document_candidates` has 0 unit tests |
| `db/stats.rs` | 55% | 70% | **Medium** — pending content, ext histogram |
| `routes/search.rs` | 57% | 60% | **Medium** — filters, pagination, archive members |
| `routes/stats.rs` | 53% | 46% | **Medium** — stats stream untested |
| `extractors/pe/src/lib.rs` | 14% | 12% | **Medium** — near-zero coverage |
| `routes/raw.rs` | 0% | 0% | **Low** — deferred (needs real FS mount) |

---

## Phase 1 — Admin routes (`routes/admin.rs`: 26% → ~65%)

Add to `crates/server/tests/admin.rs`.

### Inbox operations (currently untested)

- **`test_inbox_status_after_indexing`** — POST /bulk to queue a file, call GET
  /admin/inbox before the worker drains, verify `pending > 0`.

- **`test_inbox_clear_pending`** — queue requests via bulk endpoint, pause inbox so
  they sit in pending, call DELETE /admin/inbox?target=pending, verify pending returns
  to zero; failed count unaffected.

- **`test_inbox_clear_all`** — seed both pending and failed gz files, call DELETE
  /admin/inbox?target=all, verify both cleared.

- **`test_inbox_retry_moves_failed_to_pending`** — directly write a `.gz` file to
  `data_dir/inbox/failed/` (rename pattern `test_*.gz`), call POST
  /admin/inbox/retry, verify the file appears in pending count.

- **`test_inbox_pause_and_resume_stops_processing`** — POST /admin/inbox/pause,
  submit a bulk request, verify it is not processed while paused (inbox pending count
  stays > 0), POST /admin/inbox/resume, wait for drain, verify processed.

### Compact with real content (currently only smoke-tested)

- **`test_compact_removes_orphaned_chunks`** — index a file (so chunks are written),
  then directly DELETE the file row from SQLite to orphan its chunks, run
  POST /admin/compact (not dry-run), verify `chunks_removed > 0` and `bytes_freed > 0`.

- **`test_compact_deletes_fully_orphaned_archive`** — same setup as above but orphan
  all entries in a given archive; verify `archives_deleted >= 1` in the response.

- **`test_compact_dry_run_shows_orphaned`** — same setup, run dry-run, verify counts
  are non-zero but the archive file still exists on disk afterward.

### Delete source with archive content

- **`test_delete_source_removes_chunk_refs`** — index a file, confirm `content_chunks`
  rows exist for its source, DELETE /admin/source, verify the source DB is gone and
  content_chunks rows for that source no longer appear in any archive scan.

---

## Phase 2 — `db/search.rs`: `document_candidates` unit tests

`document_candidates()` is the document-mode search path (used when `mode=document`).
It has 0 unit tests despite being ~100 lines of non-trivial logic (token intersection,
per-file cap, uncovered-terms tracking).

Add inline unit tests to `crates/server/src/db/search.rs`:

- **`document_candidates_returns_file_for_matching_token`** — index a file containing
  `"hello world"`, call `document_candidates("hello", ...)`, verify at least one result
  for that file.

- **`document_candidates_multi_token_requires_all_tokens`** — index file A with both
  `"alpha"` and `"beta"`, file B with only `"alpha"`. Query `"alpha beta"` in document
  mode; only file A should appear.

- **`document_candidates_per_file_cap`** — index a file with the same keyword on 20+
  lines; verify the result count for that file does not exceed the per-file cap
  (currently 5 or whatever `MAX_LINES_PER_FILE` is set to).

- **`document_candidates_empty_query_returns_empty`** — empty token list, verify no
  panic and empty result.

---

## Phase 3 — Context route (`routes/context.rs`: 32% → ~75%)

New file: `crates/server/tests/context.rs`

- **`test_get_context_returns_surrounding_lines`** — index a multi-line file, search
  for a term on an interior line, call GET /api/v1/context with the returned
  `(source, path, line_number)`, verify the response includes lines before and after.

- **`test_context_returns_correct_line_count`** — request `before=3&after=3`, verify
  response has exactly those surrounding lines (clamped at file boundaries).

- **`test_context_for_archive_member`** — index a ZIP containing a text file with
  known content, search to get a line from the member, call /context with the
  composite path, verify lines returned.

- **`test_context_batch_multiple_items`** — POST /api/v1/context-batch with 3 items
  (different sources/paths), verify 3 results returned in the same order.

- **`test_context_missing_file_returns_404`** — call /context with a non-existent
  path, verify 404.

---

## Phase 4 — Recent route (`routes/recent.rs`: 31% → ~70%)

New file: `crates/server/tests/recent.rs`

- **`test_recent_returns_indexed_files`** — index 3 files, call GET /api/v1/recent,
  verify all 3 paths appear.

- **`test_recent_sorted_by_mtime`** — index files with different `mtime` values, call
  GET /api/v1/recent?sort=mtime, verify descending order.

- **`test_recent_limit_respected`** — index 10 files, call with `limit=3`, verify
  exactly 3 results.

- **`test_recent_stream_returns_sse_headers`** — GET /api/v1/recent/stream, verify
  `Content-Type: text/event-stream` in response headers (don't try to consume stream
  body).

---

## Phase 5 — Stats route + `db/stats.rs` (`routes/stats.rs`: 53% → ~75%)

### Route-level (add to `crates/server/tests/stats_cache.rs` or new `stats.rs`)

- **`test_stats_endpoint_returns_source`** — index files for a named source, GET
  /api/v1/stats, verify source name appears in `sources[]` with non-zero `files` count.

- **`test_stats_by_ext_in_stats_response`** — index `.js` and `.py` files, verify
  the ext histogram in the stats response includes those extensions.

- **`test_stats_stream_returns_sse_headers`** — same header-only check as recent/stream.

### Unit tests (add inline to `crates/server/src/db/stats.rs`)

- **`test_get_files_pending_content_counts_unarchived`** — insert a row into `files`
  with a non-null `content_hash` but no corresponding `content_chunks` row; verify
  `get_files_pending_content()` returns 1. Then insert the `content_chunks` row and
  verify it returns 0.

- **`test_get_stats_by_ext_excludes_archive_members`** — insert a regular file
  (`path = "foo.js"`) and an archive member (`path = "outer.zip::bar.js"`); verify
  `get_stats_by_ext()` only counts the regular file.

- **`test_upsert_indexing_errors_increments_count`** — call `upsert_indexing_errors`
  for the same path twice; verify the resulting row has `count = 2`.

---

## Phase 6 — Search filters and pagination (`routes/search.rs`: 57% → ~75%)

Add to `crates/server/tests/search_modes.rs` or new `crates/server/tests/search_filters.rs`:

- **`test_search_kind_filter`** — index one text file and one image file; search with
  `kind=text`, verify only the text file appears; repeat with `kind=image`.

- **`test_search_filename_only_filter`** — index a file, search for a term that appears
  only in the filename (line 0), use `filename_only=true`, verify hit; search with a
  term only in content, verify no hit.

- **`test_search_pagination_page_two`** — index 15+ files all containing the same
  keyword, fetch page 1 (limit=5), fetch page 2 (offset=5), verify no overlap between
  pages and combined count equals total.

- **`test_search_archive_member_content`** — index a ZIP with a member containing
  known text, search for that text, verify a result with a composite path (`outer.zip::member.txt`).

- **`test_search_returns_duplicate_paths`** — index two files with identical content,
  search for content term, verify at least one result has non-empty `duplicate_paths`.

---

## Phase 7 — PE extractor (`extractors/pe/src/lib.rs`: 14% → ~70%)

The PE extractor has near-zero coverage because no unit tests exist and the dispatcher
integration tests don't cover it.

Add inline unit tests to `crates/extractors/pe/src/lib.rs`:

- **Test fixture**: Add a minimal PE32 and PE64 binary fixture to
  `crates/extractors/pe/tests/fixtures/` (these can be pre-built minimal EXEs;
  alternatively use `pelite`'s test fixtures if available).

- **`test_accepts_exe_dll_extensions`** — verify `accepts()` returns true for `.exe`,
  `.dll`, `.sys`, false for `.txt`, `.zip`.

- **`test_extract_from_minimal_pe32`** — call `extract()` on a minimal PE32 fixture,
  verify at least a `[PE:x86]` or version line is returned.

- **`test_extract_from_minimal_pe64`** — same for PE64.

- **`test_extract_gracefully_handles_non_pe_bytes`** — pass random bytes to
  `extract_from_bytes()`, verify it returns an `Err` (or at minimum doesn't panic).

---

## Testing Strategy Notes

- All server integration tests use `TestServer::spawn()` from `crates/server/tests/helpers/`.
- To test inbox operations that require files to sit unprocessed, use
  `TestServer::spawn_paused()` (if it exists) or POST /admin/inbox/pause before the
  bulk request.
- For `db/` unit tests, open an in-memory SQLite DB with `db::open(":memory:")` and
  call `db::init_schema()` before each test (pattern already used in `db/search.rs`
  inline tests).
- For PE fixtures, keep binaries small — a 512-byte stub PE is sufficient to exercise
  the parsing paths without bloating the repo.

---

## Files Changed

| File | Change |
|------|--------|
| `crates/server/tests/admin.rs` | Add inbox and compact-with-content tests |
| `crates/server/tests/context.rs` | New file — context and context-batch tests |
| `crates/server/tests/recent.rs` | New file — recent files endpoint tests |
| `crates/server/tests/search_filters.rs` | New file — kind, pagination, archive member, dedup |
| `crates/server/tests/stats_cache.rs` | Add stats endpoint and stream header tests |
| `crates/server/src/db/search.rs` | Inline unit tests for `document_candidates` |
| `crates/server/src/db/stats.rs` | Inline unit tests for `get_files_pending_content`, `get_stats_by_ext`, `upsert_indexing_errors` count |
| `crates/extractors/pe/src/lib.rs` | Inline unit tests + fixtures in `tests/fixtures/` |

---

## Expected Outcome

| File | Current | Target |
|------|---------|--------|
| `routes/admin.rs` | 26% lines | ~65% |
| `routes/context.rs` | 32% | ~75% |
| `routes/recent.rs` | 31% | ~70% |
| `routes/search.rs` | 57% | ~75% |
| `routes/stats.rs` | 53% | ~70% |
| `db/search.rs` | 62% | ~80% |
| `db/stats.rs` | 55% | ~75% |
| `extractors/pe/src/lib.rs` | 14% | ~70% |
| **TOTAL** | **65% lines / 70% fn** | **~78% lines / ~82% fn** |
