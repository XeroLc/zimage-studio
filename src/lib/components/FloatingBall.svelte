<script lang="ts">
	import { onMount } from 'svelte';
	import { pressable } from '#lib/actions';
	import QuickGenPanel from '#lib/components/panels/QuickGenPanel.svelte';
	import QueuePanel from '#lib/components/panels/QueuePanel.svelte';
	import GpuPanel from '#lib/components/panels/GpuPanel.svelte';
	import GalleryPanel from '#lib/components/panels/GalleryPanel.svelte';
	import ToolsPanel from '#lib/components/panels/ToolsPanel.svelte';

	type PanelId = '' | 'quickgen' | 'queue' | 'gpu' | 'gallery' | 'tools';

	let menuOpen = $state(false);
	let activePanel = $state<PanelId>('');
	let ballPos = $state({ x: 0, y: 0 }); // 相对右下角（负值偏移）
	let panelPos = $state({ x: 0, y: 0 });
	let dragging = $state(false);

	// 面板元信息
	const panels: { id: Exclude<PanelId, ''>; label: string; title: string }[] = [
		{ id: 'quickgen', label: '快速生图', title: '快速生图' },
		{ id: 'queue', label: '任务队列', title: '任务队列' },
		{ id: 'gpu', label: '显存监控', title: '显存 / GPU' },
		{ id: 'gallery', label: '作品画廊', title: '作品画廊' },
		{ id: 'tools', label: '快捷工具', title: '快捷工具' }
	];

	onMount(() => {
		try {
			const saved = localStorage.getItem('zimg.ball.pos');
			if (saved) ballPos = JSON.parse(saved);
			const ps = localStorage.getItem('zimg.panel.pos');
			if (ps) panelPos = JSON.parse(ps);
		} catch {
			/* ignore */
		}
	});

	function savePos() {
		try {
			localStorage.setItem('zimg.ball.pos', JSON.stringify(ballPos));
		} catch {
			/* ignore */
		}
	}

	function startBallDrag(e: PointerEvent) {
		e.preventDefault();
		dragging = false;
		const startX = e.clientX;
		const startY = e.clientY;
		const ox = ballPos.x;
		const oy = ballPos.y;
		let moved = false;
		const move = (ev: PointerEvent) => {
			const dx = ev.clientX - startX;
			const dy = ev.clientY - startY;
			if (!moved && Math.hypot(dx, dy) > 4) moved = true;
			if (moved) {
				dragging = true;
				ballPos = { x: ox + dx, y: oy + dy };
			}
		};
		const up = () => {
			window.removeEventListener('pointermove', move);
			window.removeEventListener('pointerup', up);
			if (moved) {
				savePos();
				setTimeout(() => (dragging = false), 0);
			} else {
				menuOpen = !menuOpen;
			}
		};
		window.addEventListener('pointermove', move);
		window.addEventListener('pointerup', up);
	}

	function startPanelDrag(e: PointerEvent) {
		e.preventDefault();
		const startX = e.clientX;
		const startY = e.clientY;
		const ox = panelPos.x;
		const oy = panelPos.y;
		const move = (ev: PointerEvent) => {
			panelPos = { x: ox + (ev.clientX - startX), y: oy + (ev.clientY - startY) };
		};
		const up = () => {
			window.removeEventListener('pointermove', move);
			window.removeEventListener('pointerup', up);
			try {
				localStorage.setItem('zimg.panel.pos', JSON.stringify(panelPos));
			} catch {
				/* ignore */
			}
		};
		window.addEventListener('pointermove', move);
		window.addEventListener('pointerup', up);
	}

	function openPanel(id: PanelId) {
		activePanel = activePanel === id ? '' : id;
		menuOpen = false;
	}

	const iconFor: Record<Exclude<PanelId, ''>, string> = {
		quickgen: 'M13 2 L5 13 L11 13 L9 22 L19 9 L13 9 Z',
		queue: 'M4 6 h16 M4 12 h16 M4 18 h10',
		gpu: 'M4 15 h4 v4 h-4 z M10 13 h4 v6 h-4 z M16 10 h4 v9 h-4 z',
		gallery: 'M3 5 h18 v14 h-18 z M3 15 L9 10 L14 14 L17 12 L21 15',
		tools:
			'M14.7 6.3 a4 4 0 0 0-5.4 5.4 L4 17 v3 h3 l5.3-5.3 a4 4 0 0 0 5.4-5.4 l-2.8 2.8 -2.5-2.5 z'
	};
</script>

<!-- 悬浮球（方砖：ComfyUI charcoal 风格） -->
<button
	class="fball"
	class:dragging
	class:menu-open={menuOpen}
	style="right:{18 - ballPos.x}px; bottom:{18 - ballPos.y}px"
	onpointerdown={startBallDrag}
	use:pressable
	title="特色功能（拖动可调整位置）"
>
	<svg width="20" height="20" viewBox="0 0 24 24"
		><rect x="3.5" y="3.5" width="7.4" height="7.4" rx="1.6" fill="currentColor" /><rect
			x="13.1"
			y="3.5"
			width="7.4"
			height="7.4"
			rx="1.6"
			fill="currentColor"
			opacity="0.55"
		/><rect
			x="3.5"
			y="13.1"
			width="7.4"
			height="7.4"
			rx="1.6"
			fill="currentColor"
			opacity="0.55"
		/><rect x="13.1" y="13.1" width="7.4" height="7.4" rx="1.6" fill="currentColor" /></svg
	>
