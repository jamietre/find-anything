<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import type { SearchResult } from '$lib/api';
	import SearchResultItem from '$lib/SearchResult.svelte';

	export let results: SearchResult[] = [];
	export let searching = false;

	const dispatch = createEventDispatcher<{ open: SearchResult }>();
</script>

<div class="result-list" class:searching>
	{#if results.length === 0 && !searching}
		<p class="empty">No results.</p>
	{:else}
		{#each results as result (`${result.source}:${result.path}:${result.archive_path ?? ''}:${result.line_number}`)}
			<div class="result-pad">
				<SearchResultItem {result} on:open={(e) => dispatch('open', e.detail)} />
			</div>
		{/each}
	{/if}
</div>

<style>
	.result-list {
		transition: opacity 0.2s ease-in-out;
	}

	.result-list.searching {
		opacity: 0.5;
		filter: blur(2px);
		pointer-events: none;
	}

	.result-pad {
		padding: 6px 0 0;
	}

	.result-pad:last-child {
		padding-bottom: 6px;
	}

	.empty {
		color: var(--text-muted);
		padding: 24px;
		text-align: center;
	}
</style>
