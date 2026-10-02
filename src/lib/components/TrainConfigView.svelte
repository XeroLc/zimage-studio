<script lang="ts">
	import { onMount } from 'svelte';
	import { store } from '#lib/store.svelte';
	import { pressable, riseIn, viewIn } from '#lib/actions';

	let selectedConfig = $state('');
	let localError = $state('');

	onMount(async () => {
		await store.loadTrainConfigs();
		if (!selectedConfig && store.trainConfigs.length) {
			selectedConfig = store.trainConfigs.includes('zimage_character_8gb.yml')
				? 'zimage_character_8gb.yml'
				: store.trainConfigs[0];
		}
	});

	async function onStart() {
		localError = '';
		if (!selectedConfig) {
			localError = '请选择一个训练配置';
			return;
		}
		try {
			await store.startTraining(selectedConfig);
		} catch (e) {
			localError = String((e as Error)?.message ?? e);
		}
	}
</script>

<div class="view scroll" transition:viewIn>
	{#if localError}
		<div class="errorbar">⚠ {localError}</div>
	{:else if store.train.state === 'error' && store.train.error}
		<div class="errorbar">⚠ {store.train.error}</div>
	{/if}
	<div class="card" use:riseIn>
		<h2>训练设置</h2>
		<div class="sub">启动 Z-Image 角色 LoRA 训练（ai-toolkit，约 4.4 秒/步）</div>

		<div class="field">
			<label for="tCfg">训练配置</label>
			<select id="tCfg" bind:value={selectedConfig}>
				{#each store.trainConfigs as c}
					<option value={c}>{c}</option>
				{/each}
			</select>
			<div class="hint">来自 ai-toolkit\config 的 *.yml，可先在其中调整步数等参数</div>
		</div>

		<div class="field">
			<label>数据集（角色图片 + 同名 .txt 描述）</label>
			<div class="dirline">
				<code>ai-toolkit\datasets\character\</code>
				<button class="btn btn-sm" onclick={() => store.openTrainDir('aitk_dataset')} use:pressable>
					打开文件夹
				</button>
			</div>
			<div class="hint">放入 10~30 张角色图片（同名 txt 写外观描述）后即可开始训练</div>
		</div>

		<div class="actions">
			<button class="btn" onclick={() => store.openTrainDir('aitk_out')} use:pressable>
				训练输出
			</button>
			<button class="btn btn-primary grow" onclick={onStart} use:pressable disabled={store.busy}>
				启动训练
			</button>
		</div>
		<div class="hint" style="margin-top:10px">训练与生图共用显存，同一时间只能运行一个</div>
	</div>
</div>
