# Web UI

[← Manual home](README.md)

---

## Search box

The search box is the primary entry point. Type to search — results update as you type (with a short debounce to avoid excessive requests while typing).

**Keyboard shortcuts in the search box:**

| Key | Action |
|---|---|
| `Enter` | Submit search immediately |
| `Escape` | Clear the search box |
| `Ctrl+P` | Open the command palette (Go to file) |

**Search mode selector** — The dropdown to the left of the search box selects the search mode (Fuzzy, Document, Exact, Regex). See [Search modes](04-search.md#search-modes) for details.

**Advanced search toggle** — Opens the Advanced filters panel below the search box, with source and date range filters.

**NLP date highlighting** — When the search query contains a recognized date phrase, it is highlighted in green inline in the search box. A chip appears below the search bar showing the active date range. Click **✕** on the chip to remove the date filter without modifying your query text. See [Natural language date queries](04-search.md#natural-language-date-queries).

---

## Results list

Each result represents a file, grouped so that all matching lines from the same file appear together.

**Result card anatomy:**

- **File header** — shows the file kind badge (e.g. `pdf`, `rs`, `txt`), the file path, and the source name. Click to open the file in the file viewer.
- **Match lines** — the matched line content with the matching terms highlighted. Lines are syntax-highlighted for source code.
- **Context lines** — lines surrounding the match, loaded lazily as the result scrolls into view.
- **Hit navigation** — when a file has multiple matching lines, arrows let you step between them without leaving the results list.

**Result count** — Shows the total number of results and, when a date filter is active, the date range:

```
390 results between 2/1/2026 and 2/28/2026
200 results after 9/1/2025
42 results
```

**Load more** — Results are paginated. Scroll to the bottom to load the next page automatically.

---

## File viewer

Clicking a result card or a file in the tree sidebar opens the file viewer on the right side of the screen.

**Path bar** — Shows the full file path at the top of the viewer. Click the copy icon to copy the path to the clipboard.

**View modes:**

| Mode | When shown |
|---|---|
| **Source/text** | Default for text and source code files; syntax-highlighted |
| **Rendered** | Markdown files are rendered as HTML |
| **Image** | Image files are displayed with EXIF metadata alongside |
| **PDF** | PDFs open in the embedded viewer; a toggle switches between the extracted text view and the original PDF render |
| **Directory** | Directories and archive files show a file listing |

**Line selection** — Click a line number to select it; click again to deselect. Hold Shift to select a range. Selected lines are highlighted and their line numbers are reflected in the URL for sharing.

**Expanding context** — In the results list, click a result to open it in the viewer, which jumps to the matching line with surrounding context. The viewer loads the full file content independently of the results list context window.

**Split vs full-width (images)** — Image files default to a split view showing the image alongside its metadata. A toggle switches to full-width image view.

---

## File tree sidebar

The sidebar on the left of the search view shows the indexed directory structure.

- **Sources** — Each indexed source appears as a top-level node. Expand it to browse its directory tree.
- **Lazy loading** — Subdirectories are loaded on demand as you expand them.
- **Archive browsing** — Archive files (`zip`, `tar.gz`, etc.) appear as expandable nodes. Expand them to browse their members just like directories.
- **Selection** — Clicking a directory or file in the tree opens it in the file viewer. The currently open file is highlighted.
- **Source filter** — When a source is selected in the tree, its results are highlighted in the search results list.

---

## Command palette (Ctrl+P)

Press **Ctrl+P** from anywhere in the UI to open the command palette — a fast file browser that lets you jump directly to any indexed file by name.

- Type to filter the file list by path. Filtering is case-insensitive substring matching.
- Use **Arrow keys** to move through the list and **Enter** to open the selected file.
- Press **Escape** to close without selecting.
- Archive members appear as `archive.zip → member/path.txt` and can be opened directly.
- When multiple sources are active, a source badge is shown next to each result.

The palette loads all file paths from the server when first opened, and caches them for the duration of the session.

---

## Settings

Access Settings via the gear icon in the top-right corner of the UI (or navigate to `/settings`).

### Preferences

| Setting | Description |
|---|---|
| **Theme** | Dark, Light, or Inherit from browser (follows `prefers-color-scheme`) |
| **Lines of context** | How many lines of context to show around each match in the results list (0–5). Overrides the server default for this browser session. |

### Stats

Shows a breakdown of the index by source:

- File counts and total indexed size per source
- Breakdown by file kind (pdf, text, image, etc.) and by extension
- Last scan time and worker status (idle / processing)
- Refreshes automatically every 30 seconds (every 2 seconds while the worker is active)

### Errors

Lists files that failed content extraction, grouped by source. Each entry shows:

- The file path
- The error message
- A link to open the file in the viewer

See [Extraction errors](03-indexing.md#extraction-errors) for how to retry failed files.

### About

Shows the server version and a summary of the running configuration.

---

[← Search](04-search.md) | [Next: Supported file types →](06-file-types.md)
