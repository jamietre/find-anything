# 3/18/26 server crash

Found it. The data is very clear:

  MemoryPeak=4171714560 — find-server consumed ~3.98 GB on a 4 GB system. It's currently at 3.47 GB just sitting idle (mounts not even mounted).

  The culprit from the logs was indexing test-index/msys64-j.treworgy.zip — this is an msys64 Windows environment zip with thousands of source files inside it. The indexer
  was processing it file-by-file through biome/prettier formatters, spawning child processes the whole time. Indexing ended at 08:07, then the archive step at 08:10 said
  "48 requests: archived 6,966 files" — after that, silence. journald itself likely got starved and stopped writing, which is why the log just cuts off at 09:21.

  Two problems to address:

  1. test-index/ shouldn't be indexed — that zip is a test artifact not a real source. It's in nas-data presumably?
  2. No memory limit on the service — MemoryMax=infinity means it can eat everything. You should set a cap in the systemd unit.

  What's in test-index/ and should it be excluded from indexing, or should the whole directory be removed?

✻ Cogitated for 1m 22s

❯ We don't want to exclude anything to solve this problem. This service needs to be capable of handling arbitrarily large files; it should be streaming and not consume a
ton of resources.

● Makes sense — this is a bug in the server, not a configuration problem. Let me find the source code.

● Searched for 1 pattern (ctrl+o to expand)

❯ Source code isn't available here, i'll pass this on to an agent where it is. But can you theorize that spawning the child processes caused a problem? Could that result
in memory leaks?

● Yes, absolutely. Here's the likely scenario:

  The formatter child process spawning is the smoking gun. Looking at the logs, the server was processing thousands of files from that zip in rapid batches — each file
  potentially spawning a biome or prettier process. A few ways this causes runaway memory:

  1. Unbounded parallelism
  If the indexer spawns formatter processes without a concurrency cap, it could have hundreds of node/biome processes alive simultaneously. Each Node.js process alone
  starts at ~30-50 MB. 100 concurrent prettier processes = 3-5 GB gone immediately.

  2. Zombie/leaked child processes
  If the parent doesn't properly wait() on children — or if children are killed mid-run and their stdout/stdin pipes aren't closed — the OS holds the process in zombie
  state. Pipe buffers also stay allocated. Over thousands of files this accumulates fast.

  3. Pipe buffer backpressure
  Formatters communicate via stdin/stdout. If the server reads formatter output faster than it writes input (or vice versa), the pipe buffers fill up and block. If the
  server isn't handling this asynchronously, it could be holding large file contents in memory waiting for blocked pipes to drain.

  4. Zip extraction buffering
  Processing files inside a zip archive is the worst case — the server likely has to decompress each entry into memory before piping it to the formatter, rather than
  streaming from disk. With a zip containing thousands of large source files, this means many in-flight decompressed buffers at once.

  The fix direction for the agent: formatter invocations should be serialized or strictly concurrency-limited (e.g. a semaphore capped at N concurrent processes),
  stdin/stdout pipes should stream and not buffer entire file contents in memory, and zip entries should be processed one at a time with back-pressure.

❯ We only process things serially, so parallism should not be an issue

● OK, that shifts the focus. With serial processing the memory growth must be accumulation, not concurrency. Most likely culprits:

  1. Zip entries buffered entirely in memory
  Zip format stores its central directory at the end of the file. Most zip libraries require a seekable reader, which means they either seek on disk (fine) or — if the zip
  is itself an entry inside another zip — they must buffer the entire compressed entry in memory before they can read it. msys64-j.treworgy.zip inside test-index/ is almost
   certainly being decompressed into a memory buffer before any entries inside it can be read. That's the whole archive in RAM at once.

  2. Formatted output accumulating before commit
  If the server collects normalized content for all files in a batch before writing to SQLite, and a single zip entry decompresses to hundreds of MB, that content sits in
  memory for the duration of the batch.

  3. No back-pressure from formatter pipe
  Even serially — if the server reads the entire file into memory