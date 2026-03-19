//! Storage benchmarking library for `ContentStore` implementations.
//!
//! Used by the `find-test` binary. No CLI concerns — pure measurement logic.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use anyhow::Result;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::Rng;

use crate::{ContentKey, ContentStore};

// ── Write benchmark ───────────────────────────────────────────────────────────

pub struct WriteBenchOpts {
    pub num_blobs: usize,
    /// Target size of each blob in bytes.
    pub blob_size_bytes: usize,
    /// RNG seed. Same seed → identical blobs and keys on every run.
    pub seed: u64,
}

pub struct WriteBenchResult {
    pub blobs_written: usize,
    pub bytes_written: u64,
    pub elapsed: Duration,
}

impl WriteBenchResult {
    pub fn mb_per_sec(&self) -> f64 {
        let secs = self.elapsed.as_secs_f64();
        if secs == 0.0 { return 0.0; }
        self.bytes_written as f64 / secs / 1_048_576.0
    }

    pub fn blobs_per_sec(&self) -> f64 {
        let secs = self.elapsed.as_secs_f64();
        if secs == 0.0 { return 0.0; }
        self.blobs_written as f64 / secs
    }
}

/// Write phase: generate and store synthetic blobs.
///
/// Returns the result and the list of inserted keys (for use in `bench_read`).
pub fn bench_write(
    store: &dyn ContentStore,
    opts: &WriteBenchOpts,
) -> Result<(WriteBenchResult, Vec<ContentKey>)> {
    let mut keys = Vec::with_capacity(opts.num_blobs);
    let mut bytes_written: u64 = 0;

    let t0 = Instant::now();
    for i in 0..opts.num_blobs {
        let blob = synthetic_blob(opts.seed, i, opts.blob_size_bytes);
        let key = blob_key(opts.seed, i);
        bytes_written += blob.len() as u64;
        store.put(&key, &blob)?;
        keys.push(key);
    }
    let elapsed = t0.elapsed();

    Ok((
        WriteBenchResult {
            blobs_written: opts.num_blobs,
            bytes_written,
            elapsed,
        },
        keys,
    ))
}

// ── Read benchmark ────────────────────────────────────────────────────────────

pub struct ReadBenchOpts {
    pub num_reads: usize,
    pub concurrency: usize,
    /// Keys to sample from — returned by `bench_write` or built from an existing store.
    pub keys: Vec<ContentKey>,
    pub seed: u64,
}

pub struct ReadBenchResult {
    pub reads: usize,
    pub concurrency: usize,
    /// Per-call durations, sorted ascending.  Index by percentile directly.
    pub latencies: Vec<Duration>,
}

impl ReadBenchResult {
    /// Duration at the given percentile (0.0–1.0).
    pub fn percentile(&self, p: f64) -> Duration {
        if self.latencies.is_empty() {
            return Duration::ZERO;
        }
        let idx = ((self.latencies.len() as f64 - 1.0) * p.clamp(0.0, 1.0)) as usize;
        self.latencies[idx]
    }

    /// Operations per second, computed as (total reads) / (wall time per thread).
    pub fn ops_per_sec(&self) -> f64 {
        if self.latencies.is_empty() || self.concurrency == 0 {
            return 0.0;
        }
        let total_secs: f64 = self.latencies.iter().map(|d| d.as_secs_f64()).sum();
        let wall_secs = total_secs / self.concurrency as f64;
        if wall_secs == 0.0 { return 0.0; }
        self.reads as f64 / wall_secs
    }
}

