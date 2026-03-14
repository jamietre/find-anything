<script lang="ts">
	import { createEventDispatcher } from 'svelte';

	export let fileState: 'normal' | 'deleted' | 'renamed' | 'modified';
	export let renamedTo: string | null = null;
	export let indexingError: string | null = null;

	const dispatch = createEventDispatcher<{
		navigate: { path: string };
		dismiss: void;
		reload: void;
	}>();
</script>

{#if fileState === 'deleted'}
	<div class="file-status-banner deleted-banner">
		<span>This file has been deleted from the index.</span>
	</div>
{:else if fileState === 'renamed' && renamedTo}
	<div class="file-status-banner renamed-banner">
		Renamed to
		<button class="banner-btn" on:click={() => dispatch('navigate', { path: renamedTo ?? '' })}>{renamedTo}</button>
		<button class="banner-dismiss" on:click={() => dispatch('dismiss')} aria-label="Dismiss">✕</button>
	</div>
{:else if fileState === 'modified'}
	<div class="file-status-banner modified-banner">
		<span>Content has changed.</span>
		<button class="banner-btn" on:click={() => dispatch('reload')}>Reload</button>
		<button class="banner-dismiss" on:click={() => dispatch('dismiss')} aria-label="Dismiss">✕</button>
	</div>
{/if}
{#if indexingError}
	<div class="indexing-error-banner">
		⚠ Indexing error: <span class="error-text">{indexingError}</span>
	</div>
{/if}

<style>
	.file-status-banner {
		padding: 8px 16px;
		font-size: 12px;
		display: flex;
		align-items: center;
		gap: 8px;
		flex-shrink: 0;
		border-bottom: 1px solid;
	}

	.deleted-banner {
		background: rgba(248, 81, 73, 0.12);
		border-color: rgba(248, 81, 73, 0.3);
		color: #f85149;
	}

	.modified-banner {
		background: rgba(230, 162, 60, 0.1);
		border-color: rgba(230, 162, 60, 0.25);
		color: #e6a23c;
	}

	.renamed-banner {
		background: rgba(88, 166, 255, 0.1);
		border-color: rgba(88, 166, 255, 0.25);
		color: var(--accent, #58a6ff);
	}

	.banner-btn {
		background: none;
		border: none;
		padding: 0;
		font: inherit;
		font-size: 12px;
		color: inherit;
		cursor: pointer;
		text-decoration: underline;
	}

	.banner-dismiss {
		background: none;
		border: none;
		padding: 0 0 0 4px;
		font-size: 12px;
		color: inherit;
		opacity: 0.6;
		cursor: pointer;
		margin-left: auto;
	}

	.banner-dismiss:hover {
		opacity: 1;
	}

	.indexing-error-banner {
		padding: 8px 16px;
		background: rgba(230, 162, 60, 0.12);
		border-bottom: 1px solid rgba(230, 162, 60, 0.3);
		color: #e6a23c;
		font-size: 12px;
		display: flex;
		align-items: baseline;
		gap: 6px;
		flex-shrink: 0;
	}

	.error-text {
		color: var(--text-muted);
		font-family: var(--font-mono);
		word-break: break-all;
	}
</style>
