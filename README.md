# find-anything

Distributed full-content file indexing and fuzzy search. Index one or more machines
into a central server, then query everything from a single CLI.

```
find "password strength"
[code] src/auth/validate.rs:142  check_password_strength(input)?;
[code] docs/security.md:87       Password strength requirements: minimum 12 chars
```

---

## How it works

Source machines run a lightweight client (`find-scan`) that walks the filesystem,
extracts text content, and submits it to a central server over HTTP. The server
stores everything in SQLite with a trigram full-text index. A CLI tool (`find`) sends
queries to the server and returns ranked results.

```
┌─────────────────────────────────────────────┐
│              Central Server                  │
│  find-server  ──►  SQLite per source         │
│       │                                      │
│  find (CLI)    find-web (SvelteKit)          │
└──────────────────────┬──────────────────────┘
                       │  HTTP + bearer token
          ┌────────────┴────────────┐
          │                         │
   ┌──────▼──────┐           ┌──────▼──────┐
   │  Machine A  │           │  Machine B  │
   │  find-scan  │           │  find-scan  │  scheduled (cron / systemd)
   │  find-watch │           │  find-watch │  real-time (inotify / FSEvents)
   └─────────────┘           └─────────────┘
```

---

## Current state

### What works today

| Component | Status | Notes |
|-----------|--------|-------|
| `find-server` | Working | axum REST API, SQLite FTS5 trigram index, bearer token auth |
| `find-scan` | Working | incremental mtime-based scan, archive content indexing |
| `find` CLI | Working | fuzzy / exact / regex modes, colored output |
| `find-web` | Working | SvelteKit web UI with live search, syntax highlighting, file preview |
| `find-watch` | Stub | compiles, not yet implemented |
| Archive indexing | Working | zip, tar, tar.gz, tar.bz2, tar.xz, .gz, .bz2, .xz, 7z |
| Multi-source search | Working | server queries all source DBs in parallel |
| PDF extraction | Working | extracts text content from PDF files |
| Image metadata | Working | EXIF tags from JPEG, TIFF, PNG, HEIF, RAW formats |
| Audio metadata | Working | ID3, Vorbis, MP4 tags from MP3, FLAC, M4A files |

### Search modes

- **Fuzzy** (default) — splits query into words, each word must appear somewhere in
  the line, results ranked by nucleo score. `find "pass strength"` finds
  `"password strength"`.
- **Exact** — literal substring match. `find "pass strength" --mode exact` finds
  only lines containing that exact string.
- **Regex** — `find "fn\s+\w+_handler" --mode regex`

---

## Installation

### Prerequisites

