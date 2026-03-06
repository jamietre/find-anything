# Administration

[← Manual home](README.md)

---

## find-admin commands

`find-admin` is the command-line tool for inspecting and managing the server. All commands require a running `find-server` and a valid `client.toml`.

### Status and monitoring

```sh
# Per-source summary: file counts, sizes, last scan, error counts
find-admin status

# Machine-readable JSON output
find-admin status --json

# List all indexed sources
find-admin sources

# Check server connectivity and auth token
find-admin check
```

**`find-admin status` output:**

```
source: home
  files:       12,430
  size:        1.2 GB
  last scan:   2026-03-06 14:22
  errors:      3

source: code
  files:       84,201
  size:        4.7 GB
  last scan:   2026-03-06 14:23
  errors:      0
```

**`find-admin check`** pings the server and verifies the token is accepted. Useful for confirming that a new client installation can reach the server before running `find-scan`.

### Rescan and deletion

```sh
# Re-index a single file immediately
find-scan /path/to/file.pdf

# Force re-index all files (ignores mtimes)
find-scan --upgrade

# Preview what would be indexed without touching the server
find-scan --dry-run
```

See [Indexing](03-indexing.md) for full `find-scan` options.

---

## Inbox management

The server processes incoming batches through an inbox directory (`data_dir/inbox/`). Normally the inbox drains automatically within a second or two of each `find-scan` run.

**Checking inbox state:**

```sh
find-admin status --json | jq '.worker_status'
```

The `worker_status` field shows whether the background worker is `idle` or `processing`, and how many batches are queued. The web UI Settings → Stats page also shows this in real time.

**If the inbox is stuck** (worker shows `processing` for a long time):

1. Check the server logs for errors: `journalctl -u find-server -n 100` (systemd) or the Docker log.
2. The most common cause is a corrupt batch file. The worker will log the filename and error and move on.
3. Restart the server if the worker appears hung: `systemctl restart find-server`.

---

## Monitoring and status

### Web UI — Settings → Stats

The Stats page in the web UI shows:

- File count and indexed size per source
- Breakdown by file kind (pdf, text, image, etc.) and by extension
- Last scan timestamp per source
- Worker status (idle / processing) with automatic refresh

### Web UI — Settings → Errors

Lists all files with extraction failures, grouped by source. Each entry shows the file path and the error. Clicking an entry opens the file in the viewer.

Extraction errors do **not** prevent the file from appearing in search results — files with errors are still indexed by filename and path. Only content-level matches are unavailable.

### CLI

```sh
# Quick summary with error counts
find-admin status

# Full JSON including per-file error details
find-admin status --json
```

**Forcing a retry on failed files:**

```sh
find-scan /path/to/problematic-file.pdf
```

This re-extracts and re-indexes that specific file, clearing the error if extraction now succeeds.

---

## Database management

Each source has its own SQLite database at `data_dir/sources/{source}.db`. File content is stored in rotating ZIP archives at `data_dir/sources/content/`.

**Backing up the index:**

```sh
# Stop the server first to avoid a partial backup
systemctl stop find-server

# Copy the entire data directory
cp -r /var/lib/find-anything /backup/find-anything-$(date +%Y%m%d)

systemctl start find-server
```

Alternatively, use SQLite's online backup API by copying the `.db` files while the server is running — SQLite WAL mode makes this safe, though a brief stop is simpler.

**Removing a source:**

To completely remove a source and its data:

1. Remove the `[[sources]]` entry from `client.toml` on the relevant client machine(s).
2. On the server machine, delete the source database: `rm data_dir/sources/{source}.db`
3. Optionally reclaim archive space: archive ZIP files are shared and do not automatically shrink when a source is deleted. Run `find-admin compact` (if available in your version) or accept that orphaned chunks will remain until the next full rebuild.

**Rebuilding from scratch:**

```sh
systemctl stop find-server
rm -rf /var/lib/find-anything/sources/
systemctl start find-server
# Then re-run find-scan on each client machine
find-scan
```

---

[← Supported file types](06-file-types.md) | [Next: Running as a service →](08-services.md)
