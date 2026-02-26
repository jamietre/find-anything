<script lang="ts">
	import { onMount } from 'svelte';
	import { getStats } from '$lib/api';
	import type { SourceStats, StatsResponse } from '$lib/api';

	let stats: StatsResponse | null = null;
	let loading = false;
	let error: string | null = null;
	let selectedSource = '';

	$: currentSource = stats?.sources.find((s) => s.name === selectedSource) ?? stats?.sources[0] ?? null;

	onMount(() => { fetchStats(); });

	async function fetchStats() {
		loading = true;
		error = null;
		try {
			stats = await getStats();
			if (stats.sources.length > 0) {
				selectedSource = stats.sources[0].name;
			}
		} catch (e) {
			error = String(e);
		} finally {
			loading = false;
		}
	}

	// ── Formatting helpers ─────────────────────────────────────────────────────

	function fmtSize(bytes: number): string {
		if (bytes >= 1e9) return (bytes / 1e9).toFixed(1) + ' GB';
		if (bytes >= 1e6) return (bytes / 1e6).toFixed(1) + ' MB';
		if (bytes >= 1e3) return (bytes / 1e3).toFixed(1) + ' KB';
		return bytes + ' B';
	}

	function fmtMs(ms: number | null): string {
		if (ms == null) return '—';
		if (ms >= 1000) return (ms / 1000).toFixed(1) + 's';
		return Math.round(ms) + 'ms';
	}

	function fmtRelativeTime(epochSecs: number | null): string {
		if (epochSecs == null) return 'never';
		const diff = Math.floor(Date.now() / 1000) - epochSecs;
		if (diff < 60) return 'just now';
		if (diff < 3600) return Math.floor(diff / 60) + 'm ago';
		if (diff < 86400) return Math.floor(diff / 3600) + 'h ago';
		return Math.floor(diff / 86400) + 'd ago';
	}

	function fmtDate(epochSecs: number): string {
		return new Date(epochSecs * 1000).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
	}

	// ── Kind bar helpers ───────────────────────────────────────────────────────

	const KIND_ORDER = ['text', 'pdf', 'image', 'audio', 'video', 'document', 'archive', 'executable'];

	function sortedKinds(src: SourceStats): Array<[string, { count: number; size: number; avg_extract_ms: number | null }]> {
		const entries = Object.entries(src.by_kind);
		return entries.sort(([a], [b]) => {
			const ai = KIND_ORDER.indexOf(a);
			const bi = KIND_ORDER.indexOf(b);
			if (ai !== -1 && bi !== -1) return ai - bi;
			if (ai !== -1) return -1;
			if (bi !== -1) return 1;
			return a.localeCompare(b);
		});
	}

	// ── SVG chart helpers ──────────────────────────────────────────────────────

	const CHART_W = 560;
	const CHART_H = 120;
	const PAD_L = 50;
	const PAD_R = 12;
	const PAD_T = 8;
	const PAD_B = 28;
	const PLOT_W = CHART_W - PAD_L - PAD_R;
	const PLOT_H = CHART_H - PAD_T - PAD_B;

	function buildChart(src: SourceStats): {
		points: string;
		yLabels: Array<{ y: number; label: string }>;
		xLabels: Array<{ x: number; label: string }>;
	} {
		const hist = src.history;
		if (hist.length === 0) return { points: '', yLabels: [], xLabels: [] };

		const maxFiles = Math.max(...hist.map((h) => h.total_files), 1);
		const minT = hist[0].scanned_at;
		const maxT = hist[hist.length - 1].scanned_at;
		const rangeT = maxT - minT || 1;

		function px(h: { scanned_at: number; total_files: number }): [number, number] {
			const x = PAD_L + ((h.scanned_at - minT) / rangeT) * PLOT_W;
			const y = PAD_T + PLOT_H - (h.total_files / maxFiles) * PLOT_H;
			return [x, y];
		}

		const points = hist.map((h) => px(h).join(',')).join(' ');

		// Y-axis: 0, half, max
		const yLabels = [
			{ y: PAD_T + PLOT_H, label: '0' },
			{ y: PAD_T + PLOT_H / 2, label: fmtCount(Math.round(maxFiles / 2)) },
			{ y: PAD_T, label: fmtCount(maxFiles) },
		];

		// X-axis: up to 5 evenly-spaced dates
		const xCount = Math.min(hist.length, 5);
		const xLabels = Array.from({ length: xCount }, (_, i) => {
			const idx = Math.round((i / (xCount - 1 || 1)) * (hist.length - 1));
			const h = hist[Math.min(idx, hist.length - 1)];
			const [x] = px(h);
			return { x, label: fmtDate(h.scanned_at) };
		});

		return { points, yLabels, xLabels };
	}

	function fmtCount(n: number): string {
		if (n >= 1000) return (n / 1000).toFixed(1) + 'k';
		return String(n);
	}
