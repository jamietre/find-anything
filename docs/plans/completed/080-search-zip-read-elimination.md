# 080 — Eliminate ZIP Reads from Search Path

## Overview

The search performance investigation (commit `22205b1`) found that
`read_chunk_for_file` was called once per FTS candidate, doing 2 SQL queries
each time — a 4-table JOIN through `files → content_blocks → content_chunks →
content_archives`. With `scoring_limit = 250` (page 0, limit=50) and multiple
sources queried in parallel on spinning-disk storage, the random ZIP accesses
dominated query latency (observed: 26s for `social security number` in fuzzy
mode).

## Status: Complete (commit `22205b1`)

All work was implemented in that commit:

- **`fts_candidates`** returns `content: String::new()` — no ZIP reads for any
  non-regex search mode.
- **`document_candidates`** returns empty content and no extras — no ZIP reads.
- **`read_content_batch`** added as the explicit entry point for callers that
  genuinely need content.
- **Regex modes** (`Regex`, `FileRegex`, `DocRegex`) call `read_content_batch`
  after `fts_candidates` and apply the post-filter to content — ZIP reads
  retained, required for correctness.
- **`filename_only` retain** changed from `c.content.starts_with("[PATH] ")` to
  `c.line_number == 0` — no content read needed; the FTS SQL
  (`SQL_FTS_FILENAME_ONLY`) already restricts candidates to `line_number = 0`.
- **Missing index** `idx_content_chunks_block_start ON content_chunks(block_id,
  start_line)` added — applied automatically at DB open for existing databases.

## Expected Outcome

| Mode | ZIP reads before | ZIP reads after |
|------|-----------------|-----------------|
| Fuzzy | 1 per candidate | **0** |
| Exact | 1 per candidate | **0** |
| Document | 1 per candidate | **0** |
| Regex | 1 per candidate | 1 per candidate (required) |
| FileFuzzy / FileExact | 1 per candidate | **0** |
| FileRegex | 1 per candidate | 1 per candidate (required) |
| DocExact | 1 per candidate | **0** |
| DocRegex | 1 per candidate | 1 per candidate (required) |

## Relationship to Plan 079

Plan 079 introduces `LINE_PATH`, `LINE_METADATA`, `LINE_CONTENT_START`
constants. When it lands, the literal `0` in the `filename_only` retain can be
replaced with `LINE_PATH` for clarity — but this is cosmetic, not functional.
