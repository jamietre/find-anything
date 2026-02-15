# TODO - High Priority

## 1. Content Archive Subfolder Strategy

**Current problem:**
- All `content_NNNNN.zip` archives in flat `data_dir/sources/` folder
- Large number of files becomes unwieldy (filesystem limits, slow listings)

**Proposed strategies:**

**Option A: Prefix-based subfolders**
```
sources/
  content/
    00/
      content_00001.zip
      content_00099.zip
    01/
      content_00100.zip
      content_00199.zip
```
- Use first 2 digits of archive number as subfolder name
- Max 100 files per subfolder (00-99)

**Option B: Fixed subfolder count**
```
sources/
  content/
    0/  (archives 0000-0999)
    1/  (archives 1000-1999)
    2/  (archives 2000-2999)
```
- Use thousands digit as subfolder
- Simpler logic, still limits files per folder

**Option C: Date-based**
```
sources/
  content/
    2026-02/
      content_00001.zip
      content_00050.zip
```
- Group by month created
- Natural temporal organization
- Requires metadata tracking

**Decision needed:**
- Which strategy is simplest and most future-proof?
- Migration path for existing flat archives?

**Files to modify:**
- `crates/server/src/archive.rs` - Archive path construction
- Add migration function to reorganize existing archives
- Update path resolution in chunk reading

**Backward compatibility:**
- Support reading from both old (flat) and new (subfolder) locations
- Migrate on first write after update?
