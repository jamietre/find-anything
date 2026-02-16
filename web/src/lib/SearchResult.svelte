<script lang="ts">
	import { createEventDispatcher, onMount } from 'svelte';
	import type { ContextLine, SearchResult } from '$lib/api';
	import { getContext as fetchContext } from '$lib/api';
	import { highlightLine } from '$lib/highlight';

	export let result: SearchResult;

	const dispatch = createEventDispatcher<{ open: SearchResult }>();

	let containerEl: HTMLElement;
	let contextLines: ContextLine[] = [];
	let contextLoaded = false;

	$: displayLines =
		contextLines.length > 0
			? contextLines
			: [{ line_number: result.line_number, content: result.snippet }];

	onMount(() => {
		const observer = new IntersectionObserver(
			(entries) => {
				if (entries[0].isIntersecting && !contextLoaded) {
					contextLoaded = true;
					fetchContext(
						result.source,
						result.path,
						result.line_number,
						3,
						result.archive_path ?? undefined
					)
						.then((resp) => {
							contextLines = resp.lines;
						})
						.catch(() => {
							// silently fall back to snippet
						});
					observer.disconnect();
				}
			},
			{ rootMargin: '200px' }
		);
		observer.observe(containerEl);
		return () => observer.disconnect();
	});

	function openFile() {
		dispatch('open', result);
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' || e.key === ' ') openFile();
	}

	function displayPath(r: SearchResult): string {
		return r.archive_path ? `${r.path}::${r.archive_path}` : r.path;
	}
</script>

<article class="result" bind:this={containerEl}>
	<!-- svelte-ignore a11y-no-static-element-interactions -->
	<div
		class="result-header"
		on:click={openFile}
		on:keydown={handleKeydown}
		role="button"
		tabindex="0"
		title="Open file at line {result.line_number}"
	>
		<span class="badge">{result.source}</span>
		<span class="file-path">{displayPath(result)}</span>
		<span class="line-ref">:{result.line_number}</span>
	</div>

	<div class="context-lines">
		{#each displayLines as line}
			<div class="line" class:match={line.line_number === result.line_number}>
				<span class="ln">{line.line_number}</span>
				<span class="arrow">{line.line_number === result.line_number ? '▶' : ' '}</span>
				<code class="lc">{@html highlightLine(line.content, result.path)}</code>
			</div>
		{/each}
	</div>
</article>

<style>
	.result {
		border: 1px solid var(--border);
		border-radius: var(--radius);
		overflow: hidden;
		margin-bottom: 12px;
	}

	.result:hover {
		border-color: var(--accent-muted);
	}

	.result-header {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 12px;
		background: var(--bg-secondary);
		cursor: pointer;
		user-select: none;
	}

	.result-header:hover {
		background: var(--bg-hover);
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
	}

	.line-ref {
		color: var(--text-dim);
		font-family: var(--font-mono);
		font-size: 12px;
		flex-shrink: 0;
	}

	.context-lines {
		background: var(--bg);
		overflow: hidden;
	}

	.line {
		display: flex;
		align-items: baseline;
		padding: 1px 0;
		overflow: hidden;
		min-width: 0;
	}

	.line.match {
		background: var(--match-line-bg);
		border-left: 2px solid var(--match-border);
	}

	.line:not(.match) {
		border-left: 2px solid transparent;
	}

	.ln {
		min-width: 48px;
		padding: 0 12px 0 8px;
		text-align: right;
		color: var(--text-dim);
		font-family: var(--font-mono);
		font-size: 12px;
		flex-shrink: 0;
		user-select: none;
	}

	.arrow {
		width: 14px;
		color: var(--accent);
		font-size: 10px;
		flex-shrink: 0;
		user-select: none;
	}

	.lc {
		padding: 0 12px 0 4px;
		white-space: pre;
		overflow: hidden;
		text-overflow: ellipsis;
		flex: 1;
		min-width: 0;
	}
</style>
