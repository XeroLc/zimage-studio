<script lang="ts">
	import '../app.css';

	let { children } = $props();
</script>

<svelte:head>
	<title>Z-Image Studio · 本地 AI 生图工作台</title>
</svelte:head>

{@render children()}
