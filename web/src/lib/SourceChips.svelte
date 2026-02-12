<script lang="ts">
	import { createEventDispatcher } from 'svelte';

	/** All available source names. */
	export let sources: string[] = [];
	/** Currently active sources (empty = all). */
	export let selected: string[] = [];

	const dispatch = createEventDispatcher<{ change: string[] }>();

	function toggle(source: string) {
		if (selected.includes(source)) {
			selected = selected.filter((s) => s !== source);
		} else {
			selected = [...selected, source];
		}
		dispatch('change', selected);
	}
</script>

<div class="chips">
	{#each sources as source}
		<button
			class="chip"
			class:active={selected.includes(source)}
			on:click={() => toggle(source)}
			title={selected.includes(source) ? `Remove ${source} filter` : `Filter to ${source}`}
		>
			{source}
			{#if selected.includes(source)}<span class="x" aria-hidden="true">✕</span>{/if}
		</button>
	{/each}
</div>

<style>
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}

	.chip {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 3px 10px;
		border-radius: 20px;
		border: 1px solid var(--border);
		background: var(--chip-bg);
		color: var(--text-muted);
		font-size: 12px;
		transition:
			background 0.1s,
			color 0.1s,
			border-color 0.1s;
	}

	.chip:hover {
		border-color: var(--accent);
		color: var(--text);
	}

	.chip.active {
		background: var(--chip-active);
		border-color: var(--chip-active);
		color: #fff;
	}

	.x {
		font-size: 10px;
		opacity: 0.8;
	}
</style>
