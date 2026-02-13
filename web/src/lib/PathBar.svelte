<script lang="ts">
	import { createEventDispatcher } from 'svelte';

	export let source: string;
	export let path: string;
	export let archivePath: string | null = null;
	/** Effective resolved base URL (server value overridden by user profile). */
	export let baseUrl: string | null = null;

	const dispatch = createEventDispatcher<{ back: void }>();

	$: displayText = archivePath ? `${path}::${archivePath}` : path;

	$: href = baseUrl
		? baseUrl.replace(/\/+$/, '') + '/' +
		  path.replace(/^\/+/, '').split('/').map(encodeURIComponent).join('/')
		: null;
</script>

<div class="path-bar">
	<button class="back-btn" on:click={() => dispatch('back')}>← results</button>
	<span class="badge">{source}</span>
	{#if href}
		<a
			class="path-link"
			{href}
			target="_blank"
			rel="noopener noreferrer"
			title={href}
		>{displayText}</a>
	{:else}
		<span class="path-plain" title={displayText}>{displayText}</span>
	{/if}
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
	}

	.path-link,
	.path-plain {
		font-family: var(--font-mono);
		font-size: 12px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		flex: 1;
		min-width: 0;
	}

	.path-link {
		color: var(--accent);
		text-decoration: none;
	}

	.path-link:hover {
		text-decoration: underline;
	}

	.path-plain {
		color: var(--accent);
	}
</style>
