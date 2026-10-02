<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { api } from '#lib/api';
	import { comfyWs } from '#lib/comfyws';
	import { pressable } from '#lib/actions';

	interface QueueEntry {
		promptId: string;
		text: string;
	}

	let runningItems = $state<QueueEntry[]>([]);
	let pendingItems = $state<QueueEntry[]>([]);
	let progress = $state({ value: 0, max: 0, node: '' });
	let error = $state('');
	let timer: ReturnType<typeof setInterval> | null = null;
	let off: (() => void) | null = null;

	function summarize(prompt: Record<string, unknown> | undefined): string {
		if (!prompt) return '(无提示)';
		for (const node of Object.values(prompt)) {
			const n = node as { class_type?: string; inputs?: { text?: string } };
			if (n?.class_type === 'CLIPTextEncode' && n.inputs?.text) {
				return n.inputs.text.slice(0, 60).replace(/\s+/g, ' ');
			}
		}
		return '(无提示)';
	}

	async function pollQueue() {
		try {
			const q = (await api.comfyApi('GET', '/queue')) as {
				queue_running?: unknown[][];
				queue_pending?: unknown[][];
			};
			const map = (arr: unknown[][] | undefined): QueueEntry[] =>
				(arr ?? []).map((e) => ({
					promptId: String(e[1] ?? ''),
					text: summarize(e[2] as Record<string, unknown>)
				}));
			runningItems = map(q.queue_running);
			pendingItems = map(q.queue_pending);
			if (!runningItems.length) progress = { value: 0, max: 0, node: '' };
			error = '';
		} catch (e) {
			error = String((e as Error)?.message ?? e);
		}
	}

	onMount(() => {
		void pollQueue();
		timer = setInterval(pollQueue, 1800);
		off = comfyWs.on((msg: { type: string; data?: Record<string, unknown> }) => {
			if (msg.type === 'progress' && msg.data) {
				progress = {
					value: Number(msg.data.value ?? 0),
					max: Number(msg.data.max ?? 0),
					node: String(msg.data.node ?? '')
				};
			}
			if (msg.type === 'executing' || msg.type === 'executed' || msg.type === 'status') {
				void pollQueue();
			}
		});
	});

	onDestroy(() => {
		if (timer) clearInterval(timer);
		off?.();
	});

	async function interrupt() {
		try {
			await api.comfyApi('POST', '/interrupt');
			void pollQueue();
		} catch (e) {
			error = String((e as Error)?.message ?? e);
		}
	}

	async function clearQueue() {
		try {
			await api.comfyApi('POST', '/queue', { clear: true });
			void pollQueue();
		} catch (e) {
			error = String((e as Error)?.message ?? e);
		}
	}

	const pct = $derived(progress.max ? Math.round((progress.value / progress.max) * 100) : 0);
</script>

<div class="qp">
	{#if error}
		<div class="qp-error">{error}</div>
	{/if}

	<div class="qp-counts">
		<span class="chip ok">运行中 {runningItems.length}</span>
		<span class="chip">排队 {pendingItems.length}</span>
	</div>

	{#if runningItems.length}
		<div class="qp-progress">
			<div class="progress-track slim">
				<div class="progress-bar" style="width:{pct}%"></div>
			</div>
			<div class="qp-progress-text">
				{#if progress.max}采样 {progress.value}/{progress.max} · {pct}%{:else}正在执行…{/if}
			</div>
			<div class="qp-item current">
				<div class="qp-item-text" title={runningItems[0].text}>{runningItems[0].text}</div>
			</div>
		</div>
	{/if}

	{#if pendingItems.length}
		<div class="qp-list">
			{#each pendingItems as item, i (item.promptId + i)}
				<div class="qp-item">
					<span class="qp-rank">{i + 1}</span>
					<div class="qp-item-text" title={item.text}>{item.text}</div>
				</div>
			{/each}
		</div>
	{/if}

	{#if !runningItems.length && !pendingItems.length && !error}
		<div class="qp-empty">队列空闲 —— 在 ComfyUI 里排个任务，或点悬浮球的「快速生图」</div>
	{/if}

	<div class="qp-actions">
		<button class="btn btn-sm" onclick={interrupt} use:pressable disabled={!runningItems.length}>
			中断当前
		</button>
		<button class="btn btn-sm" onclick={clearQueue} use:pressable disabled={!pendingItems.length}>
			清空排队
		</button>
	</div>
</div>

<style>
	.qp-counts {
		display: flex;
		gap: 8px;
		margin-bottom: 10px;
	}
	.chip {
		font-size: 11.5px;
		padding: 3px 10px;
		border-radius: 999px;
		border: 1px solid var(--border-color);
		color: var(--fg-muted);
	}
	.chip.ok {
		border-color: #2f6f57;
		color: #52d9a8;
	}
	.qp-progress {
		margin-bottom: 10px;
	}
	.progress-track.slim {
		height: 6px;
		margin: 0 0 6px;
	}
	.qp-progress-text {
		font-size: 11.5px;
		color: var(--fg-muted);
		text-align: center;
		margin-bottom: 6px;
	}
	.qp-list {
		display: flex;
		flex-direction: column;
		gap: 5px;
		max-height: 240px;
		overflow-y: auto;
	}
	.qp-item {
		display: flex;
		align-items: center;
		gap: 8px;
		background: var(--charcoal-700);
		border: 1px solid var(--border-subtle);
		border-radius: 6px;
		padding: 6px 9px;
	}
	.qp-item.current {
		border-color: #2f6f57;
	}
	.qp-rank {
		font-size: 11px;
		color: var(--fg-muted);
		min-width: 14px;
	}
	.qp-item-text {
		font-size: 12px;
		color: var(--fg-dim);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.qp-empty {
		font-size: 12px;
		color: var(--fg-muted);
		text-align: center;
		padding: 18px 0;
	}
	.qp-actions {
		display: flex;
		gap: 8px;
		margin-top: 12px;
	}
	.qp-error {
		font-size: 12px;
		color: #ff9a94;
		margin-bottom: 8px;
	}
</style>