</button>

<!-- 径向菜单 -->
{#if menuOpen}
	<div
		class="fmenu"
		style="right:{18 - ballPos.x + 8}px; bottom:{18 - ballPos.y + 74}px"
	>
		{#each panels as p (p.id)}
			<button
				class="fmenu-item"
				class:active={activePanel === p.id}
				onclick={() => openPanel(p.id)}
				use:pressable
			>
				<svg width="15" height="15" viewBox="0 0 24 24"
					><path
						d={iconFor[p.id]}
						fill="none"
						stroke="currentColor"
						stroke-width="1.8"
						stroke-linecap="round"
						stroke-linejoin="round"
					/></svg
				>
				<span>{p.label}</span>
			</button>
		{/each}
	</div>
{/if}

<!-- 浮动面板 -->
{#if activePanel}
	<div
		class="fpanel"
		class:wide={activePanel === 'gallery'}
		style="right:{80 - panelPos.x}px; top:{56 + panelPos.y}px"
	>
		<div class="fpanel-head" onpointerdown={startPanelDrag}>
			<span class="fpanel-title">{panels.find((p) => p.id === activePanel)?.title}</span>
			<span class="spacer"></span>
			<button class="fpanel-close" onclick={() => (activePanel = '')} use:pressable title="关闭"
				>✕</button
			>
		</div>
		<div class="fpanel-body">
			{#if activePanel === 'quickgen'}
				<QuickGenPanel />
			{:else if activePanel === 'queue'}
				<QueuePanel />
			{:else if activePanel === 'gpu'}
				<GpuPanel />
			{:else if activePanel === 'gallery'}
				<GalleryPanel />
			{:else if activePanel === 'tools'}
				<ToolsPanel />
			{/if}
		</div>
	</div>
{/if}

<style>
	.fball {
		position: fixed;
		z-index: 60;
		width: 46px;
		height: 46px;
		border-radius: 8px;
		border: 1px solid var(--border-color);
		background: var(--menu-bg);
		color: var(--fg-dim);
		cursor: grab;
		display: flex;
		align-items: center;
		justify-content: center;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.45);
		touch-action: none;
		transition:
			background 0.12s,
			border-color 0.12s,
			color 0.12s;
	}
	.fball:hover {
		background: var(--charcoal-300);
		border-color: #56565e;
		color: var(--fg);
	}
	.fball.menu-open {
		border-color: var(--accent);
		color: var(--accent-hover);
	}
	.fball.dragging {
		cursor: grabbing;
	}
	.fmenu {
		position: fixed;
		z-index: 60;
		display: flex;
		flex-direction: column-reverse;
		gap: 8px;
		align-items: flex-end;
		animation: fmenu-in 0.18s ease-out;
	}
	@keyframes fmenu-in {
		from {
			opacity: 0;
			transform: translateY(10px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}
	.fmenu-item {
		display: flex;
		align-items: center;
		gap: 8px;
		background: var(--menu-bg);
		border: 1px solid var(--border-color);
		color: var(--fg-dim);
		border-radius: 8px;
		padding: 8px 12px;
		font-size: 12.5px;
		font-family: inherit;
		cursor: pointer;
		box-shadow: 0 4px 16px rgba(0, 0, 0, 0.45);
		width: 132px;
		transition:
			background 0.12s,
			border-color 0.12s,
			color 0.12s;
	}
	.fmenu-item:hover {
		background: var(--charcoal-300);
		border-color: #56565e;
		color: var(--fg);
	}
	.fmenu-item.active {
		border-color: var(--accent);
		color: var(--accent-hover);
	}
	.fpanel {
		position: fixed;
		z-index: 58;
		width: 380px;
		max-height: calc(100vh - 140px);
		display: grid;
		grid-template-rows: auto 1fr;
		background: rgba(32, 33, 35, 0.96);
		border: 1px solid var(--border-color);
		border-radius: 10px;
		box-shadow: 0 12px 40px rgba(0, 0, 0, 0.55);
		backdrop-filter: blur(10px);
		overflow: hidden;
		animation: fmenu-in 0.16s ease-out;
	}
	.fpanel.wide {
		width: 620px;
	}
	.fpanel-head {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 9px 10px 9px 14px;
		background: var(--charcoal-600);
		border-bottom: 1px solid var(--border-subtle);
		cursor: grab;
		user-select: none;
		touch-action: none;
	}
	.fpanel-title {
		font-size: 12.5px;
		font-weight: 600;
		color: var(--fg-dim);
	}
	.fpanel-head .spacer {
		flex: 1;
	}
	.fpanel-close {
		background: none;
		border: none;
		color: var(--fg-muted);
		cursor: pointer;
		font-size: 12px;
		padding: 2px 8px;
		border-radius: 4px;
	}
	.fpanel-close:hover {
		background: #c42b1c;
		color: #fff;
	}
	.fpanel-body {
		padding: 14px;
		overflow-y: auto;
		min-height: 0;
	}
</style>