- Rust toolchain (`rustup.rs`)
- (Optional, for web UI) Node.js 18+ - recommended via [mise](https://mise.jdx.dev)

### Build

```bash
git clone https://github.com/jamietre/find-anything
cd find-anything
cargo build --release
```

Binaries are produced in `target/release/`:

| Binary | Purpose |
|--------|---------|
| `find-server` | Central index server |
| `find-scan` | Index a source machine |
| `find` | Query the index |
| `find-watch` | Real-time file watcher (planned) |

---

## Quick start

### 1. Configure and start the server

```toml
# server.toml
[server]
bind     = "127.0.0.1:8765"
data_dir = "/var/lib/find-anything"
token    = "change-me"
```

```bash
./find-server server.toml
```

### 2. Configure and run a scan

```toml
# client.toml
[server]
url   = "http://127.0.0.1:8765"
token = "change-me"

# Multiple sources can be defined
[[sources]]
name  = "code"
paths = ["/home/user/code"]

[[sources]]
name  = "documents"
paths = ["/home/user/Documents"]

[scan]
exclude = [
    "**/.git/**",
    "**/node_modules/**",
    "**/target/**",
]
max_file_size_kb = 1024
```

```bash
./find-scan --config client.toml          # incremental
./find-scan --config client.toml --full   # full rescan
```

### 3. Query

```bash
./find "some pattern"
./find "some pattern" --mode exact
./find "some pattern" --source my-machine --limit 20
```

### 4. Start the web UI (optional)

The web UI provides a browser-based interface with live fuzzy search, syntax highlighting, and file preview.

**Prerequisites:**
- Node.js 18+ (managed via mise)
- pnpm (managed via corepack)

**Setup with mise (recommended):**

```bash
# Install mise if not already installed: https://mise.jdx.dev
mise trust        # trust the .mise.toml config
mise install      # installs Node.js

# Create .env file from example
cd web
cp .env.example .env
```

**Setup without mise:**

```bash
# Enable corepack for pnpm
corepack enable

cd web
pnpm install

# Create .env file from example
cp .env.example .env
```

Edit `.env` to match your server configuration:

```bash
FIND_SERVER_URL=http://localhost:8765
FIND_TOKEN=change-me  # must match server.toml token
```

**Development mode:**

```bash
pnpm dev
```

The web UI will be available at `http://localhost:5173`

**Production build:**

```bash
pnpm build
pnpm preview
```

The production server runs on `http://localhost:4173` by default.

---

## Linux: automated scanning with systemd

**Nightly rescan** (`/etc/systemd/system/find-scan.timer` + `.service`):

```ini
# find-scan.service
[Service]
Type=oneshot
ExecStart=/usr/local/bin/find-scan --config /etc/find-anything/client.toml

# find-scan.timer
[Timer]
OnCalendar=*-*-* 02:00:00
Persistent=true
[Install]
WantedBy=timers.target
```

```bash
systemctl enable --now find-scan.timer
```

**Real-time watcher** (`/etc/systemd/system/find-watch.service`):

```ini
[Service]
Type=simple
ExecStart=/usr/local/bin/find-watch --config /etc/find-anything/client.toml
Restart=always
```

---

## Configuration reference

### Server (`server.toml`)

```toml
[server]
bind     = "127.0.0.1:8765"   # address to listen on
data_dir = "/var/lib/find-anything"
token    = "your-token"

[search]
default_limit       = 50
max_limit           = 500
fts_candidate_limit = 2000    # FTS5 rows passed to nucleo re-scorer
```

### Client (`client.toml`)

```toml
[server]
url   = "http://host:8765"
token = "your-token"

# Define multiple sources - each will be scanned and indexed separately
[[sources]]
name  = "unique-source-name"   # alphanumeric, hyphens, underscores
paths = ["/path/to/index"]     # multiple paths per source

[[sources]]
name  = "another-source"
paths = ["/another/path"]

[scan]
exclude          = ["**/.git/**", "**/node_modules/**"]
max_file_size_kb = 1024
follow_symlinks  = false
include_hidden   = false

[scan.archives]
enabled = true
```

---

## Roadmap

### Near-term

- **`find-watch`** — real-time incremental indexing via inotify (Linux) /
  FSEvents (macOS) / ReadDirectoryChangesW (Windows). Already scaffolded;
  needs debounce logic and event → API mapping.

- **PDF text extraction** — `pdf-extract` crate (pure Rust, no external deps).
  Stub exists in `crates/common/src/extract/pdf.rs`.

- **Image EXIF metadata** — `kamadak-exif` crate. Index tags like Make, Model,
  DateTimeOriginal, GPS coordinates, ImageDescription. Stub in `extract/image.rs`.

- **Audio tag extraction** — `id3` (MP3), `metaflac` (FLAC), `mp4ameta` (M4A/AAC).
  Stub in `extract/audio.rs`.

- **Systemd unit files** — ship ready-to-install `.service` and `.timer` files
  in `deploy/systemd/`.

### Medium-term

- **Native Windows client** — a Windows service and Task Scheduler integration
  wrapping `find-scan` and `find-watch`, replacing the systemd dependency.
  Likely a small Rust binary that registers itself as a Windows Service
  (`windows-service` crate) and schedules periodic rescans. Optionally a
  system-tray status icon.

- **Search ranking improvements** — recency bias (recently modified files rank
  higher), result deduplication across sources with identical content.

### Post-MVP

- **OCR** — opt-in (`ocr = true` in config), requires `tesseract` in PATH.
  Images run through tesseract; scanned PDFs fall back to page-render + OCR
  when `pdf-extract` returns no text.

- **Saved searches / search history** — stored in web UI (localStorage or
  server-side per-user).

- **Webhook notifications** — POST to a URL when new content matching a saved
  pattern is indexed.

- **Index export** — `find-server export --source <name> --format json`

- **Token rotation** — multiple named tokens with expiry; add a `tokens` table
  to replace the current single shared token.
