<script lang="ts">
	import { createEventDispatcher } from 'svelte';

	export let source: string;
	export let path: string;
	export let archivePath: string | null = null;
	/** Effective resolved base URL (server value overridden by user profile). */
	export let baseUrl: string | null = null;

	const dispatch = createEventDispatcher<{
		back: void;
		navigate: { type: 'dir'; prefix: string } | { type: 'file'; path: string; kind: string };
	}>();

	type Segment = {
		label: string;
		separator: '/' | '::' | null; // separator BEFORE this segment (null for first)
		action: { type: 'dir'; prefix: string } | { type: 'file'; path: string; kind: string } | { type: 'current' };
	};

	$: segments = computeSegments(path, archivePath);

	$: externalHref = baseUrl
		? baseUrl.replace(/\/+$/, '') + '/' +
		  path.replace(/^\/+/, '').split('/').map(encodeURIComponent).join('/')
		: null;

	function computeSegments(outerPath: string, innerPath: string | null): Segment[] {
		const outerParts = outerPath.split('/');
		const result: Segment[] = [];

		for (let i = 0; i < outerParts.length; i++) {
			const cumulative = outerParts.slice(0, i + 1).join('/');
			const isLast = i === outerParts.length - 1;
			const sep: '/' | '::' | null = i === 0 ? null : '/';

			if (isLast && !innerPath) {
				result.push({ label: outerParts[i], separator: sep, action: { type: 'current' } });
			} else if (isLast && innerPath) {
				// Last outer segment is an archive — clicking opens its FileViewer
				result.push({ label: outerParts[i], separator: sep, action: { type: 'file', path: cumulative, kind: 'archive' } });
			} else {
				result.push({ label: outerParts[i], separator: sep, action: { type: 'dir', prefix: cumulative + '/' } });
			}
		}

		if (innerPath) {
			const innerParts = innerPath.split('/');
			for (let i = 0; i < innerParts.length; i++) {
				const cumulativeInner = innerParts.slice(0, i + 1).join('/');
				const isLast = i === innerParts.length - 1;
				const sep: '/' | '::' | null = i === 0 ? '::' : '/';

				if (isLast) {
					result.push({ label: innerParts[i], separator: sep, action: { type: 'current' } });
				} else {
					result.push({ label: innerParts[i], separator: sep, action: { type: 'dir', prefix: `${outerPath}::${cumulativeInner}/` } });
				}
			}
		}

		return result;
	}

	function handleSegmentClick(seg: Segment) {
		if (seg.action.type === 'current') return;
		dispatch('navigate', seg.action);
	}
</script>

<div class="path-bar">
	<button class="back-btn" on:click={() => dispatch('back')}>← results</button>
	<button class="badge" on:click={() => dispatch('navigate', { type: 'dir', prefix: '' })}>{source}</button>
	<span class="path-plain">
		{#each segments as seg}
			{#if seg.separator}<span class="sep">{seg.separator}</span>{/if}
			{#if seg.action.type === 'current'}
				<span class="seg seg--current">{seg.label}</span>
			{:else}
				<button class="seg seg--link" on:click={() => handleSegmentClick(seg)}>{seg.label}</button>
			{/if}
		{/each}
		{#if externalHref}
			<a class="external-link" href={externalHref} target="_blank" rel="noopener noreferrer" title="Open in file manager">↗</a>
		{/if}
	</span>
</div>

<style>
	.path-bar {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 16px;
		background: var(--bg-secondary);
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
		min-height: 38px;
		overflow: hidden;
	}

	.back-btn {
		background: none;
		border: 1px solid var(--border);
		color: var(--text-muted);
		padding: 3px 10px;
		border-radius: var(--radius);
		font-size: 12px;
		flex-shrink: 0;
		cursor: pointer;
	}

	.back-btn:hover {
		border-color: var(--accent);
		color: var(--accent);
	}

	.badge {
		padding: 1px 8px;
		border-radius: 20px;
		background: var(--badge-bg);
		color: var(--badge-text);
		font-size: 11px;
		flex-shrink: 0;
		border: none;
		cursor: pointer;
	}

	.badge:hover {
		opacity: 0.75;
	}

	.path-plain {
		font-family: var(--font-mono);
		font-size: 12px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		flex: 1;
		min-width: 0;
		display: flex;
		align-items: baseline;
		gap: 0;
		color: var(--accent);
	}

	.sep {
		color: var(--text-dim);
		padding: 0 1px;
		user-select: none;
	}

	.seg {
		font-family: var(--font-mono);
		font-size: 12px;
		white-space: nowrap;
	}

	.seg--current {
		color: var(--accent);
	}

	.seg--link {
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		color: var(--text-muted);
	}

	.seg--link:hover {
		color: var(--accent);
		text-decoration: underline;
	}

	.external-link {
		margin-left: 6px;
		color: var(--text-dim);
		text-decoration: none;
		font-size: 11px;
		flex-shrink: 0;
	}

	.external-link:hover {
		color: var(--accent);
	}
</style>
