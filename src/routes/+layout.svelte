<script lang="ts">
	import '../app.css';

	let { children } = $props();
</script>

<svelte:head>
	<title>Z-Image Studio</title>
	<link rel="icon" href="/comfy-c.svg" />
</svelte:head>

{@render children()}
