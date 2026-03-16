# 068 — Direct Link Sharing (`/v/:code`)

## Overview

Add a "share" button to the file detail view that generates a short, capability-based URL
(`/v/xxxxxx`) pointing to a distraction-free viewer for a file. The viewer shows images
(with zoom/pan), PDFs (browser native), or extracted text — without the full search UI,
metadata panels, or tree sidebar. Links work without authentication and expire after a
configurable period.

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

6 characters → 56⁶ ≈ 30.8 billion combinations. No sequential enumeration risk with
even minimal rate-limiting.

### Auth model: capability-based — code is the credential

The 6-char code grants unauthenticated read access to that specific file only. No bearer
token is required to resolve a link or fetch the linked file's content. This enables true
sharing: anyone with the URL can view the file.

The code replaces bearer auth for three read-only operations scoped to the linked file:
- `GET /api/v1/links/:code` — resolve metadata
- `GET /api/v1/raw?link_code=:code` — fetch raw bytes (image, PDF)
- `GET /api/v1/file?link_code=:code` — fetch extracted text lines

The server validates the code, checks it hasn't expired, and confirms the `source`/`path`
in the request matches the row. Creating a link still requires bearer auth.

**Rate limiting:** `GET /api/v1/links/:code` is limited to ~60 req/min per IP to prevent
brute-force enumeration of valid codes.

### Link expiry

Links have a TTL configured in `server.toml` under a new `[links]` section:

```toml
[links]
# How long generated share links remain valid. Default: 30 days.
# Accepts integer + unit: "30d", "7d", "24h", "1h".
ttl = "30d"
```

The `expires_at` timestamp is stored in `links.db`. Expired links return `410 Gone`
(not 404) so the UI can show a clear "This link has expired" message rather than a
generic not-found page.

A background task sweeps expired rows out of `links.db` once per hour.

### Storage

A dedicated `data_dir/links.db` — not per-source. This keeps link resolution fast and
independent of source schema versions.

```sql
CREATE TABLE links (
    code        TEXT PRIMARY KEY,
    source      TEXT NOT NULL,
    path        TEXT NOT NULL,
    archive_path TEXT,
    created_at  INTEGER NOT NULL,  -- Unix seconds
    expires_at  INTEGER NOT NULL   -- Unix seconds; NULL not allowed
);
CREATE INDEX links_expires ON links(expires_at);
```

### URL routing

`/v/:code` is served by SvelteKit via client-side routing. The Rust server already falls
back to `index.html` for unmatched paths, so no server-side route change is needed.

The SvelteKit route `web/src/routes/v/[code]/+page.svelte` calls
`GET /api/v1/links/:code` to resolve the file, then renders the appropriate viewer.

### Raw content access via link code

`/api/v1/raw` and `/api/v1/file` currently require bearer or session-cookie auth. To serve
content to unauthenticated viewers, both endpoints accept an optional `?link_code=xxxxxx`
query param as an alternative credential:

1. Look up the code in `links.db`; reject if missing or expired
2. Confirm the `source` + `path` (+ `archive_path`) in the request match the stored row
3. Serve the content as normal

This means the direct view page can use plain `<img src="/api/v1/raw?link_code=...">` and
`<iframe src="/api/v1/raw?link_code=...">` without any session setup, and these URLs are
safe to embed in HTML, share via email, etc.

### Image viewer: custom Svelte component

Rather than adding a dependency, implement a lightweight pan/zoom viewer in Svelte using
CSS transforms and pointer events:

- Mouse wheel or pinch → zoom (clamped 0.1× – 10×)
- Click-drag → pan when zoomed in
- Double-click → reset to fit
- Toolbar buttons: Zoom in (+), Zoom out (−), Reset (⊙)
- On load: if `naturalWidth < viewportWidth && naturalHeight < viewportHeight` → show at
  native resolution (scale 1); otherwise → fit-to-viewport (`min(vw/nw, vh/nh)`)

~100 lines of Svelte, zero new runtime dependencies. Replace with `panzoom` (3 KB, MIT)
or PhotoSwipe later if multi-touch or tile-based zoom is needed.

### Direct view layout

