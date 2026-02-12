<script lang="ts">
	import { onMount } from 'svelte';
	import SearchBox from '$lib/SearchBox.svelte';
	import SourceChips from '$lib/SourceChips.svelte';
	import ResultList from '$lib/ResultList.svelte';
	import FileViewer from '$lib/FileViewer.svelte';
	import { search, listSources } from '$lib/api';
	import type { SearchResult } from '$lib/api';

	// ── State ──────────────────────────────────────────────────────────────────

	type View = 'empty' | 'results' | 'file';

	let view: View = 'empty';
	let query = '';
	let mode = 'fuzzy';

	let sources: string[] = [];
	let selectedSources: string[] = [];

	let results: SearchResult[] = [];
	let totalResults = 0;
	let searching = false;
	let searchError: string | null = null;

	// File viewer state
	let fileSource = '';
	let filePath = '';
	let fileArchivePath: string | null = null;
	let fileTargetLine: number | null = null;

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
			view = 'empty';
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
	}

	function backToResults() {
		view = results.length > 0 ? 'results' : 'empty';
	}
</script>

<div class="page">
	{#if view === 'file'}
		<!-- ── File viewer ─────────────────────────────────────────────────────── -->
		<div class="topbar topbar--compact">
			<span class="logo">find-anything</span>
			<div class="search-wrap">
				<SearchBox {query} {mode} on:change={handleSearchChange} />
			</div>
		</div>
		<div class="content content--full">
			<FileViewer
				source={fileSource}
				path={filePath}
				archivePath={fileArchivePath}
				targetLine={fileTargetLine}
				on:back={backToResults}
			/>
		</div>
	{:else if view === 'empty'}
		<!-- ── Empty / landing ────────────────────────────────────────────────── -->
		<div class="landing">
			<h1 class="logo logo--large">find-anything</h1>
			{#if sources.length > 0}
				<div class="landing-chips">
					<SourceChips {sources} selected={selectedSources} on:change={handleSourceChange} />
				</div>
			{/if}
			<div class="landing-search">
				<SearchBox {query} {mode} on:change={handleSearchChange} />
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

	.logo--large {
		font-size: 32px;
		margin-bottom: 20px;
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

	/* ── Landing ────────────────────────────────────────────────────────────── */
	.landing {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		height: 100vh;
		padding: 24px;
	}

	.landing-chips {
		margin-bottom: 16px;
	}

	.landing-search {
		width: 100%;
		max-width: 560px;
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
