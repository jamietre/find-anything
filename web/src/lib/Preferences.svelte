<script lang="ts">
	import type { SourceInfo } from '$lib/api';
	import { profile } from '$lib/profile';

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
</script>

<div class="section-title">Base URL overrides</div>
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
