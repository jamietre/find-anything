# Indexing

[← Manual home](README.md)

---

## How indexing works

Indexing is a two-step pipeline:

1. **Extraction** — `find-scan` or `find-watch` walks the filesystem, identifies changed files, extracts their text content using the appropriate `find-extract-*` extractor, and assembles batches.
2. **Ingestion** — Batches are compressed (gzip) and `POST`ed to `find-server`, which writes them to a per-source SQLite database and stores content in rotating ZIP archives.

The server processes incoming batches asynchronously in a background worker. The HTTP endpoint returns `202 Accepted` immediately; actual indexing happens a moment later. This design keeps the server's write path single-threaded and contention-free.

```
find-scan
  │
  ├── walk filesystem
  ├── compare mtimes with server state
  ├── extract content (in-process or via find-extract-* subprocess)
  └── POST /api/v1/bulk (gzip JSON)
                │
                ▼
          find-server inbox/
                │
                ▼ (background worker, ~1s)
          SQLite + content ZIPs
```

---

## Running find-scan

`find-scan` performs an **incremental scan** by default: it walks all source paths, compares each file's modification time against the server's record, and only re-indexes files that have changed or are new. Deleted files are removed from the index.

```sh
# Incremental scan (normal usage)
find-scan

# Preview what would change without touching the index
find-scan --dry-run

# Re-index any files that were scanned with an older tool version
find-scan --upgrade

# Re-index a single specific file immediately
find-scan /home/alice/documents/report.pdf

# Re-index all files in a directory
find-scan /home/alice/projects/myapp/

# Suppress per-file logs (show only summary)
find-scan --quiet
```

**When to run `find-scan`:**

- Once after first installation, for the initial full index
- After `find-watch` has been offline for a period (to catch up on missed changes)
- After bulk operations that don't go through the normal filesystem (rsync, restore from backup)
- After updating to a new version, with `--upgrade` to pick up extraction improvements

---

## Running find-watch

`find-watch` is a long-running daemon that listens for filesystem events (inotify on Linux, FSEvents on macOS, ReadDirectoryChangesW on Windows) and incrementally re-indexes files as they change.

```sh
find-watch
```

Changes are debounced (default: 500 ms) to avoid re-indexing a file multiple times during a rapid sequence of writes (e.g. an editor saving incrementally).

**Important:** `find-watch` does not perform an initial scan on startup. Run `find-scan` once first to populate the index, then start `find-watch` to keep it current.

In production, `find-watch` should be managed as a service — see [Running as a service](08-services.md).

---

## Incremental vs full scans

| Scenario | Command |
|---|---|
| Normal daily usage | `find-watch` (always running) |
| First-time index population | `find-scan` |
| After being offline | `find-scan` |
| After tool upgrade | `find-scan --upgrade` |
| Re-index one file now | `find-scan /path/to/file` |
| Check what would change | `find-scan --dry-run` |

`find-scan` without `--upgrade` uses **mtime-based** change detection: if a file's modification time matches what the server has recorded, the file is skipped. This makes incremental scans fast even over large trees.

`find-scan --upgrade` ignores the mtime comparison for files that were indexed with an older scanner version, forcing them through the current extractor. Use this after updating find-anything to pick up improvements in content extraction.

---

## Archives

When `scan.archives.enabled = true` (the default), archive files are opened and their members are extracted and indexed individually. This applies recursively — archives within archives are supported up to `max_depth` (default: 10).

**Supported archive formats:** ZIP, TAR, TGZ, TBZ2, TXZ, GZ, BZ2, XZ, 7Z

Archive members are identified in the index using a composite path with `::` as the separator:

```
taxes/2024.zip::W2.pdf
projects/backup.tar.gz::src/main.rs
data.zip::inner.zip::nested-file.txt
```

In the web UI, archive files expand in the file tree like directories, and their members appear as individual search results with the full composite path shown.

**Memory considerations for 7z:** 7z solid archives decompress an entire solid block to access any member. The `max_7z_solid_block_mb` setting caps this. On memory-constrained systems (NAS boxes, containers with limited RAM), lower this value (e.g. `64`). Members in blocks that exceed the limit are indexed by filename only.

---

## Text normalization

Before content is written to the index, the server normalises each text file to improve search quality:

1. **Built-in pretty-printing** — JSON and TOML are reformatted with consistent indentation.
2. **External formatter** — optional tools (Biome, Prettier, Ruff, …) can reformat code files.
3. **Word-wrap** — lines longer than `max_line_length` (default: 120) are split at word boundaries.

External formatters run in **batch mode** (one process per request batch, not one per file), which keeps indexing fast even when a batch contains hundreds of JS or TS files.

See [Configuration → Text normalization](02-configuration.md#text-normalization) for the full formatter reference and recommended Biome/Prettier setup.

---

## Checking indexing status

```sh
# Per-source summary: file counts, sizes, last scan, error counts
find-admin status

# Machine-readable JSON
find-admin status --json

# Check server connectivity and auth
find-admin check

# List all indexed sources
find-admin sources
```

In the web UI, go to **Settings → Statistics** for a visual breakdown of file counts and sizes by type and source.

---

## Extraction errors

Extraction failures (corrupt PDFs, malformed archives, files that exceed memory limits) are tracked per file in the index. They do not stop the scan — the file is noted as failed, and the scan continues.

**Viewing errors:**

- **Web UI:** Settings → Errors — lists all files with extraction failures, grouped by source
- **CLI:** `find-admin status` shows an error count per source; use `--json` for the full list

**What causes extraction errors:**

- Corrupt or truncated PDF files
- Password-protected archives (content cannot be read without the password)
- Files whose decompressed size exceeds memory limits
- Unsupported internal formats within otherwise-recognized container types

Files with extraction errors are still indexed by their filename and path, so they remain findable — just without content-level matches.

**Forcing a retry:**

```sh
# Re-index a specific file to retry extraction
find-scan /path/to/problematic-file.pdf
```

---

[← Configuration](02-configuration.md) | [Next: Search →](04-search.md)
