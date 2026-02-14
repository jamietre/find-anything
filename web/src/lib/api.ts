// ── Types ────────────────────────────────────────────────────────────────────

export interface SourceInfo {
	name: string;
	base_url: string | null;
}

export interface ContextLine {
	line_number: number;
	content: string;
}

export interface SearchResult {
	source: string;
	path: string;
	archive_path: string | null;
	line_number: number;
	snippet: string;
	score: number;
	context_lines: ContextLine[];
}

export interface SearchResponse {
	results: SearchResult[];
	total: number;
}

export interface FileResponse {
	lines: ContextLine[];
	file_kind: string;
	total_lines: number;
}

export interface ContextResponse {
	lines: ContextLine[];
	file_kind: string;
}

export interface DirEntry {
	name: string;
	path: string;
	entry_type: 'dir' | 'file';
	kind?: string;
	size?: number;
	mtime?: number;
}

export interface TreeResponse {
	entries: DirEntry[];
}

// ── API calls (hit the SvelteKit proxy, which adds the bearer token) ─────────

export async function listSources(): Promise<SourceInfo[]> {
	const resp = await fetch('/api/v1/sources');
	if (!resp.ok) throw new Error(`listSources: ${resp.status} ${resp.statusText}`);
	return resp.json();
}

export interface SearchParams {
	q: string;
	mode?: string;
	sources?: string[];
	limit?: number;
	offset?: number;
}

export async function search(params: SearchParams): Promise<SearchResponse> {
	const url = new URL('/api/v1/search', location.origin);
	url.searchParams.set('q', params.q);
	if (params.mode) url.searchParams.set('mode', params.mode);
	if (params.sources) params.sources.forEach((s) => url.searchParams.append('source', s));
	if (params.limit != null) url.searchParams.set('limit', String(params.limit));
	if (params.offset != null) url.searchParams.set('offset', String(params.offset));

	const resp = await fetch(url.toString());
	if (!resp.ok) throw new Error(`search: ${resp.status} ${resp.statusText}`);
	return resp.json();
}

export async function getFile(
	source: string,
	path: string,
	archivePath?: string
): Promise<FileResponse> {
	const url = new URL('/api/v1/file', location.origin);
	url.searchParams.set('source', source);
	url.searchParams.set('path', path);
	if (archivePath) url.searchParams.set('archive_path', archivePath);

	const resp = await fetch(url.toString());
	if (!resp.ok) throw new Error(`getFile: ${resp.status} ${resp.statusText}`);
	return resp.json();
}

export async function listFiles(source: string): Promise<FileRecord[]> {
	const url = new URL('/api/v1/files', location.origin);
	url.searchParams.set('source', source);
	const resp = await fetch(url.toString());
	if (!resp.ok) throw new Error(`listFiles: ${resp.status} ${resp.statusText}`);
	return resp.json();
}

export interface FileRecord {
	path: string;
	mtime: number;
	kind: string;
}

export async function listDir(source: string, prefix = ''): Promise<TreeResponse> {
	const url = new URL('/api/v1/tree', location.origin);
	url.searchParams.set('source', source);
	if (prefix) url.searchParams.set('prefix', prefix);

	const resp = await fetch(url.toString());
	if (!resp.ok) throw new Error(`listDir: ${resp.status} ${resp.statusText}`);
	return resp.json();
}

/** List the inner members of an archive file by using the "::" prefix convention. */
export async function listArchiveMembers(source: string, archivePath: string): Promise<TreeResponse> {
	return listDir(source, archivePath + '::');
}

export interface ContextBatchItem {
	source: string;
	path: string;
	archive_path?: string | null;
	line: number;
	window?: number;
}

export interface ContextBatchResult {
	source: string;
	path: string;
	line: number;
	lines: ContextLine[];
	file_kind: string;
}

export interface ContextBatchResponse {
	results: ContextBatchResult[];
}

export async function contextBatch(requests: ContextBatchItem[]): Promise<ContextBatchResponse> {
	const resp = await fetch('/api/v1/context-batch', {
		method: 'POST',
		headers: { 'content-type': 'application/json' },
		body: JSON.stringify({ requests })
	});
	if (!resp.ok) throw new Error(`contextBatch: ${resp.status} ${resp.statusText}`);
	return resp.json();
}

export async function getContext(
	source: string,
	path: string,
	line: number,
	window = 5,
	archivePath?: string
): Promise<ContextResponse> {
	const url = new URL('/api/v1/context', location.origin);
	url.searchParams.set('source', source);
	url.searchParams.set('path', path);
	url.searchParams.set('line', String(line));
	url.searchParams.set('window', String(window));
	if (archivePath) url.searchParams.set('archive_path', archivePath);

	const resp = await fetch(url.toString());
	if (!resp.ok) throw new Error(`getContext: ${resp.status} ${resp.statusText}`);
	return resp.json();
}
