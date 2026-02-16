<script lang="ts">
	import { onMount } from 'svelte';
	import SearchBox from '$lib/SearchBox.svelte';
	import SourceSelector from '$lib/SourceSelector.svelte';
	import ResultList from '$lib/ResultList.svelte';
	import FileViewer from '$lib/FileViewer.svelte';
	import DirectoryTree from '$lib/DirectoryTree.svelte';
	import PathBar from '$lib/PathBar.svelte';
	import DirListing from '$lib/DirListing.svelte';
	import CommandPalette from '$lib/CommandPalette.svelte';
	import Settings from '$lib/Settings.svelte';
	import { search, listSources } from '$lib/api';
	import type { SearchResult, SourceInfo } from '$lib/api';
	import { profile } from '$lib/profile';
	import {
		type LineSelection,
		parseHash,
		formatHash
	} from '$lib/lineSelection';
	import { FilePath } from '$lib/filePath';

	// ── State ──────────────────────────────────────────────────────────────────

	type View = 'results' | 'file';
	type PanelMode = 'file' | 'dir';

	let view: View = 'results';
	let query = '';
	let mode = 'fuzzy';

	let sources: SourceInfo[] = [];
	let selectedSources: string[] = [];

	let results: SearchResult[] = [];
	let totalResults = 0;
	let searching = false;
	let isTyping = false;
	let isLoadingMore = false;
	let searchError: string | null = null;

	$: isSearchActive = isTyping || searching;
	$: canLoadMore = results.length < totalResults;
	$: nextBatchSize = canLoadMore ? Math.min(500, totalResults - results.length) : 0;

	// File / directory detail state
	let fileSource = '';
	let currentFile: FilePath | null = null;
	let fileSelection: LineSelection = [];

	let panelMode: PanelMode = 'file';
	let currentDirPrefix = '';

	let showTree = false;
	let showPalette = false;
	let showSettings = false;

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

	function onResizeKeydown(e: KeyboardEvent) {
		const step = e.shiftKey ? 32 : 8;
		if (e.key === 'ArrowRight') {
			e.preventDefault();
			sidebarWidth = Math.min(600, sidebarWidth + step);
			profile.update((p) => ({ ...p, sidebarWidth }));
		} else if (e.key === 'ArrowLeft') {
			e.preventDefault();
			sidebarWidth = Math.max(120, sidebarWidth - step);
			profile.update((p) => ({ ...p, sidebarWidth }));
		}
	}

	// ── Browser history ─────────────────────────────────────────────────────────

	interface AppState {
		view: View;
		query: string;
		mode: string;
		selectedSources: string[];
		fileSource: string;
		currentFile: FilePath | null;
		fileSelection: LineSelection;
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
			currentFile,
			fileSelection,
			panelMode,
			currentDirPrefix
		};
	}

	function buildUrl(s: AppState): string {
		const p = new URLSearchParams();
		if (s.query) p.set('q', s.query);
		if (s.mode && s.mode !== 'fuzzy') p.set('mode', s.mode);
		s.selectedSources.forEach((src) => p.append('source', src));
		if (s.view === 'file' && s.currentFile) {
			p.set('view', 'file');
			p.set('fsource', s.fileSource);
			// Serialize FilePath as separate path/apath for backward compatibility
			p.set('path', s.currentFile.outer);
			if (s.currentFile.inner) p.set('apath', s.currentFile.inner);
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
		const hash = formatHash(fileSelection);
		history.pushState(s, '', buildUrl(s) + hash);
	}

	/** Update just the hash without adding a history entry. */
	function syncHash() {
		const hash = formatHash(fileSelection);
		const base = window.location.pathname + window.location.search;
		history.replaceState(history.state, '', hash ? base + hash : base);
	}

	function applyState(s: AppState) {
		view = s.view;
		query = s.query;
		mode = s.mode;
		selectedSources = s.selectedSources;
		fileSource = s.fileSource;
		currentFile = s.currentFile;
		fileSelection = s.fileSelection;
		panelMode = s.panelMode;
		currentDirPrefix = s.currentDirPrefix;
		// Only change view when performing a live interactive search (push=true).
		if (s.query) doSearch(s.query, s.mode, s.selectedSources, false);
	}

	function restoreFromParams(params: URLSearchParams) {
		const q = params.get('q') ?? '';
		const m = params.get('mode') ?? 'fuzzy';
		const srcs = params.getAll('source');
		const v = (params.get('view') ?? 'results') as View;

		// Deserialize FilePath from URL params
		const path = params.get('path');
		const apath = params.get('apath');
		const restoredFile = path ? FilePath.fromParts(path, apath) : null;

		const s: AppState = {
			view: v,
			query: q,
			mode: m,
			selectedSources: srcs,
			fileSource: params.get('fsource') ?? '',
			currentFile: restoredFile,
			fileSelection: parseHash(window.location.hash),
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

		// Use capture so Ctrl+P is intercepted before any child's stopPropagation
		// (e.g. the command palette panel) and before the browser's print shortcut.
		function handleKeydown(e: KeyboardEvent) {
			const ctrl = e.ctrlKey || e.metaKey;
			if (ctrl && e.key === 'p') {
				e.preventDefault();
				showPalette = !showPalette;
			}
		}

		window.addEventListener('popstate', handlePopState);
		window.addEventListener('keydown', handleKeydown, { capture: true });
		return () => {
			window.removeEventListener('popstate', handlePopState);
			window.removeEventListener('keydown', handleKeydown, { capture: true });
		};
	});

	// ── Search ──────────────────────────────────────────────────────────────────

	async function doSearch(
		q: string,
		m: string,
		srcs: string[],
		push = true,
		append = false
	) {
		if (q.trim().length < 3) {
			results = [];
			totalResults = 0;
			searchError = null;
			return;
		}

		if (append) {
			isLoadingMore = true;
		} else {
			searching = true;
		}

		searchError = null;
		if (push) pushState();
		try {
			// Use offset-based pagination for "Load More"
			// Initial search: offset=0, limit=50
			// Load more: offset=current results length, limit=min(500, remaining)
			const offset = append ? results.length : 0;
			const limit = append ? Math.min(500, totalResults - results.length) : 50;

			const resp = await search({ q, mode: m, sources: srcs, limit, offset });

			if (append) {
				// Append new results to existing ones
				results = [...results, ...resp.results];
			} else {
				// Replace results for new search
				results = resp.results;
			}
			totalResults = resp.total;
			// Only switch to results view on interactive searches, not state restoration.
			if (push) view = 'results';
		} catch (e) {
			searchError = String(e);
			if (push) view = 'results';
		} finally {
			if (append) {
				isLoadingMore = false;
			} else {
				searching = false;
			}
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

	function loadMore() {
		doSearch(query, mode, selectedSources, false, true);
	}

	// ── File viewer ─────────────────────────────────────────────────────────────

	function openFile(e: CustomEvent<SearchResult>) {
		const r = e.detail;
		fileSource = r.source;
		currentFile = FilePath.fromParts(r.path, r.archive_path ?? null);
		fileSelection = r.line_number ? [r.line_number] : [];
		panelMode = 'file';
		view = 'file';
		showTree = true;
		pushState();
	}

	function openFileFromTree(e: CustomEvent<{ source: string; path: string; kind: string; archivePath?: string; showAsDirectory?: boolean }>) {
		fileSource = e.detail.source;
		currentFile = FilePath.fromParts(e.detail.path, e.detail.archivePath ?? null);
		fileSelection = [];

		// Archives should show as directory listings, not file views
		if (e.detail.showAsDirectory || (e.detail.kind === 'archive' && !e.detail.archivePath)) {
			panelMode = 'dir';
			currentDirPrefix = currentFile.full + '::';
		} else {
			panelMode = 'file';
		}

		view = 'file';
		pushState();
	}

	function handleDirOpenFile(e: CustomEvent<{ source: string; path: string; kind: string; archivePath?: string }>) {
		currentFile = FilePath.fromParts(e.detail.path, e.detail.archivePath ?? null);
		fileSelection = [];
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

	function handleLineSelect(e: CustomEvent<{ selection: LineSelection }>) {
		fileSelection = e.detail.selection;
		syncHash();
	}

	// ── Command palette (Ctrl+P) ────────────────────────────────────────────────

	function handlePaletteSelect(e: CustomEvent<{ source: string; path: string; archivePath: string | null; kind: string }>) {
		fileSource = e.detail.source;
		fileSelection = [];
		view = 'file';
		showTree = true;

		currentFile = FilePath.fromParts(e.detail.path, e.detail.archivePath);

		// Archive files (top-level or nested) show directory listing instead of file content
		if (e.detail.kind === 'archive') {
			panelMode = 'dir';
			currentDirPrefix = currentFile.full + '::';
		} else {
			panelMode = 'file';
		}
		pushState();
	}

	// ── Derived ────────────────────────────────────────────────────────────────

	/** Source names for SourceSelector / CommandPalette (string[]). */
	$: sourceNames = sources.map((s) => s.name);

	/** Server-configured base URLs keyed by source name. */
	$: serverBaseUrls = Object.fromEntries(
		sources.filter((s) => s.base_url != null).map((s) => [s.name, s.base_url as string])
	);

	/** Effective base URL for a source: user override > server value > null. */
	function effectiveBaseUrl(src: string): string | null {
		return $profile.sourceBaseUrls?.[src] ?? serverBaseUrls[src] ?? null;
	}

	// Sources available for Ctrl+P: prefer selected, fall back to all, then current source.
	$: paletteSources = selectedSources.length
		? selectedSources
		: fileSource
			? [fileSource]
			: sourceNames;

	// Path for the PathBar component
	$: pathBarPath = panelMode === 'dir' ? currentDirPrefix : (currentFile?.full ?? '');
</script>

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
				<SearchBox {query} {mode} {searching} bind:isTyping on:change={handleSearchChange} />
			</div>
			{#if sourceNames.length > 0}
				<SourceSelector sources={sourceNames} selected={selectedSources} on:change={handleSourceChange} />
			{/if}
			<button class="gear-btn" title="Settings" on:click={() => (showSettings = !showSettings)}>⚙</button>
		</div>
		<div class="content content--full content--with-tree">
			{#if showTree}
				<div class="sidebar" style="width: {sidebarWidth}px">
					<DirectoryTree
						source={fileSource}
						activePath={currentFile?.full ?? null}
						on:open={openFileFromTree}
					/>
				</div>
				<button
					class="resize-handle"
					type="button"
					aria-label="Resize sidebar"
					on:mousedown={onResizeStart}
					on:keydown={onResizeKeydown}
				/>
			{/if}
			<div class="viewer-wrap">
				<PathBar
					source={fileSource}
					path={pathBarPath}
					archivePath={panelMode === 'file' ? currentFile?.inner ?? null : null}
					baseUrl={effectiveBaseUrl(fileSource)}
					on:back={backToResults}
				/>
				{#if panelMode === 'dir'}
					<DirListing
						source={fileSource}
						prefix={currentDirPrefix}
						on:openFile={handleDirOpenFile}
						on:openDir={handleDirOpenDir}
					/>
				{:else if currentFile}
					{#key `${fileSource}:${currentFile.full}`}
						<FileViewer
							source={fileSource}
							path={currentFile.outer}
							archivePath={currentFile.inner}
							selection={fileSelection}
							on:lineselect={handleLineSelect}
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
				<SearchBox {query} {mode} {searching} bind:isTyping on:change={handleSearchChange} />
			</div>
			{#if sourceNames.length > 0}
				<SourceSelector sources={sourceNames} selected={selectedSources} on:change={handleSourceChange} />
			{/if}
			<button class="gear-btn" title="Settings" on:click={() => (showSettings = !showSettings)}>⚙</button>
		</div>
		<div class="content">
			{#if searchError}
				<div class="status error">{searchError}</div>
			{:else if query.trim().length >= 3}
				<div class="result-meta">
					{#if results.length < totalResults}
						Showing {results.length.toLocaleString()} of {totalResults.toLocaleString()} results
					{:else}
						{totalResults.toLocaleString()} result{totalResults !== 1 ? 's' : ''}
					{/if}
				</div>
				<ResultList
					{results}
					{totalResults}
					{nextBatchSize}
					{isLoadingMore}
					searching={isSearchActive}
					on:open={openFile}
					on:loadmore={loadMore}
				/>
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

<Settings
	open={showSettings}
	{sources}
	on:close={() => (showSettings = false)}
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

	/* ── Content area ───────────────────────────────────────────────────────── */
	.content {
		flex: 1;
		overflow-y: auto;
		overflow-x: hidden;
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
		border: none;
		padding: 0;
		transition: background 0.15s;
	}

	.resize-handle:focus-visible {
		outline: 2px solid var(--accent, #58a6ff);
		outline-offset: 0;
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

	.gear-btn {
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

	.gear-btn:hover {
		background: var(--bg-hover, rgba(255, 255, 255, 0.08));
		color: var(--text);
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
