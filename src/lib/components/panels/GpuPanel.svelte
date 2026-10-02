<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { api, type SysStats } from '#lib/api';

	let stats = $state<SysStats | null>(null);
	let timer: ReturnType<typeof setInterval> | null = null;

	async function refresh() {
		try {
			stats = await api.sysStats();
		} catch {
			/* ignore */
		}
	}

	onMount(() => {
		void refresh();
		timer = setInterval(refresh, 2000);
	});
	onDestroy(() => {
		if (timer) clearInterval(timer);
	});

	const memPct = $derived(
		stats && stats.mem_total_mb ? Math.round((stats.mem_used_mb / stats.mem_total_mb) * 100) : 0
	);
	const memLevel = $derived(memPct > 90 ? 'err' : memPct > 75 ? 'warn' : 'ok');
</script>

<div class="gp">
	{#if !stats}
		<div class="gp-empty">读取中…</div>
	{:else if !stats.available}
		<div class="gp-empty">未检测到 NVIDIA GPU（nvidia-smi 不可用）</div>
	{:else}
		<div class="gp-name">{stats.name}</div>

		<div class="gp-label">
			<span>显存</span><span class="gp-val">{(stats.mem_used_mb / 1024).toFixed(1)} / {(stats.mem_total_mb / 1024).toFixed(1)} GB（{memPct}%）</span>
		</div>
		<div class="gp-track"><div class="gp-bar {memLevel}" style="width:{memPct}%"></div></div>

		<div class="gp-label">
			<span>利用率</span><span class="gp-val">{stats.util}%</span>
		</div>
		<div class="gp-track"><div class="gp-bar ok" style="width:{stats.util}%"></div></div>

		<div class="gp-row">
			<span>温度 {stats.temp}°C</span>
			<span class="gp-dim">2 秒刷新</span>
		</div>
	{/if}
</div>

<style>
	.gp-name {
		font-size: 12.5px;
		color: var(--fg-dim);
		margin-bottom: 12px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.gp-label {
		display: flex;
		justify-content: space-between;
		font-size: 11.5px;
		color: var(--fg-muted);
		margin-bottom: 4px;
	}
	.gp-val {
		color: var(--fg-dim);
		font-variant-numeric: tabular-nums;
	}
	.gp-track {
		height: 8px;
		background: var(--charcoal-700);
		border: 1px solid var(--border-subtle);
		border-radius: 4px;
		overflow: hidden;
		margin-bottom: 12px;
	}
	.gp-bar {
		height: 100%;
		transition: width 0.6s ease;
	}
	.gp-bar.ok {
		background: linear-gradient(90deg, var(--accent), var(--accent-hover));
	}
	.gp-bar.warn {
		background: var(--warn);
	}
	.gp-bar.err {
		background: var(--err);
	}
	.gp-row {
		display: flex;
		justify-content: space-between;
		font-size: 12px;
		color: var(--fg-dim);
	}
	.gp-dim {
		color: var(--fg-muted);
		font-size: 11px;
	}
	.gp-empty {
		font-size: 12px;
		color: var(--fg-muted);
		text-align: center;
		padding: 20px 0;
	}
</style>
