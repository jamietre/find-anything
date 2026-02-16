# find-anything

Distributed full-content file indexing and fuzzy search. Index one or more machines
into a central server, then query everything from a single CLI or web UI.

```
find "password strength"
[code] src/auth/validate.rs:142  check_password_strength(input)?;
[code] docs/security.md:87       Password strength requirements: minimum 12 chars
```

---

## How it works

A central **server** stores the index. Client machines run **`find-scan`** to do
an initial index and **`find-watch`** to keep it current as files change. The
**`find`** CLI and web UI query the server over HTTP.

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
   │  find-scan  │           │  find-scan  │  initial index
   │  find-watch │           │  find-watch │  real-time updates
   └─────────────┘           └─────────────┘
```

The server can run anywhere — on a home server, NAS, VPS, or your local machine.
Client tools run on each machine whose files you want to index.

---

## Installation

### Option 1 — Install script (Linux & macOS)

Downloads pre-built binaries for your platform from GitHub Releases:

```sh
curl -fsSL https://raw.githubusercontent.com/jamietre/find-anything/main/install.sh | sh
```

Installs to `~/.local/bin` by default. Override with `INSTALL_DIR=/usr/local/bin`.

### Option 2 — Docker (server only)

Run the server with Docker Compose. Clients still install natively via the
install script above.

```sh
git clone https://github.com/jamietre/find-anything
cd find-anything
cp server.toml.example server.toml   # edit: set token and data_dir
docker compose up -d
```

### Option 3 — Build from source

```sh
git clone https://github.com/jamietre/find-anything
cd find-anything
cargo build --release
```

---

## Binaries

| Binary | Role | Runs on |
|--------|------|---------|
| `find-server` | Central index server | server machine |
| `find-scan` | Initial filesystem indexer | each client machine |
| `find-watch` | Real-time file watcher (incremental) | each client machine |
| `find` | CLI search client | anywhere |
| `find-extract-text` | Text/Markdown extractor | client (used by find-watch) |
| `find-extract-pdf` | PDF extractor | client (used by find-watch) |
| `find-extract-media` | Image/audio/video metadata extractor | client (used by find-watch) |
| `find-extract-archive` | ZIP/TAR/7Z extractor | client (used by find-watch) |
| `find-extract-html` | HTML extractor | client (used by find-watch) |
| `find-extract-office` | Office document extractor (DOCX/XLSX/PPTX) | client (used by find-watch) |
| `find-extract-epub` | EPUB ebook extractor | client (used by find-watch) |

The `find-extract-*` binaries are used by `find-watch` to extract file content
in subprocesses. They must be co-located with `find-watch` or on PATH.

---

## Quick start

### 1. Start the server

**With Docker Compose:**
```sh
cp server.toml.example server.toml
# Edit server.toml: set a strong token value
docker compose up -d
```

**Or run directly:**
```sh
cat > server.toml <<EOF
[server]
bind     = "127.0.0.1:8080"
data_dir = "/var/lib/find-anything"
token    = "change-me"
EOF

find-server --config server.toml
```

### 2. Create a client config

```toml
# client.toml
[server]
url   = "http://127.0.0.1:8080"
token = "change-me"

[[sources]]
name  = "home"
paths = ["/home/alice/documents", "/home/alice/projects"]

[scan]
exclude = ["**/.git/**", "**/node_modules/**", "**/target/**"]
max_file_size_kb = 1024
```

### 3. Run an initial scan

```sh
find-scan --config client.toml
```

### 4. Start the file watcher

```sh
find-watch --config client.toml
```

`find-watch` keeps the index current as files are created, modified, or deleted.
Run `find-scan` once first; `find-watch` does not do an initial scan on startup.

### 5. Search

```sh
find "some pattern"
find "some pattern" --mode exact
find "fn handler" --mode regex --source home --limit 20
```

### 6. Web UI (optional)

```sh
cd web
cp .env.example .env          # edit: set FIND_SERVER_URL and FIND_TOKEN
pnpm install
pnpm dev                      # http://localhost:5173
```

---

## Linux: running as a service

See [`docs/systemd/README.md`](docs/systemd/README.md) for ready-to-use systemd
unit files and full installation instructions for both user-mode (personal
workstation) and system-mode (multi-user server) setups.

Quick summary:

```sh
# Copy unit files
cp docs/systemd/user/find-server.service ~/.config/systemd/user/
cp docs/systemd/user/find-watch.service  ~/.config/systemd/user/
systemctl --user daemon-reload

# Run initial scan, then enable the watcher
find-scan --config ~/.config/find-anything/client.toml
systemctl --user enable --now find-server find-watch
```

---

## Supported file types

| Type | What's extracted |
|------|-----------------|
| Text, source code, Markdown | Full content; Markdown YAML frontmatter as structured fields |
| PDF | Full text content |
| HTML (.html, .htm, .xhtml) | Visible text from headings/paragraphs; title and description as metadata |
| Office (DOCX, XLSX, XLS, XLSM, PPTX) | Paragraphs, rows, slide text; document title/author as metadata |
| EPUB | Full chapter text; title, creator, publisher, language as metadata |
| Images (JPEG, PNG, TIFF, HEIC, RAW) | EXIF metadata (camera, GPS, dates) |
| Audio (MP3, FLAC, M4A, OGG) | ID3/Vorbis/MP4 tags (title, artist, album) |
| Video (MP4, MKV, WebM, AVI, MOV) | Format, resolution, duration |
| Archives (ZIP, TAR, 7Z, GZ) | Recursive extraction of all member files |

---

## Configuration reference

### Server (`server.toml`)

```toml
[server]
bind     = "127.0.0.1:8080"         # address to listen on
data_dir = "/var/lib/find-anything"  # index and archive storage
token    = "your-token"              # bearer token for all API requests

[search]
default_limit       = 50
max_limit           = 500
fts_candidate_limit = 2000           # FTS5 rows passed to the re-scorer
```

### Client (`client.toml`)

```toml
[server]
url   = "http://host:8080"
token = "your-token"

[[sources]]
name     = "home"
paths    = ["/home/alice/documents", "/home/alice/projects"]
base_url = "file:///home/alice"      # optional: makes results hyperlinkable

[scan]
exclude          = ["**/.git/**", "**/node_modules/**", "**/target/**"]
max_file_size_kb = 1024
follow_symlinks  = false
include_hidden   = false

[scan.archives]
enabled   = true
max_depth = 10    # max nesting depth (guards against zip bombs)

[watch]
debounce_ms   = 500               # ms of silence before processing events
extractor_dir = "/usr/local/bin"  # optional; auto-detected if omitted
```

---

## Roadmap

See [ROADMAP.md](ROADMAP.md) for the full development roadmap, including
completed features, planned work, and future ideas.