```
┌──────────────────────────────────────────────────────────────┐
│ find-anything   filename.ext            [⬇ Download] [⧉ Open] │  ← fixed header
├──────────────────────────────────────────────────────────────┤
│                                                              │
│               image / pdf / text viewer                      │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

- **Header**: "find-anything" (links to `/`), filename, Download button
  (`/api/v1/raw?link_code=:code` with `download` attribute), "Open in app" link
- **No**: source badge, file path, metadata panel, tree sidebar, search box
- Header is minimal/fixed; viewer fills remaining height
- Expired link → header replaced by a centred "This link has expired" message

### "Open in app" URL

```
/?source={source}&path={outer_path}&archivePath={inner_path}
```
Best-effort; the user will need to log in if the app requires auth.

---

## Server config

New section in `server.toml` (and `examples/server.toml`):

```toml
[links]
ttl = "30d"   # default; supports h/d suffixes
```

`ttl` is parsed into seconds server-side. The `[links]` section is optional; defaults
apply if absent.

---

## API

### `POST /api/v1/links` *(requires bearer auth)*

**Request body:**
```json
{ "source": "projects", "path": "photos/sunset.jpg", "archive_path": null }
```

**Response `201 Created`:**
```json
{
  "code": "aB3mZx",
  "url": "/v/aB3mZx",
  "expires_at": 1752345600
}
```

### `GET /api/v1/links/:code` *(no auth required)*

**Response `200 OK`:**
```json
{
  "source": "projects",
  "path": "photos/sunset.jpg",
  "archive_path": null,
  "kind": "image",
  "filename": "sunset.jpg",
  "expires_at": 1752345600
}
```

**Response `410 Gone`:** code exists but has expired — UI shows "This link has expired".

**Response `404 Not Found`:** code was never issued.

### `GET /api/v1/raw?link_code=:code&source=...&path=...` *(no bearer auth)*

Same as current `/api/v1/raw` but authenticates via the link code instead of bearer/cookie.
The `source` and `path` params must match the stored row; returns `403` if they don't.

### `GET /api/v1/file?link_code=:code&source=...&path=...` *(no bearer auth)*

Same scoping as above, for text file content.

---

## Implementation

### Phase 1 — Backend (Rust)

1. **`[links]` config** — add `LinksConfig { ttl_secs: u64 }` to server config;
   parse `ttl` string (`"30d"`, `"7d"`, `"24h"`) into seconds; default 30 days

2. **`data_dir/links.db`** — open at startup; create `links` table with `expires_at`;
   expose `create_link(code, source, path, archive_path, expires_at)`,
   `resolve_link(code) -> Option<LinkRow>` (returns None if expired OR missing;
   returns `Expired` variant if code exists but past `expires_at`),
   `sweep_expired()` in `crates/server/src/db/links.rs`

3. **`POST /api/v1/links`** — requires bearer; compute `expires_at = now + ttl_secs`;
   generate random 6-char code, retry on collision; insert; return JSON

4. **`GET /api/v1/links/:code`** — no auth; look up; return 200/410/404; rate-limit
   60 req/min per IP (simple `DashMap<IpAddr, (u32, Instant)>` in `AppState`)

5. **`/api/v1/raw` and `/api/v1/file`** — accept optional `link_code` query param;
   if present: look up code, verify not expired, verify `source`+`path` match row,
   skip bearer check; otherwise auth as normal

6. **Expiry sweep** — background `tokio::spawn` loop, runs `sweep_expired()` every hour

7. Wire all routes into `lib.rs` / `routes.rs`

### Phase 2 — Web UI: link button in PathBar

1. Add a chain/link icon button to `PathBar.svelte` next to the copy button
2. On click: `POST /api/v1/links`, copy the full absolute URL
   (`window.location.origin + response.url`) to clipboard, show "Copied!" for 2 s
3. Add `createLink(source, path, archivePath?)` returning `{ code, url, expires_at }`
   to `api.ts`

### Phase 3 — Web UI: direct view route

1. **`web/src/routes/v/[code]/+page.svelte`**:
   - On mount: `GET /api/v1/links/:code`
   - States: loading → resolved (dispatch by kind) / expired (410) / not-found (404)
   - Pass `link_code` to child viewers so they can construct unauthenticated URLs

2. **`web/src/lib/DirectImageViewer.svelte`** — custom pan/zoom (see Design above);
   src = `/api/v1/raw?link_code=:code&source=...&path=...`

3. **`web/src/lib/DirectHeader.svelte`**:
   - "find-anything" → `/`
   - Filename
   - Download: `<a href="/api/v1/raw?link_code=...&download=1">⬇ Download</a>`
   - "Open in app": `/?source=...&path=...`

4. **Viewer dispatch in `+page.svelte`**:
   - `image` → `<DirectImageViewer>`
   - `pdf` → `<iframe src="/api/v1/raw?link_code=...">` (browser native PDF)
   - anything else → call `GET /api/v1/file?link_code=...` and render text in `<pre>`
     with highlight.js

5. **Expired / not-found states**: show the header with "find-anything" link, and a
   centred message: "This link has expired" (410) or "Link not found" (404)

---

## Files Created

- `crates/server/src/db/links.rs` — `open_links_db`, `create_link`, `resolve_link`, `sweep_expired`
- `crates/server/src/routes/links.rs` — `POST /api/v1/links`, `GET /api/v1/links/:code`
- `web/src/routes/v/[code]/+page.svelte` — direct view page
- `web/src/lib/DirectImageViewer.svelte` — pan/zoom image viewer
- `web/src/lib/DirectHeader.svelte` — minimal header

## Files Modified

- `crates/common/src/config.rs` — add `LinksConfig { ttl_secs: u64 }`; parse TTL string
- `crates/server/src/lib.rs` — open `links.db`; spawn expiry sweep; wire routes
- `crates/server/src/routes/links.rs` — new (see above)
- `crates/server/src/routes/file.rs` — accept `link_code` param
- `crates/server/src/routes/raw.rs` — accept `link_code` param
- `web/src/lib/api.ts` — add `createLink`
- `web/src/lib/PathBar.svelte` — add link icon button
- `examples/server.toml` — add commented `[links]` section

---

## Testing

- Unit: `create_link` / `resolve_link` including expired-code path
- Unit: `sweep_expired` deletes only expired rows
- Unit: TTL string parsing (`"30d"` → 2592000, `"24h"` → 86400, unknown unit → error)
- Integration: `POST /api/v1/links` requires auth; `GET /api/v1/links/:code` does not
- Integration: `GET /api/v1/raw?link_code=...` works without bearer; wrong source/path → 403
- Integration: expired code returns 410 from resolve and from raw/file endpoints
- Manual: create link for image / PDF / text — verify all three viewers render correctly
- Manual: zoom/pan on image viewer (wheel, drag, double-click reset, fit-on-load)
- Manual: copy-to-clipboard on the link button shows "Copied!" confirmation
- Manual: expired link shows "This link has expired" page

---

## Future / Out of Scope for v1

- Per-code manual revocation
- Analytics (view count per link)
- Configurable per-link TTL override at creation time
- QR code display alongside the short URL
- Bulk link generation
