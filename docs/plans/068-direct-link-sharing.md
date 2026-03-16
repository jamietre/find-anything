# 068 — Direct Link Sharing (`/v/:code`)

## Overview

Add a "share" button to the file detail view that generates a short, capability-based URL
(`/v/xxxxxx`) pointing to a distraction-free viewer for a file. The viewer shows images
(with zoom/pan), PDFs (browser native), or extracted text — without the full search UI,
metadata panels, or tree sidebar.

---

## Design Decisions

### Short code alphabet

Exclude characters that are visually ambiguous when reading aloud or printed in small type:

| Excluded | Reason |
|---|---|
| `0` | Confused with `O` |
| `1` | Confused with `l` / `I` |
| `l` (lowercase L) | Confused with `1` / `I` |
| `I` (capital i) | Confused with `1` / `l` |
| `O` (capital o) | Confused with `0` |

Safe alphabet (56 characters):
- Digits: `23456789` (8)
- Uppercase: `ABCDEFGHJKLMNPQRSTUVWXYZ` (24)
- Lowercase: `abcdefghijkmnpqrstuvwxyz` (24)

6 characters → 56⁶ ≈ 30.8 billion combinations. Vastly more than enough for a single-user
instance; no sequential enumeration risk with even minimal rate-limiting.

### Auth model: capability-based (code IS the credential)

The `/api/v1/links/:code` endpoint and the `/v/:code` page do **not** require the bearer
token. Possession of the code is sufficient to view that file. This is the standard model
for share links (Dropbox, Google Drive "anyone with link").

Implications:
- Codes must be kept confidential; treat them like short-lived passwords
- There is no revocation per code in v1 (add if needed)
- The server should rate-limit `/api/v1/links/:code` GETs to prevent enumeration
  (e.g. 60 req/min per IP)

### Storage

A new `links` table in a dedicated `data_dir/links.db` (not per-source). This keeps
link resolution fast and avoids coupling to source schema versions.

```sql
CREATE TABLE links (
    code       TEXT PRIMARY KEY,
    source     TEXT NOT NULL,
    path       TEXT NOT NULL,
    archive_path TEXT,
    created_at INTEGER NOT NULL
);
```

### URL routing

`/v/:code` is served by SvelteKit via client-side routing. The Rust server already falls
back to `index.html` for unmatched paths, so no server-side route change is needed.

The SvelteKit route `web/src/routes/v/[code]/+page.svelte` calls
`GET /api/v1/links/:code` to resolve the file, then renders the appropriate viewer.

### Image viewer: custom Svelte component

Rather than adding a dependency, implement a lightweight pan/zoom viewer directly in
Svelte using CSS transforms and pointer events. The requirement is minimal:

- Mouse wheel or pinch → zoom (clamped to, say, 0.1× – 10×)
- Click-drag → pan (when zoomed in)
- Double-click → reset to fit
- Toolbar buttons: Zoom in (+), Zoom out (−), Reset (⊙)
- Shows at native resolution when image fits the viewport; scales down to fit otherwise

This is ~100 lines of Svelte and avoids pulling in a JS library. If more sophisticated
behaviour is needed later (e.g. multi-touch on mobile, deep zoom tiles), replace with
`panzoom` (3 KB, MIT) or `PhotoSwipe` at that point.

### Direct view layout

