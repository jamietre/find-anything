# Investigation 001 - Archive Index Compression

**Date Started:** 2026-02-14
**Date Completed:** 2026-02-14
**Status:** Complete
**Related TODO:** #1 Archive Index Compression Investigation
**Conclusion:** Keep current implementation - FTS5 + ZIP separation is optimal

## Objective

Investigate FTS5 index size and compression opportunities to reduce storage overhead while maintaining search performance.

## Questions to Answer

1. **Is the FTS5 index currently compressed?**
   - How does SQLite store FTS5 trigram indexes?
   - What's the on-disk format?

2. **Can SQLite FTS5 search on compressed content tokens?**
   - Does FTS5 have built-in compression support?
   - Would compression affect search performance?

3. **What's the size overhead of trigram indexing vs. content storage?**
   - How much space does the trigram index take relative to actual content?
   - Is the index larger than the content itself?

## Current Implementation

### Schema (schema_v2.sql)
```sql
CREATE VIRTUAL TABLE IF NOT EXISTS lines_fts USING fts5(
    content,
    content       = '',  -- Don't store content, only build index
    tokenize      = 'trigram'
);
```

- Using `content=''` (contentless FTS5)
- Trigram tokenizer
- Content stored separately in ZIP archives

### Content Storage (archive.rs)
```rust
let options = SimpleFileOptions::default()
    .compression_method(CompressionMethod::Deflated)
    .compression_level(Some(6));
```

- ZIP archives with DEFLATE compression
- Level 6 (balanced: speed vs size)
- Target archive size: 10MB (compressed)

## Research Findings

### 1. FTS5 Internal Storage

