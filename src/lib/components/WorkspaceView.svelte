<script lang="ts">
	import { onMount } from 'svelte';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { store } from '#lib/store.svelte';
	import { comfyWs } from '#lib/comfyws';
	import { pressable, pulse } from '#lib/actions';
	import FloatingBall from '#lib/components/FloatingBall.svelte';
	import { api } from '#lib/api';

	const win = getCurrentWindow();
	let copied = $state(false);
	let iframeLoaded = $state(false);
	let wsConnected = $state(false);

	onMount(() => {
		comfyWs.setUrl(store.workspaceUrl || store.status.url);
		const off = comfyWs.onStatus((c) => (wsConnected = c));
		wsConnected = comfyWs.connected;
		return off;
	});

	$effect(() => {
		// 服务地址变化时同步 WS
		comfyWs.setUrl(store.workspaceUrl || store.status.url);
	});

	function copyUrl() {
		navigator.clipboard?.writeText(store.status.url);
		copied = true;
		setTimeout(() => (copied = false), 1200);
	}
</script>

<div class="ws-shell">
	<header class="ws-topbar" data-tauri-drag-region>
		<button class="ws-back" onclick={() => store.exitWorkspace()} use:pressable title="返回控制台">
			<svg width="13" height="13" viewBox="0 0 14 14"
				><path
					d="M9 2 L4 7 L9 12"
					fill="none"
					stroke="currentColor"
					stroke-width="1.6"
					stroke-linecap="round"
				/></svg
			>
			控制台
		</button>
		<span class="ws-title">ComfyUI 工作区</span>
		<span class="badge" use:pulse={store.status.state}>
			<span class="dot {store.dotClass}"></span>
			<span>{store.stateLabel}</span>
		</span>
		<button class="ws-url" onclick={copyUrl} title="点击复制">
			{copied ? '已复制 ✓' : store.status.url}
		</button>
		<span class="spacer"></span>
		{#if !wsConnected}
			<span class="ws-wsbadge warn">连接中…</span>
		{:else}
			<span class="ws-wsbadge">实时通道已连接</span>
		{/if}
		<button
			class="btn btn-sm"
			onclick={() => (store.iframeKey++)}
			use:pressable
			title="重新加载界面"
		>
			⟳ 重载
		</button>
		<button class="btn btn-sm" onclick={() => api.openComfyWindow()} use:pressable>
			独立窗口
		</button>
		<button class="btn btn-sm" onclick={() => store.openBrowser()} use:pressable>浏览器</button>
		<button
			class="btn btn-sm btn-danger"
			onclick={() => store.stop()}
			use:pressable
			disabled={store.busy}
		>
			停止
		</button>
		<div class="wbtns">
			<div class="wbtn" onclick={() => win.minimize()} use:pressable title="最小化">
				<svg width="12" height="12" viewBox="0 0 12 12">
					<line x1="1" y1="6" x2="11" y2="6" stroke="currentColor" stroke-width="1.2" />
				</svg>
			</div>
			<div
				class="wbtn close"
				onclick={() => win.close()}
				use:pressable
				title="隐藏到系统托盘（ComfyUI 继续运行）"
			>
				<svg width="12" height="12" viewBox="0 0 12 12">
					<path d="M1 1 L11 11 M11 1 L1 11" stroke="currentColor" stroke-width="1.2" />
				</svg>
			</div>
		</div>
	</header>

	<div class="ws-body">
		{#if !iframeLoaded}
			<div class="ws-loading">
				<div class="spinner"></div>
				<span>正在载入 ComfyUI 界面…</span>
			</div>
		{/if}
		{#key store.iframeKey}
			<iframe
				class="ws-frame"
				src={store.workspaceUrl || store.status.url}
				title="ComfyUI"
				onload={() => (iframeLoaded = true)}
			></iframe>
		{/key}
	</div>

	<FloatingBall />
</div>

<style>
	.ws-shell {
		height: 100vh;
		display: grid;
		grid-template-rows: var(--topbar-h) 1fr;
		background: var(--bg-color);
		border: 1px solid var(--border-subtle);
		border-radius: 10px;
		overflow: hidden;
	}
	.ws-topbar {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 0 0 0 8px;
		background: var(--charcoal-600);
		border-bottom: 1px solid var(--border-subtle);
		font-size: 12.5px;
	}
	.ws-back {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		background: none;
		border: none;
		color: var(--fg-dim);
		font-size: 12.5px;
		font-family: inherit;
		cursor: pointer;
		padding: 4px 8px;
		border-radius: 6px;
	}
	.ws-back:hover {
		background: var(--charcoal-300);
		color: var(--fg);
	}
	.ws-title {
		font-weight: 600;
		color: var(--fg-dim);
		pointer-events: none;
	}
	.ws-url {
		background: var(--charcoal-700);
		border: 1px solid var(--border-subtle);
		border-radius: 999px;
		color: var(--accent);
		font-family: Consolas, monospace;
		font-size: 11.5px;
		padding: 2px 10px;
		cursor: pointer;
	}
	.ws-url:hover {
		color: var(--accent-hover);
	}
	.ws-wsbadge {
		font-size: 11px;
		color: var(--ok);
		border: 1px solid #2f6f57;
		border-radius: 999px;
		padding: 2px 8px;
		pointer-events: none;
	}
	.ws-wsbadge.warn {
		color: var(--warn);
		border-color: #8a6d1f;
	}
	.ws-body {
		position: relative;
		min-height: 0;
	}
	.ws-frame {
		width: 100%;
		height: 100%;
		border: none;
		display: block;
		background: #181818;
	}
	.ws-loading {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 10px;
		color: var(--fg-muted);
		font-size: 13px;
		background: var(--bg-color);
		z-index: 5;
	}
	.wbtns {
		display: flex;
		margin-left: 2px;
	}
</style>
