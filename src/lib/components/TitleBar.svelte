<script lang="ts">
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import { store } from '#lib/store.svelte';
	import { pressable, pulse } from '#lib/actions';

	const win = getCurrentWindow();
	const minimize = () => win.minimize();
	const close = () => win.close();
</script>

<header class="topbar" data-tauri-drag-region>
	<img class="logo" src="/comfy-logo.svg" alt="ComfyUI" draggable="false" />
	<span class="title">Z-Image Studio</span>
	<div class="tabs">
		<div
			class="tab"
			class:active={store.appMode === 'image'}
			onclick={() => (store.appMode = 'image')}
			use:pressable
		>
			生图
		</div>
		<div
			class="tab"
			class:active={store.appMode === 'train'}
			onclick={() => (store.appMode = 'train')}
			use:pressable
		>
			训练
		</div>
		<div
			class="tab"
			class:active={store.appMode === 'setup'}
			onclick={() => {
				store.appMode = 'setup';
				void store.refreshSetup();
			}}
			use:pressable
		>
			资源中心
		</div>
	</div>
	<span class="spacer"></span>
	<span class="badge" use:pulse={store.status.state}>
		<span class="dot {store.dotClass}"></span>
		<span>{store.stateLabel}</span>
	</span>
	<div class="wbtns">
		<div class="wbtn" onclick={minimize} use:pressable title="最小化">
			<svg width="12" height="12" viewBox="0 0 12 12">
				<line x1="1" y1="6" x2="11" y2="6" stroke="currentColor" stroke-width="1.2" />
			</svg>
		</div>
		<div class="wbtn close" onclick={close} use:pressable title="隐藏到系统托盘（ComfyUI 继续运行）">
			<svg width="12" height="12" viewBox="0 0 12 12">
				<path d="M1 1 L11 11 M11 1 L1 11" stroke="currentColor" stroke-width="1.2" />
			</svg>
		</div>
	</div>
</header>