```
┌─────────────────────────────────────────────────────────────┐
│ [find-anything]   filename.ext   [⬇ Download] [⧉ Open in app] │  ← minimal header
├─────────────────────────────────────────────────────────────┤
│                                                             │
│              image / pdf / text viewer                      │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

- **Header**: "find-anything" text (links to `/`), filename, Download button,
  "Open in app" link (links to main app with file view pre-opened)
- **No**: source badge, file path, metadata panel, tree sidebar, search box
- Header is minimal/fixed; viewer fills remaining height

### "Open in app" URL

The main app serialises its state to URL params (`?source=X&path=Y&...`). The direct view
can construct a similar URL:
```
/?source={source}&path={outer_path}&archivePath={inner_path}
```
This is best-effort; if the app is behind auth the user will need to log in first.

### PDF viewer

Use `<iframe src="/api/v1/raw?source=...&path=...">` — the same approach as the current
detail view's "View Original" mode. The browser's native PDF viewer handles rendering.
The raw endpoint is already auth-guarded; the direct view calls it via the cookie session
set on the same origin, or (if capability auth is extended to raw) via a signed token.

**Auth note for raw endpoint:** Currently `/api/v1/raw` requires bearer or session-cookie
auth. For the direct view to embed a PDF in an `<iframe>`, the browser needs to be able
to fetch it directly. Options:
1. Use a session cookie established when the short-link page loads (simplest; works when
   `find-anything` runs in a browser tab)
2. Embed the content in the JSON response for the link resolution endpoint (only for small
   text files; impractical for PDFs/images)
3. Accept a `?link_code=xxxxxx` param on `/api/v1/raw` that bypasses bearer auth

Option 1 works when the user is already logged in. Option 3 is needed for truly unauthenticated
sharing. Implement option 1 in v1; revisit option 3 when unauthenticated sharing is a priority.

---

## API

### `POST /api/v1/links`

Requires bearer auth (only logged-in users create links).

**Request body:**
```json
{ "source": "projects", "path": "photos/sunset.jpg", "archive_path": null }
```

**Response `201 Created`:**
```json
{ "code": "aB3mZx", "url": "/v/aB3mZx" }
```

### `GET /api/v1/links/:code`

No auth required (capability-based).

**Response `200 OK`:**
```json
{
  "source": "projects",
  "path": "photos/sunset.jpg",
  "archive_path": null,
  "kind": "image",
  "filename": "sunset.jpg"
}
```

**Response `404 Not Found`:** code doesn't exist.

---

## Implementation

### Phase 1 — Backend (Rust)

1. **`data_dir/links.db`** — new SQLite database opened at startup; create `links` table
   if it doesn't exist; expose `create_link` / `resolve_link` helpers in a new
   `crates/server/src/db/links.rs` module

2. **`POST /api/v1/links`** — generate random 6-char code (retry on collision; statistically
   never needed), insert row, return JSON

3. **`GET /api/v1/links/:code`** — look up row, fetch `kind` from source DB (`SELECT kind FROM
   files WHERE path = ?`), return JSON; rate-limit in the handler (simple per-IP counter with
   `tokio::time`)

4. Wire both routes into `lib.rs` / `routes.rs`

### Phase 2 — Web UI: link button in PathBar

1. Add a link/chain icon button to `PathBar.svelte` next to the existing copy button
2. On click: call `POST /api/v1/links` via `api.ts`, then copy the resulting URL to clipboard
   and show a brief "Copied!" confirmation (same pattern as the path copy button)
3. Add `createLink(source, path, archivePath?)` to `api.ts`

### Phase 3 — Web UI: direct view route

1. **`web/src/routes/v/[code]/+page.svelte`** — the direct view page:
   - On mount: call `GET /api/v1/links/:code`
   - Show loading / 404 states
   - Render appropriate viewer based on `kind`

2. **`web/src/lib/DirectImageViewer.svelte`** — custom pan/zoom viewer:
   - `<img>` inside a `<div class="viewport">` with `overflow: hidden`
   - State: `scale = 1`, `translateX = 0`, `translateY = 0`
   - Wheel event → `scale *= 1.1 ** (delta / 100)`, clamped to `[0.1, 10]`
   - Pointerdown/move/up → pan when `scale > 1`
   - On image load: if `naturalWidth < viewportWidth && naturalHeight < viewportHeight`
     → `scale = 1` (native res); otherwise → `scale = min(w/nw, h/nh)` (fit)
   - Reset button → restore initial fit scale + zero translation
   - Zoom in / zoom out buttons → `scale *= 1.25`, clamped

3. **`web/src/lib/DirectHeader.svelte`** — minimal fixed header:
   - "find-anything" link → `/`
   - Filename (from link resolution response)
   - Download link → `/api/v1/raw?source=...&path=...` with `download` attribute
   - "Open in app" link → `/?source=...&path=...`

4. **`web/src/routes/v/[code]/+page.svelte`** viewer dispatch:
   - `kind === 'image'` → `<DirectImageViewer src="/api/v1/raw?..."/>`
   - `kind === 'pdf'` → `<iframe src="/api/v1/raw?..." class="pdf-frame"/>`
   - everything else → fetch `/api/v1/file` and render extracted text in a `<pre>` with
     highlight.js (same path as the main `FileViewer`)

---

## Files Created

- `crates/server/src/db/links.rs` — `open_links_db`, `create_link`, `resolve_link`
- `crates/server/src/routes/links.rs` — `POST /api/v1/links`, `GET /api/v1/links/:code`
- `web/src/routes/v/[code]/+page.svelte` — direct view page
- `web/src/lib/DirectImageViewer.svelte` — pan/zoom image viewer
- `web/src/lib/DirectHeader.svelte` — minimal header with download + "open in app"

## Files Modified

- `crates/server/src/lib.rs` — open `links.db` at startup; wire new routes
- `crates/server/src/routes.rs` — add link route imports
- `web/src/lib/api.ts` — add `createLink` function
- `web/src/lib/PathBar.svelte` — add link icon button

---

## Testing

- Unit test `create_link` / `resolve_link` with collision and not-found cases
- Confirm `POST /api/v1/links` requires auth; `GET /api/v1/links/:code` does not
- Manual: create link for image, PDF, text file — verify each viewer renders correctly
- Manual: verify zoom/pan on image viewer (wheel, drag, reset, fit-to-viewport)
- Manual: confirm header links work (home, download, open in app)
- Manual: confirm 404 page shown for unknown/deleted code

---

## Future / Out of Scope for v1

- Code expiry / TTL (links are permanent in v1)
- Per-code revocation
- Unauthenticated raw serving for truly shareable links (requires option 3 auth above)
- Analytics (view count per link)
- Bulk link generation
- QR code display alongside the short URL
