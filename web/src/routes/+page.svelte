<script lang="ts">
	import { onMount } from 'svelte';
	import SearchBox from '$lib/SearchBox.svelte';
	import SourceChips from '$lib/SourceChips.svelte';
	import ResultList from '$lib/ResultList.svelte';
	import FileViewer from '$lib/FileViewer.svelte';
	import DirectoryTree from '$lib/DirectoryTree.svelte';
	import Breadcrumb from '$lib/Breadcrumb.svelte';
	import DirListing from '$lib/DirListing.svelte';
	import CommandPalette from '$lib/CommandPalette.svelte';
	import { search, listSources } from '$lib/api';
	import type { SearchResult } from '$lib/api';
	import { profile } from '$lib/profile';

	// ── State ──────────────────────────────────────────────────────────────────

	type View = 'results' | 'file';
	type PanelMode = 'file' | 'dir';

	let view: View = 'results';
	let query = '';
	let mode = 'fuzzy';

	let sources: string[] = [];
	let selectedSources: string[] = [];

	let results: SearchResult[] = [];
	let totalResults = 0;
	let searching = false;
	let searchError: string | null = null;

	// File / directory detail state
	let fileSource = '';
	let filePath = '';
	let fileArchivePath: string | null = null;
	let fileTargetLine: number | null = null;

	let panelMode: PanelMode = 'file';
	let currentDirPrefix = '';

	let showTree = false;
	let showPalette = false;

	// Sidebar resize
	let sidebarWidth = $profile.sidebarWidth ?? 240;

	function onResizeStart(e: MouseEvent) {
		const startX = e.clientX;
		const startWidth = sidebarWidth;

		function onMove(ev: MouseEvent) {
			sidebarWidth = Math.min(600, Math.max(120, startWidth + ev.clientX - startX));
		}
		function onUp() {
			document.removeEventListener('mousemove', onMove);
			document.removeEventListener('mouseup', onUp);
			profile.update((p) => ({ ...p, sidebarWidth }));
		}
		document.addEventListener('mousemove', onMove);
		document.addEventListener('mouseup', onUp);
	}

	// ── Browser history ─────────────────────────────────────────────────────────

	interface AppState {
		view: View;
		query: string;
		mode: string;
		selectedSources: string[];
		fileSource: string;
		filePath: string;
		fileArchivePath: string | null;
		fileTargetLine: number | null;
		panelMode: PanelMode;
		currentDirPrefix: string;
	}

	function captureState(): AppState {
		return {
			view,
			query,
			mode,
			selectedSources,
			fileSource,
			filePath,
			fileArchivePath,
			fileTargetLine,
			panelMode,
			currentDirPrefix
		};
	}

	function buildUrl(s: AppState): string {
		const p = new URLSearchParams();
		if (s.query) p.set('q', s.query);
		if (s.mode && s.mode !== 'fuzzy') p.set('mode', s.mode);
		s.selectedSources.forEach((src) => p.append('source', src));
		if (s.view === 'file') {
			p.set('view', 'file');
			p.set('fsource', s.fileSource);
			p.set('path', s.filePath);
			if (s.fileArchivePath) p.set('apath', s.fileArchivePath);
			if (s.fileTargetLine != null) p.set('line', String(s.fileTargetLine));
			if (s.panelMode === 'dir') {
				p.set('panel', 'dir');
				p.set('dir', s.currentDirPrefix);
			}
		}
		const qs = p.toString();
		return qs ? `?${qs}` : window.location.pathname;
	}

	function pushState() {
		const s = captureState();
		history.pushState(s, '', buildUrl(s));
	}

	function applyState(s: AppState) {
		view = s.view;
		query = s.query;
		mode = s.mode;
		selectedSources = s.selectedSources;
		fileSource = s.fileSource;
		filePath = s.filePath;
		fileArchivePath = s.fileArchivePath;
		fileTargetLine = s.fileTargetLine;
		panelMode = s.panelMode;
		currentDirPrefix = s.currentDirPrefix;
		if (s.query) doSearch(s.query, s.mode, s.selectedSources, false);
	}

	function restoreFromParams(params: URLSearchParams) {
		const q = params.get('q') ?? '';
		const m = params.get('mode') ?? 'fuzzy';
		const srcs = params.getAll('source');
		const v = (params.get('view') ?? 'results') as View;
		const s: AppState = {
			view: v,
			query: q,
			mode: m,
			selectedSources: srcs,
			fileSource: params.get('fsource') ?? '',
			filePath: params.get('path') ?? '',
			fileArchivePath: params.get('apath') ?? null,
			fileTargetLine: params.has('line') ? parseInt(params.get('line')!) : null,
			panelMode: (params.get('panel') ?? 'file') as PanelMode,
			currentDirPrefix: params.get('dir') ?? ''
		};
		if (v === 'file') showTree = true;
		applyState(s);
	}

	// ── Lifecycle ───────────────────────────────────────────────────────────────

	onMount(() => {
		(async () => {
			try {
				sources = await listSources();
			} catch (e) {
				console.warn('Failed to load sources:', e);
			}

			// Restore state from URL on initial load.
			const params = new URLSearchParams(window.location.search);
			if (params.has('q') || params.has('path')) {
				restoreFromParams(params);
				// Replace so the initial entry has a full state object.
				history.replaceState(captureState(), '', window.location.href);
			}
		})();

		function handlePopState(e: PopStateEvent) {
			if (e.state) {
				applyState(e.state as AppState);
			} else {
				restoreFromParams(new URLSearchParams(window.location.search));
			}
		}

		window.addEventListener('popstate', handlePopState);
		return () => window.removeEventListener('popstate', handlePopState);
	});

	// ── Search ──────────────────────────────────────────────────────────────────

	async function doSearch(
		q: string,
		m: string,
		srcs: string[],
		push = true
	) {
		if (!q.trim()) {
			results = [];
			totalResults = 0;
			return;
		}
		searching = true;
		searchError = null;
		if (push) pushState();
		try {
			const resp = await search({ q, mode: m, sources: srcs, limit: 50 });
			results = resp.results;
			totalResults = resp.total;
			view = 'results';
		} catch (e) {
			searchError = String(e);
			view = 'results';
		} finally {
			searching = false;
		}
	}

	function handleSearchChange(e: CustomEvent<{ query: string; mode: string }>) {
		query = e.detail.query;
		mode = e.detail.mode;
		doSearch(query, mode, selectedSources);
	}

	function handleSourceChange(e: CustomEvent<string[]>) {
		selectedSources = e.detail;
		if (query.trim()) doSearch(query, mode, selectedSources);
	}

	// ── File viewer ─────────────────────────────────────────────────────────────

	function openFile(e: CustomEvent<SearchResult>) {
		const r = e.detail;
		fileSource = r.source;
		filePath = r.path;
		fileArchivePath = r.archive_path ?? null;
		fileTargetLine = r.line_number;
		panelMode = 'file';
		view = 'file';
		showTree = true;
		pushState();
	}

	function openFileFromTree(e: CustomEvent<{ source: string; path: string; kind: string }>) {
		fileSource = e.detail.source;
		filePath = e.detail.path;
		fileArchivePath = null;
		fileTargetLine = null;
		panelMode = 'file';
		view = 'file';
		pushState();
	}

	function handleBreadcrumbNavigate(e: CustomEvent<{ prefix: string }>) {
		currentDirPrefix = e.detail.prefix;
		panelMode = 'dir';
		pushState();
	}

	function handleDirOpenFile(e: CustomEvent<{ source: string; path: string; kind: string }>) {
		filePath = e.detail.path;
		fileArchivePath = null;
		fileTargetLine = null;
		panelMode = 'file';
		pushState();
	}

	function handleDirOpenDir(e: CustomEvent<{ prefix: string }>) {
		currentDirPrefix = e.detail.prefix;
		pushState();
	}

	function backToResults() {
		view = 'results';
		pushState();
	}

	// ── Command palette (Ctrl+P) ────────────────────────────────────────────────

	function handlePaletteSelect(e: CustomEvent<{ source: string; path: string }>) {
		fileSource = e.detail.source;
		filePath = e.detail.path;
		fileArchivePath = null;
		fileTargetLine = null;
		panelMode = 'file';
		view = 'file';
		showTree = true;
		pushState();
	}

	// ── Global keyboard shortcuts ───────────────────────────────────────────────

	function handleGlobalKeydown(e: KeyboardEvent) {
		const ctrl = e.ctrlKey || e.metaKey;
		if (ctrl && e.key === 'p') {
			e.preventDefault();
			showPalette = !showPalette;
		}
	}

	// ── Derived ────────────────────────────────────────────────────────────────

	$: breadcrumbPath = panelMode === 'dir' ? currentDirPrefix.replace(/\/$/, '') : filePath;
	$: breadcrumbIsDir = panelMode === 'dir';

	// Sources available for Ctrl+P: prefer selected, fall back to all, then current source.
	$: paletteSources = selectedSources.length
		? selectedSources
		: fileSource
			? [fileSource]
			: sources;
