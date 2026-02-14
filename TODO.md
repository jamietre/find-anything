# TODO - High Priority

## Archive Node Click Behavior (Next Session)

**Current behavior:**
- Clicking archive name in tree: expands/collapses + shows directory listing
- Ctrl+P to archive: highlights node + shows directory listing

**Desired behavior to implement:**
- Clicking archive node should:
  1. ✅ Select/highlight the node (like Ctrl+P)
  2. ✅ Show directory listing in right pane
  3. ❓ Expand behavior: Always expand? Or toggle?
     - Option A: Always expand one level when clicked (never collapse on click)
     - Option B: Keep current toggle behavior
     - **Decision needed:** Which feels more intuitive?

**Applies to:**
- Top-level archives (outer.zip)
- Nested archives (outer.zip::middle.zip, outer.zip::middle.zip::inner.zip)

**Files to modify:**
- `web/src/lib/TreeRow.svelte` - Click handler logic
- Potentially separate "expand arrow" click from "name" click behavior

**Testing:**
- Click outer.zip → should it expand or toggle?
- Click middle.zip when already expanded → collapse or stay expanded?
- Click inner.zip → behavior consistent?
