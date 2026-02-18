<script lang="ts">
	import { onMount, tick } from 'svelte';
	import SearchView from '$lib/SearchView.svelte';
	import FileView from '$lib/FileView.svelte';
	import CommandPalette from '$lib/CommandPalette.svelte';
	import Settings from '$lib/Settings.svelte';
	import Dashboard from '$lib/Dashboard.svelte';
	import { search, listSources, getSettings } from '$lib/api';
	import type { SearchResult, SourceInfo } from '$lib/api';
	import { contextWindow } from '$lib/settingsStore';
	import { formatHash } from '$lib/lineSelection';
	import type { LineSelection } from '$lib/lineSelection';
	import { FilePath } from '$lib/filePath';
	import { buildUrl, restoreFromParams } from '$lib/appState';
	import type { AppState } from '$lib/appState';
	import { profile } from '$lib/profile';

	// ── State ──────────────────────────────────────────────────────────────────

	let view: 'results' | 'file' = 'results';
	let query = '';
	let mode = 'fuzzy';

	let sources: SourceInfo[] = [];
	let selectedSources: string[] = [];

	let results: SearchResult[] = [];
	let totalResults = 0;
	let searching = false;
	let searchError: string | null = null;
	let searchId = 0;

	let fileSource = '';
	let currentFile: FilePath | null = null;
	let fileSelection: LineSelection = [];
	let panelMode: 'file' | 'dir' = 'file';
	let currentDirPrefix = '';
	let showTree = false;
	let showPalette = false;
	let showSettings = false;
	let showDashboard = false;

	// ── History ─────────────────────────────────────────────────────────────────

	function captureState(): AppState {
		return { view, query, mode, selectedSources, fileSource, currentFile, fileSelection, panelMode, currentDirPrefix };
	}

	function pushState() {
		const s = captureState();
		history.pushState(s, '', buildUrl(s) + formatHash(fileSelection));
	}

	function syncHash() {
		const hash = formatHash(fileSelection);
		const base = location.pathname + location.search;
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
		if (s.query) doSearch(s.query, s.mode, s.selectedSources, false);
	}

	// ── Lifecycle ───────────────────────────────────────────────────────────────

	onMount(() => {
		(async () => {
			try { sources = await listSources(); } catch { /* silent */ }
			try { const s = await getSettings(); contextWindow.set(s.context_window); } catch { /* silent */ }

			const params = new URLSearchParams(location.search);
			if (params.has('q') || params.has('path')) {
				const restored = restoreFromParams(params);
				showTree = restored.showTree;
				applyState(restored);
				history.replaceState(captureState(), '', location.href);
			}
		})();

		function handlePopState(e: PopStateEvent) {
			if (e.state) applyState(e.state as AppState);
			else applyState(restoreFromParams(new URLSearchParams(location.search)));
		}

		function handleKeydown(e: KeyboardEvent) {
			if ((e.ctrlKey || e.metaKey) && e.key === 'p') {
				e.preventDefault();
				showPalette = !showPalette;
			}
		}

		window.addEventListener('popstate', handlePopState);
		window.addEventListener('keydown', handleKeydown, { capture: true });
		// Scroll events as a secondary trigger: when the window is scrollable
		// and the user scrolls near the sentinel, load more results.
		window.addEventListener('scroll', checkScroll, { passive: true });
		return () => {
			window.removeEventListener('popstate', handlePopState);
			window.removeEventListener('keydown', handleKeydown, { capture: true });
			window.removeEventListener('scroll', checkScroll);
		};
	});

	// ── Load more ───────────────────────────────────────────────────────────────

	let loadingMore = false;
	let noMoreResults = false;
	// Tracks server cursor independently of results.length. Client dedup can
	// reduce how many items are added per page; using results.length as the
	// offset would then re-request the same range and stall pagination.
	let loadOffset = 0;
	let sentinel: HTMLElement | null = null;

	// getBoundingClientRect() forces a synchronous layout reflow and returns
	// position relative to the viewport — reliable regardless of scroll container
	// or CSS layout (unlike scrollHeight - scrollY - innerHeight which breaks when
	// html/body have height:100%).
	function isNearBottom(): boolean {
		if (!sentinel) return false;
		return sentinel.getBoundingClientRect().top < window.innerHeight + 600;
	}

	function checkScroll() {
		if (loadingMore || noMoreResults || view !== 'results' || query.trim().length < 3) return;
		if (isNearBottom()) triggerLoad();
	}

	async function triggerLoad() {
		if (loadingMore || noMoreResults || query.trim().length < 3) return;
		loadingMore = true;
		try {
			const resp = await search({ q: query, mode, sources: selectedSources, limit: 50, offset: loadOffset });
			if (resp.results.length === 0) {
				noMoreResults = true;
			} else {
				// IMPORTANT: client-side dedup must not be removed. The server
				// deduplicates within each request, but cross-page duplicates occur
				// because scoring_limit grows with each page (offset + limit + 200),
				// causing the server to re-rank candidates. An item at position 45 on
				// page 0 can shift to position 69 on page 1 and appear in both.
				// Duplicate keys in the keyed {#each} throw a runtime error and prevent
				// DOM updates, which keeps the sentinel pinned and causes an infinite
				// request loop. See CLAUDE.md §"Search result keys and load-more dedup".
				const seen = new Set(results.map(r => `${r.source}:${r.path}:${r.archive_path ?? ''}:${r.line_number}`));
				const fresh = resp.results.filter(r => !seen.has(`${r.source}:${r.path}:${r.archive_path ?? ''}:${r.line_number}`));
				results = [...results, ...fresh];
				totalResults = resp.total;
				// Advance by full server response, not fresh.length — see loadOffset comment above.
				loadOffset += resp.results.length;
			}
			await tick();
		} catch { /* silent */ }
		loadingMore = false;
		// getBoundingClientRect() forces layout, so this is accurate after tick().
		// If sentinel is still near the bottom, keep filling the viewport.
		if (isNearBottom()) triggerLoad();
	}

	// ── Search ──────────────────────────────────────────────────────────────────

	async function doSearch(q: string, m: string, srcs: string[], push = true) {
		if (q.trim().length < 3) {
			results = []; totalResults = 0; noMoreResults = false; loadOffset = 0; searchError = null;
			return;
		}
		searching = true;
		searchError = null;
		searchId += 1;
		noMoreResults = false;
		loadOffset = 0;
		if (push) {
			pushState();
			window.scrollTo(0, 0);
		}
		try {
			const resp = await search({ q, mode: m, sources: srcs, limit: 50, offset: 0 });
			results = resp.results;
			totalResults = resp.total;
			loadOffset = resp.results.length; // server cursor starts after page 0
			if (resp.results.length === 0) noMoreResults = true;
			if (push) view = 'results';
		} catch (e) {
			searchError = String(e);
			results = []; totalResults = 0; noMoreResults = true; loadOffset = 0;
			if (push) view = 'results';
		} finally {
			searching = false;
		}
		// Auto-fill viewport if the first page doesn't reach the scroll threshold.
		await tick();
		if (isNearBottom()) triggerLoad();
	}

	// ── Search event handlers ────────────────────────────────────────────────────

	function handleSearch(e: CustomEvent<{ query: string; mode: string }>) {
		query = e.detail.query;
		mode = e.detail.mode;
		doSearch(query, mode, selectedSources);
	}

	function handleSourceChange(e: CustomEvent<string[]>) {
		selectedSources = e.detail;
		if (query.trim()) doSearch(query, mode, selectedSources);
	}

	// ── File viewer event handlers ───────────────────────────────────────────────

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

	function handleOpenFileFromTree(e: CustomEvent<{ source: string; path: string; kind: string; archivePath?: string; showAsDirectory?: boolean }>) {
		fileSource = e.detail.source;
		currentFile = FilePath.fromParts(e.detail.path, e.detail.archivePath ?? null);
		fileSelection = [];
		if (e.detail.showAsDirectory || (e.detail.kind === 'archive' && !e.detail.archivePath)) {
			panelMode = 'dir';
			currentDirPrefix = currentFile.full + '::';
		} else {
			panelMode = 'file';
		}
		view = 'file';
		pushState();
	}

	function handleOpenDirFile(e: CustomEvent<{ source: string; path: string; kind: string; archivePath?: string }>) {
		currentFile = FilePath.fromParts(e.detail.path, e.detail.archivePath ?? null);
		fileSelection = [];
		panelMode = 'file';
		pushState();
	}

	function handleOpenDir(e: CustomEvent<{ prefix: string }>) {
		currentDirPrefix = e.detail.prefix;
		pushState();
	}

	function handleLineSelect(e: CustomEvent<{ selection: LineSelection }>) {
		fileSelection = e.detail.selection;
		syncHash();
	}

	function handleTreeToggle() {
		showTree = !showTree;
	}

	function handleBack() {
		view = 'results';
		pushState();
	}

	// ── Command palette ──────────────────────────────────────────────────────────

	function handlePaletteSelect(e: CustomEvent<{ source: string; path: string; archivePath: string | null; kind: string }>) {
		fileSource = e.detail.source;
		fileSelection = [];
		view = 'file';
		showTree = true;
		currentFile = FilePath.fromParts(e.detail.path, e.detail.archivePath);
		if (e.detail.kind === 'archive') {
			panelMode = 'dir';
			currentDirPrefix = currentFile.full + '::';
		} else {
			panelMode = 'file';
		}
		pushState();
	}

	// ── Derived ──────────────────────────────────────────────────────────────────

	$: sourceNames = sources.map((s) => s.name);
	$: serverBaseUrls = Object.fromEntries(sources.filter((s) => s.base_url != null).map((s) => [s.name, s.base_url as string]));
	$: paletteSources = selectedSources.length ? selectedSources : fileSource ? [fileSource] : sourceNames;
	$: fileBaseUrl = $profile.sourceBaseUrls?.[fileSource] ?? serverBaseUrls[fileSource] ?? null;
