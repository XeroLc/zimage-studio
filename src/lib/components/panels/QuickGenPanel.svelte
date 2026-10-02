<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { api } from '#lib/api';
	import { store } from '#lib/store.svelte';
	import { comfyWs } from '#lib/comfyws';
	import { pressable } from '#lib/actions';

	type Variant = 'int8' | 'bf16';

	const SIZES = [
		{ label: '1024 × 1024', w: 1024, h: 1024 },
		{ label: '832 × 1216（竖）', w: 832, h: 1216 },
		{ label: '1216 × 832（横）', w: 1216, h: 832 },
		{ label: '768 × 768', w: 768, h: 768 }
	];

	let prompt = $state('');
	let negative = $state('');
	let variant = $state<Variant>('int8');
	let sizeIdx = $state(0);
	let steps = $state(8);
	let seed = $state(-1);

	let running = $state(false);
	let progressPct = $state(0);
	let progressText = $state('');
	let error = $state('');
	let resultUrl = $state('');
	let resultInfo = $state('');
	let resultSub = $state('');
	let promptId = $state('');

	let template: Record<string, { class_type?: string; inputs: Record<string, unknown> }> | null = null;
	let lastRunAt = 0;

	const baseUrl = $derived(store.workspaceUrl || store.status.url);

	onMount(async () => {
		try {
			const raw = await api.quickgenTemplate();
			template = JSON.parse(raw);
		} catch (e) {
			error = '快速生图模板加载失败：' + String((e as Error)?.message ?? e);
		}
		try {
			const saved = localStorage.getItem('zimg.quickgen.last');
			if (saved) {
				const s = JSON.parse(saved);
				prompt = s.prompt ?? prompt;
				negative = s.negative ?? negative;
				variant = s.variant ?? variant;
				sizeIdx = s.sizeIdx ?? sizeIdx;
				steps = s.steps ?? steps;
			}
		} catch {
			/* ignore */
		}
		off = comfyWs.on(handleWs);
	});

	let off: (() => void) | null = null;
	onDestroy(() => off?.());

	function handleWs(msg: { type: string; data?: Record<string, unknown> }) {
		if (!running || !msg.data) return;
		if (msg.type === 'progress' && msg.data.prompt_id === promptId) {
			const value = Number(msg.data.value ?? 0);
			const max = Number(msg.data.max ?? steps);
			progressPct = max ? Math.round((value / max) * 100) : 0;
			progressText = `采样中 ${value}/${max}`;
		} else if (msg.type === 'executing') {
			if (msg.data.prompt_id === promptId) {
				if (!msg.data.node) {
					progressText = '收尾中…';
				} else {
					progressText = `执行节点 ${msg.data.node}`;
				}
			}
		} else if (msg.type === 'executed' && msg.data.prompt_id === promptId) {
			const out = msg.data.output as
				| { images?: { filename: string; subfolder: string; type: string }[] }
				| undefined;
			const img = out?.images?.[0];
			if (img) {
				const qs = new URLSearchParams({
					filename: img.filename,
					subfolder: img.subfolder ?? '',
					type: img.type ?? 'output'
				});
				resultUrl = `${baseUrl}/view?${qs.toString()}`;
				resultInfo = img.filename;
				resultSub = img.subfolder ?? '';
			}
		} else if (msg.type === 'execution_error' && msg.data.prompt_id === promptId) {
			error = '执行出错：' + String(msg.data.exception_message ?? '查看 ComfyUI 日志');
			finish(false);
		}
	}

	function saveLast() {
		try {
			localStorage.setItem(
				'zimg.quickgen.last',
				JSON.stringify({ prompt, negative, variant, sizeIdx, steps })
			);
		} catch {
			/* ignore */
		}
	}

	async function run() {
		if (!template || !prompt.trim() || running) return;
		error = '';
		resultUrl = '';
		progressPct = 0;
		progressText = '已提交…';
		running = true;
		saveLast();
		const p: Record<string, { class_type?: string; inputs: Record<string, unknown> }> = JSON.parse(
			JSON.stringify(template)
		);
		const int8 = variant === 'int8';
		p['1'].inputs.unet_name = int8
			? 'z_image_turbo_int8_convrot.safetensors'
			: 'z_image_turbo_bf16.safetensors';
		p['2'].inputs.clip_name = int8 ? 'qwen_3_4b_fp8_mixed.safetensors' : 'qwen_3_4b.safetensors';
		p['4'].inputs.text = prompt;
		p['5'].inputs.conditioning = ['4', 0];
		p['7'].inputs.width = SIZES[sizeIdx].w;
		p['7'].inputs.height = SIZES[sizeIdx].h;
		p['8'].inputs.steps = steps;
		p['8'].inputs.seed = seed >= 0 ? seed : Math.floor(Math.random() * 1e15);
		if (negative.trim()) {
			p['11'] = {
				class_type: 'CLIPTextEncode',
				inputs: { text: negative, clip: ['2', 0] }
			};
			p['8'].inputs.negative = ['11', 0];
		}
		try {
			const res = (await api.comfyApi('POST', '/prompt', {
				prompt: p,
				client_id: comfyWs.clientId
			})) as { prompt_id?: string; error?: unknown; node_errors?: unknown };
			if (!res.prompt_id) {
				throw new Error(JSON.stringify(res.error ?? res.node_errors ?? res).slice(0, 400));
			}
			promptId = res.prompt_id;
			lastRunAt = Date.now();
			// 兜底轮询（万一 WS 错过事件）
			void pollResult();
		} catch (e) {
			error = String((e as Error)?.message ?? e);
			finish(false);
		}
	}

	async function pollResult() {
		const id = promptId;
		for (let i = 0; i < 240; i++) {
			await new Promise((r) => setTimeout(r, 1500));
			if (!running || promptId !== id) return;
			try {
				const h = (await api.comfyApi('GET', `/history/${id}`)) as Record<
					string,
					{
						status?: { completed?: boolean; status_str?: string };
						outputs?: Record<string, { images?: { filename: string; subfolder: string; type: string }[] }>;
					}
				>;
				const entry = h[id];
				if (entry?.outputs) {
					for (const node of Object.values(entry.outputs)) {
						const img = node.images?.[0];
						if (img) {
							const qs = new URLSearchParams({
								filename: img.filename,
								subfolder: img.subfolder ?? '',
								type: img.type ?? 'output'
							});
							resultUrl = `${baseUrl}/view?${qs.toString()}`;
							resultInfo = img.filename;
							resultSub = img.subfolder ?? '';
							finish(true);
							return;
						}
					}
				}
				if (entry?.status?.status_str === 'error') {
					error = '执行出错（见 ComfyUI 日志）';
					finish(false);
					return;
				}
			} catch {
				/* keep polling */
			}
			if (Date.now() - lastRunAt > 6 * 60 * 1000) {
				error = '等待超时';
				finish(false);
				return;
			}
		}
	}

	function openResultFile() {
		if (!resultInfo) return;
		const dir = (store.status.comfy_dir || '').replace(/\\/g, '/').replace(/\/$/, '') + '/output';
		const rel = (resultSub ? resultSub.replace(/\\/g, '/') + '/' : '') + resultInfo;
		void api.openPath(`${dir}/${rel}`);
	}

	function finish(ok: boolean) {
		running = false;
		progressPct = ok ? 100 : progressPct;
		progressText = ok ? '完成' : '';
	}
