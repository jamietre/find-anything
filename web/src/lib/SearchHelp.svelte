<script lang="ts">
	import SearchHelpContent from './SearchHelpContent.svelte';
	export let open = false;
	function toggle() { open = !open; }
	function close() { open = false; }
</script>

<div class="help-wrap">
	<button
		class="help-btn"
		class:active={open}
		data-tooltip={open ? null : 'Search syntax help'}
		on:click={toggle}
		aria-label="Search syntax help"
	>?</button>
	{#if open}
		<!-- svelte-ignore a11y-no-static-element-interactions -->
		<!-- svelte-ignore a11y-click-events-have-key-events -->
		<div class="help-backdrop" on:click={close}></div>
		<div class="help-popup" role="dialog" aria-label="Search syntax help">
			<SearchHelpContent />
		</div>
	{/if}
</div>

<style>
	.help-wrap {
		position: relative;
		flex-shrink: 0;
	}

	.help-btn {
		background: none;
		border: 1px solid var(--text-muted);
		cursor: pointer;
		color: var(--text-muted);
		font-size: 11px;
		font-weight: 700;
		width: 18px;
		height: 18px;
		border-radius: 50%;
		padding: 0;
		line-height: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		position: relative;
	}

	.help-btn[data-tooltip]::after {
		content: attr(data-tooltip);
		position: absolute;
		top: calc(100% + 4px);
		left: 50%;
		transform: translateX(-50%);
		white-space: nowrap;
		background: var(--bg-secondary);
		border: 1px solid var(--border);
		color: var(--text-muted);
		padding: 2px 6px;
		border-radius: 3px;
		font-size: 11px;
		opacity: 0;
		pointer-events: none;
		transition: opacity 0.1s;
		z-index: 100;
	}

	.help-btn[data-tooltip]:hover::after { opacity: 1; }

	.help-btn:hover {
		background: var(--bg-hover, rgba(255, 255, 255, 0.08));
		color: var(--text);
		border-color: var(--text);
	}

	.help-btn.active {
		color: var(--accent, #58a6ff);
		border-color: var(--accent, #58a6ff);
	}

	.help-backdrop {
		position: fixed;
		inset: 0;
		z-index: 199;
	}

	.help-popup {
		position: absolute;
		top: calc(100% + 8px);
		left: 0;
		z-index: 200;
		background: var(--bg-secondary);
		border: 1px solid var(--border);
		border-radius: 6px;
		min-width: 300px;
		max-height: calc(100vh - 80px);
		overflow-y: auto;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
		font-size: 13px;
	}
</style>
