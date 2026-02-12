<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import type { SearchResult } from '$lib/api';
	import SearchResultItem from '$lib/SearchResult.svelte';

	export let results: SearchResult[] = [];

	const dispatch = createEventDispatcher<{ open: SearchResult }>();
</script>

<div class="result-list">
	{#each results as result (result.source + result.path + result.line_number)}
		<SearchResultItem {result} on:open={(e) => dispatch('open', e.detail)} />
	{/each}

	{#if results.length === 0}
		<p class="empty">No results.</p>
	{/if}
</div>

<style>
	.result-list {
		padding: 12px 0;
	}

	.empty {
		color: var(--text-muted);
		padding: 24px;
		text-align: center;
	}
</style>
