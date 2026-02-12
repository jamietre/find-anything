<script lang="ts">
	import { createEventDispatcher, onMount, tick } from 'svelte';
	import { getFile } from '$lib/api';
	import { highlightFile } from '$lib/highlight';

	export let source: string;
	export let path: string;
	export let archivePath: string | null = null;
	export let targetLine: number | null = null;

	const dispatch = createEventDispatcher<{ back: void }>();

	let loading = true;
	let error: string | null = null;
	let highlightedCode = '';
	let totalLines = 0;
	/** Maps line_number → 0-based index in the rendered lines array */
	let lineOffsets: number[] = [];

	onMount(async () => {
		try {
			const data = await getFile(source, path, archivePath ?? undefined);
			totalLines = data.total_lines;
			const contents = data.lines.map((l) => l.content);
			lineOffsets = data.lines.map((l) => l.line_number);
			highlightedCode = highlightFile(contents, path);
		} catch (e) {
			error = String(e);
		} finally {
			loading = false;
		}

		if (targetLine !== null) {
			await tick();
			scrollToLine(targetLine);
		}
	});

	function scrollToLine(ln: number) {
		const el = document.getElementById(`line-${ln}`);
		if (el) el.scrollIntoView({ behavior: 'smooth', block: 'center' });
	}

	function displayPath(): string {
		return archivePath ? `${path}::${archivePath}` : path;
	}

	// Split the full highlighted block into per-line segments.
	// We do this after rendering so we can assign IDs.
	$: codeLines = highlightedCode ? highlightedCode.split('\n') : [];
</script>

<div class="file-viewer">
	<div class="viewer-header">
		<button class="back-btn" on:click={() => dispatch('back')}>← results</button>
		<span class="badge">{source}</span>
		<span class="file-path">{displayPath()}</span>
		{#if totalLines > 0}
			<span class="line-count">{totalLines} lines</span>
		{/if}
	</div>

	{#if loading}
		<div class="status">Loading…</div>
	{:else if error}
		<div class="status error">{error}</div>
	{:else}
		<div class="code-container">
			<table class="code-table" cellspacing="0" cellpadding="0">
				<tbody>
					{#each codeLines as line, i}
						{@const lineNum = lineOffsets[i] ?? i + 1}
						<tr
							id="line-{lineNum}"
							class="code-row"
							class:target={lineNum === targetLine}
						>
							<td class="td-ln">{lineNum}</td>
							<td class="td-arrow">{lineNum === targetLine ? '▶' : ''}</td>
							<td class="td-code"><code>{@html line}</code></td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>

<style>
	.file-viewer {
		display: flex;
		flex-direction: column;
		height: 100%;
		overflow: hidden;
	}

	.viewer-header {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 16px;
		background: var(--bg-secondary);
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	.back-btn {
		background: none;
		border: 1px solid var(--border);
		color: var(--text-muted);
		padding: 3px 10px;
		border-radius: var(--radius);
		font-size: 12px;
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

	.file-path {
		color: var(--accent);
		font-family: var(--font-mono);
		font-size: 12px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		flex: 1;
	}

	.line-count {
		color: var(--text-dim);
		font-size: 12px;
		flex-shrink: 0;
	}

	.status {
		padding: 24px;
		color: var(--text-muted);
		text-align: center;
	}

	.status.error {
		color: #f85149;
	}

	.code-container {
		flex: 1;
		overflow: auto;
		background: var(--bg);
	}

	.code-table {
		width: 100%;
		border-collapse: collapse;
		font-family: var(--font-mono);
		font-size: 13px;
		line-height: 1.6;
	}

	.code-row {
		border-left: 2px solid transparent;
	}

	.code-row.target {
		background: var(--match-line-bg);
		border-left-color: var(--match-border);
	}

	.td-ln {
		min-width: 52px;
		padding: 0 12px 0 8px;
		text-align: right;
		color: var(--text-dim);
		user-select: none;
		vertical-align: top;
	}

	.td-arrow {
		width: 16px;
		color: var(--accent);
		font-size: 10px;
		user-select: none;
		vertical-align: top;
	}

	.td-code {
		padding: 0 16px 0 4px;
		white-space: pre;
		vertical-align: top;
	}
</style>
