# Z-Image Studio

ComfyUI 的一体化桌面工作台：**资源中心（一键/按需配置整套环境）→ 启动控制 → 内嵌工作区（ComfyUI 界面 + 悬浮球特色功能）**，另集成 ai-toolkit LoRA 训练。

## 技术栈

| 层 | 选型 |
|---|---|
| 前端 | Svelte 5（runes）/ SvelteKit 3（静态 SPA）/ TypeScript |
| 动效 | GSAP（视图转场、状态徽标脉冲）+ Motion（按钮/卡片微交互） |
| 桌面壳 | Tauri 2（Rust）+ 系统 WebView2 |
| 视觉 | 设计 token 提取自 ComfyUI 官方前端（charcoal/azure 色板、Inter、#4e4e4e 边框） |

## 功能（v0.2）

- **资源中心（聚合平台核心）**：manifest 驱动的一键/按需安装 —— uv 工具、ComfyUI v0.38.0（含 PyTorch cu130）、全部模型（ModelScope 直连，支持断点续传）、5 个工作流、ai-toolkit 训练环境（含 3 处 8GB 兼容补丁自动应用）。
  换新电脑：装好客户端 → 资源中心选数据目录 → 勾选组件 → 开始安装（支持“8GB 显存 / 全精度”套餐一键勾选）。
- **内嵌工作区**：ComfyUI 界面在程序内以 iframe 打开（全窗口 + 顶部条），不再跳系统浏览器。
  启动时自动附加 `--enable-cors-header`（可关）以支持嵌入。
- **悬浮球特色功能**（独立于 ComfyUI，通过其 HTTP/WS API 工作，浮动面板可拖动）：
  - ⚡ 快速生图：提示词 → 直接出图（内置 txt2img API 模板，int8/bf16 可切换，实时进度）
  - ☰ 任务队列：运行中/排队任务、采样进度、中断/清空
  - 📊 显存监控：nvidia-smi 实时显存/利用率/温度
  - 🖼 作品画廊：输出目录缩略图墙、PNG 元数据（提示词/种子/尺寸/模型）、删除
  - 🔧 快捷工具：目录快捷打开、复制地址、重载界面、退出工作区
- **生图控制台**：启动前配置（数据目录/监听/显存模式/预览/打开方式）→ 一键启动（实时日志、就绪探测、端口检测、超时保护）→ 运行控制（进入工作区/独立窗口/浏览器、停止）。
- **LoRA 训练**：选配置 → 一键启动 → 实时进度（进度条/步数/loss/速度/剩余）；显存互斥（生图与训练同时只能跑一个）。
- **系统托盘**：显示主界面/启动/停止/进入工作区/退出；关闭窗口收进托盘，ComfyUI 继续运行。
- **单实例**：重复启动会聚焦已有窗口。
- **自动更新（GitHub Releases）**：启动后静默检查新版本，控制台（启动设置页底部）一键下载、安装并自动重启；端点已配置为 gh-proxy 加速地址，国内可直连。
- **资源清单同步**：资源中心的一切由 `setup-manifest.json` 驱动，可从 GitHub 仓库一键「同步清单」——新增模型、调整下载源无需发布新版本应用。
- 无边框圆角窗口（Windows 11 风格），自绘顶栏（可拖动/最小化/关闭）。
- **命令行参数**：`--autostart`（启动即拉起 ComfyUI）· `--tab train|setup`（进入对应页）· `--autotrain`（进训练页并自动开始 `zimage_character_8gb.yml`）。

## 开发与构建

```bash
npm install
npm run tauri dev          # 开发模式
npm run tauri build        # 发布构建
```

产物：
- 可执行文件：`src-tauri/target/release/zimage-studio.exe`
- 安装包：`src-tauri/target/release/bundle/`（nsis 安装程序 / msi）

## 配置文件（v0.2 起）

- 配置：`%APPDATA%\com.zimage.studio\config.json`（旧版 exe 同目录配置会自动迁移并改名 `*.migrated`）
- 日志：`%APPDATA%\com.zimage.studio\logs\`（comfy.log / train.log —— 不再放 %TEMP%，避免被 360 清理）
- 资源清单：随安装包内置 `src-tauri/resources/setup-manifest.json`（组件/下载源/安装步骤全在这一个文件里，可改）；从 GitHub 同步的副本存 `%APPDATA%\com.zimage.studio\manifest.json`，优先生效。

## 自动更新（GitHub Releases）

- 应用内置 Tauri 更新器：启动后台静默检查，控制台提供「检查更新」与一键安装（下载 → 安装 → 自动重启）。
- 更新端点：`https://gh-proxy.com/https://raw.githubusercontent.com/XeroLc/zimage-studio/main/latest.json`。
- 签名密钥：`.keys/zimage-studio.key`（私钥）+ `.keys/password.txt`（密码），两者**已在 .gitignore，切勿提交**；公钥已内置于 `tauri.conf.json`。CI 侧在仓库 Secrets 里配置 `TAURI_SIGNING_PRIVATE_KEY`（=私钥文件全文）与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`（=密码）。

### 维护者：发布新版本

1. 改版本号：`src-tauri/Cargo.toml` 与 `src-tauri/tauri.conf.json` 的两处 `version`
2. `git tag vX.Y.Z && git push origin vX.Y.Z`
3. `release.yml` 工作流自动执行：Windows 构建 → 签名 → 创建 Release（NSIS 中文安装程序 / MSI / 更新包）→ 生成 `latest.json` 并把下载地址改写为 gh-proxy 前缀提交回 main
4. 已装旧版本的用户下次启动即收到更新提示

## 资源清单同步（配置走 GitHub）

资源中心由 `setup-manifest.json` 数据驱动。应用优先使用从 GitHub 同步的副本，资源中心右上角「同步清单」一键拉取最新；改仓库里的清单即可上线新模型 / 新下载源 / 调整安装步骤，**无需发布新版本应用**。清单地址在配置项 `manifest_url`（默认指向本仓库，经 gh-proxy 加速）。

## 关键实现备注

- ComfyUI 启动参数含 `--enable-cors-header http://tauri.localhost`：ComfyUI 默认的跨源防护会拦截 iframe 加载与跨源请求；内嵌工作区依赖它（配置里可关）。
- ComfyUI API 的读写都经 Rust 侧 `comfy_api` 命令转发（reqwest），页面自身只直连 WebSocket（ws 不受跨源限制）。
- 安装引擎：`download`（ModelScope/gh-proxy/hf-mirror 前缀解析、`.part` 断点续传）/ `unzip` / `cmd`（实时输出、可取消、超时）/ `write_file`（{{ROOT}} 模板）/ `copy_resource` / `apply_patch`（幂等，带标记守卫）。
- 状态经事件 `setup://progress` 推送 + `get_install_state` 轮询兜底。

## 本机环境（已就绪）

Node 24 · Rust 1.99（cargo 走 rsproxy.cn 镜像）· VS 2022 Community + Windows SDK 10.0.22621 · WebView2 Runtime

> 构建提示：Windows 下请从 **PowerShell/cmd** 运行 cargo/tauri 命令（Git Bash 的 coreutils `link` 会干扰 MSVC 链接器）；
> 内存紧张的机器可设 `CARGO_BUILD_JOBS=6` 限制并行编译；**编译前退出 360 安全卫士**（主动防御会拦新编译的程序）。
> 另注意：跑 release 编译时别同时开 ComfyUI 生成（其 pinned memory 可占 ~10GB 内存，与编译叠加会 OOM 杀进程）。
