<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import { listDir } from '$lib/api';
	import type { DirEntry } from '$lib/api';

	export let source: string;
	export let entry: DirEntry;
	export let activePath: string | null = null;
	export let depth: number = 0;

	const dispatch = createEventDispatcher<{ open: { source: string; path: string; kind: string } }>();

	let expanded = false;
	let children: DirEntry[] = [];
	let loaded = false;
	let loadError = false;

	// Auto-expand this directory if activePath is a descendant.
	$: if (entry.entry_type === 'dir' && activePath && activePath.startsWith(entry.path)) {
		if (!expanded) {
			expandDir();
		}
	}

	async function expandDir() {
		if (!loaded) {
			try {
				const resp = await listDir(source, entry.path);
				children = resp.entries;
				loaded = true;
			} catch {
				loadError = true;
				return;
			}
		}
		expanded = true;
	}

	async function toggleDir() {
		if (!expanded) {
			await expandDir();
		} else {
			expanded = false;
		}
	}

	function openFile() {
		dispatch('open', {
			source,
			path: entry.path,
			kind: entry.kind ?? 'text',
		});
	}
</script>

<li class="row-item">
	{#if entry.entry_type === 'dir'}
		<button class="row row--dir" style="padding-left: {8 + depth * 16}px" on:click={toggleDir}>
			<span class="icon">{expanded ? '▾' : '▸'}</span>
			<span class="name">{entry.name}</span>
		</button>
		{#if expanded}
			{#if loadError}
				<div class="load-error" style="padding-left: {8 + (depth + 1) * 16}px">Error loading</div>
			{:else}
				<ul class="tree-list">
					{#each children as child (child.path)}
						<svelte:self
							source={source}
							entry={child}
							activePath={activePath}
							depth={depth + 1}
							on:open
						/>
					{/each}
				</ul>
			{/if}
		{/if}
	{:else}
		<button
			class="row row--file"
			class:active={entry.path === activePath}
			style="padding-left: {8 + depth * 16}px"
			on:click={openFile}
		>
			<span class="icon kind-icon" title={entry.kind}>·</span>
			<span class="name">{entry.name}</span>
		</button>
	{/if}
</li>

<style>
	.row-item {
		list-style: none;
	}

	.tree-list {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.row {
		display: flex;
		align-items: center;
		gap: 4px;
		width: 100%;
		background: none;
		border: none;
		cursor: pointer;
		padding-top: 2px;
		padding-bottom: 2px;
		padding-right: 8px;
		text-align: left;
		color: var(--text);
		font-size: 13px;
		white-space: nowrap;
		overflow: hidden;
	}

	.row:hover {
		background: var(--bg-hover, rgba(255, 255, 255, 0.06));
	}

	.row--file.active {
		background: var(--accent-subtle, rgba(88, 166, 255, 0.15));
		color: var(--accent, #58a6ff);
	}

	.icon {
		flex-shrink: 0;
		width: 14px;
		text-align: center;
		color: var(--text-muted);
		font-size: 11px;
	}

	.row--dir .icon {
		color: var(--text);
	}

	.name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.load-error {
		font-size: 12px;
		color: #f85149;
		padding-top: 2px;
		padding-bottom: 2px;
	}
</style>
