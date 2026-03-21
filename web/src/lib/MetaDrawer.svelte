<script lang="ts">
	/** Whether the drawer starts open. */
	export let initialOpen: boolean = false;

	let open = initialOpen;
</script>

<div class="meta-drawer">
	<button class="drawer-toggle" on:click={() => (open = !open)} title={open ? 'Hide metadata' : 'Show metadata'}>
		<svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
			{#if open}
				<polyline points="4,2 8,7 4,12" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>
				<polyline points="8,2 12,7 8,12" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>
			{:else}
				<polyline points="10,2 6,7 10,12" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>
				<polyline points="6,2 2,7 6,12" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>
			{/if}
		</svg>
	</button>
	<div class="drawer-content drawer-always-open" class:drawer-open={open}>
		<div class="drawer-inner">
			<slot />
		</div>
	</div>
</div>

<style>
	.meta-drawer {
		display: flex;
		flex-direction: row;
		flex-shrink: 0;
	}

	.drawer-toggle {
		width: 40px;
		flex-shrink: 0;
		align-self: stretch;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--bg-secondary);
		border: none;
		border-left: 1px solid var(--border, rgba(255, 255, 255, 0.1));
		color: var(--text-dim);
		cursor: pointer;
		padding: 0;
		transition: color 0.15s;
	}

	.drawer-toggle:hover {
		color: var(--accent);
	}

	.drawer-content {
		width: 0;
		overflow: hidden;
		transition: width 0.2s ease;
		flex-shrink: 0;
	}

	.drawer-content.drawer-open {
		width: 300px;
	}

	.drawer-inner {
		width: 300px;
		height: 100%;
		overflow-y: auto;
		padding: 12px 16px;
		font-family: var(--font-mono);
		font-size: 12px;
		color: var(--text-muted);
		background: var(--bg-secondary);
		border-left: 1px solid var(--border, rgba(255, 255, 255, 0.1));
		box-sizing: border-box;
	}

	@media (max-width: 768px) {
		.meta-drawer { flex-direction: column; }
		.drawer-toggle { display: none; }
		/* Always show content on mobile regardless of toggle state */
		.drawer-content.drawer-always-open {
			width: auto !important;
			overflow: visible;
			border-top: 1px solid var(--border);
		}
		.drawer-inner {
			width: auto;
			height: auto;
			overflow-y: auto;
			max-height: 40vh;
			border-left: none;
		}
	}
</style>
