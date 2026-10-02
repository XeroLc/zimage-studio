<script lang="ts">
	import { onMount } from 'svelte';
	import { store } from '#lib/store.svelte';
	import { pressable, riseIn, viewIn } from '#lib/actions';
	import { open } from '@tauri-apps/plugin-dialog';
	import { api } from '#lib/api';

	let lastPhase = $state('');
	let syncing = $state(false);

	onMount(async () => {
		if (!store.setup) await store.refreshSetup();
	});

	async function syncNow() {
		if (syncing) return;
		syncing = true;
		try {
			const r = await api.syncManifest();
			store.toast(`清单已同步：v${r.version} · ${r.groups} 组 ${r.components} 个组件`, 4500);
			await store.refreshSetup(false);
		} catch (e) {
			store.toast(String((e as Error)?.message ?? e), 6000);
		} finally {
			syncing = false;
		}
	}

	$effect(() => {
		const ph = store.install.phase;
		if (ph === 'done' && lastPhase !== 'done') {
			store.toast('安装完成 ✓', 4000);
			void store.refreshSetup(false);
		}
		if (ph === 'error' && lastPhase !== 'error') {
			store.toast('安装出错：' + (store.install.error || '查看详情'), 6000);
			void store.refreshSetup(false);
		}
		lastPhase = ph;
	});

	function fmtMb(mb: number): string {
		return mb >= 1024 ? (mb / 1024).toFixed(1) + ' GB' : mb + ' MB';
	}

	function fmtBytes(b: number): string {
		if (!b) return '0 B';
		if (b >= 1073741824) return (b / 1073741824).toFixed(2) + ' GB';
		if (b >= 1048576) return (b / 1048576).toFixed(1) + ' MB';
		return (b / 1024).toFixed(0) + ' KB';
	}

	function fmtSpeed(bps: number): string {
		if (!bps) return '';
		return (bps / 1048576).toFixed(1) + ' MB/s';
	}

	async function pickRoot() {
		const picked = await open({ directory: true, title: '选择数据目录（存放 ComfyUI/模型/输出）' });
		if (typeof picked === 'string' && picked) {
			await store.setDataRoot(picked.replace(/\\/g, '/'));
		}
	}

	const install = $derived(store.install);

	// 下载源通道元信息（名称 + 候选列表，来自清单）
	const channelsMeta = $derived(store.setup?.source_candidates ?? []);

	type SourceKey = 'models' | 'github' | 'pypi' | 'torch';

	function sourceValue(channel: string): string {
		return store.config.sources[channel as SourceKey] ?? 'auto';
	}

	function onSourceChange(channel: string, value: string) {
		void store.setSource(channel as SourceKey, value);
	}

	// 当前生效源摘要行
	const effectiveLine = $derived.by(() => {
		const eff = store.setup?.effective ?? [];
		if (!eff.length) return '';
		const parts = eff.map((c) => `${chanLabel(c.channel)} ${c.name}`).join(' · ');
		const at = store.setup?.source_test_at ?? 0;
		if (at > 0) {
			const d = new Date(at * 1000);
			const p = (n: number) => String(n).padStart(2, '0');
			return `现行：${parts}（测速于 ${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}）`;
		}
		return `现行：${parts}`;
	});

	function chanLabel(channel: string): string {
		return channelsMeta.find((c) => c.channel === channel)?.name ?? channel;
	}

	function rowsFor(channel: string) {
		return (store.sourceTest.rows ?? []).filter((r) => r.channel === channel);
	}
</script>

