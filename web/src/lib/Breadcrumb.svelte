<script lang="ts">
	import { createEventDispatcher } from 'svelte';

	/** Full path to the current file or directory. */
	export let path: string;
	/** Whether the final segment is a directory (adds trailing "/"). */
	export let isDir = false;

	const dispatch = createEventDispatcher<{ navigate: { prefix: string } }>();

	interface Segment {
		label: string;
		prefix: string; // directory path ending with "/"
	}

	$: segments = buildSegments(path);

	function buildSegments(p: string): Segment[] {
		const parts = p.split('/').filter(Boolean);
		const segs: Segment[] = [];
		let cumulative = '';
		for (const part of parts) {
			cumulative += part + '/';
			segs.push({ label: part, prefix: cumulative });
		}
		return segs;
	}

	function navigate(seg: Segment) {
		dispatch('navigate', { prefix: seg.prefix });
	}
</script>

<nav class="breadcrumb" aria-label="File path">
	<span class="root" on:click={() => dispatch('navigate', { prefix: '' })} role="button" tabindex="0" on:keydown={(e) => e.key === 'Enter' && dispatch('navigate', { prefix: '' })}>
		/
	</span>
	{#each segments as seg, i (seg.prefix)}
		<span class="sep">/</span>
		{#if i < segments.length - 1 || isDir}
			<!-- Directory segment — clickable -->
			<button class="seg seg--dir" on:click={() => navigate(seg)}>
				{seg.label}
			</button>
		{:else}
			<!-- Final file segment — not a link -->
			<span class="seg seg--file">{seg.label}</span>
		{/if}
	{/each}
</nav>

<style>
	.breadcrumb {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 0;
		font-size: 13px;
		color: var(--text-muted);
		padding: 6px 16px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-secondary);
		min-height: 32px;
		overflow: hidden;
	}

	.root {
		cursor: pointer;
		color: var(--text-muted);
		padding: 0 2px;
		border-radius: 3px;
	}

	.root:hover {
		color: var(--accent, #58a6ff);
	}

	.sep {
		color: var(--border);
		margin: 0 1px;
		user-select: none;
	}

	.seg {
		background: none;
		border: none;
		font: inherit;
		padding: 0 2px;
		border-radius: 3px;
		white-space: nowrap;
	}

	.seg--dir {
		cursor: pointer;
		color: var(--text-muted);
	}

	.seg--dir:hover {
		color: var(--accent, #58a6ff);
		background: var(--bg-hover, rgba(255, 255, 255, 0.06));
	}

	.seg--file {
		color: var(--text);
		font-weight: 500;
	}
</style>
