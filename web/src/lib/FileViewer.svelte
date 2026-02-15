<script lang="ts">
	import { createEventDispatcher, onMount, tick } from 'svelte';
	import { getFile } from '$lib/api';
	import { highlightFile } from '$lib/highlight';
	import {
		type LineSelection,
		selectionSet,
		firstLine,
		toggleLine
	} from '$lib/lineSelection';
	import { profile } from '$lib/profile';

	export let source: string;
	export let path: string;
	export let archivePath: string | null = null;
	export let selection: LineSelection = [];

	const dispatch = createEventDispatcher<{ lineselect: { selection: LineSelection } }>();

	let loading = true;
	let error: string | null = null;
	let highlightedCode = '';
	/** Maps 0-based render index → line_number */
	let lineOffsets: number[] = [];

	// Word wrap preference (default: false for code, true for text files)
	$: wordWrap = $profile.wordWrap ?? false;

	function toggleWordWrap() {
		$profile.wordWrap = !wordWrap;
	}

	onMount(async () => {
		try {
			const data = await getFile(source, path, archivePath ?? undefined);
			const contents = data.lines.map((l) => l.content);
			lineOffsets = data.lines.map((l) => l.line_number);
			highlightedCode = highlightFile(contents, path);
		} catch (e) {
			error = String(e);
		} finally {
			loading = false;
		}

		const ln = firstLine(selection);
		if (ln !== null) {
			await tick();
			scrollToLine(ln);
		}
	});

	function scrollToLine(ln: number) {
		const el = document.getElementById(`line-${ln}`);
		if (el) el.scrollIntoView({ behavior: 'smooth', block: 'center' });
	}

	function handleLineClick(lineNum: number, e: MouseEvent) {
		let next: LineSelection;
		if (e.ctrlKey || e.metaKey) {
			next = toggleLine(selection, lineNum);
		} else if (e.shiftKey && selection.length > 0) {
			const anchor = firstLine(selection)!;
			next = [anchor <= lineNum ? [anchor, lineNum] : [lineNum, anchor]];
		} else {
			next = [lineNum];
		}
		selection = next;
		dispatch('lineselect', { selection: next });
	}

	$: codeLines = highlightedCode ? highlightedCode.split('\n') : [];
	$: highlightedSet = selectionSet(selection);
	$: arrowLine = firstLine(selection);
</script>

<div class="file-viewer">
	{#if loading}
		<div class="status">Loading…</div>
	{:else if error}
		<div class="status error">{error}</div>
	{:else}
		<div class="toolbar">
			<button class="toolbar-btn" on:click={toggleWordWrap} title="Toggle word wrap">
				{wordWrap ? '⊟' : '⊞'} Wrap
			</button>
		</div>
		<div class="code-container">
			<table class="code-table" cellspacing="0" cellpadding="0">
				<tbody>
					{#each codeLines as line, i}
						{@const lineNum = lineOffsets[i] ?? i + 1}
						<!-- svelte-ignore a11y-click-events-have-key-events -->
						<!-- svelte-ignore a11y-no-static-element-interactions -->
						<tr
							id="line-{lineNum}"
							class="code-row"
							class:target={highlightedSet.has(lineNum)}
							on:click={(e) => handleLineClick(lineNum, e)}
						>
							<td class="td-ln">{lineNum}</td>
							<td class="td-arrow">{lineNum === arrowLine ? '▶' : ''}</td>
							<td class="td-code" class:wrap={wordWrap}><code>{@html line}</code></td>
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
		cursor: pointer;
	}

	.code-row:hover {
		background: var(--bg-hover, rgba(255, 255, 255, 0.04));
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

	.td-code.wrap {
		white-space: pre-wrap;
		word-break: break-word;
	}

	.toolbar {
		display: flex;
		gap: 8px;
		padding: 8px 12px;
		border-bottom: 1px solid var(--border, rgba(255, 255, 255, 0.1));
		background: var(--bg-secondary, rgba(0, 0, 0, 0.2));
	}

	.toolbar-btn {
		padding: 4px 12px;
		font-size: 12px;
		font-family: var(--font-mono);
		background: var(--bg-hover, rgba(255, 255, 255, 0.05));
		border: 1px solid var(--border, rgba(255, 255, 255, 0.15));
		border-radius: 4px;
		color: var(--text);
		cursor: pointer;
		transition: background 0.15s;
	}

	.toolbar-btn:hover {
		background: var(--bg-hover-strong, rgba(255, 255, 255, 0.1));
	}

	.toolbar-btn:active {
		transform: translateY(1px);
	}
</style>
