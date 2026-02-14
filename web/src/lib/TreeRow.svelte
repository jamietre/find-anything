<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import { listDir, listArchiveMembers } from '$lib/api';
	import type { DirEntry } from '$lib/api';

	export let source: string;
	export let entry: DirEntry;
	export let activePath: string | null = null;
	export let depth: number = 0;

	const dispatch = createEventDispatcher<{
		open: { source: string; path: string; kind: string; archivePath?: string };
	}>();

	let expanded = false;
	let children: DirEntry[] = [];
	let loaded = false;
	let loadError = false;

	// An archive file (kind='archive') can be expanded like a directory.
	$: isExpandable = entry.entry_type === 'dir' || entry.kind === 'archive';

	// Auto-expand directories and archives if activePath is a descendant or exact match.
	$: if (isExpandable && activePath) {
		// For directories: expand if activePath is inside this directory
		// For archives: expand if activePath matches exactly OR points to an archive member
		const shouldExpand = entry.entry_type === 'dir'
			? activePath.startsWith(entry.path)
			: activePath === entry.path || activePath.startsWith(entry.path + '::');

		if (shouldExpand && !expanded) {
			expandDir();
		}
	}

	async function expandDir() {
		if (!loaded) {
			try {
				const resp = entry.kind === 'archive'
					? await listArchiveMembers(source, entry.path)
					: await listDir(source, entry.path);
				children = resp.entries;
				loaded = true;
			} catch {
				loadError = true;
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
		// For composite paths ("archive.zip::member.txt"), split into path + archivePath.
		const i = entry.path.indexOf('::');
		if (i >= 0) {
			dispatch('open', {
				source,
				path: entry.path.slice(0, i),
				kind: entry.kind ?? 'text',
				archivePath: entry.path.slice(i + 2),
			});
		} else if (entry.kind === 'archive') {
			// Special case: clicking an archive file should expand it AND show its contents
			// in the right pane as a directory listing.
			if (!expanded) {
				expandDir();
			}
			dispatch('open', {
				source,
				path: entry.path,
				kind: 'archive',
				showAsDirectory: true,
			});
		} else {
			dispatch('open', {
				source,
				path: entry.path,
				kind: entry.kind ?? 'text',
			});
		}
	}
</script>

<li class="row-item">
	{#if isExpandable}
		<button class="row row--dir" style="padding-left: {8 + depth * 16}px" on:click={toggleDir}>
			<span class="icon">{expanded ? '▾' : '▸'}</span>
			<span class="name">{entry.name}</span>
		</button>
		{#if expanded}
			{#if loadError}
				<div class="load-error" style="padding-left: {8 + (depth + 1) * 16}px">Error loading</div>
			{:else if children.length === 0}
				<div class="empty-msg" style="padding-left: {8 + (depth + 1) * 16}px">Empty</div>
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

	.empty-msg {
		font-size: 12px;
		color: var(--text-muted);
		padding-top: 2px;
		padding-bottom: 2px;
		font-style: italic;
	}
</style>
