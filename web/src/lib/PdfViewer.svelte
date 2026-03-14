<script lang="ts">
	export let src: string;

	let loaded = false;
	// Reset loaded state whenever the source URL changes.
	$: { src; loaded = false; }
</script>

<div class="original-panel">
	{#if !loaded}<div class="pdf-loading"><div class="pdf-spinner"></div></div>{/if}
	<iframe {src} title="Original file" class="original-iframe"
		class:iframe-hidden={!loaded}
		on:load={() => loaded = true}></iframe>
</div>

<style>
	.original-panel {
		flex: 1;
		overflow: auto;
		display: flex;
		flex-direction: column;
		background: var(--bg);
	}

	.original-iframe {
		flex: 1;
		width: 100%;
		height: 100%;
		border: none;
		min-height: 400px;
	}

	.iframe-hidden {
		display: none;
	}

	.pdf-loading {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.pdf-spinner {
		width: 32px;
		height: 32px;
		border: 3px solid rgba(255, 255, 255, 0.08);
		border-top-color: var(--accent, #58a6ff);
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}

	@keyframes spin {
		to { transform: rotate(360deg); }
	}
</style>
