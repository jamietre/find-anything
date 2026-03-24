import { expandTreePath } from '$lib/api';
import type { DirEntry } from '$lib/api';

// Cache of directory listings, keyed by `${source}:${prefix}`.
//
// Populated two ways:
//   1. prefetchTreePath() — called fire-and-forget before navigation so all
//      intermediate levels are ready in a single request before TreeRow mounts.
//   2. TreeRow.expandDir() — uses the same expand endpoint on cache miss so
//      a single round-trip warms all ancestor levels simultaneously.
const dirCache = new Map<string, DirEntry[]>();

// In-flight expand promises, keyed by `${source}:${outerPath}`.
// Concurrent callers for the same path share one request instead of racing.
const inflight = new Map<string, Promise<void>>();

export function getCachedDir(source: string, prefix: string): DirEntry[] | undefined {
	return dirCache.get(`${source}:${prefix}`);
}

export function setCachedDir(source: string, prefix: string, entries: DirEntry[]): void {
	dirCache.set(`${source}:${prefix}`, entries);
}

/**
 * Fetch all directory levels needed to reveal `filePath` in one request.
 * Concurrent calls for the same (source, path) share the in-flight promise.
 *
 * Composite paths (`archive.zip::member`) are handled by stripping the `::…`
 * suffix — only outer filesystem directories are fetchable via expand.
 */
export function prefetchTreePath(source: string, filePath: string): Promise<void> {
	const outerPath = filePath.includes('::')
		? filePath.slice(0, filePath.indexOf('::'))
		: filePath;

	const key = `${source}:${outerPath}`;
	const existing = inflight.get(key);
	if (existing) return existing;

	const promise = doExpand(source, outerPath).finally(() => inflight.delete(key));
	inflight.set(key, promise);
	return promise;
}

async function doExpand(source: string, outerPath: string): Promise<void> {
	try {
		const resp = await expandTreePath(source, outerPath);
		for (const [prefix, entries] of Object.entries(resp.levels)) {
			if (!dirCache.has(`${source}:${prefix}`)) {
				dirCache.set(`${source}:${prefix}`, entries);
			}
		}
	} catch {
		// Ignore — TreeRow will fall back to its own listDir fetch.
	}
}
