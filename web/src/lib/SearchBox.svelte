<script lang="ts">
	import { createEventDispatcher } from 'svelte';

	export let query = '';
	export let mode = 'fuzzy';

	const dispatch = createEventDispatcher<{
		change: { query: string; mode: string };
	}>();

	let debounceTimer: ReturnType<typeof setTimeout>;

	function handleInput() {
		clearTimeout(debounceTimer);
		debounceTimer = setTimeout(() => {
			dispatch('change', { query, mode });
		}, 200);
	}

	function handleModeChange() {
		dispatch('change', { query, mode });
	}

	export function focus() {
		inputEl?.focus();
	}

	let inputEl: HTMLInputElement;
</script>

<div class="search-box">
	<select bind:value={mode} on:change={handleModeChange} class="mode-select">
		<option value="fuzzy">Fuzzy</option>
		<option value="exact">Exact</option>
		<option value="regex">Regex</option>
	</select>
	<input
		bind:this={inputEl}
		bind:value={query}
		on:input={handleInput}
		type="text"
		placeholder="Search…"
		autocomplete="off"
		spellcheck="false"
		class="search-input"
	/>
</div>

<style>
	.search-box {
		display: flex;
		align-items: center;
		background: var(--bg-secondary);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		overflow: hidden;
	}

	.search-input {
		flex: 1;
		padding: 8px 12px;
		background: transparent;
		border: none;
		color: var(--text);
		outline: none;
	}

	.search-input::placeholder {
		color: var(--text-dim);
	}

	.mode-select {
		padding: 8px 10px;
		background: var(--bg-hover);
		border: none;
		border-right: 1px solid var(--border);
		color: var(--text-muted);
		cursor: pointer;
		outline: none;
	}

	.mode-select:hover {
		color: var(--text);
	}
</style>
