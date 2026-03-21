# Plan 089 — Mobile Device Support

## Overview

The UI is desktop-first with no responsive layout. On a phone the sidebar
overlaps the content, the search bar overflows, touch targets are tiny, and
the split viewer (content + MetaDrawer side-by-side) is unusable. This plan
adds a mobile-aware layout while keeping the desktop experience unchanged.

The guiding principle is **ruthless reduction**: mobile users get a single
focused column, contextual navigation (back button instead of a persistent
tree), and a scroll-first information hierarchy. Every feature that requires
horizontal space or hover interaction is either hidden or reorganised into
vertical flow.

---

## Design Decisions

### Breakpoint strategy

One breakpoint: **`max-width: 768px`** (phones and small tablets in portrait).
Tablets in landscape and anything wider keep the existing desktop layout.
All changes are CSS-only (media queries) unless a component needs structural
rearrangement, in which case a Svelte `{#if isMobile}` branch is used.

A reactive store `isMobile` is set by a `matchMedia` listener in `+page.svelte`
and passed down as needed. This avoids SSR issues (no `window` on server) and
keeps a single source of truth.

### Navigation model — single-column stack

Desktop has a persistent two-pane layout (search results | file viewer).
Mobile cannot. Instead:

- **Panel A** — search (always the root, no back needed)
- **Panel B** — file viewer (replaces Panel A; back button returns to results)

The existing `fileView` / `showTree` state already tracks which panel is
"active". On mobile the CSS simply hides the inactive panel
(`display: none`) and the back button in `PathBar` becomes the primary
navigation affordance. No routing changes are required.

### Tree view — hidden on mobile

The `MultiSourceTree` sidebar is suppressed on mobile (`display: none`).
The tree toggle button is also hidden. Source navigation happens through
the search bar's `source:` prefix chip. This is already implemented and
works fine on narrow screens.

### Download button — hidden on mobile

The "Download" and "Download Archive" buttons in `FileViewer` toolbars are
hidden on mobile via CSS. Download is impractical on most mobile browsers for
arbitrary file types, and the button wastes toolbar space.

### Search results — filename only

The full `source/path/to/file.txt` path is often too long to scan on a
narrow screen. On mobile the search result card shows only the filename
(last path component) as the primary label; the full path is shown in a
smaller secondary line below. This matches the mental model of searching
file content rather than navigating a tree.

Concretely: in `SearchResult.svelte` the path display gets a `.path-label`
wrapper with two child spans — `.filename` (bold) and `.full-path` (muted,
small). On desktop both are inline; on mobile `.full-path` wraps below.

### Image viewer — stacked layout

Desktop: image left, MetaDrawer slides in from the right.
Mobile: image on top, EXIF metadata scrolls below (no drawer, no toggle).

`DirectImageViewer` keeps its pan/zoom behaviour. `MetaDrawer` is replaced
on mobile by an inline `<div class="meta-below">` that renders the same slot
content. The drawer toggle button is hidden.

The image height on mobile is capped at `60vh` so metadata is always visible
without scrolling all the way down.

### Split views in general

Any side-by-side layout (image + drawer, PDF toolbar area, code viewer header)
collapses to a vertical stack on mobile. Toolbars wrap to two lines if needed
(`flex-wrap: wrap`).

### Touch targets

Minimum tap target: **44 × 44 px** (Apple HIG / WCAG 2.5.5). Affected
elements:
- Tree toggle button (currently 32px wide strip — hidden on mobile anyway)
- MetaDrawer toggle (currently 40px — hidden on mobile)
- Result card click area (already full-width — fine)
- Toolbar buttons (currently ~28px — need padding bump on mobile)
- Pagination "Load more" button (fine)

### Keyboard / hover interactions

Hover-only affordances (resize handle, tooltip `::after`) are suppressed on
mobile. The resize handle is hidden; sidebar width is not adjustable on touch.

### Scrolling

