# Roadmap

This document tracks the development roadmap for find-anything, from completed features to future ideas.

---

## Recently Completed

### ✅ Core Search & Indexing (v0.1)
- Full-text search with FTS5 trigram indexing
- Fuzzy, exact, and regex search modes
- Multi-source support (one client can manage multiple named sources)
- Archive content indexing (zip, tar, tar.gz, tar.bz2, tar.xz, 7z)
- Incremental scanning based on mtime
- File exclusion patterns (gitignore-style globs)
- Streaming text extraction (memory-efficient for large files)

### ✅ Rich Content Extraction (v0.2)
- **PDF text extraction** — Extract and index text from PDF files
- **Image EXIF metadata** — Index camera make/model, GPS, dates, descriptions
- **Audio metadata** — Index MP3 (ID3), FLAC (Vorbis), M4A tags

### ✅ Web UI (v0.3)
- SvelteKit-based web interface
- Live fuzzy search with syntax highlighting
- File preview and context display
- Source filtering
- Development tooling (mise, pnpm, corepack)

### ✅ Advanced Features (v0.4)
- **Resource base URLs** — Hyperlinkable search results (file://, http://, smb://)
- **Smart context retrieval** — File-type-aware context (metadata for images/audio, paragraph extracts for PDFs)

---

## Near-term (Next 3-6 months)

### Real-time File Watching
**Status:** Scaffolded, needs implementation

- `find-watch` daemon for real-time incremental indexing
- Platform-specific implementations:
  - Linux: inotify
  - macOS: FSEvents
  - Windows: ReadDirectoryChangesW
- Event debouncing (500ms) to handle editor save storms
- Efficient event → API mapping (CREATE/MODIFY/DELETE/RENAME)

**Plan:** Create `004-realtime-watch.md`

### Systemd Unit Files
**Status:** Not started

- Ready-to-install `.service` and `.timer` files
- Package in `deploy/systemd/` directory
- Installation script with user/group setup
- Logging configuration

### Web UI Enhancements - Phase 1
**Status:** In progress

High-priority UI improvements:
- Clickable resource URLs (uses base_url feature)
- Keyboard shortcuts for navigation
- Dark mode / theme switcher
- Search history (localStorage)
- File type icons and better result formatting

---

## Medium-term (6-12 months)

### Windows Native Client
**Status:** Design phase

Full Windows support with native tooling:
- Windows Service wrapper for `find-scan` and `find-watch`
- Task Scheduler integration for periodic scans
- System tray icon with status indicator
- MSI installer
- PowerShell setup scripts
- Windows-specific path handling

**Goal:** First-class Windows experience matching Linux/macOS

### Search Ranking Improvements
**Status:** Research phase

Enhance result quality:
- Recency bias (recently modified files rank higher)
- Result deduplication across sources (same content hash)
- Frequency scoring (commonly accessed files)
- Custom ranking per file type (e.g., prioritize code over archives)

### Web UI Enhancements - Phase 2
**Status:** Backlog

Advanced features:
- Advanced search filters (file type, date range, size)
- Search result export (JSON, CSV)
- Saved searches
- Search query builder UI
- Pagination or infinite scroll
- Multi-select for batch operations

---

## Long-term (12+ months)

### OCR Support
**Status:** Post-MVP

Optional OCR for images and scanned PDFs:
- Opt-in via `ocr = true` in config
- Requires `tesseract` in PATH
- Images run through tesseract
- Scanned PDFs (no embedded text) fall back to page-render + OCR
- Background processing with concurrency limits
- Content hash caching to avoid re-OCR

**Considerations:**
- Expensive operation (seconds per page)
- Large disk space for cache
- Quality vs performance trade-offs

### Multi-user & Authentication
**Status:** Design needed

Move beyond single shared token:
- Per-user accounts and authentication
- Token rotation and expiry
- Role-based access control (read-only, admin)
- User-specific saved searches and preferences
- Audit logging

### Advanced Integrations

**Webhook notifications:**
- POST to URL when new content matches saved pattern
- Real-time alerts for monitoring

**Index export:**
- `find-server export --source <name> --format json`
- Backup and migration support

**Plugin system:**
- Custom extractors for proprietary formats
- User-defined processing pipelines
- Language-specific analyzers

---

## Ideas & Future Enhancements

These are less structured ideas that may evolve into formal features.

### Web UI Ideas

- [ ] Search suggestions / autocomplete
- [ ] Recent searches dropdown
- [ ] Command palette (Cmd+K style)
- [ ] Regex helper / tester UI
- [ ] Result grouping by file type or source
- [ ] Timeline view for date-based results
- [ ] Graph visualization of file relationships

### Advanced URI Handling

- [x] Base URL configuration (implemented)
- [ ] Custom URI scheme (`find://source/path:line`)
- [ ] Protocol handler for opening files in local editor
- [ ] Deep linking from external tools (IDE, chat apps)
- [ ] Line number anchors (`#L42`)
- [ ] Archive entry URLs (`archive.zip#!/entry.txt`)
- [ ] Template-based URL construction

### Performance & Scalability

- [ ] Distributed indexing (multiple scan clients per source)
- [ ] Database partitioning for large sources (>100GB)
- [ ] Elasticsearch backend option (alternative to SQLite)
- [ ] Read replicas for search load balancing
- [ ] Index compression strategies
- [ ] Incremental FTS5 rebuilds

### Additional Content Types

- [ ] Video metadata (MP4, MKV duration, resolution, codecs)
- [ ] Office documents (DOCX, XLSX, PPTX via external tools)
- [ ] Markdown frontmatter extraction
- [ ] Code symbol indexing (functions, classes, imports)
- [ ] Email (mbox, PST) indexing
- [ ] Database dumps (SQL, JSON schemas)

### Search Features

- [ ] Fuzzy file path matching (like fuzzy finder)
- [ ] Boolean operators (AND, OR, NOT)
- [ ] Field-specific search (`path:src author:john`)
- [ ] Proximity search (words within N words of each other)
- [ ] Phonetic search
- [ ] Language-specific analyzers (stemming, synonyms)

### Operations & Monitoring

- [ ] Index statistics dashboard (size, file count, growth rate)
- [ ] Health checks and alerting
- [ ] Performance metrics (query latency percentiles)
- [ ] Slow query logging
- [ ] Database vacuuming automation
- [ ] Backup and restore utilities
- [ ] Source priority and quotas

### Developer Tools

- [ ] GraphQL API (alternative to REST)
- [ ] Python client library
- [ ] JavaScript client library
- [ ] VS Code extension
- [ ] CLI autocomplete (bash, zsh, fish)
- [ ] Docker compose for easy deployment

### Integration Ideas

- [ ] Slack/Discord bot for search
- [ ] Browser extension for web archiving
- [ ] Git commit message indexing
- [ ] Issue tracker integration (GitHub, Jira)
- [ ] Calendar event indexing
- [ ] Chat history indexing (Slack export, Discord)

---

## Contributing Ideas

Have an idea not listed here? Consider:
1. **Quick wins** → Open an issue or PR
2. **Substantial features** → Discuss in an issue first
3. **Major changes** → Create a plan in `docs/plans/NNN-feature-name.md`

See `CLAUDE.md` for project conventions and planning guidelines.
