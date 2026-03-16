<script lang="ts">
	import { onMount, onDestroy } from 'svelte';

	export let src: string;

	let container: HTMLDivElement;
	let img: HTMLImageElement;

	let scale = 1;
	let offsetX = 0;
	let offsetY = 0;
	let fitScale = 1;

	let dragging = false;
	let dragStartX = 0;
	let dragStartY = 0;
	let dragOriginX = 0;
	let dragOriginY = 0;

	const MIN_SCALE = 0.1;
	const MAX_SCALE = 10;

	function clamp(v: number, min: number, max: number) {
		return Math.max(min, Math.min(max, v));
	}

	function applyTransform() {
		if (img) {
			img.style.transform = `translate(${offsetX}px, ${offsetY}px) scale(${scale})`;
		}
	}

	function onImageLoad() {
		const vw = container.clientWidth;
		const vh = container.clientHeight;
		const nw = img.naturalWidth;
		const nh = img.naturalHeight;

		if (nw <= vw && nh <= vh) {
			fitScale = 1;
		} else {
			fitScale = Math.min(vw / nw, vh / nh);
		}
		scale = fitScale;
		offsetX = 0;
		offsetY = 0;
		applyTransform();
	}

	function onWheel(e: WheelEvent) {
		e.preventDefault();
		const delta = e.deltaY > 0 ? 0.9 : 1.1;
		scale = clamp(scale * delta, MIN_SCALE, MAX_SCALE);
		applyTransform();
	}

	function onPointerDown(e: PointerEvent) {
		if (e.button !== 0) return;
		dragging = true;
		dragStartX = e.clientX;
		dragStartY = e.clientY;
		dragOriginX = offsetX;
		dragOriginY = offsetY;
		container.setPointerCapture(e.pointerId);
	}

	function onPointerMove(e: PointerEvent) {
		if (!dragging) return;
		offsetX = dragOriginX + (e.clientX - dragStartX);
		offsetY = dragOriginY + (e.clientY - dragStartY);
		applyTransform();
	}

	function onPointerUp() {
		dragging = false;
	}

	function onDblClick() {
		scale = fitScale;
		offsetX = 0;
		offsetY = 0;
		applyTransform();
	}

	function zoomIn() {
		scale = clamp(scale * 1.25, MIN_SCALE, MAX_SCALE);
		applyTransform();
	}

	function zoomOut() {
		scale = clamp(scale / 1.25, MIN_SCALE, MAX_SCALE);
		applyTransform();
	}

	function reset() {
		scale = fitScale;
		offsetX = 0;
		offsetY = 0;
		applyTransform();
	}

	onMount(() => {
		container.addEventListener('wheel', onWheel, { passive: false });
	});

	onDestroy(() => {
		if (container) container.removeEventListener('wheel', onWheel);
	});
</script>

<div class="viewer-wrap">
	<div class="toolbar">
		<button on:click={zoomIn} title="Zoom in">+</button>
		<button on:click={zoomOut} title="Zoom out">−</button>
		<button on:click={reset} title="Reset zoom">⊙</button>
	</div>
	<div
		class="container"
		class:dragging
		bind:this={container}
		on:pointerdown={onPointerDown}
		on:pointermove={onPointerMove}
		on:pointerup={onPointerUp}
		on:pointercancel={onPointerUp}
		on:dblclick={onDblClick}
		role="img"
		aria-label="Image viewer"
	>
		<img
			bind:this={img}
			{src}
			alt=""
			on:load={onImageLoad}
			draggable="false"
		/>
	</div>
</div>

<style>
	.viewer-wrap {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-height: 0;
		overflow: hidden;
	}

	.toolbar {
		display: flex;
		gap: 4px;
		padding: 6px 12px;
		background: var(--bg-secondary, #1a1a2e);
		border-bottom: 1px solid var(--border, #333);
		flex-shrink: 0;
	}

	.toolbar button {
		background: var(--bg-tertiary, #222);
		border: 1px solid var(--border, #333);
		color: var(--text, #cdd6f4);
		padding: 2px 10px;
		border-radius: var(--radius, 4px);
		cursor: pointer;
		font-size: 14px;
		line-height: 1.4;
	}

	.toolbar button:hover {
		border-color: var(--accent, #7aa2f7);
		color: var(--accent, #7aa2f7);
	}

	.container {
		flex: 1;
		overflow: hidden;
		position: relative;
		display: flex;
		align-items: center;
		justify-content: center;
		cursor: grab;
		user-select: none;
		background: var(--bg-primary, #13131f);
	}

	.container.dragging {
		cursor: grabbing;
	}

	img {
		position: absolute;
		transform-origin: center center;
		max-width: none;
		max-height: none;
		display: block;
		pointer-events: none;
	}
</style>
