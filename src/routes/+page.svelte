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
		// 2s 轮询，且窗口隐藏（托盘/后台）时暂停，避免无谓唤醒与刷新
		const tick = () => {
			if (!document.hidden) store.poll();
		};
		const timer = setInterval(tick, 2000);
		const onVis = () => {
			if (!document.hidden) store.poll();
		};
		document.addEventListener('visibilitychange', onVis);
		return () => {
			clearInterval(timer);
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
