<script lang="ts">
	import { createVirtualizer } from '@tanstack/svelte-virtual';
	import { get } from 'svelte/store';
	import { createEventDispatcher } from 'svelte';
	import type { SearchResult } from '$lib/api';
	import SearchResultItem from '$lib/SearchResult.svelte';

	export let results: SearchResult[] = [];
	export let totalResults = 0;
	export let isLoadingMore = false;
	export let searching = false;

	const dispatch = createEventDispatcher<{ open: SearchResult; loadmore: void }>();

	let listEl: HTMLDivElement;

	$: hasMore = results.length < totalResults;
	// +1 for the loader sentinel row when there are more results
	$: count = results.length + (hasMore ? 1 : 0);

	// Recreating when count changes is fine — TanStack reads scroll position from
	// the DOM element directly, so listEl.scrollTop is preserved across recreations.
	$: virtualizer = createVirtualizer<HTMLDivElement, HTMLDivElement>({
		count,
		getScrollElement: () => listEl,
		estimateSize: (i) => (i === results.length ? 56 : 100),
		overscan: 5,
	});

	$: virtualItems = $virtualizer.getVirtualItems();

	// Trigger infinite load when the last visible item is within 3 of the end
	$: {
		const last = virtualItems[virtualItems.length - 1];
		if (last && last.index >= results.length - 3 && hasMore && !isLoadingMore) {
			dispatch('loadmore');
		}
	}

	// Svelte action: measures item height on mount, re-measures when height changes
	// (e.g. when context lines lazy-load and expand the card)
	function measureEl(node: HTMLElement) {
		const el = node as HTMLDivElement;
		get(virtualizer).measureElement(el);
		const ro = new ResizeObserver(() => get(virtualizer).measureElement(el));
		ro.observe(node);
		return { destroy: () => ro.disconnect() };
	}
</script>

<div class="result-list" class:searching>
	{#if results.length === 0 && !searching}
		<p class="empty">No results.</p>
	{:else}
		<div class="scroll-container" bind:this={listEl}>
			<div style="position: relative; height: {$virtualizer.getTotalSize()}px">
				{#each virtualItems as row (row.index)}
					<div
						use:measureEl
						style="position: absolute; top: {row.start}px; left: 0; width: 100%"
					>
						{#if row.index < results.length}
							<div class="result-pad">
								<SearchResultItem
									result={results[row.index]}
									on:open={(e) => dispatch('open', e.detail)}
								/>
							</div>
						{:else}
							<div class="loader-row">
								{#if isLoadingMore}
									<div class="spinner">
										<svg viewBox="0 0 24 24" fill="none">
											<circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3" opacity="0.25"/>
											<path d="M12 2a10 10 0 0 1 10 10" stroke="currentColor" stroke-width="3" stroke-linecap="round"/>
										</svg>
									</div>
									<span>Loading more results…</span>
								{/if}
							</div>
						{/if}
					</div>
				{/each}
			</div>
		</div>
	{/if}
</div>

<style>
	.result-list {
		display: flex;
		flex-direction: column;
		min-height: 0;
		flex: 1;
	}

	.scroll-container {
		flex: 1;
		overflow-y: auto;
		overflow-x: hidden;
		transition: opacity 0.2s ease-in-out;
	}

	.result-list.searching .scroll-container {
		opacity: 0.5;
		filter: blur(2px);
	}

	.result-pad {
		padding: 6px 0 0;
	}

	.result-pad:last-child {
		padding-bottom: 6px;
	}

	.loader-row {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 10px;
		height: 56px;
		color: var(--text-muted);
		font-size: 14px;
	}

	.loader-row .spinner {
		width: 16px;
		height: 16px;
		flex-shrink: 0;
	}

	.loader-row .spinner svg {
		width: 100%;
		height: 100%;
		color: var(--accent);
		animation: spin 0.8s linear infinite;
	}

	.empty {
		color: var(--text-muted);
		padding: 24px;
		text-align: center;
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}
</style>