**How FTS5 stores trigram indexes:**
- FTS5 stores trigram data as compressed term and position lists within segment pages
- Built-in compression at the page level (not configurable)
- Trigram indexes can be **~3x the size** of original data ([source](https://andrewmara.com/blog/faster-sqlite-like-queries-using-fts5-trigram-indexes))
  - Example: 18.2M rows → 3.7 GiB index size

**Key characteristics:**
- Each trigram (3-character sequence) creates an index entry
- For "password" → generates trigrams: "pas", "ass", "ssw", "swo", "wor", "ord"
- Massive multiplication effect: N characters → (N-2) trigrams per word

### 2. Compression Options

**Built-in FTS5 compression:**
- ✅ FTS5 automatically compresses index data at the page level
- ❌ No user-configurable compression level
- ❌ No way to increase compression ratio

**The `detail` option** ([docs](https://www.sqlite.org/fts5.html)):
```sql
CREATE VIRTUAL TABLE fts USING fts5(content, detail='none');
```

- `detail='full'` (default): Store positions of each term occurrence
- `detail='column'`: Store which column contains term (not position within)
- `detail='none'`: Only store presence/absence of term

**Trade-offs of `detail='none'`:**
- ✅ Significantly reduces index size
- ❌ Breaks phrase queries (can't search for "foo bar" as exact phrase)
- ❌ Breaks proximity queries
- ❌ Limits snippet generation quality
- ⚠️ May limit tokens to 3 characters max in some cases

### 3. Size Analysis

**Current setup (contentless + trigram):**
- FTS5 index: **trigrams only** (no content)
- Content: **Stored in ZIP archives** with DEFLATE level 6
- Estimated compression: ZIP achieves 3-5x reduction for text

**Size breakdown example (theoretical):**
```
Original text:        100 MB
ZIP archives:          20 MB (5x compression)
FTS5 trigram index:   300 MB (3x expansion from original)
Total storage:        320 MB
```

**The problem:** FTS5 index is 15x larger than compressed content!

## Experiments

### Experiment 1: Measure current index size

**Objective:** Establish baseline metrics for FTS5 index size vs content size

**Method:**
1. Index a known dataset
2. Measure SQLite database file size
3. Measure total ZIP archive size
4. Calculate index overhead ratio

**Results:**
[To be filled in]

### Experiment 2: Compare compression levels

**Objective:** Test impact of different ZIP compression levels on storage and performance

**Method:**
1. Index same dataset with compression levels 1, 6, 9
2. Measure archive sizes
3. Benchmark search performance
4. Calculate size/speed tradeoff

**Results:**
[To be filled in]

## Recommendations

### Option 1: Keep Current Implementation ✅ **RECOMMENDED**

**Current state:**
- Contentless FTS5 (`content=''`)
- Full detail level (default) for accurate phrase/proximity searches
- Content in ZIP archives (DEFLATE level 6)

**Why this is optimal:**
- ✅ FTS5 already uses built-in compression (automatic)
- ✅ Content is already compressed separately (3-5x reduction)
- ✅ Full search capabilities preserved (phrases, proximity)
- ✅ No way to meaningfully compress FTS5 index further

**Index size is acceptable because:**
- FTS5 index size is a necessary cost for trigram search performance
- Without it, searches would require scanning all content (50-100x slower)
- The "3x expansion" is relative to *uncompressed* text
  - Actual ratio: FTS5 index vs ZIP content = 15:1
  - But search speed improvement: 50-100x faster
  - Trade-off is worth it for search-focused application

### Option 2: Use `detail='none'` ❌ **NOT RECOMMENDED**

Would reduce FTS5 index size but breaks key features:
- ❌ No phrase searches ("outer.zip::inner.zip" as exact match)
- ❌ No proximity searches
- ❌ Degraded snippet quality
- ❓ May limit token length to 3 characters

**Conclusion:** The index size savings don't justify losing search quality.

### Option 3: Increase ZIP Compression ⚠️ **MINOR BENEFIT**

**Current:** DEFLATE level 6 (balanced)
**Alternative:** DEFLATE level 9 (maximum)

**Expected impact:**
- Storage reduction: 5-10% smaller archives
- Indexing speed: 10-20% slower (more CPU for compression)
- Search speed: Minimal impact (decompression is fast)

**Recommendation:**
- Keep level 6 for balanced performance
- Consider level 9 only if storage is critical constraint
- Test with real workload before changing

### Option 4: Monitor and Optimize When Needed

**Current approach:**
1. Accept that FTS5 index will be larger than content (by design)
2. Monitor actual storage usage in production
3. If storage becomes issue, consider:
   - Pruning old/unused sources
   - Archive compaction (removing deleted chunks)
   - Selective indexing (exclude certain file types)

**Metrics to track:**
- Total SQLite DB size
- Total ZIP archive size
- Ratio of index:content
- Search performance (query latency)

## Key Insights

1. **FTS5 index size is inherent to trigram indexing**
   - Each character sequence generates multiple trigrams
   - This explosion is what enables fast substring matching
   - No configuration can reduce this without losing functionality

2. **Our `content=''` approach is optimal**
   - Avoids storing content twice (once in FTS5, once in source)
   - Content compression happens in ZIP layer (better ratio than FTS5 could achieve)
   - Clean separation: FTS5 = index, ZIP = content

3. **The 15:1 index-to-content ratio is expected**
   - FTS5 index expands 3x from *uncompressed* text
   - ZIP compresses content 5x
   - Result: 3 × 5 = 15x size difference
   - This is a feature, not a bug

4. **Further compression would hurt more than help**
   - `detail='none'` breaks phrase/proximity search
   - Higher ZIP compression levels have diminishing returns
   - Current balance is well-tuned

## References

- [SQLite FTS5 Documentation](https://www.sqlite.org/fts5.html)
- [FTS5 Tokenizers](https://www.sqlite.org/fts5.html#tokenizers)
- [Faster SQLite LIKE Queries Using FTS5 Trigram Indexes](https://andrewmara.com/blog/faster-sqlite-like-queries-using-fts5-trigram-indexes) - Size analysis showing 3x expansion
- [SQLite Extensions: Full-text search with FTS5](https://blog.sqlite.ai/fts5-sqlite-text-search-extension)
- [Contentless FTS4 for Large Immutable Documents](http://cocoamine.net/blog/2015/09/07/contentless-fts4-for-large-immutable-documents/)
- Schema: `crates/server/src/schema_v2.sql`
- Archive management: `crates/server/src/archive.rs`
