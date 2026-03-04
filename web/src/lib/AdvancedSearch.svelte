<script lang="ts">
	import { createEventDispatcher } from 'svelte';
	import { clickOutside } from '$lib/clickOutside';

	/** All available source names. */
	export let sources: string[] = [];
	/** Currently active sources (empty = all). */
	export let selectedSources: string[] = [];
	/** Current date-from value as ISO string (YYYY-MM-DD), or empty. */
	export let dateFrom = '';
	/** Current date-to value as ISO string (YYYY-MM-DD), or empty. */
	export let dateTo = '';

	const dispatch = createEventDispatcher<{
		change: { sources: string[]; dateFrom?: number; dateTo?: number };
	}>();

	let isOpen = false;

	// Draft state — what the user is currently editing inside the panel.
	let draftSources: string[] = [];
	let draftFrom = '';
	let draftTo = '';

	// Sync draft from props whenever the panel opens.
	function openPanel() {
		draftSources = [...selectedSources];
		draftFrom = dateFrom;
		draftTo = dateTo;
		isOpen = true;
	}

	function isoToUnix(iso: string): number | undefined {
		if (!iso) return undefined;
		const ms = Date.parse(iso + 'T00:00:00Z');
		return isNaN(ms) ? undefined : Math.floor(ms / 1000);
	}

	function apply() {
		dispatch('change', {
			sources: draftSources,
			dateFrom: isoToUnix(draftFrom),
			dateTo: isoToUnix(draftTo)
		});
		isOpen = false;
	}

	function clearAll() {
		draftSources = [];
		draftFrom = '';
		draftTo = '';
		dispatch('change', { sources: [] });
		isOpen = false;
	}

	function toggleDraftSource(source: string) {
		if (draftSources.includes(source)) {
			draftSources = draftSources.filter((s) => s !== source);
		} else {
			draftSources = [...draftSources, source];
		}
	}

	// Whether the draft differs from what's currently applied (props).
	$: isDirty =
		JSON.stringify(draftSources.slice().sort()) !== JSON.stringify(selectedSources.slice().sort()) ||
		draftFrom !== dateFrom ||
		draftTo !== dateTo;

	$: sourceFiltered = selectedSources.length > 0 && selectedSources.length < sources.length;
	$: dateFiltered = dateFrom !== '' || dateTo !== '';
	$: anyFilter = sourceFiltered || dateFiltered;

	// Count badge: number of active filter dimensions
	$: filterCount = (sourceFiltered ? 1 : 0) + (dateFiltered ? 1 : 0);

	function showFromPicker() {
		(document.getElementById('adv-date-from') as HTMLInputElement)?.showPicker();
	}
	function showToPicker() {
		(document.getElementById('adv-date-to') as HTMLInputElement)?.showPicker();
	}
</script>

