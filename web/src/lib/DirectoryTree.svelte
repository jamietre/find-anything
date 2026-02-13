<script lang="ts">
	import { onMount } from 'svelte';
	import { listDir } from '$lib/api';
	import type { DirEntry } from '$lib/api';
	import TreeRow from '$lib/TreeRow.svelte';

	export let source: string;
	/** Currently open file path — highlighted in the tree. */
	export let activePath: string | null = null;

	let roots: DirEntry[] = [];
	let loading = true;
	let error: string | null = null;

	onMount(async () => {
		try {
			const resp = await listDir(source, '');
			roots = resp.entries;
		} catch (e) {
			error = String(e);
		} finally {
			loading = false;
		}
	});
</script>

<div class="tree">
	{#if loading}
		<div class="tree-status">Loading…</div>
	{:else if error}
		<div class="tree-status tree-error">{error}</div>
	{:else if roots.length === 0}
		<div class="tree-status">No files indexed.</div>
	{:else}
		<ul class="tree-list">
			{#each roots as entry (entry.path)}
				<TreeRow {source} {entry} {activePath} depth={0} on:open />
			{/each}
		</ul>
	{/if}
</div>

<style>
	.tree {
		font-size: 13px;
		overflow-y: auto;
		height: 100%;
		padding: 4px 0;
		background: var(--bg-secondary);
		border-right: 1px solid var(--border);
	}

	.tree-status {
		padding: 12px;
		color: var(--text-muted);
	}

	.tree-error {
		color: #f85149;
	}

	.tree-list {
		list-style: none;
		margin: 0;
		padding: 0;
	}
</style>
