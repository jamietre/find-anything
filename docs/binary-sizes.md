# Binary Size Tracking

Unstripped release builds (`cargo build --release`) on x86_64 Linux.
Sizes are recorded periodically to catch regressions, especially on
space-constrained NAS/ARM targets.

> **Note:** These are unstripped sizes. The ARM cross-compiled binaries
> (`mise run build-arm`) will differ; add a separate table if/when that
> becomes relevant to track.

---

## 2026-03-13 — v0.6.2 (baseline)

Recorded before adding RAR support. `find-extract-archive` is the binary
to watch when adding new archive format support.

| Binary | Size (KB) |
|---|---:|
| find-server | 20,952 |
| find-scan | 12,983 |
| find-watch | 12,862 |
| find-extract-archive | 10,987 |
| find-upload | 10,456 |
| find-admin | 9,551 |
| find-anything | 8,882 |
| find-extract-dispatch | 8,723 |
| find-extract-pdf | 4,632 |
| find-extract-office | 4,204 |
| find-extract-html | 3,686 |
| find-extract-media | 3,413 |
| find-extract-epub | 3,234 |
| find-extract-text | 2,833 |
| find-extract-pe | 2,603 |
| find-tray | 425 |

**Total** (all binaries): ~114 MB

### Notes
- `find-extract-archive` already includes 7z (sevenz-rust2), xz, bz2, zlib, zip — hence large relative to other extractors.

---

## 2026-03-13 — v0.6.2 (after RAR support via `unrar` crate)

| Binary | Size (KB) | Δ vs baseline |
|---|---:|---:|
| find-server | 20,948 | -4 |
| find-scan | 13,002 | +19 |
| find-watch | 12,881 | +19 |
| **find-extract-archive** | **11,323** | **+336** |
| find-upload | 10,456 | 0 |
| find-admin | 9,551 | 0 |
| find-anything | 8,882 | 0 |
| find-extract-dispatch | 8,723 | 0 |
| find-extract-pdf | 4,632 | 0 |
| find-extract-office | 4,204 | 0 |
| find-extract-html | 3,686 | 0 |
| find-extract-media | 3,413 | 0 |
| find-extract-epub | 3,234 | 0 |
| find-extract-text | 2,833 | 0 |
| find-extract-pe | 2,603 | 0 |
| find-tray | 425 | 0 |

**Total**: ~115 MB (Δ +1 MB vs baseline)

### Notes
- `unrar` (C++ bindings to unrar 7.0.9) added +336 KB to `find-extract-archive` only — well within acceptable range for space-constrained systems; no separate `find-extract-rar` binary needed.
- Minor noise (+19 KB) on `find-scan`/`find-watch` is from the `subprocess.rs` change routing `.rar` to `find-extract-archive`.

---

<!-- Template for future entries:

## YYYY-MM-DD — vX.Y.Z (reason for recording)

| Binary | Size (KB) | Δ vs baseline |
|---|---:|---:|
| find-server | | |
| find-scan | | |
| find-watch | | |
| find-extract-archive | | |
| find-upload | | |
| find-admin | | |
| find-anything | | |
| find-extract-dispatch | | |
| find-extract-pdf | | |
| find-extract-office | | |
| find-extract-html | | |
| find-extract-media | | |
| find-extract-epub | | |
| find-extract-text | | |
| find-extract-pe | | |
| find-tray | | |

**Total**: ~X MB (Δ +/- Y MB vs baseline)

### Notes
-

-->
