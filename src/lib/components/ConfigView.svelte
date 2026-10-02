<script lang="ts">
	import { store } from '#lib/store.svelte';
	import { pressable, riseIn, viewIn } from '#lib/actions';
	import { open } from '@tauri-apps/plugin-dialog';

	let localError = $state('');
	let saveFlash = $state(false);

	const error = $derived(
		store.status.state === 'error' && store.status.error ? store.status.error : localError
	);

	async function onStart() {
		localError = '';
		try {
			await store.start();
		} catch (e) {
			localError = String((e as Error)?.message ?? e);
		}
	}

	async function onSave() {
		localError = '';
		try {
			await store.save();
			saveFlash = true;
			setTimeout(() => (saveFlash = false), 1400);
		} catch (e) {
			localError = String((e as Error)?.message ?? e);
		}
	}

	async function pickRoot() {
		try {
			const picked = await open({ directory: true, title: '选择数据目录（存放 ComfyUI/模型/输出）' });
			if (typeof picked === 'string' && picked) {
				await store.setDataRoot(picked.replace(/\\/g, '/'));
				localStorage.removeItem('zimg.quickgen.last');
			}
		} catch (e) {
			localError = String((e as Error)?.message ?? e);
		}
	}

	async function gotoSetup() {
		store.appMode = 'setup';
		await store.refreshSetup();
	}
</script>

<style>
	.aboutline {
		display: flex;
		align-items: center;
		gap: 10px;
		margin-top: 16px;
		padding-top: 12px;
		border-top: 1px solid var(--border-subtle);
		font-size: 11.5px;
	}
	.aboutline .dim {
		color: var(--fg-muted);
	}
	.aboutline .spacer {
		flex: 1;
	}
	.updnote {
		font-size: 11px;
		color: var(--fg-muted);
		margin-top: 8px;
		padding: 8px 10px;
		background: var(--charcoal-700);
		border: 1px solid var(--border-subtle);
		border-radius: 6px;
		white-space: pre-wrap;
		max-height: 90px;
		overflow-y: auto;
		user-select: text;
	}
</style>

<div class="view scroll" transition:viewIn>
	{#if error}
		<div class="errorbar">⚠ {error}</div>
	{/if}
	{#if !store.status.configured}
		<div class="errorbar" style="background:#2b2413;border-color:#8a6d1f;color:#fdab34">
			🛈 环境尚未配置：请到「资源中心」选择数据目录并一键安装（ComfyUI / 模型 / 工作流）
		</div>
	{/if}
	<div class="card" use:riseIn>
		<h2>启动设置</h2>
		<div class="sub">数据目录 · 启动参数 · 就绪后的打开方式</div>

		<div class="field">
			<label for="fRoot">数据目录（整套环境所在位置）</label>
			<div class="dirline">
				<code id="fRoot">{store.config.data_root || '（未设置 —— 到资源中心选择）'}</code>
				<button class="btn btn-sm" onclick={pickRoot} use:pressable>选择…</button>
				<button class="btn btn-sm" onclick={gotoSetup} use:pressable>资源中心</button>
			</div>
			<div class="hint">ComfyUI / 模型 / 工作流 / 训练环境 / 输出 都放在这里，换电脑可用资源中心一键装齐</div>
		</div>

		<div class="row">
			<div class="field">
				<label for="fHost">监听地址</label>
				<select id="fHost" bind:value={store.config.host}>
					<option value="127.0.0.1">仅本机 (127.0.0.1)</option>
					<option value="0.0.0.0">局域网可访问 (0.0.0.0)</option>
				</select>
			</div>
			<div class="field">
				<label for="fPort">端口</label>
				<input id="fPort" type="number" min="1024" max="65535" bind:value={store.config.port} />
			</div>
		</div>

		<div class="row">
			<div class="field">
				<label for="fVram">显存模式</label>
				<select id="fVram" bind:value={store.config.vram_mode}>
					<option value="auto">自动检测（推荐）</option>
					<option value="normal">标准（NormalVRAM）</option>
					<option value="low">低显存模式（--lowvram）</option>
					<option value="none">纯 CPU（--novram）</option>
				</select>
			</div>
			<div class="field">
				<label for="fPreview">采样预览</label>
				<select id="fPreview" bind:value={store.config.preview}>
					<option value="auto">自动（推荐）</option>
					<option value="latent2rgb">Latent2RGB（更快）</option>
					<option value="none">关闭</option>
				</select>
			</div>
		</div>

		<div class="row">
			<div class="field">
				<label for="fTarget">就绪后打开方式</label>
				<select id="fTarget" bind:value={store.config.open_target}>
					<option value="workspace">内嵌工作区（推荐，程序内打开）</option>
					<option value="window">应用独立窗口</option>
					<option value="browser">系统浏览器</option>
				</select>
			</div>
			<div class="field">
				<label for="fExtra">附加启动参数</label>
				<input
					id="fExtra"
					type="text"
					bind:value={store.config.extra_args}
					placeholder="例如：--fast --disable-smart-memory"
				/>
			</div>
		</div>

		<div class="field">
			<label class="check">
				<input type="checkbox" bind:checked={store.config.auto_enter} />
				就绪后自动打开（按上方方式）
			</label>
			<label class="check" style="margin-top:8px">
				<input type="checkbox" bind:checked={store.config.embed_support} />
				启用内嵌兼容模式（给 ComfyUI 加 --enable-cors-header，内嵌工作区必需）
			</label>
		</div>

		<div class="actions">
			<button class="btn" onclick={onSave} use:pressable>
				{saveFlash ? '已保存 ✓' : '保存配置'}
			</button>
			<button class="btn btn-primary grow" onclick={onStart} use:pressable disabled={store.busy}>
				启动 ComfyUI
			</button>
		</div>

		<div class="aboutline">
			<span class="dim">Z-Image Studio {store.appVersion ? 'v' + store.appVersion : ''}</span>
			<span class="spacer"></span>
			{#if store.update.installing}
				<span class="dim">正在下载更新… {store.update.progress}%（完成后自动重启）</span>
			{:else if store.update.available}
				<button class="btn btn-sm btn-primary" onclick={() => store.installUpdate()} use:pressable>
					更新到 v{store.update.version}
				</button>
			{:else}
				<button
					class="btn btn-sm"
					onclick={() => store.checkUpdate()}
					use:pressable
					disabled={store.update.checking}
				>
					{store.update.checking ? '检查中…' : '检查更新'}
				</button>
			{/if}
		</div>
		{#if store.update.available && store.update.notes}
			<div class="updnote">{store.update.notes.slice(0, 300)}</div>
		{/if}
	</div>
</div>