</script>

<svelte:window on:keydown={handleGlobalKeydown} />

<div class="page">
	{#if view === 'file'}
		<!-- ── File viewer ────────────────────────────────────────────────────── -->
		<div class="topbar topbar--compact">
			<span class="logo">find-anything</span>
			<button
				class="tree-toggle"
				class:active={showTree}
				title="Toggle file tree (Ctrl+P to search files)"
				on:click={() => (showTree = !showTree)}
			>⊞</button>
			<div class="search-wrap">
				<SearchBox {query} {mode} on:change={handleSearchChange} />
			</div>
		</div>
		{#if sources.length > 0}
			<div class="source-bar">
				<SourceChips {sources} selected={selectedSources} on:change={handleSourceChange} />
			</div>
		{/if}
		<div class="content content--full content--with-tree">
			{#if showTree}
				<div class="sidebar" style="width: {sidebarWidth}px">
					<DirectoryTree
						source={fileSource}
						activePath={filePath}
						on:open={openFileFromTree}
					/>
				</div>
				<!-- svelte-ignore a11y-no-static-element-interactions -->
				<div class="resize-handle" on:mousedown={onResizeStart} role="separator" />
			{/if}
			<div class="viewer-wrap">
				<Breadcrumb
					path={breadcrumbPath}
					isDir={breadcrumbIsDir}
					on:navigate={handleBreadcrumbNavigate}
				/>
				{#if panelMode === 'dir'}
					<DirListing
						source={fileSource}
						prefix={currentDirPrefix}
						on:openFile={handleDirOpenFile}
						on:openDir={handleDirOpenDir}
					/>
				{:else}
					{#key `${fileSource}:${filePath}:${fileArchivePath}`}
						<FileViewer
							source={fileSource}
							path={filePath}
							archivePath={fileArchivePath}
							targetLine={fileTargetLine}
							on:back={backToResults}
						/>
					{/key}
				{/if}
			</div>
		</div>
	{:else}
		<!-- ── Results ──────────────────────────────────────────────────────── -->
		<div class="topbar">
			<span class="logo">find-anything</span>
			<div class="search-wrap">
				<SearchBox {query} {mode} on:change={handleSearchChange} />
			</div>
		</div>
		{#if sources.length > 0}
			<div class="source-bar">
				<SourceChips {sources} selected={selectedSources} on:change={handleSourceChange} />
			</div>
		{/if}
		<div class="content">
			{#if searching}
				<div class="status">Searching…</div>
			{:else if searchError}
				<div class="status error">{searchError}</div>
			{:else}
				<div class="result-meta">{totalResults} result{totalResults !== 1 ? 's' : ''}</div>
				<ResultList {results} on:open={openFile} />
			{/if}
		</div>
	{/if}
</div>

<CommandPalette
	open={showPalette}
	sources={paletteSources}
	on:select={handlePaletteSelect}
	on:close={() => (showPalette = false)}
/>

<style>
	.page {
		display: flex;
		flex-direction: column;
		height: 100vh;
		overflow: hidden;
	}

	/* ── Top bar ────────────────────────────────────────────────────────────── */
	.topbar {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 16px;
		background: var(--bg-secondary);
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	.topbar--compact {
		flex-wrap: nowrap;
	}

	.logo {
		font-size: 14px;
		font-weight: 600;
		color: var(--text);
		white-space: nowrap;
		flex-shrink: 0;
	}

	.search-wrap {
		min-width: 260px;
		flex: 1;
	}

	/* ── Source bar ─────────────────────────────────────────────────────────── */
	.source-bar {
		padding: 6px 16px;
		background: var(--bg-secondary);
		border-bottom: 1px solid var(--border);
		overflow-x: auto;
		flex-shrink: 0;
	}

	/* ── Content area ───────────────────────────────────────────────────────── */
	.content {
		flex: 1;
		overflow-y: auto;
		padding: 0 16px;
		width: 100%;
	}

	.content--full {
		padding: 0;
	}

	.content--with-tree {
		display: flex;
		flex-direction: row;
		overflow: hidden;
	}

	.sidebar {
		flex-shrink: 0;
		overflow: hidden;
		display: flex;
		flex-direction: column;
	}

	.resize-handle {
		width: 4px;
		flex-shrink: 0;
		cursor: col-resize;
		background: var(--border);
		transition: background 0.15s;
	}

	.resize-handle:hover {
		background: var(--accent, #58a6ff);
	}

	.viewer-wrap {
		flex: 1;
		overflow: hidden;
		display: flex;
		flex-direction: column;
	}

	.tree-toggle {
		background: none;
		border: none;
		cursor: pointer;
		color: var(--text-muted);
		font-size: 16px;
		padding: 2px 6px;
		border-radius: 4px;
		line-height: 1;
		flex-shrink: 0;
	}

	.tree-toggle:hover {
		background: var(--bg-hover, rgba(255, 255, 255, 0.08));
		color: var(--text);
	}

	.tree-toggle.active {
		color: var(--accent, #58a6ff);
	}

	/* ── Status / meta ──────────────────────────────────────────────────────── */
	.status {
		padding: 24px;
		color: var(--text-muted);
		text-align: center;
	}

	.status.error {
		color: #f85149;
	}

	.result-meta {
		padding: 12px 0 4px;
		color: var(--text-muted);
		font-size: 13px;
	}
</style>
