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
		const timer = setInterval(() => store.poll(), 1000);
		return () => {
			clearInterval(timer);
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
