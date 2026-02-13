# TODO - Ideas and Enhancements

This file tracks ideas and potential features that are not yet fully formulated or prioritized for the roadmap. Items here may eventually become formal plans in `docs/plans/` or be added to the README roadmap.

---

## Web UI Enhancements

Ideas for improving the web interface:

- [ ] Keyboard shortcuts for navigation and search
- [ ] Search history and saved searches
- [ ] Dark mode / theme switcher
- [ ] Advanced search filters (file type, date range, source)
- [ ] Search result export (JSON, CSV)
- [ ] Debounced live search optimization
- [ ] File preview for more file types
- [ ] Pagination or infinite scroll for large result sets
- [ ] Multi-select results for batch operations
- [ ] Search query builder UI
- [ ] Recent searches dropdown
- [ ] Search suggestions / autocomplete

---

## Windows Client

Support for Windows environments:

- [ ] Windows service wrapper for `find-scan` and `find-watch`
- [ ] Task Scheduler integration for periodic scans
- [ ] System tray icon with status indicator
- [ ] Windows installer (MSI or similar)
- [ ] ReadDirectoryChangesW implementation for `find-watch`
- [ ] PowerShell setup scripts
- [ ] Windows-specific path handling improvements
- [ ] Integration with Windows Search (optional)

---

## Resource URIs and Hyperlinking

Add addressable URIs for indexed resources:

- [ ] Define URI scheme for resources (e.g., `find://source/path` or `find://source/path:line`)
- [ ] Server endpoint to resolve URIs to file locations
- [ ] Client-side URI handler registration
- [ ] Web UI support for clickable resource links
- [ ] Protocol handler for opening files in local editor from web UI
- [ ] Support for line number anchors in URIs
- [ ] Cross-platform URI handling (file:// on local machine, custom protocol for remote)
- [ ] URI-based sharing of search results
- [ ] Deep linking from external tools (e.g., IDE, chat apps)

### Example URI schemes to consider:
```
find://code/src/main.rs              # File in "code" source
find://code/src/main.rs:42           # File with line number
find://code/archive.zip!/entry.txt   # Archive entry
find://search?q=pattern              # Deep link to search
```

---

## Other Ideas

- [ ] Result deduplication across sources (same content hash)
- [ ] Incremental indexing progress indicator
- [ ] Index statistics and health dashboard
- [ ] API rate limiting and throttling
- [ ] Multi-user support with per-user tokens
- [ ] Webhook notifications for index updates
- [ ] Index backup and restore utilities
- [ ] Search result ranking improvements (recency, frequency)
- [ ] Support for indexing remote filesystems (S3, WebDAV, etc.)
- [ ] Plugin system for custom extractors
- [ ] GraphQL API alternative to REST
- [ ] Elasticsearch backend option (alternative to SQLite)

---

## Notes

- Items marked with checkboxes can be checked off as they're moved to formal plans or implemented
- Add new ideas to the appropriate section or create new sections as needed
- When an idea is mature enough, create a plan file in `docs/plans/NNN-feature-name.md`
