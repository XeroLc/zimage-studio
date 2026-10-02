<script lang="ts">
	import { onMount } from 'svelte';
	import gsap from 'gsap';
	import { pressable } from '#lib/actions';
	import { REPO, RELEASES, VERSION } from '#lib/site';

	onMount(() => {
		if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;
		const tl = gsap.timeline({ defaults: { ease: 'power3.out' } });
		tl.from('.hero-badge', { opacity: 0, y: 10, duration: 0.45 })
			.from('.hero h1', { opacity: 0, y: 20, duration: 0.6 }, '-=0.2')
			.from('.hero-sub', { opacity: 0, y: 16, duration: 0.55 }, '-=0.35')
			.from('.hero-cta', { opacity: 0, y: 12, duration: 0.5 }, '-=0.35')
			.from(
				'.hero-shot',
				{ opacity: 0, y: 34, scale: 0.985, duration: 0.9, ease: 'power2.out' },
				'-=0.25'
			);
	});

	const features = [
		{
			title: '资源中心，一键配齐',
			body: 'ComfyUI 引擎、CUDA 版 PyTorch、全部模型与工作流按需勾选，从 ModelScope 断点续传。清单从 GitHub 仓库同步——新增模型、调整下载源都不用更新应用。'
		},
		{
			title: '程序内的工作区',
			body: 'ComfyUI 界面直接开在应用窗口里，不再跳系统浏览器。点左上角「控制台」随时回来看日志与状态，服务不中断。'
		},
		{
			title: '悬浮球工具',
			body: '快速生图、任务队列、显存监控、作品画廊浮在界面之上，通过 ComfyUI API 独立工作——不往 ComfyUI 里装任何插件。'
		},
		{
			title: '集成 LoRA 训练',
			body: 'ai-toolkit 收进「训练」页：选配置、一键开训，实时步数、loss 与剩余时间。生图与训练共享显存，应用会自动互斥。'
		},
		{
			title: '全程本地',
			body: '引擎、模型、生成结果都在你的硬盘上。除首次在资源中心下载组件外，出图与训练不需要联网。'
		},
		{
			title: '托盘常驻，单实例',
			body: '关掉窗口只是收起面板，ComfyUI 在托盘后面继续跑；重复双击图标只会唤回已有窗口，不会开出第二个实例。'
		}
	];

	const steps = [
		{
			title: '下载安装',
			body: '运行安装程序，或解压绿色版直接双击。首次启动会自动引导到资源中心。'
		},
		{
			title: '一键配置',
			body: '选择数据目录，勾选「8GB 显存」套餐，点开始安装。约 15 GB 下载，国内源实测 10～60 分钟。'
		},
		{
			title: '开始创作',
			body: '进入工作区用内置工作流出图，或点悬浮球「快速生图」。想固定角色形象，把照片放进训练页跑一个 LoRA。'
		}
	];

	const requirements = [
		['系统', 'Windows 10 / 11（64 位）'],
		['显卡', 'NVIDIA 独显，推荐 8GB 及以上显存；无独显可用 CPU 模式，速度很慢'],
		['内存', '16 GB 及以上'],
		['磁盘', '约 30 GB（含两套模型与训练组件）'],
		['运行库', 'WebView2（Win 11 自带；Win 10 首次运行会提示安装）']
	];

	const faqs = [
		[
			'生成需要联网吗？',
			'不需要。除首次在资源中心下载组件外，出图与训练全程离线；下载源已按国内网络选择（ModelScope 直连 + gh-proxy 加速）。'
		],
		[
			'显存不到 8GB 怎么办？',
			'资源中心选 int8 量化套餐即可：8GB 笔记本显卡实测 1024×1024 约 12.5 秒一张；显存更小可把分辨率降到 768。'
		],
		[
			'生成的文件存在哪？',
			'数据目录（配置时可改）下的 ComfyUI/output。应用内的作品画廊可以直接浏览、查看参数、删除。'
		],
		[
			'卸载会连模型一起删掉吗？',
			'不会。卸载程序只移除应用本体，数据目录（模型与输出）原样保留，需要时手动删除即可。'
		],
		[
			'应用怎么更新？',
			'内置更新器：启动后自动检查 GitHub Releases，控制台里一键下载安装并自动重启。资源中心的组件清单同样从仓库同步，模型与下载源的调整不需要重装应用。'
		],
		[
			'能直接用原版 ComfyUI 界面吗？',
			'可以。工作区就是完整版 ComfyUI，另外还提供「独立窗口」与「系统浏览器」两种打开方式。'
		]
	];
