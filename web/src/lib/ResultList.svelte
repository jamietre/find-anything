<script lang="ts">
	import { createEventDispatcher, tick } from 'svelte';
	import type { SearchResult } from '$lib/api';
	import SearchResultItem from '$lib/SearchResult.svelte';

	export let results: SearchResult[] = [];
	export let searching = false;

	const dispatch = createEventDispatcher<{ open: SearchResult }>();

	let displayedResults: SearchResult[] = [];
	let incomingResults: SearchResult[] = [];
	let showCurrent = true;

	$: {
		// When results change, prepare incoming results and flip visibility
		if (results !== displayedResults) {
			incomingResults = results;
			tick().then(() => {
				// After incoming results are rendered (but hidden), flip visibility
				showCurrent = !showCurrent;
				displayedResults = results;
			});
		}
	}
</script>

<div class="result-list" class:searching>
	<div class="result-container" class:visible={showCurrent} class:hidden={!showCurrent}>
		{#each displayedResults as result (result.source + result.path + (result.archive_path ?? '') + result.line_number)}
			<SearchResultItem {result} on:open={(e) => dispatch('open', e.detail)} />
		{/each}
		{#if displayedResults.length === 0}
			<p class="empty">No results.</p>
		{/if}
	</div>
	<div class="result-container" class:visible={!showCurrent} class:hidden={showCurrent}>
		{#each incomingResults as result (result.source + result.path + (result.archive_path ?? '') + result.line_number)}
			<SearchResultItem {result} on:open={(e) => dispatch('open', e.detail)} />
		{/each}
		{#if incomingResults.length === 0}
			<p class="empty">No results.</p>
		{/if}
	</div>
</div>

<style>
	.result-list {
		padding: 12px 0;
		display: grid;
		grid-template-columns: 1fr;
	}

	.result-container {
		grid-column: 1;
		grid-row: 1;
		transition: opacity 0.15s ease-in-out;
	}

	.result-container.visible {
		opacity: 1;
		z-index: 1;
	}

	.result-container.hidden {
		opacity: 0;
		z-index: 0;
		pointer-events: none;
	}

	.result-list.searching .result-container.visible {
		filter: blur(2px);
		opacity: 0.6;
		transition: opacity 0.15s ease-in-out, filter 0.15s ease-in-out;
	}

	.empty {
		color: var(--text-muted);
		padding: 24px;
		text-align: center;
	}
</style>
