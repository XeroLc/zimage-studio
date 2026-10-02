<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import TitleBar from '#lib/components/TitleBar.svelte';
	import ConfigView from '#lib/components/ConfigView.svelte';
	import StartingView from '#lib/components/StartingView.svelte';
	import RunningView from '#lib/components/RunningView.svelte';
	import TrainConfigView from '#lib/components/TrainConfigView.svelte';
	import TrainRunningView from '#lib/components/TrainRunningView.svelte';
	import SetupView from '#lib/components/SetupView.svelte';
	import WorkspaceView from '#lib/components/WorkspaceView.svelte';
	import { store } from '#lib/store.svelte';
	import { viewIn } from '#lib/actions';

	onMount(() => {
		store.init();
		store.poll();
		// 自适应轮询：空闲（已停止/错误）5s、启动/运行中 1s；窗口隐藏（托盘/后台）时暂停
		let timer: ReturnType<typeof setTimeout> | null = null;
		const tick = async () => {
			if (!document.hidden) await store.poll();
			const s = store.status.state;
			const delay = s === 'stopped' || s === 'error' ? 5000 : 1000;
			timer = setTimeout(tick, delay);
		};
		timer = setTimeout(tick, 1200);
		const onVis = () => {
			if (!document.hidden) store.poll();
		};
		document.addEventListener('visibilitychange', onVis);
		return () => {
			if (timer) clearTimeout(timer);
			document.removeEventListener('visibilitychange', onVis);
			store.destroy();
		};
	});
</script>

{#if store.workspace}
	<WorkspaceView />
{:else}
	<div class="shell">
		<TitleBar />
		<main>
			{#if store.appMode === 'image'}
				{#if store.view === 'config'}
					<ConfigView />
				{:else if store.view === 'starting'}
					<StartingView />
				{:else}
					<RunningView />
				{/if}
			{:else if store.appMode === 'train'}
				{#if store.trainView === 'config'}
					<TrainConfigView />
				{:else}
					<TrainRunningView />
				{/if}
			{:else}
				<SetupView />
			{/if}
		</main>
	</div>
{/if}

{#if store.notice}
	<div class="toast" transition:viewIn>{store.notice}</div>
{/if}