/// Read phase: call `get_lines()` against random keys across multiple threads.
pub fn bench_read(store: &dyn ContentStore, opts: &ReadBenchOpts) -> Result<ReadBenchResult> {
    if opts.keys.is_empty() || opts.num_reads == 0 {
        return Ok(ReadBenchResult {
            reads: 0,
            concurrency: opts.concurrency,
            latencies: vec![],
        });
    }

    let concurrency = opts.concurrency.max(1);
    let all_latencies: Mutex<Vec<Duration>> = Mutex::new(Vec::with_capacity(opts.num_reads));

    std::thread::scope(|scope| {
        let reads_per_thread = opts.num_reads.div_ceil(concurrency);

        for t in 0..concurrency {
            let latencies = &all_latencies;
            let keys = &opts.keys;
            let seed = opts.seed ^ (t as u64).wrapping_mul(0x9e3779b97f4a7c15);
            let reads_this_thread = if t == concurrency - 1 {
                // Last thread handles the remainder.
                opts.num_reads.saturating_sub(reads_per_thread * (concurrency - 1))
            } else {
                reads_per_thread
            };

            scope.spawn(move || {
                let mut rng = StdRng::seed_from_u64(seed);
                let mut local: Vec<Duration> = Vec::with_capacity(reads_this_thread);

                for _ in 0..reads_this_thread {
                    let key = &keys[rng.random_range(0..keys.len())];
                    // Request a mid-file range (avoids line 0 which is always
                    // the path and would be cached trivially).
                    let lo: usize = rng.random_range(1..20);
                    let hi: usize = lo + rng.random_range(1..15);

                    let t0 = Instant::now();
                    let _ = store.get_lines(key, lo, hi);
                    local.push(t0.elapsed());
                }

                latencies.lock().unwrap().extend(local);
            });
        }
    });

    let mut latencies = all_latencies.into_inner().unwrap();
    latencies.sort_unstable();

    Ok(ReadBenchResult {
        reads: opts.num_reads,
        concurrency,
        latencies,
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Generate a deterministic content key for blob index `i` under `seed`.
pub fn blob_key(seed: u64, i: usize) -> ContentKey {
    // 64 hex chars from two u64 values derived from seed and index.
    let a = seed.wrapping_add(i as u64).wrapping_mul(6364136223846793005);
    let b = (i as u64).wrapping_add(1).wrapping_mul(seed ^ 0xdeadbeefcafe1234);
    ContentKey::new(format!("{a:016x}{b:016x}{a:016x}{b:016x}").as_str())
}

/// Generate a synthetic text blob of approximately `target_bytes`.
///
/// Produces line-delimited ASCII text with lines of ~60–80 chars,
/// matching realistic file content so chunking behaviour is representative.
fn synthetic_blob(seed: u64, i: usize, target_bytes: usize) -> String {
    if target_bytes == 0 {
        return String::new();
    }

    let mut rng = StdRng::seed_from_u64(seed ^ (i as u64).wrapping_mul(0x517cc1b727220a95));
    let mut out = String::with_capacity(target_bytes + 80);

    while out.len() < target_bytes {
        // Line length 40–80 chars.
        let line_len: usize = rng.random_range(40..=80);
        for _ in 0..line_len {
            // Printable ASCII 32–126 (space to tilde), skewing toward word chars.
            let ch = if rng.random_bool(0.85) {
                rng.random_range(b'a'..=b'z') as char
            } else {
                rng.random_range(b' '..=b'~') as char
            };
            out.push(ch);
        }
        out.push('\n');
    }

    out
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SqliteContentStore;
    use tempfile::TempDir;

    fn make_store() -> (SqliteContentStore, TempDir) {
        let dir = TempDir::new().unwrap();
        (SqliteContentStore::open(dir.path(), None, None).unwrap(), dir)
    }

    #[test]
    fn write_bench_produces_nonzero_throughput() {
        let (store, _dir) = make_store();
        let opts = WriteBenchOpts { num_blobs: 50, blob_size_bytes: 512, seed: 42 };
        let (result, keys) = bench_write(&store, &opts).unwrap();
        assert_eq!(result.blobs_written, 50);
        assert!(result.bytes_written > 0);
        assert!(result.mb_per_sec() > 0.0);
        assert!(result.blobs_per_sec() > 0.0);
        assert_eq!(keys.len(), 50);
    }

    #[test]
    fn read_bench_produces_nonzero_latencies() {
        let (store, _dir) = make_store();
        let write_opts = WriteBenchOpts { num_blobs: 50, blob_size_bytes: 512, seed: 99 };
        let (_, keys) = bench_write(&store, &write_opts).unwrap();

        let read_opts = ReadBenchOpts { num_reads: 100, concurrency: 2, keys, seed: 99 };
        let result = bench_read(&store, &read_opts).unwrap();
        assert_eq!(result.reads, 100);
        assert_eq!(result.latencies.len(), 100);
        assert!(result.percentile(0.99) > Duration::ZERO);
    }

    #[test]
    fn blob_keys_are_unique() {
        let keys: Vec<ContentKey> = (0..100).map(|i| blob_key(1234, i)).collect();
        let unique: std::collections::HashSet<_> = keys.iter().map(|k| k.as_str()).collect();
        assert_eq!(unique.len(), 100);
    }

    #[test]
    fn synthetic_blob_reaches_target_size() {
        for target in [128, 512, 4096, 16384] {
            let blob = synthetic_blob(7, 0, target);
            assert!(
                blob.len() >= target,
                "blob len {} < target {target}",
                blob.len()
            );
        }
    }

    #[test]
    fn read_bench_empty_keys_is_ok() {
        let (store, _dir) = make_store();
        let opts = ReadBenchOpts { num_reads: 10, concurrency: 1, keys: vec![], seed: 0 };
        let result = bench_read(&store, &opts).unwrap();
        assert_eq!(result.reads, 0);
    }
}
