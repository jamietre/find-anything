<script lang="ts">
	import '../app.css';
	// SvelteKit passes params to every layout/page component. Declare it to avoid
	// the runtime "unknown prop" warning. Assigned to _params to signal that it
	// is intentionally unused (access via $page.params if ever needed).
	export let params: Record<string, string>;
	const _params = params;
</script>

<slot />
