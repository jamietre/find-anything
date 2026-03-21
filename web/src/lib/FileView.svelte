<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import { goto } from '$app/navigation';
	import SearchBox from '$lib/SearchBox.svelte';
	import AdvancedSearch from '$lib/AdvancedSearch.svelte';
	import PathBar from '$lib/PathBar.svelte';
	import FileViewer from '$lib/FileViewer.svelte';
	import DirListing from '$lib/DirListing.svelte';
	import type { LineSelection } from '$lib/lineSelection';
	import type { FileViewState } from '$lib/appState';
	import type { SearchScope, SearchMatchType } from '$lib/searchPrefixes';
	import SearchHelp from '$lib/SearchHelp.svelte';

	export let fileView: FileViewState;
	export let showTree: boolean;
	export let query: string;
	export let scope: SearchScope = 'line';
	export let matchType: SearchMatchType = 'fuzzy';
	export let searching: boolean;
	export let sources: string[];
	export let selectedSources: string[];
	export let selectedKinds: string[] = [];
	export let dateFrom = '';
	export let dateTo = '';
	export let caseSensitive = false;

	const dispatch = createEventDispatcher<{
		back: void;
		search: { query: string };
		filterChange: { sources: string[]; kinds: string[]; dateFrom?: number; dateTo?: number; caseSensitive: boolean; scope: SearchScope; matchType: SearchMatchType };
		treeToggle: void;
		openFileFromTree: { source: string; path: string; kind: string; archivePath?: string; showAsDirectory?: boolean };
		openDirFile: { source: string; path: string; kind: string; archivePath?: string };
		openDir: { prefix: string };
		lineselect: { selection: LineSelection };
		navigateDir: { prefix: string };
	}>();

	let isTyping = false;

	$: pathBarPath = fileView.panelMode === 'dir' ? fileView.dirPrefix : fileView.file.outer;
</script>

<div class="topbar">
	<span class="logo" aria-label="find-anything">
		<span class="logo-full">find-anything</span><span class="logo-short" aria-hidden="true">fa</span>
	</span>
	<button
		class="tree-toggle"
		class:active={showTree}
		data-tooltip="Toggle file tree"
		on:click={() => dispatch('treeToggle')}
	>◫</button>
	<div class="help-wrap-outer"><SearchHelp /></div>
	<div class="search-wrap">
		<SearchBox
			{query}
			{searching}
			bind:isTyping
			on:change={(e) => dispatch('search', { query: e.detail.query })}
		/>
	</div>
	{#if sources.length > 0}
		<div class="advanced-wrap">
			<AdvancedSearch
				{sources}
				{selectedSources}
				{selectedKinds}
				{dateFrom}
				{dateTo}
				{caseSensitive}
				{scope}
				{matchType}
				on:change={(e) => dispatch('filterChange', e.detail)}
			/>
		</div>
	{/if}
	<button class="gear-btn" on:click={() => goto('/settings')}>⚙</button>
</div>

<div class="viewer-wrap">
	<PathBar
		source={fileView.source}
		path={pathBarPath}
		archivePath={fileView.panelMode === 'file' ? fileView.file.inner ?? null : null}
		on:back={() => dispatch('back')}
		on:navigate={(e) => {
			if (e.detail.type === 'dir') {
				dispatch('openDir', { prefix: e.detail.prefix });
			} else {
				dispatch('openFileFromTree', { source: fileView.source, path: e.detail.path, kind: e.detail.kind });
			}
		}}
	/>
	{#if fileView.panelMode === 'dir'}
		<DirListing
			source={fileView.source}
			prefix={fileView.dirPrefix}
			on:openFile={(e) => dispatch('openDirFile', e.detail)}
			on:openDir={(e) => dispatch('openDir', e.detail)}
		/>
	{:else}
		{#key `${fileView.source}:${fileView.file.full}`}
			<FileViewer
				source={fileView.source}
				path={fileView.file.outer}
				archivePath={fileView.file.inner}
				selection={fileView.selection}
				preferOriginal={fileView.selection.length === 0}
				on:lineselect={(e) => dispatch('lineselect', e.detail)}
				on:open={(e) => dispatch('openDirFile', e.detail)}
				on:navigateDir={(e) => dispatch('openDir', e.detail)}
				on:navigate={(e) => dispatch('openFileFromTree', { source: fileView.source, path: e.detail.path, kind: 'unknown' })}
			/>
		{/key}
	{/if}
</div>

<style>
	.topbar {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 16px;
		background: var(--bg-secondary);
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	.logo {
		font-size: 14px;
		font-weight: 600;
		color: var(--text);
		white-space: nowrap;
		flex-shrink: 0;
	}

	.logo-short { display: none; }

	.help-wrap-outer { display: contents; }
	.advanced-wrap { display: contents; }

	.search-wrap {
		min-width: 260px;
		flex: 1;
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
		position: relative;
	}

	.tree-toggle:hover {
		background: var(--bg-hover, rgba(255, 255, 255, 0.08));
		color: var(--text);
	}

	.tree-toggle.active {
		color: var(--accent, #58a6ff);
	}

	.tree-toggle[data-tooltip]::after {
		content: attr(data-tooltip);
		position: absolute;
		top: calc(100% + 4px);
		left: 50%;
		transform: translateX(-50%);
		white-space: nowrap;
		background: var(--bg-secondary);
		border: 1px solid var(--border);
		color: var(--text-muted);
		padding: 2px 6px;
		border-radius: 3px;
		font-size: 11px;
		opacity: 0;
		pointer-events: none;
		transition: opacity 0.1s;
		z-index: 100;
	}

	.tree-toggle[data-tooltip]:hover::after {
		opacity: 1;
	}

	.gear-btn {
		background: none;
		border: none;
		cursor: pointer;
		color: var(--text-muted);
		font-size: 20px;
		padding: 2px 6px;
		border-radius: 4px;
		line-height: 1;
		flex-shrink: 0;
	}

	.gear-btn:hover {
		background: var(--bg-hover, rgba(255, 255, 255, 0.08));
		color: var(--text);
	}

	.viewer-wrap {
		flex: 1;
		overflow: hidden;
		display: flex;
		flex-direction: column;
	}

	@media (max-width: 768px) {
		.topbar { gap: 6px; padding: 6px 10px; }
		.tree-toggle { display: none; }
		.help-wrap-outer { display: none; }
		.logo { order: 1; }
		.logo-full { display: none; }
		.logo-short { display: inline; }
		.search-wrap { order: 2; flex: 1 1 0; min-width: 0; }
		.advanced-wrap { order: 3; display: block; }
		.gear-btn { order: 4; min-width: 36px; min-height: 36px; display: flex; align-items: center; justify-content: center; }
	}
</style>
