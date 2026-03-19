// Moved from find-server crates/server/src/archive.rs — chunk_lines and related types.

const CHUNK_SIZE: usize = 1024; // 1 KB chunks

/// A chunk of file content to be stored (old API using block_id, kept for
/// the transition shim while archive_batch.rs is still using the old path).
#[derive(Debug, Clone)]
pub struct Chunk {
    pub block_id: i64,
    pub chunk_number: usize,
    pub content: String,
}

/// Line range covered by a single chunk.
#[derive(Debug, Clone)]
pub struct ChunkRange {
    pub chunk_number: usize,
    /// First line_number stored in this chunk.
    pub start_line: usize,
    /// Last line_number stored in this chunk (inclusive).
    pub end_line: usize,
}

/// Result of chunking: chunks + one range per chunk.
pub struct ChunkResult {
    pub chunks: Vec<Chunk>,
    pub ranges: Vec<ChunkRange>,
}

/// Chunk file content into fixed-size pieces, tracking the line range each
/// chunk covers.
///
/// Used by the old `archive_batch.rs` transition path.
pub fn chunk_lines(block_id: i64, lines: &[(usize, String)]) -> ChunkResult {
    let mut chunks = Vec::new();
    let mut ranges = Vec::new();
    let mut current_chunk = String::new();
    let mut chunk_number = 0;
    let mut chunk_start_line: Option<usize> = None;
    let mut chunk_last_line: usize = 0;

    for (line_num, content) in lines {
        let line_text = format!("{}\n", content);

        if current_chunk.len() + line_text.len() > CHUNK_SIZE && !current_chunk.is_empty() {
            chunks.push(Chunk {
                block_id,
                chunk_number,
                content: current_chunk.clone(),
            });
            ranges.push(ChunkRange {
                chunk_number,
                start_line: chunk_start_line.unwrap_or(0),
                end_line: chunk_last_line,
            });
            chunk_number += 1;
            current_chunk.clear();
            chunk_start_line = None;
        }

        if chunk_start_line.is_none() {
            chunk_start_line = Some(*line_num);
        }
        chunk_last_line = *line_num;
        current_chunk.push_str(&line_text);
    }

    if !current_chunk.is_empty() {
        chunks.push(Chunk {
            block_id,
            chunk_number,
            content: current_chunk,
        });
        ranges.push(ChunkRange {
            chunk_number,
            start_line: chunk_start_line.unwrap_or(0),
            end_line: chunk_last_line,
        });
    }

    ChunkResult { chunks, ranges }
}

// ── New blob-chunking API (used by ZipContentStore) ───────────────────────────

/// A chunk of blob content to be stored in a ZIP archive.
/// Uses key-prefix naming instead of block_id.
pub(crate) struct BlobChunk {
    pub chunk_num: usize,
    pub content: String,
    /// 0-based index of the first line in this chunk.
    pub start_pos: usize,
    /// 0-based index of the last line in this chunk (inclusive).
    pub end_pos: usize,
}

/// Split a blob (lines joined by `'\n'`) into ~1 KB chunks.
pub(crate) fn chunk_blob(blob: &str) -> Vec<BlobChunk> {
    let mut chunks: Vec<BlobChunk> = Vec::new();
    let mut current = String::new();
    let mut chunk_num = 0usize;
    let mut chunk_start: Option<usize> = None;
    let mut chunk_last: usize = 0;

    for (pos, line) in blob.split('\n').enumerate() {
        let line_text = format!("{}\n", line);

        if current.len() + line_text.len() > CHUNK_SIZE && !current.is_empty() {
            chunks.push(BlobChunk {
                chunk_num,
                content: std::mem::take(&mut current),
                start_pos: chunk_start.unwrap_or(0),
                end_pos: chunk_last,
            });
            chunk_num += 1;
            chunk_start = None;
        }

        if chunk_start.is_none() {
            chunk_start = Some(pos);
        }
        chunk_last = pos;
        current.push_str(&line_text);
    }

    if !current.is_empty() {
        chunks.push(BlobChunk {
            chunk_num,
            content: current,
            start_pos: chunk_start.unwrap_or(0),
            end_pos: chunk_last,
        });
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_lines() {
        let lines = vec![
            (1, "a".repeat(500)),
            (2, "b".repeat(500)),
            (3, "c".repeat(500)),
        ];
        let result = chunk_lines(42, &lines);
        assert_eq!(result.chunks.len(), 2);
        assert_eq!(result.chunks[0].chunk_number, 0);
        assert_eq!(result.chunks[1].chunk_number, 1);
        assert_eq!(result.chunks[0].block_id, 42);
        assert_eq!(result.chunks[1].block_id, 42);
        assert_eq!(result.ranges[0].start_line, 1);
        assert_eq!(result.ranges[0].end_line, 2);
        assert_eq!(result.ranges[1].start_line, 3);
        assert_eq!(result.ranges[1].end_line, 3);
    }

    #[test]
    fn test_chunk_single_large_line() {
        let lines = vec![(1, "x".repeat(2000))];
        let result = chunk_lines(42, &lines);
        assert_eq!(result.chunks.len(), 1);
        assert_eq!(result.ranges[0].start_line, 1);
        assert_eq!(result.ranges[0].end_line, 1);
    }

    #[test]
    fn chunk_blob_round_trip() {
        let blob = (0..200).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
        let chunks = chunk_blob(&blob);
        assert!(!chunks.is_empty());
        // Reconstruct from chunks and verify all positions accounted for.
        let mut positions: Vec<usize> = Vec::new();
        for chunk in &chunks {
            for (i, line) in chunk.content.split('\n').enumerate() {
                if line.is_empty() {
                    continue; // trailing newline artifact
                }
                positions.push(chunk.start_pos + i);
            }
        }
        let original_lines: Vec<&str> = blob.split('\n').collect();
        assert_eq!(positions.len(), original_lines.len());
        for (i, &pos) in positions.iter().enumerate() {
            assert_eq!(pos, i);
        }
    }
}
