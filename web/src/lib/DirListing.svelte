<script lang="ts">
	import { onMount } from 'svelte';
	import { createEventDispatcher } from 'svelte';
	import { listDir } from '$lib/api';
	import type { DirEntry } from '$lib/api';

	export let source: string;
	export let prefix: string; // "" for root, "foo/bar/" for subdirectory

	const dispatch = createEventDispatcher<{
		openFile: { source: string; path: string; kind: string };
		openDir: { prefix: string };
	}>();

	let entries: DirEntry[] = [];
	let loading = true;
	let error: string | null = null;

	// Reload when prefix changes.
	$: load(source, prefix);

	async function load(_source: string, _prefix: string) {
		loading = true;
		error = null;
		try {
			const resp = await listDir(_source, _prefix);
			entries = resp.entries;
		} catch (e) {
			error = String(e);
		} finally {
			loading = false;
		}
	}

	function formatSize(bytes: number | undefined): string {
		if (bytes == null) return '';
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
	}

	function formatDate(mtime: number | undefined): string {
		if (mtime == null) return '';
		return new Date(mtime * 1000).toLocaleDateString();
	}
</script>

<div class="listing">
	{#if loading}
		<div class="status">Loading…</div>
	{:else if error}
		<div class="status error">{error}</div>
	{:else if entries.length === 0}
		<div class="status">Empty directory.</div>
	{:else}
		<table class="table">
			<thead>
				<tr>
					<th class="col-name">Name</th>
					<th class="col-kind">Kind</th>
					<th class="col-size">Size</th>
					<th class="col-date">Modified</th>
				</tr>
			</thead>
			<tbody>
				{#each entries as entry (entry.path)}
					<tr
						class="row"
						class:row--dir={entry.entry_type === 'dir'}
						on:click={() =>
							entry.entry_type === 'dir'
								? dispatch('openDir', { prefix: entry.path })
								: dispatch('openFile', { source, path: entry.path, kind: entry.kind ?? 'text' })}
					>
						<td class="col-name">
							<span class="icon">{entry.entry_type === 'dir' ? '▸' : '·'}</span>
							<span class="name">{entry.name}{entry.entry_type === 'dir' ? '/' : ''}</span>
						</td>
						<td class="col-kind">{entry.kind ?? ''}</td>
						<td class="col-size">{formatSize(entry.size)}</td>
						<td class="col-date">{formatDate(entry.mtime)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</div>

<style>
	.listing {
		height: 100%;
		overflow-y: auto;
	}

	.status {
		padding: 24px;
		color: var(--text-muted);
		text-align: center;
	}

	.status.error {
		color: #f85149;
	}

	.table {
		width: 100%;
		border-collapse: collapse;
		font-size: 13px;
	}

	thead th {
		padding: 6px 12px;
		text-align: left;
		color: var(--text-muted);
		font-weight: 500;
		border-bottom: 1px solid var(--border);
		background: var(--bg-secondary);
		position: sticky;
		top: 0;
		white-space: nowrap;
	}

	.row {
		cursor: pointer;
	}

	.row:hover td {
		background: var(--bg-hover, rgba(255, 255, 255, 0.04));
	}

	.row td {
		padding: 5px 12px;
		border-bottom: 1px solid var(--border);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.col-name {
		width: 100%;
	}

	.col-kind,
	.col-size,
	.col-date {
		color: var(--text-muted);
		text-align: right;
		min-width: 60px;
	}

	.icon {
		display: inline-block;
		width: 14px;
		text-align: center;
		margin-right: 4px;
		color: var(--text-muted);
		font-size: 11px;
	}

	.row--dir .name {
		color: var(--accent, #58a6ff);
	}

	.row--dir .icon {
		color: var(--accent, #58a6ff);
	}
</style>
