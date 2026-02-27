<script lang="ts">
	import type { SourceInfo } from '$lib/api';
	import { profile } from '$lib/profile';
	import { contextWindow } from '$lib/settingsStore';

	export let sources: SourceInfo[] = [];

	function setOverride(name: string, value: string) {
		profile.update((p) => ({
			...p,
			sourceBaseUrls: { ...p.sourceBaseUrls, [name]: value }
		}));
	}

	function clearOverride(name: string) {
		profile.update((p) => {
			const urls = { ...(p.sourceBaseUrls ?? {}) };
			delete urls[name];
			return { ...p, sourceBaseUrls: urls };
		});
	}

	function setContextWindow(value: number) {
		profile.update((p) => ({ ...p, contextWindow: value }));
		contextWindow.set(value);
	}
</script>

<div class="section-title">Search results</div>
<div class="pref-row">
	<label class="pref-label" for="ctx-window">Lines of context</label>
	<div class="pref-control">
		<select
			id="ctx-window"
			class="select"
			value={$profile.contextWindow ?? $contextWindow}
			on:change={(e) => setContextWindow(Number(e.currentTarget.value))}
		>
			<option value={0}>0 (match only)</option>
			<option value={1}>1 (±1 line)</option>
			<option value={2}>2 (±2 lines)</option>
			<option value={3}>3 (±3 lines)</option>
			<option value={5}>5 (±5 lines)</option>
		</select>
		{#if $profile.contextWindow !== undefined}
			<button class="clear-btn" on:click={() => { profile.update(p => { const {contextWindow: _, ...rest} = p; return rest; }); contextWindow.set(1); }}>Reset</button>
		{/if}
	</div>
</div>

<div class="section-title" style="margin-top: 24px;">Base URL overrides</div>
{#if sources.length === 0}
	<p class="empty">No sources indexed yet.</p>
{:else}
	{#each sources as source (source.name)}
		<div class="source-row">
			<div class="source-name">{source.name}</div>
			{#if source.base_url}
				<div class="server-hint">Server: <code>{source.base_url}</code></div>
			{/if}
			<div class="override-row">
				<input
					class="override-input"
					type="text"
					placeholder="Override base URL…"
					value={$profile.sourceBaseUrls?.[source.name] ?? ''}
					on:input={(e) => setOverride(source.name, e.currentTarget.value)}
				/>
				{#if $profile.sourceBaseUrls?.[source.name]}
					<button class="clear-btn" on:click={() => clearOverride(source.name)}>Clear</button>
				{/if}
			</div>
		</div>
	{/each}
{/if}

<style>
	.section-title {
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-muted);
		margin-bottom: 12px;
	}

	.pref-row {
		display: flex;
		align-items: center;
		gap: 16px;
		margin-bottom: 16px;
	}

	.pref-label {
		font-size: 13px;
		color: var(--text);
		min-width: 140px;
	}

	.pref-control {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.select {
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		color: var(--text);
		font-size: 13px;
		padding: 5px 8px;
		outline: none;
		cursor: pointer;
	}

	.select:focus {
		border-color: var(--accent);
	}

	.empty {
		color: var(--text-muted);
		font-size: 13px;
	}

	.source-row {
		margin-bottom: 16px;
		padding-bottom: 16px;
		border-bottom: 1px solid var(--border);
	}

	.source-row:last-child {
		margin-bottom: 0;
		padding-bottom: 0;
		border-bottom: none;
	}

	.source-name {
		font-size: 13px;
		font-weight: 500;
		color: var(--text);
		margin-bottom: 4px;
	}

	.server-hint {
		font-size: 11px;
		color: var(--text-muted);
		margin-bottom: 8px;
	}

	.server-hint code {
		font-family: var(--font-mono);
		color: var(--text-dim);
	}

	.override-row {
		display: flex;
		gap: 8px;
		align-items: center;
	}

	.override-input {
		flex: 1;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		color: var(--text);
		font-family: var(--font-mono);
		font-size: 12px;
		padding: 5px 8px;
		outline: none;
	}

	.override-input:focus {
		border-color: var(--accent);
	}

	.clear-btn {
		background: none;
		border: 1px solid var(--border);
		color: var(--text-muted);
		font-size: 12px;
		padding: 4px 10px;
		border-radius: var(--radius);
		cursor: pointer;
		flex-shrink: 0;
	}

	.clear-btn:hover {
		border-color: #f85149;
		color: #f85149;
	}
</style>
