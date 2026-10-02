<script lang="ts">
	import { store } from '#lib/store.svelte';
	import { pressable, riseIn, viewIn } from '#lib/actions';
	import { api } from '#lib/api';

	let showLog = $state(false);

	$effect(() => {
		void store.log.length; // track log changes
	});
</script>

<div class="view" transition:viewIn>
	<div class="running-grid">
		<div class="runbar">
			<span class="dot ok"></span>
			<span>ComfyUI 运行中{store.status.external ? '（外部实例）' : ''}</span>
			<span class="spacer"></span>
			<button class="btn btn-sm" onclick={() => (showLog = !showLog)} use:pressable>
				{showLog ? '返回' : '运行日志'}
			</button>
			<button
				class="btn btn-sm btn-danger"
				onclick={() => store.stop()}
				use:pressable
				disabled={store.busy}
				title={store.status.external ? '将按端口定位进程并结束' : '结束本应用启动的实例'}
			>
				停止服务
			</button>
		</div>
		<div class="runcenter">
			<div class="status-card" use:riseIn>
				<div class="pulse-icon"><span class="dot ok"></span></div>
				<h2>ComfyUI 已就绪</h2>
				<div class="url" onclick={() => api.openPath('outputs')} title="点击打开输出目录"
					>{store.status.url}</div
				>
				<div class="running-actions">
					<button class="btn btn-primary" onclick={() => store.enterWorkspace()} use:pressable>
						进入工作区
					</button>
					<button class="btn" onclick={() => store.openBrowser()} use:pressable> 浏览器打开 </button>
				</div>
				<div class="running-actions" style="margin-top:10px">
					<button class="btn btn-sm" onclick={() => api.openComfyWindow()} use:pressable>
						独立窗口打开
					</button>
					<button class="btn btn-sm" onclick={() => api.openPath('outputs')} use:pressable>
						打开输出目录
					</button>
					<button class="btn btn-sm" onclick={() => api.openPath('models')} use:pressable>
						打开模型目录
					</button>
				</div>
				<div class="hint-line">
					工作区=内嵌界面 + 悬浮球工具（快速生图/队列/显存/画廊）；关闭窗口不影响服务运行
				</div>
			</div>
		</div>
		{#if showLog}
			<div class="logpanel">
				<div class="logpanel-head">
					<span>运行日志（自动滚动）</span>
					<span class="spacer"></span>
					<button class="btn btn-sm" onclick={() => (showLog = false)} use:pressable>
						返回
					</button>
				</div>
				<div class="log">{store.log.join('\n')}</div>
			</div>
		{/if}
	</div>
</div>