<div class="advanced-search" use:clickOutside={() => (isOpen = false)}>
	<button
		class="trigger"
		class:active={anyFilter}
		on:click={() => (isOpen ? (isOpen = false) : openPanel())}
		title="Advanced search filters"
	>
		<span class="icon">⚙</span>
		<span class="text">Advanced</span>
		{#if anyFilter}
			<span class="badge">{filterCount}</span>
		{/if}
		<span class="chevron" class:open={isOpen}>▾</span>
	</button>

	{#if isOpen}
		<div class="panel">
			{#if sources.length > 0}
				<div class="section">
					<div class="section-header">
						<span class="section-title">Sources</span>
						{#if draftSources.length > 0 && draftSources.length < sources.length}
							<button class="clear-link" on:click={() => (draftSources = [])}>All</button>
						{/if}
					</div>
					<div class="source-list">
						{#each sources as source}
							<label class="source-item">
								<input
									type="checkbox"
									checked={draftSources.includes(source)}
									on:change={() => toggleDraftSource(source)}
								/>
								<span class="source-name">{source}</span>
							</label>
						{/each}
					</div>
				</div>
			{/if}

			<div class="section">
				<div class="section-header">
					<span class="section-title">Date range</span>
					{#if draftFrom || draftTo}
						<button class="clear-link" on:click={() => { draftFrom = ''; draftTo = ''; }}>Clear</button>
					{/if}
				</div>
				<div class="date-row">
					<label class="date-label" for="adv-date-from">From</label>
					<div class="date-wrap">
						<input
							id="adv-date-from"
							class="date-input"
							class:no-value={!draftFrom}
							type="date"
							bind:value={draftFrom}
						/>
						<button class="cal-btn" tabindex="-1" on:click={showFromPicker}>📅</button>
					</div>
				</div>
				<div class="date-row">
					<label class="date-label" for="adv-date-to">To</label>
					<div class="date-wrap">
						<input
							id="adv-date-to"
							class="date-input"
							class:no-value={!draftTo}
							type="date"
							bind:value={draftTo}
						/>
						<button class="cal-btn" tabindex="-1" on:click={showToPicker}>📅</button>
					</div>
				</div>
			</div>

			<div class="footer">
				{#if anyFilter}
					<button class="clear-all" on:click={clearAll}>Clear all</button>
				{/if}
				<button class="apply-btn" class:dirty={isDirty} disabled={!isDirty} on:click={apply}>Apply</button>
			</div>
		</div>
	{/if}
</div>

<style>
	.advanced-search {
		position: relative;
		display: inline-block;
	}

	.trigger {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 5px 10px;
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--bg);
		color: var(--text);
		font-size: 13px;
		cursor: pointer;
		transition: all 0.15s;
	}

	.trigger:hover {
		border-color: var(--accent);
		background: var(--hover-bg);
	}

	.trigger.active {
		border-color: var(--accent);
		background: var(--chip-active);
		color: #fff;
	}

	.icon {
		font-size: 13px;
	}

	.text {
		white-space: nowrap;
	}

	.badge {
		background: rgba(255, 255, 255, 0.3);
		border-radius: 10px;
		padding: 1px 6px;
		font-size: 11px;
		font-weight: 600;
	}

	.chevron {
		font-size: 10px;
		transition: transform 0.2s;
		opacity: 0.7;
	}

	.chevron.open {
		transform: rotate(180deg);
	}

	.panel {
		position: absolute;
		top: calc(100% + 4px);
		right: 0;
		min-width: 240px;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 6px;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
		z-index: 1000;
		overflow: hidden;
	}

	.section {
		padding: 10px 12px;
		border-bottom: 1px solid var(--border);
	}

	.section-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 6px;
	}

	.section-title {
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-muted);
	}

	.clear-link {
		background: none;
		border: none;
		color: var(--accent);
		font-size: 12px;
		cursor: pointer;
		padding: 0;
	}

	.clear-link:hover {
		text-decoration: underline;
	}

	.source-list {
		max-height: 200px;
		overflow-y: auto;
	}

	.source-item {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 4px 0;
		cursor: pointer;
	}

	.source-item input[type='checkbox'] {
		cursor: pointer;
		margin: 0;
	}

	.source-name {
		font-size: 13px;
		color: var(--text);
	}

	.date-row {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-top: 6px;
	}

	.date-label {
		font-size: 12px;
		color: var(--text-muted);
		width: 28px;
		flex-shrink: 0;
	}

	.date-wrap {
		flex: 1;
		display: flex;
		align-items: center;
		border: 1px solid var(--border);
		border-radius: 4px;
		background: var(--bg);
	}

	.date-wrap:focus-within {
		border-color: var(--accent);
	}

	.date-input {
		flex: 1;
		padding: 4px 6px;
		border: none;
		background: transparent;
		color: var(--text);
		font-size: 12px;
		font-family: inherit;
		/* hide the browser's built-in calendar icon */
		&::-webkit-calendar-picker-indicator { display: none; }
	}

	/* dim the placeholder format text when no value is set */
	.date-input.no-value::-webkit-datetime-edit {
		opacity: 0.25;
	}

	.date-input:focus {
		outline: none;
	}

	.cal-btn {
		background: none;
		border: none;
		border-left: 1px solid var(--border);
		padding: 2px 6px;
		cursor: pointer;
		font-size: 13px;
		line-height: 1;
		color: var(--text-muted);
		flex-shrink: 0;
	}

	.cal-btn:hover {
		color: var(--text);
		background: var(--hover-bg);
	}

	.footer {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 12px;
		background: var(--hover-bg);
		border-top: 1px solid var(--border);
	}

	.clear-all {
		background: none;
		border: none;
		color: var(--text-muted);
		font-size: 12px;
		cursor: pointer;
		padding: 0;
	}

	.clear-all:hover {
		color: var(--text);
		text-decoration: underline;
	}

	.apply-btn {
		margin-left: auto;
		padding: 4px 14px;
		border-radius: 4px;
		border: 1px solid var(--border);
		background: none;
		color: var(--text-muted);
		font-size: 12px;
		font-weight: 600;
		cursor: default;
		transition: all 0.15s;
	}

	.apply-btn.dirty {
		border-color: var(--accent);
		background: var(--accent);
		color: #fff;
		cursor: pointer;
	}

	.apply-btn.dirty:hover {
		opacity: 0.85;
	}
</style>
