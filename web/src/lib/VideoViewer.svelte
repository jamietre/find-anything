<script lang="ts">
	export let src: string;
</script>

<div class="video-panel">
	<!-- svelte-ignore a11y-media-has-caption -->
	<video controls {src} class="video-player">
		Your browser does not support the video tag.
	</video>
</div>

<style>
	.video-panel {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--bg);
		overflow: auto;
		padding: 16px;
	}

	.video-player {
		max-width: 100%;
		max-height: 100%;
		outline: none;
		border-radius: 4px;
	}
</style>
