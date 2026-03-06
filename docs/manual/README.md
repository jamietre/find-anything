# find-anything Manual

Full documentation for find-anything — distributed full-content file indexing and search.

---

## Table of Contents

1. [Installation](01-installation.md)
   - [Server (Linux & macOS)](01-installation.md#server-linux--macos)
   - [Client (Linux & macOS)](01-installation.md#client-linux--macos)
   - [Windows client](01-installation.md#windows-client)
   - [Docker](01-installation.md#docker)
   - [Build from source](01-installation.md#build-from-source)

2. [Configuration](02-configuration.md)
   - [Server config (server.toml)](02-configuration.md#server-config-servertoml)
   - [Client config (client.toml)](02-configuration.md#client-config-clienttoml)
   - [Sources](02-configuration.md#sources)
   - [Scan settings](02-configuration.md#scan-settings)
   - [Archive settings](02-configuration.md#archive-settings)
   - [Per-directory control (.noindex / .index)](02-configuration.md#per-directory-control-noindex--index)
   - [Log suppression](02-configuration.md#log-suppression)

3. [Indexing](03-indexing.md)
   - [How indexing works](03-indexing.md#how-indexing-works)
   - [Running find-scan](03-indexing.md#running-find-scan)
   - [Running find-watch](03-indexing.md#running-find-watch)
   - [Incremental vs full scans](03-indexing.md#incremental-vs-full-scans)
   - [Archives](03-indexing.md#archives)
   - [Checking indexing status](03-indexing.md#checking-indexing-status)
   - [Extraction errors](03-indexing.md#extraction-errors)

4. [Search](04-search.md)
   - [Search modes](04-search.md#search-modes)
   - [Natural language date queries](04-search.md#natural-language-date-queries)
   - [Date range quick reference](04-search.md#date-range-quick-reference)
   - [Advanced filters](04-search.md#advanced-filters)
   - [Filtering by source](04-search.md#filtering-by-source)
   - [CLI search](04-search.md#cli-search)

5. [Web UI](05-web-ui.md)
   - [Search box](05-web-ui.md#search-box)
   - [Results list](05-web-ui.md#results-list)
   - [File viewer](05-web-ui.md#file-viewer)
   - [File tree sidebar](05-web-ui.md#file-tree-sidebar)
   - [Command palette (Ctrl+P)](05-web-ui.md#command-palette-ctrlp)
   - [Settings](05-web-ui.md#settings)

6. [Supported file types](06-file-types.md)
   - [Text and source code](06-file-types.md#text-and-source-code)
   - [Documents](06-file-types.md#documents)
   - [Archives](06-file-types.md#archives)
   - [Media](06-file-types.md#media)
   - [Windows executables](06-file-types.md#windows-executables)

7. [Administration](07-administration.md)
   - [find-admin commands](07-administration.md#find-admin-commands)
   - [Inbox management](07-administration.md#inbox-management)
   - [Monitoring and status](07-administration.md#monitoring-and-status)

8. [Running as a service](08-services.md)
   - [systemd (Linux)](08-services.md#systemd-linux)
   - [Windows service](08-services.md#windows-service)
   - [Docker](08-services.md#docker)

9. [Troubleshooting](09-troubleshooting.md)
   - [find-watch crashes immediately](09-troubleshooting.md#find-watch-crashes-immediately)
   - [Files not appearing in search](09-troubleshooting.md#files-not-appearing-in-search)
   - [Synology NAS](09-troubleshooting.md#synology-nas)
   - [High memory usage](09-troubleshooting.md#high-memory-usage)
   - [Slow search](09-troubleshooting.md#slow-search)

---

## Quick orientation

find-anything is a **two-process system**:

- **`find-server`** — runs on one central machine. Stores the index (SQLite + ZIP archives) and serves search queries via HTTP.
- **Client tools** (`find-scan`, `find-watch`) — run on each machine whose files you want to search. They extract content and push it to the server.

The web UI and the `find-anything` CLI are both query clients — they talk to the server to retrieve results. Neither stores any data locally.

```
[client machine]                   [server machine]
  find-scan  ──── POST /bulk ────▶  find-server ──▶ SQLite + ZIP
  find-watch ──── POST /bulk ────▶       │
                                         │◀── GET /search
                                   web UI / find-anything CLI
```

See [Installation](01-installation.md) to get started.
