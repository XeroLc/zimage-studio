<script lang="ts">
	import { store } from '#lib/store.svelte';
	import { pressable, viewIn } from '#lib/actions';

	let logEl: HTMLDivElement;

	$effect(() => {
		void store.log.length; // track log changes
		if (logEl) logEl.scrollTop = logEl.scrollHeight;
	});

	const stopping = $derived(store.status.state === 'stopping');
</script>

<div class="view scroll" transition:viewIn>
	<div class="starting-wrap">
		<div class="starting-head">
			{#if !stopping}
				<span class="spinner"></span>
			{/if}
			<span>{stopping ? '正在停止 ComfyUI …' : '正在启动 ComfyUI …'}</span>
		</div>
		<div class="log" bind:this={logEl}>{store.log.join('\n')}</div>
		<div class="actions">
			<button class="btn btn-danger" onclick={() => store.stop()} use:pressable disabled={store.busy}>
				取消启动
			</button>
		</div>
	</div>
</div>