</script>

<header class="nav">
	<div class="wrap nav-inner">
		<a class="brand" href="#top">
			<img src="favicon.svg" alt="" width="20" height="20" />
			<span>Z-Image Studio</span>
		</a>
		<nav class="nav-links">
			<a href="#features">功能</a>
			<a href="#shots">界面</a>
			<a href="#start">快速开始</a>
			<a href="#faq">常见问题</a>
		</nav>
		<a class="btn btn-sm" href={REPO} target="_blank" rel="noreferrer">GitHub</a>
	</div>
</header>

<main id="top">
	<section class="hero">
		<div class="wrap">
			<div class="hero-badge">
				<span class="dot"></span>
				{VERSION} · Windows 10/11 · 数据全部留在本机
			</div>
			<h1>本地 AI 生图，<br />装好即画</h1>
			<p class="hero-sub">
				Z-Image Studio 把 ComfyUI 引擎、Z-Image Turbo 模型、LoRA 训练环境和一套悬浮工具打包成单个
				Windows 应用。资源中心勾选一次，之后每次打开，直接进工作区出图。
			</p>
			<div class="hero-cta">
				<a class="btn btn-primary" href={RELEASES} use:pressable>下载 Windows 安装包</a>
				<a class="btn" href={REPO} use:pressable>查看源码</a>
				<span class="cta-note">安装包 3.4 MB · 免安装绿色版也在 Releases 里</span>
			</div>
			<div class="hero-shot shot">
				<img src="shots/01-workspace.png" alt="Z-Image Studio 工作区：ComfyUI 界面与悬浮球" />
			</div>
		</div>
	</section>

	<section id="features" class="section">
		<div class="wrap">
			<h2>为什么它顺手</h2>
			<p class="section-sub">从装环境到出图，把容易出错的环节都收进一个窗口。</p>
			<ul class="feature-list">
				{#each features as f (f.title)}
					<li>
						<span class="mark" aria-hidden="true"></span>
						<div>
							<h3>{f.title}</h3>
							<p>{f.body}</p>
						</div>
					</li>
				{/each}
			</ul>
		</div>
	</section>

	<section id="shots" class="section">
		<div class="wrap">
			<h2>看一眼真实界面</h2>
			<div class="shot-row">
				<div class="shot-copy">
					<h3>悬浮球 · 快速生图</h3>
					<p>
						写一句提示词，选 int8 或全精度、尺寸与步数，直接出图。结果自动落进输出目录，画廊里随时回看、查参数。
					</p>
				</div>
				<div class="shot shot-small">
					<img src="shots/03-quickgen.png" alt="快速生图面板：提示词与生成结果" />
				</div>
			</div>
			<div class="shot-row reverse">
				<div class="shot-copy">
					<h3>资源中心 · 勾选即装</h3>
					<p>
						引擎、模型、工作流、训练环境按组罗列：已安装的一眼可见，缺失的勾选即装。8GB
						显存与全精度两套套餐，换电脑十分钟起步。
					</p>
				</div>
				<div class="shot shot-small">
					<img src="shots/05-resource-center.png" alt="资源中心：组件分组与安装状态" />
				</div>
			</div>
		</div>
	</section>

	<section id="start" class="section">
		<div class="wrap">
			<h2>三步开画</h2>
			<ol class="steps">
				{#each steps as s, i (s.title)}
					<li>
						<span class="step-no">{i + 1}</span>
						<h3>{s.title}</h3>
						<p>{s.body}</p>
					</li>
				{/each}
			</ol>

			<div class="req-faq">
				<div class="req">
					<h3>系统要求</h3>
					<table>
						<tbody>
							{#each requirements as [k, v] (k)}
								<tr><th>{k}</th><td>{v}</td></tr>
							{/each}
						</tbody>
					</table>
				</div>
				<div id="faq" class="faq">
					<h3>常见问题</h3>
					{#each faqs as [q, a] (q)}
						<details>
							<summary>{q}</summary>
							<p>{a}</p>
						</details>
					{/each}
				</div>
			</div>
		</div>
	</section>

	<section class="closing">
		<div class="wrap closing-inner">
			<p>引擎、模型、工作流、训练——一个窗口，全部就位。</p>
			<div class="closing-actions">
				<a class="btn btn-primary" href={RELEASES} use:pressable>下载 Windows 安装包</a>
				<a class="btn" href={REPO} use:pressable>查看源码</a>
			</div>
		</div>
	</section>
</main>

<footer>
	<div class="wrap footer-inner">
		<span>Z-Image Studio · 本地 AI 生图工作台</span>
		<span class="footer-right">
			<a href={REPO} target="_blank" rel="noreferrer">GitHub</a>
			<span>{VERSION}</span>
		</span>
	</div>
</footer>

<style>
	/* ---------------- 导航 ---------------- */
	.nav {
		position: sticky;
		top: 0;
		z-index: 20;
		background: rgba(23, 24, 26, 0.82);
		backdrop-filter: blur(12px);
		border-bottom: 1px solid var(--line);
	}
	.nav-inner {
		display: flex;
		align-items: center;
		gap: 26px;
		height: 58px;
	}
	.brand {
		display: flex;
		align-items: center;
		gap: 9px;
		font-weight: 700;
		font-size: 15px;
		letter-spacing: -0.01em;
	}
	.nav-links {
		display: flex;
		gap: 22px;
		margin-left: auto;
		font-size: 13.5px;
		color: var(--muted);
	}
	.nav-links a:hover {
		color: var(--text);
	}
	.btn-sm {
		padding: 7px 16px;
		font-size: 13px;
	}

	/* ---------------- Hero ---------------- */
	.hero {
		position: relative;
		padding: 84px 0 96px;
		text-align: center;
		overflow: hidden;
	}
	.hero::before {
		content: '';
		position: absolute;
		inset: 0;
		background-image: radial-gradient(circle, rgba(78, 78, 78, 0.3) 1px, transparent 1px);
		background-size: 26px 26px;
		-webkit-mask-image: radial-gradient(ellipse 75% 58% at 50% 0%, #000 28%, transparent 74%);
		mask-image: radial-gradient(ellipse 75% 58% at 50% 0%, #000 28%, transparent 74%);
		pointer-events: none;
	}
	.hero::after {
		content: '';
		position: absolute;
		inset: 0;
		background: radial-gradient(ellipse 46% 34% at 50% -4%, rgba(11, 140, 233, 0.16), transparent 70%);
		pointer-events: none;
	}
	.hero .wrap {
		position: relative;
		z-index: 1;
	}
	.hero-badge {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		font-size: 12.5px;
		color: var(--dim);
		border: 1px solid var(--line-strong);
		background: var(--panel);
		border-radius: 999px;
		padding: 5px 14px;
	}
	.hero-badge .dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--ok);
	}
	.hero h1 {
		font-size: clamp(36px, 5.6vw, 62px);
		font-weight: 800;
		letter-spacing: -0.025em;
		margin: 26px 0 20px;
	}
	.hero-sub {
		max-width: 640px;
		margin: 0 auto;
		color: var(--dim);
		font-size: 16.5px;
		line-height: 1.8;
	}
	.hero-cta {
		display: flex;
		align-items: center;
		justify-content: center;
		flex-wrap: wrap;
		gap: 14px;
		margin: 34px 0 14px;
	}
	.cta-note {
		width: 100%;
		font-size: 12.5px;
		color: var(--muted);
	}
	.hero-shot {
		margin-top: 46px;
	}

	/* 窗口框：让截图看起来就是那个应用窗口 */
	.shot {
		border: 1px solid var(--line-strong);
		border-radius: 12px;
		overflow: hidden;
		background: var(--panel);
		box-shadow:
			0 30px 80px rgba(0, 0, 0, 0.6),
			0 2px 0 rgba(255, 255, 255, 0.03) inset;
	}
	.shot img {
		width: 100%;
	}

	/* ---------------- 通用 section ---------------- */
	.section {
		padding: 88px 0;
		border-top: 1px solid var(--line);
		scroll-margin-top: 58px;
	}
	.section h2 {
		font-size: clamp(26px, 3.4vw, 36px);
		margin-bottom: 12px;
	}
	.section-sub {
		color: var(--muted);
		margin-bottom: 42px;
	}

	/* ---------------- 功能清单（分组细线列表，非卡片阵列） ---------------- */
	.feature-list {
		list-style: none;
		columns: 2;
		column-gap: 56px;
	}
	.feature-list li {
		display: flex;
		gap: 14px;
		break-inside: avoid;
		padding: 20px 0;
		border-bottom: 1px solid var(--line);
	}
	.feature-list .mark {
		flex: none;
		width: 9px;
		height: 9px;
		margin-top: 9px;
		border-radius: 2px;
		background: var(--azure);
	}
	.feature-list h3 {
		font-size: 15.5px;
		font-weight: 650;
		margin-bottom: 4px;
	}
	.feature-list p {
		font-size: 14px;
		color: var(--muted);
		line-height: 1.7;
	}

	/* ---------------- 截图段 ---------------- */
	.shot-row {
		display: grid;
		grid-template-columns: 5fr 6fr;
		gap: 48px;
		align-items: center;
		padding: 34px 0;
	}
	.shot-row + .shot-row {
		border-top: 1px solid var(--line);
	}
	.shot-row.reverse .shot-copy {
		order: 2;
	}
	.shot-row.reverse .shot {
		order: 1;
	}
	.shot-copy h3 {
		font-size: 21px;
		margin-bottom: 10px;
	}
	.shot-copy p {
		color: var(--muted);
		font-size: 14.5px;
	}

	/* ---------------- 三步（真序列，用数字） ---------------- */
	.steps {
		list-style: none;
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 34px;
		margin-bottom: 78px;
	}
	.steps li {
		border-top: 2px solid var(--azure);
		padding-top: 18px;
	}
	.step-no {
		display: inline-block;
		font-size: 13px;
		font-weight: 700;
		color: var(--azure-hi);
		margin-bottom: 8px;
	}
	.steps h3 {
		font-size: 17px;
		margin-bottom: 6px;
	}
	.steps p {
		font-size: 14px;
		color: var(--muted);
	}

	/* ---------------- 系统要求 + FAQ ---------------- */
	.req-faq {
		display: grid;
		grid-template-columns: 5fr 6fr;
		gap: 56px;
	}
	.req-faq h3 {
		font-size: 17px;
		margin-bottom: 16px;
	}
	.req table {
		width: 100%;
		border-collapse: collapse;
		font-size: 14px;
	}
	.req tr {
		border-bottom: 1px solid var(--line);
	}
	.req th {
		text-align: left;
		font-weight: 500;
		color: var(--muted);
		padding: 10px 16px 10px 0;
		white-space: nowrap;
		vertical-align: top;
	}
	.req td {
		color: var(--dim);
		padding: 10px 0;
	}
	.faq details {
		border-bottom: 1px solid var(--line);
	}
	.faq summary {
		cursor: pointer;
		list-style: none;
		padding: 12px 26px 12px 0;
		font-size: 14.5px;
		font-weight: 550;
		position: relative;
	}
	.faq summary::-webkit-details-marker {
		display: none;
	}
	.faq summary::after {
		content: '+';
		position: absolute;
		right: 4px;
		top: 10px;
		color: var(--muted);
		font-size: 17px;
		transition: transform 0.15s;
	}
	.faq details[open] summary::after {
		transform: rotate(45deg);
	}
	.faq details p {
		font-size: 13.5px;
		color: var(--muted);
		padding: 0 24px 14px 0;
	}

	/* ---------------- 收尾 ---------------- */
	.closing {
		border-top: 1px solid var(--line);
		background:
			radial-gradient(ellipse 60% 130% at 50% 120%, rgba(11, 140, 233, 0.13), transparent 70%),
			var(--panel);
	}
	.closing-inner {
		padding: 74px 24px;
		text-align: center;
	}
	.closing p {
		font-size: clamp(20px, 2.6vw, 27px);
		font-weight: 700;
		letter-spacing: -0.015em;
		margin-bottom: 26px;
	}
	.closing-actions {
		display: flex;
		justify-content: center;
		flex-wrap: wrap;
		gap: 14px;
	}

	footer {
		border-top: 1px solid var(--line);
	}
	.footer-inner {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 12px;
		padding: 26px 24px;
		font-size: 13px;
		color: var(--muted);
	}
	.footer-right {
		display: flex;
		gap: 18px;
	}
	.footer-right a:hover {
		color: var(--text);
	}

	/* ---------------- 响应式 ---------------- */
	@media (max-width: 900px) {
		.feature-list {
			columns: 1;
		}
		.shot-row,
		.shot-row.reverse {
			grid-template-columns: 1fr;
			gap: 22px;
		}
		.shot-row.reverse .shot-copy,
		.shot-row.reverse .shot {
			order: unset;
		}
		.steps {
			grid-template-columns: 1fr;
			gap: 24px;
		}
		.req-faq {
			grid-template-columns: 1fr;
			gap: 40px;
		}
		.nav-links {
			display: none;
		}
		.hero {
			padding: 56px 0 64px;
		}
	}
</style>