</script>

<div class="page" class:file-view={view === 'file'}>
	{#if view === 'file'}
		<FileView
			{fileSource}
			{currentFile}
			{fileSelection}
			{panelMode}
			{currentDirPrefix}
			{showTree}
			baseUrl={fileBaseUrl}
			{query}
			{mode}
			{searching}
			sources={sourceNames}
			{selectedSources}
			on:back={handleBack}
			on:search={handleSearch}
			on:sourceChange={handleSourceChange}
			on:gear={() => (showSettings = !showSettings)}
			on:dashboard={() => (showDashboard = !showDashboard)}
			on:treeToggle={handleTreeToggle}
			on:openFileFromTree={handleOpenFileFromTree}
			on:openDirFile={handleOpenDirFile}
			on:openDir={handleOpenDir}
			on:lineselect={handleLineSelect}
		/>
	{:else}
		<SearchView
			{query}
			{mode}
			{searching}
			sources={sourceNames}
			{selectedSources}
			{results}
			{totalResults}
			{searchError}
			{searchId}
			on:search={handleSearch}
			on:sourceChange={handleSourceChange}
			on:open={openFile}
			on:gear={() => (showSettings = !showSettings)}
			on:dashboard={() => (showDashboard = !showDashboard)}
		/>
		<div bind:this={sentinel}></div>
		{#if loadingMore}
			<div class="load-row">
				<div class="spinner">
					<svg viewBox="0 0 24 24" fill="none">
						<circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3" opacity="0.25"/>
						<path d="M12 2a10 10 0 0 1 10 10" stroke="currentColor" stroke-width="3" stroke-linecap="round"/>
					</svg>
				</div>
				<span>Loading more results…</span>
			</div>
		{/if}
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

<Dashboard
	open={showDashboard}
	on:close={() => (showDashboard = false)}
/>

<style>
	.page.file-view {
		display: flex;
		flex-direction: column;
		height: 100vh;
		overflow: hidden;
	}

	.load-row {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 10px;
		height: 56px;
		color: var(--text-muted);
		font-size: 14px;
		padding: 0 16px;
	}

	.load-row .spinner {
		width: 16px;
		height: 16px;
		flex-shrink: 0;
	}

	.load-row .spinner svg {
		width: 100%;
		height: 100%;
		color: var(--accent);
		animation: spin 0.8s linear infinite;
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}
</style>
