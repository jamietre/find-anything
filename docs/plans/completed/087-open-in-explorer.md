# Plan 087 — Open in Explorer

## Overview

Two cooperating features that let users jump from a search result directly to the
file's location in their OS file manager (Windows Explorer, Finder, Nautilus, etc.):

1. **Source root mapping** — the web UI lets users configure, per source name, the
   local filesystem root path for that source.  This bridges the server-relative
   path (`docs/report.pdf`) to a local absolute path (`C:\Share\docs\report.pdf`).

2. **`findanything://` protocol handler** — a small native binary registered as a
   custom URL scheme handler.  The web UI constructs a `findanything://open?path=…`
   URL and the OS dispatches it to the binary, which runs `explorer /select,<path>`
   (or the platform equivalent).

The result is a new **"Open in Explorer"** toolbar button in the file detail view —
sitting alongside the existing Download button — that is only visible when a source
root has been configured for the current file's source.

---

## Part 1 — Source Root Mapping (UI / client-side)

### Where the roots are stored

Roots are purely a browser-side concern: the server doesn't need to know about them
and they may differ per user workstation.  They go in `UserProfile` (localStorage,
`web/src/lib/profile.ts`):

```typescript
interface UserProfile {
  // … existing fields …
  sourceRoots?: Record<string, string>;  // source name → absolute local path
}
```

### Settings UI

Add a new **"Source roots"** section to the settings page
(`web/src/routes/settings/+page.svelte`).  Each row shows the source name (populated
from `sourceNames` fetched from the server) alongside a text input for the root path.
Users paste their local path (e.g. `C:\Share` or `/mnt/nas`).  Changes are saved to
the profile store immediately (same pattern as the existing word-wrap / theme
settings).

No validation beyond basic trimming — paths are passed through verbatim to the
protocol handler.

### Path construction

When the button is clicked:

```
localPath = sourceRoot.trimEnd('/\\') + separator + result.path.replace('/', separator)
```

where `separator` is `\` on Windows (detected from the stored root containing a
backslash) and `/` otherwise.  The path is URL-encoded and placed into a
`findanything://open?path=<encoded>` URL.

For **archive members** (`archivePath` is set), use the outer archive file path
(i.e. `result.path`, not `result.path + '::' + archivePath`) — Explorer can select
the `.zip` but cannot open a virtual member path.

---

## Part 2 — `findanything://` Protocol Handler

### New crate: `crates/handler/`

A minimal binary crate `find-handler` that:

1. Receives its first argument as the full URL:
   `findanything://open?path=C%3A%5CShare%5Cdocs%5Creport.pdf`
2. URL-decodes the `path` query parameter.
3. Spawns the platform file-manager command and exits immediately (does not wait):
   - **Windows**: `explorer.exe /select,"<path>"` for files;
     `explorer.exe "<path>"` if the path looks like a directory.
   - **macOS**: `open -R "<path>"` (reveals in Finder).
   - **Linux**: `xdg-open "<parent dir>"` (opens the containing folder; revealing
     a specific file is not universally supported).

The binary is intentionally tiny (~50 lines) with no async runtime; it uses
`std::process::Command`. Only one dependency beyond `std`: a small URL parser
(`url` crate) to decode the query string.

### Windows protocol registration

Registered in the Inno Setup installer (`packaging/windows/find-anything.iss`):

```ini
[Registry]
Root: HKCR; Subkey: "findanything";                  ValueType: string; ValueName: ""; ValueData: "URL:Find Anything Protocol"
Root: HKCR; Subkey: "findanything";                  ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKCR; Subkey: "findanything\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\find-handler.exe"" ""%1"""
```

`find-handler.exe` is added to the `[Files]` section alongside the other binaries.

On **uninstall** Inno Setup removes the registry keys automatically.

### macOS / Linux (best-effort)

- **macOS**: register via a `CFBundleURLSchemes` entry in an `Info.plist` bundled
  with a minimal `.app` wrapper.  This can be a stretch goal; the binary logic is
  the same.
- **Linux**: register via a `.desktop` file placed in
  `~/.local/share/applications/` with `MimeType=x-scheme-handler/findanything;`.
  The installer script (`install.sh`) can drop this file and run
  `xdg-mime default find-handler.desktop x-scheme-handler/findanything`.

Both platforms are lower priority than Windows.  The button simply does not appear
if no source root is configured, so non-Windows users are unaffected.

---

## Web UI changes

### Button visibility

In `FileViewer.svelte`, the "Open in Explorer" button is shown only when:
- `$profile.sourceRoots?.[source]` is set and non-empty, **and**
- the file is not an inline archive member where only a virtual path exists
  (i.e. `canOpenInExplorer` is a derived `$:` boolean).

```svelte
{#if canOpenInExplorer}
  <button class="toolbar-btn" on:click={openInExplorer}>Open in Explorer</button>
{/if}
```

The label could be made OS-aware in future ("Reveal in Finder", etc.), but for now
"Open in Explorer" is acceptable as a generic label.

### `openInExplorer()` function

```typescript
function openInExplorer() {
  const root = ($profile.sourceRoots ?? {})[source] ?? '';
  if (!root) return;
  const sep = root.includes('\\') ? '\\' : '/';
  const rel = path.split('/').join(sep);
  const full = root.replace(/[\\/]+$/, '') + sep + rel;
  window.location.href = 'findanything://open?path=' + encodeURIComponent(full);
}
```

---

## Files Changed

| File | Change |
|------|--------|
| `web/src/lib/profile.ts` | Add `sourceRoots?: Record<string, string>` to `UserProfile` |
| `web/src/routes/settings/+page.svelte` | Add "Source roots" configuration section |
| `web/src/lib/FileViewer.svelte` | Add `canOpenInExplorer` derived, `openInExplorer()`, and the toolbar button |
| `crates/handler/Cargo.toml` | New crate |
| `crates/handler/src/main.rs` | New binary (~50 lines) |
| `Cargo.toml` (workspace) | Add `crates/handler` to workspace members |
| `packaging/windows/find-anything.iss` | Registry keys + install `find-handler.exe` |
| `install.sh` | Drop `.desktop` file for Linux (stretch goal) |

No server-side changes are required.

---

## Testing

- **Unit**: test path construction logic (separator detection, archive member
  stripping, URL encoding) as TypeScript unit tests in `web/src/lib/`.
- **Manual (Windows)**: install handler, configure a source root in settings, open
  a file result, click button → Explorer opens with the file selected.
- **No protocol handler installed**: clicking the button silently fails in the browser
  (unknown scheme); this is acceptable — the button only appears when a root is
  configured, which implies intent to use the feature.

---

## Breaking Changes

None.  The feature is purely additive: new optional profile field, new binary, new
registry keys only created by the installer.
