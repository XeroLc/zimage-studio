<script lang="ts">
	import { onMount } from 'svelte';
	import { api, type OutputItem, type OutputMeta } from '#lib/api';
	import { store } from '#lib/store.svelte';
	import { pressable } from '#lib/actions';

	let loading = $state(true);
	let selected = $state<OutputItem | null>(null);
	let meta = $state<OutputMeta | null>(null);
	let metaLoading = $state(false);
	let broken = $state<Record<string, boolean>>({});

	const base = $derived(store.workspaceUrl || store.status.url);

	function viewUrl(item: OutputItem, preview = false): string {
		const qs = new URLSearchParams({
			filename: item.filename,
			subfolder: item.subfolder ?? '',
			type: 'output'
		});
		if (preview) qs.set('preview', 'webp');
		return `${base}/view?${qs.toString()}`;
	}

	async function refresh() {
		loading = true;
		await store.refreshOutputs();
		loading = false;
	}

	onMount(refresh);

	async function open(item: OutputItem) {
		selected = item;
		meta = null;
		metaLoading = true;
		try {
			meta = await api.outputMeta(item.rel);
		} catch {
			meta = null;
		}
		metaLoading = false;
	}

	async function del(item: OutputItem) {
		if (!confirm(`删除这张图？\n${item.rel}`)) return;
		try {
			await api.deleteOutput(item.rel);
			if (selected?.rel === item.rel) selected = null;
			await store.refreshOutputs();
		} catch (e) {
			alert('删除失败：' + String((e as Error)?.message ?? e));
		}
	}

	function fmtSize(b: number) {
		return b >= 1048576 ? (b / 1048576).toFixed(2) + ' MB' : (b / 1024).toFixed(0) + ' KB';
	}
	function fmtTime(t: number) {
		if (!t) return '';
		const d = new Date(t * 1000);
		const p = (n: number) => String(n).padStart(2, '0');
		return `${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
	}
</script>

<div class="gl">
	<div class="gl-head">
		<span class="gl-count">{store.outputs.length} 张（最近）</span>
		<span class="spacer"></span>
		<button class="btn btn-sm" onclick={() => refresh()} use:pressable disabled={loading}>
			{loading ? '读取中…' : '刷新'}
		</button>
		<button class="btn btn-sm" onclick={() => api.openPath('outputs')} use:pressable>目录</button>
	</div>

	{#if selected}
		<div class="gl-detail">
			<img class="gl-big" src={viewUrl(selected)} alt={selected.filename} />
			<div class="gl-meta">
				<div class="gl-file" title={selected.rel}>{selected.rel}</div>
				<div class="gl-info">
					<span>{fmtSize(selected.size)}</span>
					<span>{fmtTime(selected.mtime)}</span>
					{#if meta?.width}<span>{meta.width}×{meta.height}</span>{/if}
					{#if meta?.seed !== null && meta?.seed !== undefined}<span>seed {meta.seed}</span>{/if}
					{#if meta?.steps}<span>{meta.steps} 步</span>{/if}
				</div>
				{#if meta?.model}<div class="gl-model" title={meta.model}>{meta.model}</div>{/if}
				{#if metaLoading}
					<div class="gl-prompt dim">读取参数中…</div>
				{:else if meta?.texts?.length}
					<div class="gl-prompt" title={meta.texts.join('\n---\n')}>
						{meta.texts[0]}
						{#if meta.texts.length > 1}
							<span class="dim">（+{meta.texts.length - 1} 段）</span>
						{/if}
					</div>
				{/if}
				<div class="gl-actions">
					<button class="btn btn-sm" onclick={() => (selected = null)} use:pressable>
						返回列表
					</button>
					<span class="spacer"></span>
					<button class="btn btn-sm btn-danger" onclick={() => del(selected!)} use:pressable>
						删除
					</button>
				</div>
			</div>
		</div>
	{:else if loading}
		<div class="gl-empty">读取输出目录…</div>
	{:else if !store.outputs.length}
		<div class="gl-empty">还没有生成过图片 —— 用「快速生图」试试</div>
	{:else}
		<div class="gl-grid">
			{#each store.outputs as item (item.rel)}
				<div
					class="gl-cell"
					onclick={() => open(item)}
					onkeydown={(e) => e.key === 'Enter' && open(item)}
					role="button"
					tabindex="0"
					title={item.rel}
				>
					{#if !broken[item.rel]}
						<img
							src={viewUrl(item, true)}
							loading="lazy"
							alt={item.filename}
							onerror={() => (broken = { ...broken, [item.rel]: true })}
						/>
					{:else}
						<div class="gl-fallback">{item.filename.slice(-14)}</div>
					{/if}
				</div>
			{/each}
		</div>
	{/if}
</div>

<style>
	.gl-head {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-bottom: 10px;
	}
	.gl-head .spacer {
		flex: 1;
	}
	.gl-count {
		font-size: 12px;
		color: var(--fg-muted);
	}
	.gl-grid {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 7px;
		max-height: 380px;
		overflow-y: auto;
	}
	.gl-cell {
		aspect-ratio: 1;
		border: 1px solid var(--border-subtle);
		border-radius: 6px;
		overflow: hidden;
		cursor: pointer;
		background: var(--charcoal-800);
	}
	.gl-cell:hover {
		border-color: var(--accent);
	}
	.gl-cell img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		display: block;
	}
	.gl-fallback {
		font-size: 10px;
		color: var(--fg-muted);
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100%;
		padding: 4px;
		word-break: break-all;
	}
	.gl-empty {
		font-size: 12px;
		color: var(--fg-muted);
		text-align: center;
		padding: 26px 0;
	}
	.gl-detail {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.gl-big {
		width: 100%;
		max-height: 340px;
		object-fit: contain;
		background: var(--charcoal-800);
		border-radius: 8px;
		border: 1px solid var(--border-subtle);
	}
	.gl-file {
		font-size: 11px;
		color: var(--fg-muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.gl-info {
		display: flex;
		flex-wrap: wrap;
		gap: 10px;
		font-size: 11.5px;
		color: var(--fg-dim);
		margin-top: 4px;
	}
	.gl-model {
		font-size: 11px;
		color: var(--accent);
		margin-top: 4px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.gl-prompt {
		font-size: 11.5px;
		color: var(--fg-dim);
		background: var(--charcoal-700);
		border: 1px solid var(--border-subtle);
		border-radius: 6px;
		padding: 7px 9px;
		margin-top: 4px;
		max-height: 72px;
		overflow: hidden;
		user-select: text;
	}
	.gl-prompt.dim,
	.dim {
		color: var(--fg-muted);
	}
	.gl-actions {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-top: 6px;
	}
	.gl-actions .spacer {
		flex: 1;
	}
</style>
