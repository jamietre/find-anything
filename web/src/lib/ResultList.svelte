<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import type { SearchResult } from '$lib/api';
	import SearchResultItem from '$lib/SearchResult.svelte';

	export let results: SearchResult[] = [];
	export let totalResults = 0;
	export let nextBatchSize = 0;
	export let isLoadingMore = false;
	export let searching = false;

	const dispatch = createEventDispatcher<{ open: SearchResult; loadmore: void }>();

	$: hasMore = results.length < totalResults;
	$: remainingCount = totalResults - results.length;

	function handleLoadMore() {
		dispatch('loadmore');
	}
</script>

<div class="result-list" class:searching>
	<div class="result-container">
		{#each results as result (result.source + result.path + (result.archive_path ?? '') + result.line_number)}
			<SearchResultItem {result} on:open={(e) => dispatch('open', e.detail)} />
		{/each}
		{#if results.length === 0 && !searching}
			<p class="empty">No results.</p>
		{/if}
	</div>

	{#if hasMore}
		<div class="load-more">
			{#if isLoadingMore}
				<div class="loading">
					<div class="spinner">
						<svg viewBox="0 0 24 24" fill="none">
							<circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3" opacity="0.25"/>
							<path d="M12 2a10 10 0 0 1 10 10" stroke="currentColor" stroke-width="3" stroke-linecap="round"/>
						</svg>
					</div>
					<span>Loading {nextBatchSize.toLocaleString()} more results...</span>
				</div>
			{:else}
				<button on:click={handleLoadMore} class="load-more-btn">
					<span class="ellipsis">⋯</span>
					<span class="count">Load {nextBatchSize.toLocaleString()} more</span>
					<span class="remaining">({remainingCount.toLocaleString()} total remaining)</span>
				</button>
			{/if}
		</div>
	{:else if results.length > 0 && results.length === totalResults}
		<div class="all-loaded">
			<span class="checkmark">✓</span> Showing all {totalResults.toLocaleString()} results
		</div>
	{/if}
</div>

<style>
	.result-list {
		padding: 12px 0;
		overflow-x: hidden;
		width: 100%;
		max-width: 100%;
	}

	.result-container {
		min-width: 0;
		max-width: 100%;
		transition: opacity 0.2s ease-in-out;
	}

	.result-list.searching .result-container {
		opacity: 0.5;
	}

	.empty {
		color: var(--text-muted);
		padding: 24px;
		text-align: center;
	}

	.load-more {
		padding: 16px 12px;
		text-align: center;
		border-top: 1px solid var(--border);
		grid-column: 1;
		grid-row: 2;
	}

	.load-more-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		padding: 10px 20px;
		background: var(--bg-secondary);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		color: var(--text-muted);
		cursor: pointer;
		font-size: 14px;
		transition: all 0.15s ease;
		width: 100%;
	}

	.load-more-btn:hover {
		background: var(--bg-hover);
		border-color: var(--accent-muted);
		color: var(--text);
	}

	.load-more-btn .ellipsis {
		font-size: 20px;
		line-height: 1;
		color: var(--accent);
	}

	.load-more-btn .count {
		font-weight: 500;
		color: var(--accent);
	}

	.load-more-btn .remaining {
		color: var(--text-dim);
		font-size: 13px;
	}

	.loading {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 10px;
		color: var(--text-muted);
		font-size: 14px;
	}

	.loading .spinner {
		width: 16px;
		height: 16px;
	}

	.loading .spinner svg {
		width: 100%;
		height: 100%;
		color: var(--accent);
		animation: spin 0.8s linear infinite;
	}

	.all-loaded {
		padding: 16px 12px;
		text-align: center;
		color: var(--text-dim);
		font-size: 13px;
		border-top: 1px solid var(--border);
		grid-column: 1;
		grid-row: 2;
	}

	.all-loaded .checkmark {
		color: var(--accent);
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}
</style>
