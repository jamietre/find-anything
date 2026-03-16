# Video Player in Detail View

## Overview

Show video files in an embedded HTML5 video player in the detail view, similar to how PDFs use an iframe and images use the ImageViewer.

## Design Decisions

- Use native `<video controls>` — no third-party library needed; browsers handle mp4/webm natively.
- Video defaults to `showOriginal = true` (like images), since there's no extracted text content worth showing first.
- Toolbar shows a "View Extracted" button to toggle to the metadata/text view (matching the image UX).
- No format conversion on the server — serve the raw file directly via `/api/v1/raw`. Formats the browser can't play (avi, wmv, flv, mkv) will show the browser's native "format not supported" message.
- The `VideoViewer` component is intentionally minimal: a full-width `<video>` element with controls.

## Files Changed

- `web/src/lib/VideoViewer.svelte` — new component, wraps `<video controls>`
- `web/src/lib/FileViewer.svelte` — import VideoViewer, extend `canViewInline` and `showOriginal` logic, add video branch in template and toolbar

## Testing

1. Index a directory containing .mp4 and .webm files.
2. Open a video file in the detail view — should show the embedded player by default.
3. Click "View Extracted" — should switch to the metadata/text view.
4. Click "View Original" — returns to the player.
5. Test with an unsupported format (e.g. .avi) — player shows browser's native error.
6. Download button should still work for all video types.