The outer `.page-layout` uses `overflow: hidden` and relies on inner
containers for scroll. On mobile this is changed to a single scrollable root
(`overflow-y: auto` on `.main-content`), which enables native momentum
scrolling and pull-to-refresh on iOS.

---

## Implementation

### Phase 1 — CSS foundation (minimal JS changes)

1. **`web/src/app.css`** — add mobile media query block at bottom:
   - `.global-sidebar { display: none }` (tree always hidden on mobile)
   - `.resize-handle { display: none }`
   - `.page-layout { flex-direction: column }`
   - `.main-content { overflow-y: auto; -webkit-overflow-scrolling: touch }`

2. **`web/src/routes/+page.svelte`** — add `isMobile` store; suppress tree
   toggle button on mobile.

3. **`web/src/lib/SearchResult.svelte`** — split path into `.filename` /
   `.full-path` spans; CSS shows only filename prominently on mobile.

4. **Toolbar buttons** — increase padding on mobile (`min-height: 44px;
   min-width: 44px`) in a global `.toolbar button` rule.

5. **Download button** — `.download-btn { display: none }` on mobile.

### Phase 2 — File viewer layout

6. **`web/src/lib/FileViewer.svelte`** — when `isMobile`, skip the
   MetaDrawer; render a `.meta-below` div below the image/content instead.
   The same slot content is passed to both targets; the inactive one is
   `display: none` via media query.

7. **`web/src/lib/DirectImageViewer.svelte`** — add `max-height: 60vh` on
   mobile; ensure the container doesn't take `height: 100%` (which would eat
   the whole viewport).

8. **Flex-wrap toolbars** — `FileViewer` toolbar and `PathBar` get
   `flex-wrap: wrap` on mobile so buttons don't overflow.

### Phase 3 — Single-column search → viewer navigation

9. **`web/src/routes/+page.svelte`** — on mobile, when `fileView !== null`,
   hide `.search-column` and show only `.file-column`, and vice-versa. This
   is just conditional CSS classes; the back button in `PathBar` already calls
   `closeFile()` which sets `fileView = null`.

10. **`web/src/lib/PathBar.svelte`** — ensure the back button is always shown
    on mobile (currently it can be hidden when history depth is 0). On mobile
    the back button doubles as "return to search results".

---

## Files Changed

| File | Change |
|------|--------|
| `web/src/app.css` | Mobile media query block |
| `web/src/routes/+page.svelte` | `isMobile` store, hide tree toggle, single-column stack |
| `web/src/lib/SearchResult.svelte` | Filename/path split display |
| `web/src/lib/FileViewer.svelte` | Pass `isMobile`; inline meta-below layout |
| `web/src/lib/DirectImageViewer.svelte` | `max-height: 60vh` on mobile |
| `web/src/lib/PathBar.svelte` | Always show back on mobile |
| `web/src/lib/MetaDrawer.svelte` | Hide drawer/toggle on mobile |

No new components are needed for Phase 1–2. Phase 3 may extract a
`MobileNav.svelte` if the single-column switching logic grows complex.

---

## Testing

- Chrome DevTools device emulation: iPhone SE (375px), iPhone 14 Pro (393px),
  iPad Mini portrait (768px), iPad Mini landscape (1024px — should use desktop layout)
- Real device testing via `pnpm run dev` with local network IP
- Keyboard navigation still works on desktop after changes (regression check)
- Search → open file → back → search flow on mobile
- Image viewer: pan/zoom still works with touch events; metadata scrolls below
- Long path names don't overflow result cards on 375px width

---

## Out of Scope

- Native app / PWA manifest / offline support
- Touch-optimised image gesture system (existing canvas pan/zoom uses pointer
  events which work on touch, but a dedicated pinch-zoom is not added here)
- Responsive advanced-search panel (collapsed to a single filter button on
  mobile — deferred to a follow-up)
- Admin, Stats, Preferences panels — still desktop-only for now
