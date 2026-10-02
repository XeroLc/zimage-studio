<script lang="ts">
	import { store } from '#lib/store.svelte';
	import { pressable, riseIn, viewIn } from '#lib/actions';
	import gsap from 'gsap';

	let showLog = $state(false);
	let logEl: HTMLDivElement;
	let barEl: HTMLDivElement | undefined = $state();

	// GSAP-smooth progress bar
	$effect(() => {
		const pct = store.progress?.pct ?? 0;
		if (barEl) gsap.to(barEl, { width: pct + '%', duration: 0.6, ease: 'power1.out' });
	});

	$effect(() => {
		void store.trainLog.length; // track log changes
		if (logEl && showLog) logEl.scrollTop = logEl.scrollHeight;
	});

	const st = $derived(store.train.state);
	const label = $derived(
		st === 'running'
			? '训练进行中'
			: st === 'stopping'
				? '正在停止训练…'
				: st === 'completed'
					? '训练完成'
					: st === 'error'
						? '训练出错'
						: '训练'
	);
</script>

<div class="view" transition:viewIn>
	<div class="running-grid">
		<div class="runbar">
			<span
				class="dot {st === 'running' || st === 'completed'
					? 'ok'
					: st === 'error'
						? 'err'
						: 'warn'}"></span>
			<span>{label}</span>
			{#if store.train.config_file}
				<span class="url-mini" style="color:var(--fg-muted)">{store.train.config_file}</span>
			{/if}
			<span class="spacer"></span>
			<button class="btn btn-sm" onclick={() => (showLog = !showLog)} use:pressable>
				{showLog ? '返回' : '训练日志'}
			</button>
			{#if st === 'running' || st === 'stopping'}
				<button
					class="btn btn-sm btn-danger"
					onclick={() => store.stopTraining()}
					use:pressable
					disabled={store.busy}
				>
					停止训练
				</button>
			{:else if st === 'completed'}
				<button class="btn btn-sm btn-primary" onclick={() => store.openTrainDir('aitk_out')} use:pressable>
					打开输出文件夹
				</button>
			{/if}
		</div>
		<div class="runcenter">
			<div class="status-card" use:riseIn>
				{#if st === 'completed'}
					<div class="pulse-icon"><span class="dot ok"></span></div>
					<h2>训练完成 ✓</h2>
					<div class="hint-line" style="margin: 2px 0 16px">
						LoRA 已保存到 outputs\training，<br />复制到 models\loras 后即可在生图工作流中加载使用
					</div>
					<div class="running-actions">
						<button
							class="btn btn-primary"
							onclick={() => store.openTrainDir('aitk_out')}
							use:pressable
						>
							打开输出文件夹
						</button>
						<button class="btn" onclick={() => store.stopTraining()} use:pressable>返回</button>
					</div>
				{:else}
					<div class="pulse-icon"><span class="dot {st === 'error' ? 'err' : 'ok'}"></span></div>
					<h2>{label}</h2>
					{#if store.progress}
						<div class="progress-track"><div class="progress-bar" bind:this={barEl}></div></div>
						<div class="progress-meta">
							<span>{store.progress.step} / {store.progress.total} 步 · {store.progress.pct}%</span>
							<span>loss {store.progress.loss}</span>
							<span>{store.progress.rate}</span>
						</div>
						<div class="progress-meta dim">
							<span>已用 {store.progress.elapsed}</span>
							<span>预计剩余 {store.progress.eta}</span>
						</div>
					{:else}
						<div class="hint-line">正在初始化（加载模型与缓存数据集，首次约 1~2 分钟）…</div>
					{/if}
					{#if st === 'error' && store.train.error}
						<div class="errorbar" style="margin: 16px auto 0; max-width: none">{store.train.error}</div>
					{/if}
				{/if}
			</div>
		</div>
		{#if showLog}
			<div class="logpanel">
				<div class="logpanel-head">
					<span>训练日志（自动滚动）</span>
					<span class="spacer"></span>
					<button class="btn btn-sm" onclick={() => (showLog = false)} use:pressable>返回</button>
				</div>
				<div class="log" bind:this={logEl}>{store.trainLog.join('\n')}</div>
			</div>
		{/if}
	</div>
</div>