</script>

<div class="qg">
	<div class="qg-field">
		<label for="qPrompt">提示词</label>
		<textarea
			id="qPrompt"
			rows="3"
			bind:value={prompt}
			placeholder="描述你想生成的画面…（Z-Image 中文提示词也支持）"
			disabled={running}
		></textarea>
	</div>

	<details class="qg-more">
		<summary>更多选项</summary>
		<div class="qg-field">
			<label for="qNeg">负面提示词（可选）</label>
			<textarea id="qNeg" rows="2" bind:value={negative} disabled={running}></textarea>
		</div>
	</details>

	<div class="qg-row">
		<div class="qg-field">
			<label for="qVar">模型</label>
			<select id="qVar" bind:value={variant} disabled={running}>
				<option value="int8">int8（8GB 推荐）</option>
				<option value="bf16">bf16 全精度</option>
			</select>
		</div>
		<div class="qg-field">
			<label for="qSize">尺寸</label>
			<select id="qSize" bind:value={sizeIdx} disabled={running}>
				{#each SIZES as s, i (s.label)}
					<option value={i}>{s.label}</option>
				{/each}
			</select>
		</div>
	</div>

	<div class="qg-row">
		<div class="qg-field narrow">
			<label for="qSteps">步数</label>
			<input id="qSteps" type="number" min="4" max="40" bind:value={steps} disabled={running} />
		</div>
		<div class="qg-field narrow">
			<label for="qSeed">种子</label>
			<input id="qSeed" type="number" bind:value={seed} disabled={running} />
		</div>
		<div class="qg-hint">-1 = 随机</div>
	</div>

	{#if error}
		<div class="qg-error">{error}</div>
	{/if}

	{#if running}
		<div class="qg-progress">
			<div class="progress-track slim">
				<div class="progress-bar" style="width:{progressPct}%"></div>
			</div>
			<div class="qg-progress-text">{progressText}</div>
		</div>
	{/if}

	<button class="btn btn-primary qg-run" onclick={run} use:pressable disabled={running || !prompt.trim()}>
		{running ? '生成中…' : '生成'}
	</button>

	{#if resultUrl}
		<div class="qg-result">
			<img src={resultUrl} alt="生成结果" />
			<div class="qg-result-bar">
				<span class="qg-result-name" title={resultInfo}>{resultInfo}</span>
				<button class="btn btn-sm" onclick={() => api.openPath('outputs')} use:pressable>
					打开目录
				</button>
				<button class="btn btn-sm" onclick={openResultFile} use:pressable>定位文件</button>
			</div>
		</div>
	{/if}
	<div class="qg-tip">结果同时保存在 output 目录，可在「作品画廊」里回看</div>
</div>

<style>
	.qg-field {
		margin-bottom: 10px;
		flex: 1;
	}
	.qg-field.narrow {
		max-width: 88px;
	}
	.qg-field label {
		display: block;
		font-size: 11.5px;
		color: var(--fg-dim);
		margin-bottom: 4px;
	}
	.qg textarea,
	.qg input[type='number'],
	.qg select {
		width: 100%;
		background: var(--input-bg);
		border: 1px solid var(--border-color);
		color: var(--fg-dim);
		border-radius: 6px;
		padding: 6px 9px;
		font-size: 12.5px;
		font-family: inherit;
		user-select: text;
		resize: vertical;
	}
	.qg textarea:focus,
	.qg input:focus,
	.qg select:focus {
		outline: none;
		border-color: var(--accent);
	}
	.qg-row {
		display: flex;
		gap: 8px;
		align-items: flex-end;
	}
	.qg-hint {
		font-size: 11px;
		color: var(--fg-muted);
		padding-bottom: 12px;
	}
	.qg-more summary {
		font-size: 11.5px;
		color: var(--fg-muted);
		cursor: pointer;
		margin-bottom: 6px;
	}
	.qg-run {
		width: 100%;
		margin-top: 4px;
	}
	.qg-error {
		font-size: 12px;
		color: #ff9a94;
		background: #962a2a33;
		border: 1px solid #962a2a;
		border-radius: 6px;
		padding: 7px 10px;
		margin-bottom: 10px;
		user-select: text;
		word-break: break-all;
	}
	.qg-progress {
		margin: 8px 0;
	}
	.qg-progress-text {
		font-size: 11.5px;
		color: var(--fg-muted);
		text-align: center;
	}
	.qg-result {
		margin-top: 12px;
		border: 1px solid var(--border-subtle);
		border-radius: 8px;
		overflow: hidden;
	}
	.qg-result img {
		width: 100%;
		display: block;
	}
	.qg-result-bar {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 8px;
		background: var(--charcoal-600);
	}
	.qg-result-name {
		flex: 1;
		font-size: 11px;
		color: var(--fg-muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.qg-result-bar a.btn {
		text-decoration: none;
	}
	.qg-tip {
		font-size: 10.5px;
		color: var(--fg-muted);
		margin-top: 8px;
		text-align: center;
	}
	.progress-track.slim {
		height: 6px;
		margin: 0;
	}
</style>