</script>

{#if loading}
	<div class="status">Loading…</div>
{:else if error}
	<div class="status error">{error}</div>
{:else if !stats || stats.sources.length === 0}
	<div class="status">No sources indexed yet.</div>
{:else}
	<!-- Source selector -->
	{#if stats.sources.length > 1}
		<div class="source-row">
			<label class="source-label" for="source-select">Source</label>
			<select id="source-select" class="source-select" bind:value={selectedSource}>
				{#each stats.sources as src (src.name)}
					<option value={src.name}>{src.name}</option>
				{/each}
			</select>
		</div>
	{/if}

	{#if currentSource}
		<!-- Summary cards -->
		<div class="cards">
			<div class="card">
				<div class="card-value">{currentSource.total_files.toLocaleString()}</div>
				<div class="card-label">files</div>
			</div>
			<div class="card">
				<div class="card-value">{fmtSize(currentSource.total_size)}</div>
				<div class="card-label">indexed</div>
			</div>
			<div class="card">
				<div class="card-value">{fmtRelativeTime(currentSource.last_scan)}</div>
				<div class="card-label">last scan</div>
			</div>
			{#if (currentSource.indexing_error_count ?? 0) > 0}
				<a class="card card-errors" href="/settings?section=errors" title="View indexing errors">
					<div class="card-value error-value">⚠ {currentSource.indexing_error_count}</div>
					<div class="card-label">errors</div>
				</a>
			{/if}
		</div>

		<!-- Global metrics (shown once, not per-source) -->
		<div class="global-metrics">
			<span>{stats.total_archives.toLocaleString()} archive{stats.total_archives !== 1 ? 's' : ''}</span>
			<span title="SQLite database size">DB: {fmtSize(stats.db_size_bytes)}</span>
			<span title="ZIP archive size">Archives: {fmtSize(stats.archive_size_bytes)}</span>
			{#if stats.inbox_pending > 0}
				<span class="pending">{stats.inbox_pending} pending</span>
			{/if}
			{#if stats.failed_requests > 0}
				<span class="failed">{stats.failed_requests} failed</span>
			{/if}
			{#if stats.worker_status.state === 'processing'}
				<span class="worker-processing" title="{stats.worker_status.source}: {stats.worker_status.file}">
					<span class="worker-dot"></span>
					indexing {stats.worker_status.source}/{stats.worker_status.file.split('/').pop()}
				</span>
			{:else}
				<span class="worker-idle">idle</span>
			{/if}
		</div>

		<!-- By Kind -->
		{#if Object.keys(currentSource.by_kind).length > 0}
			<div class="section-title">By Kind</div>
			<div class="kinds">
				{#each sortedKinds(currentSource) as [kind, ks] (kind)}
					{@const pct = currentSource.total_files > 0 ? (ks.count / currentSource.total_files) * 100 : 0}
					<div class="kind-row">
						<span class="kind-name">{kind}</span>
						<div class="kind-bar-wrap">
							<div class="kind-bar" style="width: {pct}%"></div>
						</div>
						<span class="kind-count">{ks.count.toLocaleString()}</span>
						<span class="kind-size">{fmtSize(ks.size)}</span>
						<span class="kind-ms">{fmtMs(ks.avg_extract_ms)}</span>
					</div>
				{/each}
			</div>
		{/if}

		<!-- Items over time -->
		{#if currentSource.history.length >= 2}
			{@const chart = buildChart(currentSource)}
			<div class="section-title">Files over time</div>
			<svg class="chart" viewBox="0 0 {CHART_W} {CHART_H}" preserveAspectRatio="none">
				<!-- Y-axis labels -->
				{#each chart.yLabels as { y, label }}
					<text class="axis-label" x={PAD_L - 6} y={y} text-anchor="end" dominant-baseline="middle">{label}</text>
				{/each}
				<!-- Y-axis line -->
				<line class="axis-line" x1={PAD_L} y1={PAD_T} x2={PAD_L} y2={PAD_T + PLOT_H} />
				<!-- X-axis line -->
				<line class="axis-line" x1={PAD_L} y1={PAD_T + PLOT_H} x2={PAD_L + PLOT_W} y2={PAD_T + PLOT_H} />
				<!-- X-axis labels -->
				{#each chart.xLabels as { x, label }}
					<text class="axis-label" x={x} y={CHART_H - 4} text-anchor="middle">{label}</text>
				{/each}
				<!-- Data line -->
				<polyline class="chart-line" points={chart.points} />
			</svg>
		{:else if currentSource.history.length === 1}
			<div class="status-small">Only one scan recorded — run another scan to see the chart.</div>
		{/if}
	{/if}
{/if}

<style>
	.status {
		color: var(--text-muted);
		font-size: 13px;
		padding: 24px;
		text-align: center;
	}

	.status.error {
		color: #f85149;
	}

	.status-small {
		color: var(--text-muted);
		font-size: 12px;
		margin-top: 8px;
	}

	/* Source selector */
	.source-row {
		display: flex;
		align-items: center;
		gap: 10px;
		margin-bottom: 16px;
	}

	.source-label {
		font-size: 12px;
		color: var(--text-muted);
		flex-shrink: 0;
	}

	.source-select {
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		color: var(--text);
		font-size: 13px;
		padding: 4px 8px;
		cursor: pointer;
	}

	/* Summary cards */
	.cards {
		display: flex;
		gap: 12px;
		margin-bottom: 16px;
	}

	.card {
		flex: 1;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 12px;
		text-align: center;
	}

	.card-value {
		font-size: 20px;
		font-weight: 600;
		color: var(--text);
		margin-bottom: 2px;
	}

	.card-label {
		font-size: 11px;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.card-errors {
		border-color: rgba(230, 162, 60, 0.4);
		background: rgba(230, 162, 60, 0.06);
		text-decoration: none;
		color: inherit;
	}

	.card-errors:hover {
		border-color: rgba(230, 162, 60, 0.7);
		background: rgba(230, 162, 60, 0.12);
	}

	.error-value {
		color: #e6a23c;
	}

	/* Global metrics strip */
	.global-metrics {
		display: flex;
		gap: 16px;
		font-size: 12px;
		color: var(--text-muted);
		margin-bottom: 16px;
	}

	.pending {
		color: #e3b341;
	}

	.failed {
		color: #f85149;
	}

	.worker-idle {
		color: var(--text-muted);
		opacity: 0.6;
	}

	.worker-processing {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		color: #58a6ff;
		max-width: 260px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.worker-dot {
		display: inline-block;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: #58a6ff;
		flex-shrink: 0;
		animation: pulse 1.2s ease-in-out infinite;
	}

	@keyframes pulse {
		0%, 100% { opacity: 1; }
		50%       { opacity: 0.3; }
	}

	/* Section title */
	.section-title {
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-muted);
		margin-bottom: 10px;
		margin-top: 16px;
	}

	/* By Kind */
	.kinds {
		display: flex;
		flex-direction: column;
		gap: 6px;
		margin-bottom: 4px;
	}

	.kind-row {
		display: grid;
		grid-template-columns: 80px 1fr 70px 70px 70px;
		align-items: center;
		gap: 8px;
		font-size: 12px;
	}

	.kind-name {
		color: var(--text);
		font-family: var(--font-mono);
	}

	.kind-bar-wrap {
		height: 8px;
		background: var(--bg);
		border: 1px solid var(--border);
		border-radius: 4px;
		overflow: hidden;
	}

	.kind-bar {
		height: 100%;
		background: var(--accent, #3b82f6);
		border-radius: 4px;
		min-width: 2px;
	}

	.kind-count {
		color: var(--text);
		text-align: right;
	}

	.kind-size {
		color: var(--text-muted);
		text-align: right;
	}

	.kind-ms {
		color: var(--text-muted);
		text-align: right;
		font-family: var(--font-mono);
	}

	/* SVG chart */
	.chart {
		width: 100%;
		height: auto;
		display: block;
		overflow: visible;
	}

	.axis-label {
		font-size: 10px;
		fill: var(--text-muted, #888);
		font-family: var(--font-mono, monospace);
	}

	.axis-line {
		stroke: var(--border, #333);
		stroke-width: 1;
	}

	.chart-line {
		fill: none;
		stroke: var(--accent, #3b82f6);
		stroke-width: 2;
		stroke-linejoin: round;
		stroke-linecap: round;
	}
</style>
