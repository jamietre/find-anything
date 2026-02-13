// ── Types ────────────────────────────────────────────────────────────────────

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

export async function listSources(): Promise<string[]> {
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
	context?: number;
}

export async function search(params: SearchParams): Promise<SearchResponse> {
	const url = new URL('/api/v1/search', location.origin);
	url.searchParams.set('q', params.q);
	if (params.mode) url.searchParams.set('mode', params.mode);
	if (params.sources) params.sources.forEach((s) => url.searchParams.append('source', s));
	if (params.limit != null) url.searchParams.set('limit', String(params.limit));
	if (params.offset != null) url.searchParams.set('offset', String(params.offset));
	if (params.context != null) url.searchParams.set('context', String(params.context));

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

export async function listDir(source: string, prefix = ''): Promise<TreeResponse> {
	const url = new URL('/api/v1/tree', location.origin);
	url.searchParams.set('source', source);
	if (prefix) url.searchParams.set('prefix', prefix);

	const resp = await fetch(url.toString());
	if (!resp.ok) throw new Error(`listDir: ${resp.status} ${resp.statusText}`);
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