<div class="view scroll" transition:viewIn>
	<div class="setup-wrap" use:riseIn>
		<div class="card setup-head">
			<h2>资源中心</h2>
			<div class="sub">
				一键 / 按需把整套生图与训练环境安装到数据目录 —— 换电脑装好客户端后，在这里勾选即可快速配置
			</div>

			<!-- ① 数据目录 -->
			<div class="head-section">
				<div class="head-label">数据目录</div>
				<div class="dirline">
					<code>{store.config.data_root || '（未设置）'}</code>
					<button class="btn btn-sm" onclick={pickRoot} use:pressable disabled={install.running}>
						选择…
					</button>
				</div>
				{#if !store.config.data_root}
					<div class="hint">
						建议：<button
							class="linklike"
							onclick={() => store.setDataRoot(store.setup?.suggested_root ?? 'D:/AI/image-gen')}
							disabled={install.running}
							>{store.setup?.suggested_root ?? 'D:/AI/image-gen'}</button
						>（点击直接使用）
					</div>
				{:else}
					<div class="hint">全部组件（引擎/模型/工作流/训练）都会装进这个目录</div>
				{/if}
			</div>

			<!-- ② 下载源 -->
			<div class="head-section">
				<div class="head-label">
					下载源
					<span class="head-label-hint">自动 = 测速优选最快线路；主源失败自动切换备用源</span>
				</div>
				<div class="src-grid">
					{#each channelsMeta as ch (ch.channel)}
						<div class="src-item">
							<label for="src-{ch.channel}">{ch.name}</label>
							<select
								id="src-{ch.channel}"
								value={sourceValue(ch.channel)}
								onchange={(e) => onSourceChange(ch.channel, e.currentTarget.value)}
								disabled={install.running || store.sourceTest.running}
							>
								<option value="auto">自动（测速优选）</option>
								{#each ch.candidates as c (c.id)}
									<option value={c.id}>{c.name}</option>
								{/each}
							</select>
						</div>
					{/each}
				</div>
				<div class="src-status">
					<button
						class="btn btn-sm"
						onclick={() => store.testSources()}
						use:pressable
						disabled={store.sourceTest.running || install.running}
					>
						{store.sourceTest.running ? '测速中…' : '测速并优选'}
					</button>
					<span class="src-effective">{effectiveLine}</span>
				</div>
				{#if store.sourceTest.rows}
					<div class="src-rows">
						{#each channelsMeta as ch (ch.channel)}
							<div class="src-row">
								<span class="src-row-name">{ch.name}</span>
								{#each rowsFor(ch.channel) as r (r.id)}
									<span class="src-chip" class:bad={!r.ok}>
										{r.name}{#if r.ok}
											· {r.latency_ms}ms · {(r.speed_kbps / 1024).toFixed(1)}MB/s
										{:else}
											· 不可达
										{/if}
									</span>
								{/each}
							</div>
						{/each}
					</div>
				{/if}
			</div>

			<!-- ③ 快速开始 -->
			<div class="head-section">
				<div class="head-label">快速开始</div>
				<div class="setup-actions">
					<span class="preset-label">套餐：</span>
					<button class="btn btn-sm" onclick={() => store.selectPreset('8gb')} use:pressable
						>8GB 显存（int8）</button
					>
					<button class="btn btn-sm" onclick={() => store.selectPreset('full')} use:pressable
						>全精度</button
					>
					<span class="spacer"></span>
					<span class="hint" style="margin:0"
						>清单{store.setup?.manifest_source === 'synced' ? '已同步' : '内置'} v{store.setup
							?.manifest_version ?? '—'} · {store.setup?.manifest_updated ?? ''}</span
					>
					<button class="btn btn-sm" onclick={syncNow} use:pressable disabled={syncing}
						>{syncing ? '同步中…' : '同步清单'}</button
					>
				</div>
			</div>
		</div>

		{#each store.setup?.groups ?? [] as group (group.id)}
			<div class="card group-card">
				<h3>{group.name}</h3>
				{#each group.components as c (c.id)}
					<div class="comp" class:disabled={install.running}>
						<label class="check comp-check">
							<input
								type="checkbox"
								checked={!!store.selected[c.id] && !c.installed}
								disabled={c.installed || install.running}
								onchange={() => store.toggleSelect(c.id)}
							/>
						</label>
						<div class="comp-main">
							<div class="comp-name">
								{c.name}
								{#if c.required}<span class="tag tag-req">必装</span>{/if}
								{#if c.preset === '8gb'}<span class="tag tag-8gb">8GB套餐</span>{/if}
								{#if c.preset === 'full'}<span class="tag">全精度套餐</span>{/if}
								{#if c.optional}<span class="tag">可选</span>{/if}
							</div>
							<div class="comp-desc">{c.desc}</div>
						</div>
						<div class="comp-size">{fmtMb(c.size_mb)}</div>
						<div class="comp-status">
							{#if c.installed}
								<span class="chip ok">已安装</span>
							{:else if install.running && install.comp_id === c.id}
								<span class="chip warn">安装中…</span>
							{:else if install.failed.includes(c.id)}
								<span class="chip err">失败</span>
							{:else}
								<span class="chip">未安装</span>
							{/if}
						</div>
					</div>
				{/each}
			</div>
		{/each}

		{#if !store.setup}
			<div class="card group-card"><div class="comp-desc">加载资源清单中…</div></div>
		{/if}
	</div>

	<div class="installbar">
		{#if install.running}
			<div class="ibar-main">
				<div class="ibar-info">
					<span class="spinner"></span>
					<b>{install.comp_name}</b>
					<span class="dim">{install.message}</span>
					{#if install.file_count > 1}
						<span class="dim">({install.file_index}/{install.file_count})</span>
					{/if}
					<span class="spacer"></span>
					<span class="dim"
						>{fmtBytes(install.downloaded)}{install.total
							? ' / ' + fmtBytes(install.total)
							: ''} · {fmtSpeed(install.speed)}</span
					>
				</div>
				<div class="progress-track slim">
					<div
						class="progress-bar"
						style="width:{install.phase === 'download' && install.total
							? Math.min(100, (install.downloaded / install.total) * 100).toFixed(1)
							: 100}%"
						class:indeterminate={install.phase !== 'download'}
					></div>
				</div>
				<div class="ibar-actions">
					<button class="btn btn-sm btn-danger" onclick={() => store.cancelInstall()} use:pressable>
						取消安装
					</button>
				</div>
			</div>
		{:else}
			<div class="ibar-main idle">
				<span class="dim"
					>已选 <b>{store.selectedIds.length}</b> 项 · 预计下载
					<b>{fmtMb(store.selectedSizeMb)}</b>（不含 PyTorch 等依赖的额外体积）</span
				>
				<span class="spacer"></span>
				{#if install.phase === 'error'}
					<span class="chip err" title={install.error}>上次安装出错</span>
				{/if}
				<button
					class="btn btn-primary"
					onclick={() => store.startInstall()}
					use:pressable
					disabled={!store.selectedIds.length || !store.config.data_root}
				>
					开始安装
				</button>
			</div>
		{/if}
	</div>
</div>

<style>
	.setup-wrap {
		max-width: 760px;
		margin: 0 auto;
		padding-bottom: 64px;
	}
	.setup-head .setup-actions {
		display: flex;
		align-items: center;
		gap: 8px;
		row-gap: 10px;
		flex-wrap: wrap;
	}
	.preset-label {
		font-size: 12px;
		color: var(--fg-muted);
		white-space: nowrap;
	}
	.setup-actions .hint {
		white-space: nowrap;
	}
	/* 头部三段式布局：数据目录 / 下载源 / 快速开始 */
	.head-section {
		padding: 14px 0 4px;
		border-top: 1px solid var(--border-subtle);
		margin-top: 14px;
	}
	.head-label {
		font-size: 12.5px;
		font-weight: 600;
		color: var(--fg-dim);
		margin-bottom: 10px;
		display: flex;
		align-items: baseline;
		gap: 8px;
	}
	.head-label-hint {
		font-size: 11px;
		font-weight: 400;
		color: var(--fg-muted);
	}
	.src-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 10px 14px;
	}
	.src-item {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.src-item label {
		font-size: 12px;
		color: var(--fg-dim);
		width: 52px;
		flex: none;
		text-align: right;
	}
	.src-item select {
		flex: 1;
		min-width: 0;
	}
	.src-status {
		display: flex;
		align-items: center;
		gap: 10px;
		margin-top: 10px;
		flex-wrap: wrap;
	}
	.src-effective {
		font-size: 11.5px;
		color: var(--fg-muted);
	}
	.src-rows {
		margin-top: 8px;
		display: flex;
		flex-direction: column;
		gap: 5px;
	}
	.src-row {
		display: flex;
		align-items: center;
		gap: 7px;
		flex-wrap: wrap;
	}
	.src-row-name {
		font-size: 11px;
		color: var(--fg-muted);
		width: 52px;
		flex: none;
		text-align: right;
	}
	.src-chip {
		font-size: 11px;
		color: #52d9a8;
		border: 1px solid #2f6f57;
		border-radius: 999px;
		padding: 1px 8px;
		white-space: nowrap;
	}
	.src-chip.bad {
		color: #ff9a94;
		border-color: #8a373a;
	}
	.group-card {
		margin-top: 14px;
		padding: 16px 18px;
	}
	.group-card h3 {
		font-size: 13px;
		font-weight: 600;
		color: var(--fg-dim);
		margin-bottom: 10px;
	}
	.comp {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 9px 4px;
		border-top: 1px solid var(--border-subtle);
	}
	.comp:first-of-type {
		border-top: none;
	}
	.comp.disabled {
		opacity: 0.75;
	}
	.comp-check {
		flex: none;
	}
	.comp-main {
		flex: 1;
		min-width: 0;
	}
	.comp-name {
		font-size: 13px;
		color: var(--fg);
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
	}
	.comp-desc {
		font-size: 11.5px;
		color: var(--fg-muted);
		margin-top: 3px;
	}
	.comp-size {
		flex: none;
		font-size: 12px;
		color: var(--fg-muted);
		font-variant-numeric: tabular-nums;
		min-width: 64px;
		text-align: right;
	}
	.comp-status {
		flex: none;
		width: 74px;
		text-align: right;
	}
	.tag {
		font-size: 10px;
		padding: 1px 6px;
		border-radius: 999px;
		border: 1px solid var(--border-color);
		color: var(--fg-muted);
	}
	.tag-req {
		border-color: #2f6f57;
		color: #52d9a8;
	}
	.tag-8gb {
		border-color: #1f5d8a;
		color: #57b8f5;
	}
	.chip {
		font-size: 11px;
		padding: 2px 8px;
		border-radius: 999px;
		border: 1px solid var(--border-color);
		color: var(--fg-muted);
		white-space: nowrap;
	}
	.chip.ok {
		border-color: #2f6f57;
		color: #52d9a8;
	}
	.chip.warn {
		border-color: #8a6d1f;
		color: var(--warn);
	}
	.chip.err {
		border-color: #8a373a;
		color: #ff9a94;
	}
	.linklike {
		background: none;
		border: none;
		color: var(--accent);
		cursor: pointer;
		font-size: 11px;
		padding: 0;
		font-family: inherit;
		text-decoration: underline;
	}
	.installbar {
		position: sticky;
		bottom: 0;
		margin: 18px auto 0;
		max-width: 760px;
		background: var(--charcoal-600);
		border: 1px solid var(--border-subtle);
		border-radius: var(--radius);
		padding: 12px 16px;
		display: block;
		box-shadow: 0 -8px 24px rgba(0, 0, 0, 0.35);
	}
	.ibar-main {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
	}
	.ibar-info {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 12.5px;
		width: 100%;
		min-width: 0;
	}
	.ibar-info b {
		font-weight: 600;
	}
	.ibar-info .dim {
		color: var(--fg-muted);
		font-size: 12px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.ibar-info .spacer {
		flex: 1;
	}
	.progress-track.slim {
		height: 6px;
		margin: 6px 0 2px;
		width: 100%;
	}
	.progress-bar.indeterminate {
		width: 100% !important;
		background: linear-gradient(
			90deg,
			var(--accent) 0%,
			var(--accent-hover) 50%,
			var(--accent) 100%
		);
		background-size: 200% 100%;
		animation: slide 1.4s linear infinite;
	}
	@keyframes slide {
		from {
			background-position: 200% 0;
		}
		to {
			background-position: 0 0;
		}
	}
	.ibar-actions {
		width: 100%;
		display: flex;
		justify-content: flex-end;
	}
	.ibar-main.idle {
		width: 100%;
	}
	.installbar .spinner {
		width: 13px;
		height: 13px;
	}
</style>
