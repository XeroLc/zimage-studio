<script lang="ts">
	import { api } from '#lib/api';
	import { store } from '#lib/store.svelte';
	import { pressable } from '#lib/actions';

	let copied = $state(false);

	const dirs: { kind: string; label: string }[] = [
		{ kind: 'data_root', label: '数据目录' },
		{ kind: 'comfy', label: 'ComfyUI' },
		{ kind: 'models', label: '模型库' },
		{ kind: 'workflows', label: '工作流' },
		{ kind: 'outputs', label: '输出图片' },
		{ kind: 'loras', label: 'LoRA' },
		{ kind: 'aitk_out', label: '训练输出' },
		{ kind: 'logs', label: '运行日志' }
	];

	async function openKind(kind: string) {
		try {
			await api.openPath(kind);
		} catch (e) {
			store.toast(String((e as Error)?.message ?? e));
		}
	}

	function copyUrl() {
		navigator.clipboard?.writeText(store.status.url);
		copied = true;
		setTimeout(() => (copied = false), 1200);
	}
</script>

<div class="tp">
	<div class="tp-section">打开目录</div>
	<div class="tp-grid">
		{#each dirs as d (d.kind)}
			<button class="btn btn-sm" onclick={() => openKind(d.kind)} use:pressable>{d.label}</button>
		{/each}
	</div>

	<div class="tp-section">操作</div>
	<div class="tp-grid">
		<button class="btn btn-sm" onclick={copyUrl} use:pressable>
			{copied ? '已复制 ✓' : '复制服务地址'}
		</button>
		<button class="btn btn-sm" onclick={() => (store.iframeKey++)} use:pressable>重载界面</button>
		<button class="btn btn-sm" onclick={() => api.openComfyWindow()} use:pressable>独立窗口</button>
		<button class="btn btn-sm" onclick={() => store.openBrowser()} use:pressable>浏览器打开</button>
	</div>

	<div class="tp-section">会话</div>
	<div class="tp-grid">
		<button class="btn btn-sm" onclick={() => store.exitWorkspace()} use:pressable>
			返回控制台
		</button>
		<button
			class="btn btn-sm btn-danger"
			onclick={() => store.stop()}
			use:pressable
			disabled={store.busy}
		>
			停止 ComfyUI
		</button>
	</div>

	<div class="tp-tip">
		ComfyUI 本身继续在其窗口里操作；本面板的功能都独立于 ComfyUI，通过其 API 工作
	</div>
</div>

<style>
	.tp-section {
		font-size: 11px;
		color: var(--fg-muted);
		margin: 10px 0 7px;
	}
	.tp-section:first-child {
		margin-top: 0;
	}
	.tp-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 7px;
	}
	.tp-tip {
		font-size: 10.5px;
		color: var(--fg-muted);
		margin-top: 12px;
		line-height: 1.5;
	}
</style>
