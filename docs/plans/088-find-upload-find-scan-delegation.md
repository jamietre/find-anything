# 088 — find-upload: delegate extraction to find-scan

## Overview

Replace the server-side `extract_lines_via_subprocess` extraction path in
`upload.rs` with a delegation to `find-scan`. After all chunks arrive, the
server places the file in a unique temp directory, writes a minimal
`client.toml` that points at that directory as the source root, and invokes
`find-scan --config <temp.toml> <abs_path>`. find-scan then runs its full
extraction pipeline — correct timeouts, TempDir mode, archive members,
file_hash, normalizers — and submits the result through the normal bulk path.

This eliminates ~80 lines of redundant server-side extraction code and gives
find-upload full parity with find-scan without any special-casing.

## Config responsibilities

| Setting | Owner | Notes |
|---------|-------|-------|
| `subprocess_timeout_secs` | Server `[scan]` | Server controls how long extractors may run |
| `max_content_size_mb` | Server `[scan]` default, client override | Client passes its value in the upload request; server default used if absent |
| `exclude` / `exclude_extra` / `include` | Client, forwarded in upload request | Needed for archive member filtering |
| `max_line_length` | Server `[normalization]` only | **Not a client/find-scan concern.** Applied during server inbox processing after find-scan submits. Removed from client `ScanConfig`. |

## Changes

### 1. New server `[scan]` config block

Add `ServerScanConfig` to `find-common/src/config.rs` and wire it into
`ServerAppSettings`:

```toml
[scan]
subprocess_timeout_secs = 600  # default
max_content_size_mb = 100      # default; overridden per-upload by client
```

`extractor_dir` stays at the `[server]` level (already there).

### 2. Deprecate `max_line_length` from client `ScanConfig`

Remove `max_line_length` from `ScanConfig`. It is not passed to extractors
and not included in any temp config. `[normalization] max_line_length` on the
server remains the sole owner.

### 3. Upload init request — forward filters and content limit

Replace the proposed `scan_config: Option<ScanConfig>` with explicit fields:

```rust
pub struct UploadInitRequest {
    pub source: String,
    pub rel_path: String,
    pub mtime: i64,
    pub size: u64,
    /// Exclude patterns from the client's [scan] config (for archive member filtering).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
    /// Extra exclude patterns (additive, same semantics as ScanConfig.exclude_extra).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_extra: Vec<String>,
    /// Include patterns from the client's [scan] config (for archive member filtering).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include: Vec<String>,
    /// Client's max_content_size_mb. Overrides the server default when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_content_size_mb: Option<u64>,
}
```

All fields are optional/defaulted so old clients remain compatible.

### 4. UploadMeta — persist forwarded fields

Add the same four fields to `UploadMeta` so they survive across chunk
requests (written on init, read on completion).

### 5. find-upload client — populate forwarded fields

In `upload_main.rs`, read from the parsed client config:
- `config.scan.exclude` / `exclude_extra` / `include`
- `config.scan.max_content_size_mb`

Pass these into `upload_init` via the expanded `UploadInitRequest`.

### 6. Server index_upload — delegate to find-scan

Replace the current `extract_lines_via_subprocess` + `write_to_inbox` logic
with:

1. **Create unique temp dir**: `std::env::temp_dir().join(format!("find-upload-{upload_id}"))`.
   Using the upload UUID guarantees uniqueness even for simultaneous uploads of
   the same relative path.

2. **Place file at relative path**: `temp_dir/<rel_path>` (create parent dirs
   with `fs::create_dir_all`). Rename/move the `.part` file to this location.

3. **Write temp client.toml** as a sibling of the temp dir (not inside it):

   ```toml
   [server]
   url = "http://127.0.0.1:<port>"
   token = "<token>"

   [sources.<source_name>]
   path = "<temp_dir>"

   [scan]
   subprocess_timeout_secs = <server_scan.subprocess_timeout_secs>
   max_content_size_mb = <meta.max_content_size_mb ?? server_scan.max_content_size_mb>
   exclude = [...]       # from meta.exclude
   exclude_extra = [...] # from meta.exclude_extra
   # include handled via per-source config if non-empty
   ```

   Note: `max_line_length` is intentionally absent — it is not a find-scan
   concern and is handled by server normalization after bulk submission.

4. **Resolve find-scan binary**: same dir as the current executable, then PATH.

5. **Spawn find-scan and await**:
   ```
   find-scan --config <temp.toml> <temp_dir>/<rel_path>
   ```
   find-scan handles: timeout, TempDir mode, archive members, file_hash, batch
   submission, indexing error reporting.

6. **Cleanup**: remove temp dir and temp toml regardless of exit code.

### 7. Routes — replace extractor params with server scan config

In `routes/upload.rs`, replace the `extractor_dir`/`ext_cfg` fields passed
into `index_upload` with `server_url: String`, `token: String`, and a
reference to the new `ServerScanConfig`.

### 8. Remove server-side extraction code

Delete `extract_lines_via_subprocess` call and its imports from `upload.rs`.
Check whether `find_common::subprocess::extract_lines_via_subprocess` is used
elsewhere before removing it from `find-common`.

## Files changed

| File | Change |
|------|--------|
| `crates/common/src/config.rs` | Add `ServerScanConfig`; add to `ServerAppSettings`; remove `max_line_length` from `ScanConfig` |
| `crates/common/src/api.rs` | Replace `scan_config: Option<ScanConfig>` with explicit filter/limit fields on `UploadInitRequest` |
| `crates/server/src/upload.rs` | `UploadMeta`: add forwarded fields; `index_upload`: replace extraction with find-scan delegation |
| `crates/server/src/upload.rs` | Remove `extract_lines_via_subprocess` import and call |
| `crates/server/src/routes/upload.rs` | Pass `server_url`/`token`/`ServerScanConfig` into `index_upload`; populate `UploadMeta` from new request fields |
| `crates/client/src/api.rs` | Thread new `UploadInitRequest` fields through `upload_init` |
| `crates/client/src/upload_main.rs` | Populate new fields from parsed client config |
| `crates/common/src/subprocess.rs` | Remove if no longer used after server extraction is deleted |

## Temp directory layout

```
/tmp/
  find-upload-{uuid}/           ← temp root (= source path in temp config)
    household/Cars/.../big.pdf  ← file at its relative path
  find-upload-{uuid}.toml       ← temp client config (outside the dir)
```

## Error handling

- If find-scan exits non-zero, log a warning. The indexing error is recorded
  by find-scan's normal failure path.
- Cleanup always runs (success or failure).
- If the find-scan binary cannot be found, log an error and return early.

## Non-goals

- No change to the chunked upload protocol or `find-upload` UX.
- No change to how `max_line_length` works in `[normalization]` on the server.

## Testing

- Extend `crates/server/tests/upload.rs` to verify an uploaded file appears
  in search results after processing (requires find-scan on PATH, or mark
  `#[ignore]`).
- Manual test: `find-upload --source nas-data --rel-path "household/..." /path/to/big.pdf`
  and verify content is searchable.
