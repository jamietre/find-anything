<script lang="ts">
	import { onMount } from 'svelte';
	import SearchBox from '$lib/SearchBox.svelte';
	import SourceChips from '$lib/SourceChips.svelte';
	import ResultList from '$lib/ResultList.svelte';
	import FileViewer from '$lib/FileViewer.svelte';
	import DirectoryTree from '$lib/DirectoryTree.svelte';
	import Breadcrumb from '$lib/Breadcrumb.svelte';
	import DirListing from '$lib/DirListing.svelte';
	import { search, listSources } from '$lib/api';
	import type { SearchResult } from '$lib/api';

	// ── State ──────────────────────────────────────────────────────────────────

	type View = 'results' | 'file';

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

	// Right-panel mode: 'file' shows FileViewer, 'dir' shows DirListing
	type PanelMode = 'file' | 'dir';
	let panelMode: PanelMode = 'file';
	let currentDirPrefix = ''; // used when panelMode === 'dir'

	// Sidebar tree toggle
	let showTree = false;

	// ── Lifecycle ──────────────────────────────────────────────────────────────

	onMount(async () => {
		try {
			sources = await listSources();
		} catch (e) {
			console.warn('Failed to load sources:', e);
		}
	});

	// ── Search ──────────────────────────────────────────────────────────────────

	async function doSearch(q: string, m: string, srcs: string[]) {
		if (!q.trim()) {
			results = [];
			totalResults = 0;
			return;
		}
		searching = true;
		searchError = null;
		try {
			const resp = await search({ q, mode: m, sources: srcs, context: 3, limit: 50 });
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
		if (query.trim()) {
			doSearch(query, mode, selectedSources);
		}
	}

	// ── File viewer ────────────────────────────────────────────────────────────

	function openFile(e: CustomEvent<SearchResult>) {
		const r = e.detail;
		fileSource = r.source;
		filePath = r.path;
		fileArchivePath = r.archive_path ?? null;
		fileTargetLine = r.line_number;
		view = 'file';
		showTree = true;
	}

	function openFileFromTree(e: CustomEvent<{ source: string; path: string; kind: string }>) {
		fileSource = e.detail.source;
		filePath = e.detail.path;
		fileArchivePath = null;
		fileTargetLine = null;
		panelMode = 'file';
		view = 'file';
	}

	function handleBreadcrumbNavigate(e: CustomEvent<{ prefix: string }>) {
		currentDirPrefix = e.detail.prefix;
		panelMode = 'dir';
	}

	function handleDirOpenFile(e: CustomEvent<{ source: string; path: string; kind: string }>) {
		filePath = e.detail.path;
		fileArchivePath = null;
		fileTargetLine = null;
		panelMode = 'file';
	}

	function handleDirOpenDir(e: CustomEvent<{ prefix: string }>) {
		currentDirPrefix = e.detail.prefix;
	}

	function backToResults() {
		view = 'results';
	}

	// Derived: breadcrumb path and isDir flag
	$: breadcrumbPath = panelMode === 'dir' ? currentDirPrefix.replace(/\/$/, '') : filePath;
	$: breadcrumbIsDir = panelMode === 'dir';
</script>

<div class="page">
	{#if view === 'file'}
		<!-- ── File viewer ─────────────────────────────────────────────────────── -->
		<div class="topbar topbar--compact">
			<span class="logo">find-anything</span>
			<button
				class="tree-toggle"
				class:active={showTree}
				title="Toggle file tree"
				on:click={() => (showTree = !showTree)}
			>⊞</button>
			<div class="search-wrap">
				<SearchBox {query} {mode} on:change={handleSearchChange} />
			</div>
		</div>
		<div class="content content--full content--with-tree">
			{#if showTree}
				<div class="sidebar">
					<DirectoryTree
						source={fileSource}
						activePath={filePath}
						on:open={openFileFromTree}
					/>
				</div>
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
					<FileViewer
						source={fileSource}
						path={filePath}
						archivePath={fileArchivePath}
						targetLine={fileTargetLine}
						on:back={backToResults}
					/>
				{/if}
			</div>
		</div>
	{:else}
		<!-- ── Results ────────────────────────────────────────────────────────── -->
		<div class="topbar">
			<span class="logo">find-anything</span>
			{#if sources.length > 0}
				<SourceChips {sources} selected={selectedSources} on:change={handleSourceChange} />
			{/if}
			<div class="search-wrap">
				<SearchBox {query} {mode} on:change={handleSearchChange} />
			</div>
		</div>
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
		flex-wrap: wrap;
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
		padding: 0 16px;
		max-width: 960px;
		width: 100%;
		margin: 0 auto;
	}

	.content--full {
		max-width: 100%;
		padding: 0;
	}

	.content--with-tree {
		display: flex;
		flex-direction: row;
		overflow: hidden;
	}

	.sidebar {
		width: 240px;
		flex-shrink: 0;
		overflow: hidden;
		display: flex;
		flex-direction: column;
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
