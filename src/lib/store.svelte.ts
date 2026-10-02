import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getVersion } from '@tauri-apps/api/app';
import { check as checkUpdate, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';
import {
  api,
  type AppConfig,
  type StatusInfo,
  type TrainInfo,
  type RunState,
  type SetupInfo,
  type InstallSnapshot,
  type OutputItem
} from './api';

export type ViewName = 'config' | 'starting' | 'running';
export type AppMode = 'image' | 'train' | 'setup';

const DEFAULT_CONFIG: AppConfig = {
  data_root: '',
  comfy_dir: '',
  ai_toolkit_dir: '',
  host: '127.0.0.1',
  port: 8188,
  vram_mode: 'auto',
  preview: 'auto',
  extra_args: '',
  open_target: 'workspace',
  auto_enter: false,
  embed_support: true,
  manifest_url:
    'https://gh-proxy.com/https://raw.githubusercontent.com/XeroLc/zimage-studio/main/src-tauri/resources/setup-manifest.json',
  sources: { models: 'auto', github: 'auto', pypi: 'auto', torch: 'auto' },
  open_browser: false
};

const DEFAULT_INSTALL: InstallSnapshot = {
  running: false,
  comp_id: '',
  comp_name: '',
  phase: 'idle',
  message: '',
  file: '',
  file_index: 0,
  file_count: 0,
  downloaded: 0,
  total: 0,
  speed: 0,
  done: [],
  failed: [],
  error: ''
};

export interface TrainProgress {
  step: number;
  total: number;
  pct: number;
  elapsed: string;
  eta: string;
  rate: string;
  loss: string;
}

/** Parse the latest training progress line (anchored on the `loss:` field). */
export function parseTrainProgress(lines: string[]): TrainProgress | null {
  const re =
    /(\d+)\/(\d+)\s*\[([\d:]+)<([\d:]+),\s*([\d.]+)(s\/it|it\/s)[^\]]*loss:\s*([\d.eE+-]+)\]/;
  for (let i = lines.length - 1; i >= Math.max(0, lines.length - 12); i--) {
    const segments = lines[i].split('\r');
    for (let j = segments.length - 1; j >= 0; j--) {
      const m = re.exec(segments[j]);
      if (m) {
        const step = +m[1];
        const total = +m[2];
        return {
          step,
          total,
          pct: total ? Math.round((step / total) * 100) : 0,
          elapsed: m[3],
          eta: m[4],
          rate: `${m[5]} ${m[6]}`,
          loss: m[7] ?? '—'
        };
      }
    }
  }
  return null;
}

export interface UpdateState {
  checking: boolean;
  available: boolean;
  version: string;
  notes: string;
  installing: boolean;
  progress: number;
  error: string;
}

const DEFAULT_UPDATE: UpdateState = {
  checking: false,
  available: false,
  version: '',
  notes: '',
  installing: false,
  progress: 0,
  error: ''
};

class StudioStore {
  appMode = $state<AppMode>('image');

  config = $state<AppConfig>({ ...DEFAULT_CONFIG });
  status = $state<StatusInfo>({
    state: 'stopped',
    url: '',
    host: '127.0.0.1',
    port: 8188,
    pid: null,
    error: '',
    autostart: false,
    initial_tab: 'image',
    autotrain: false,
    configured: false,
    external: false,
    data_root: '',
    comfy_dir: '',
    open_target: 'workspace',
    auto_enter: false
  });
  log = $state<string[]>([]);
  initialized = $state(false);
  busy = $state(false);
  notice = $state('');
  #noticeTimer: ReturnType<typeof setTimeout> | null = null;
  #autoStarted = false;
  #autoTrained = false;
  #tabApplied = false;
  #setupAutoOpened = false;
  #prevRunState = 'stopped';

  /** 内嵌工作区模式（ComfyUI 全窗口 + 悬浮球） */
  workspace = $state(false);
  workspaceUrl = $state('');
  iframeKey = $state(0);

  train = $state<TrainInfo>({
    state: 'idle',
    config_file: '',
    pid: null,
    error: '',
    ai_toolkit_dir: ''
  });
  trainLog = $state<string[]>([]);
  trainConfigs = $state<string[]>([]);

  // ---------------- setup center ----------------
  setup = $state<SetupInfo | null>(null);
  selected = $state<Record<string, boolean>>({});
  install = $state<InstallSnapshot>({ ...DEFAULT_INSTALL });

  /** 下载源测速状态 */
  sourceTest = $state<{
    running: boolean;
    rows: import('./api').SourceTestRow[] | null;
    error: string;
  }>({ running: false, rows: null, error: '' });

  // ---------------- gallery (workspace) ----------------
  outputs = $state<OutputItem[]>([]);

  // ---------------- 应用更新（GitHub Releases） ----------------
  appVersion = $state('');
  update = $state<UpdateState>({ ...DEFAULT_UPDATE });
  #updateObj: Update | null = null;
  #updateChecked = false;

  #unlisten: UnlistenFn[] = [];

  view = $derived<ViewName>(
    this.status.state === 'ready'
      ? 'running'
      : this.status.state === 'starting' || this.status.state === 'stopping'
        ? 'starting'
        : 'config'
  );

  trainView = $derived<'config' | 'running'>(this.train.state === 'idle' ? 'config' : 'running');

  progress = $derived(parseTrainProgress(this.trainLog));

  stateLabel = $derived(
    (
      {
        stopped: '已停止',
        starting: '启动中…',
        ready: '运行中',
        stopping: '停止中…',
        error: '错误'
      } as Record<RunState, string>
    )[this.status.state]
  );

  dotClass = $derived(
    this.status.state === 'ready'
      ? 'ok'
      : this.status.state === 'error'
        ? 'err'
        : this.status.state === 'stopped'
          ? ''
          : 'warn'
  );

  installProgress = $derived(
    this.install.total > 0
      ? Math.min(100, Math.round((this.install.downloaded / this.install.total) * 100))
      : 0
  );

  selectedSizeMb = $derived.by(() => {
    let sum = 0;
    for (const g of this.setup?.groups ?? []) {
      for (const c of g.components) {
        if (this.selected[c.id] && !c.installed) sum += c.size_mb;
      }
    }
    return sum;
  });

  selectedIds = $derived.by(() => {
    const ids: string[] = [];
    for (const g of this.setup?.groups ?? []) {
      for (const c of g.components) {
        if (this.selected[c.id] && !c.installed) ids.push(c.id);
      }
    }
    return ids;
  });

  async init() {
    try {
      this.config = await api.getConfig();
    } catch (e) {
      console.error('init failed', e);
    }
    try {
      this.appVersion = await getVersion();
    } catch {
      /* 非 Tauri 环境 */
    }
    try {
      this.workspaceUrl = await api.comfyUrl();
    } catch {
      /* ignore */
    }
    // 启动后静默检查更新（GitHub Releases）
    setTimeout(() => void this.checkUpdate(false), 8000);
    // 事件订阅
    try {
      this.#unlisten.push(
        await listen<InstallSnapshot>('setup://progress', (ev) => {
          this.install = ev.payload;
          if (
            ev.payload.phase === 'done' ||
            ev.payload.phase === 'error' ||
            ev.payload.phase === 'cancelled'
          ) {
            void this.refreshSetup(false);
          }
        })
      );
      this.#unlisten.push(
        await listen('app://workspace', () => {
          void this.enterWorkspace();
        })
      );
      this.#unlisten.push(
        await listen<string>('app://notice', (ev) => this.toast(ev.payload))
      );
    } catch {
      /* 非 Tauri 环境忽略 */
    }
    this.initialized = true;
  }

  destroy() {
    for (const un of this.#unlisten) un();
    this.#unlisten = [];
  }

  // ---------------- 应用更新 ----------------

  /** 检查 GitHub Releases 是否有新版本；manual=true 时给出提示 */
  async checkUpdate(manual = true) {
    if (this.update.checking || this.update.installing) return;
    this.update = { ...this.update, checking: true, error: '' };
    try {
      const u = await checkUpdate();
      this.#updateObj = u;
      if (u) {
        this.update = {
          ...this.update,
          checking: false,
          available: true,
          version: u.version,
          notes: u.body ?? ''
        };
        this.toast(`发现新版本 v${u.version}（控制台里可一键更新）`, 6000);
      } else {
        this.update = { ...this.update, checking: false, available: false };
        if (manual) this.toast('已是最新版本');
      }
      this.#updateChecked = true;
    } catch (e) {
      this.update = {
        ...this.update,
        checking: false,
        error: String((e as Error)?.message ?? e)
      };
      if (manual) this.toast('检查更新失败：' + this.update.error);
    }
  }

  /** 下载并安装更新，完成后自动重启应用 */
  async installUpdate() {
    if (!this.#updateObj || this.update.installing) return;
    this.update = { ...this.update, installing: true, progress: 0, error: '' };
    try {
      let total = 0;
      let got = 0;
      await this.#updateObj.downloadAndInstall((ev) => {
        if (ev.event === 'Started') {
          total = ev.data.contentLength ?? 0;
        } else if (ev.event === 'Progress') {
          got += ev.data.chunkLength;
          this.update = {
            ...this.update,
            progress: total ? Math.min(99, Math.round((got / total) * 100)) : 0
          };
        } else if (ev.event === 'Finished') {
          this.update = { ...this.update, progress: 100 };
        }
      });
      this.toast('更新已安装，正在重启…');
      await relaunch();
    } catch (e) {
      this.update = {
        ...this.update,
        installing: false,
        error: String((e as Error)?.message ?? e)
      };
      this.toast('更新失败：' + this.update.error, 6000);
    }
  }

  toast(msg: string, ms = 3200) {
    this.notice = msg;
    if (this.#noticeTimer) clearTimeout(this.#noticeTimer);
    this.#noticeTimer = setTimeout(() => (this.notice = ''), ms);
  }

  async poll() {
    try {
      const st = await api.getStatus();
      const wasReady = this.#prevRunState === 'ready';
      this.status = st;
      this.#prevRunState = st.state;

      if (!this.#tabApplied && st.initial_tab !== 'image') {
        this.#tabApplied = true;
        if (st.initial_tab === 'train' || st.initial_tab === 'setup') this.appMode = st.initial_tab;
      }
      // 全新电脑首启（未配置且无数据目录）→ 自动进入资源中心
      if (!this.#setupAutoOpened && !st.configured && !st.data_root) {
        this.#setupAutoOpened = true;
        this.appMode = 'setup';
        void this.refreshSetup();
      }
      if (st.autostart && !this.#autoStarted) {
        this.#autoStarted = true;
        setTimeout(() => this.start(), 400);
      }
      if (st.autotrain && !this.#autoTrained) {
        this.#autoTrained = true;
        this.appMode = 'train';
        setTimeout(() => this.startTraining('zimage_character_8gb.yml'), 600);
      }
      // 就绪 → 按配置自动进入工作区/窗口/浏览器
      if (st.state === 'ready' && !wasReady && !this.workspace && st.auto_enter) {
        await this.openTarget();
      }
      if (st.state === 'starting' || st.state === 'ready' || st.state === 'stopping') {
        this.log = await api.getLog();
      }
    } catch {
      /* ignore transient errors */
    }
    try {
      const ti = await api.getTrainInfo();
      this.train = ti;
      if (ti.state !== 'idle') {
        this.trainLog = await api.getTrainLog();
      }
    } catch {
      /* ignore transient errors */
    }
    if (this.install.running) {
      try {
        this.install = await api.getInstallState();
      } catch {
        /* ignore */
      }
    }
  }

  // ----- image -----
  async start() {
    this.busy = true;
    try {
      await api.saveConfig({ ...this.config });
      await api.start();
      await this.poll();
    } finally {
      this.busy = false;
    }
  }

  async save() {
    await api.saveConfig({ ...this.config });
  }

  async stop() {
    this.busy = true;
    try {
      await api.stop();
      await this.poll();
    } finally {
      this.busy = false;
    }
  }

  async openBrowser() {
    await api.openBrowser();
  }

  /** 按配置的打开方式（工作区 / 独立窗口 / 浏览器） */
  async openTarget() {
    const t = this.status.open_target || this.config.open_target;
    if (t === 'window') await api.openComfyWindow();
    else if (t === 'browser') await api.openBrowser();
    else await this.enterWorkspace();
  }

  async enterWorkspace() {
    if (this.status.state !== 'ready') {
      this.toast('ComfyUI 尚未就绪');
      return;
    }
    if (!this.workspace) {
      this.workspace = true;
      this.iframeKey++;
      try {
        await api.enterWorkspace();
      } catch {
        /* ignore */
      }
    }
  }

  async exitWorkspace() {
    this.workspace = false;
    try {
      await api.exitWorkspace();
    } catch {
      /* ignore */
    }
  }

  // ----- training -----
  async loadTrainConfigs() {
    try {
      this.trainConfigs = await api.listTrainConfigs();
    } catch {
      this.trainConfigs = [];
    }
  }

  async startTraining(configFile: string) {
    this.busy = true;
    try {
      await api.startTraining(configFile);
      this.train = await api.getTrainInfo();
    } finally {
      this.busy = false;
    }
  }

  async stopTraining() {
    this.busy = true;
    try {
      await api.stopTraining();
      this.train = await api.getTrainInfo();
    } finally {
      this.busy = false;
    }
  }

  /** 打开训练相关目录（aitk_dataset / aitk_out / aitk_configs / aitk） */
  async openTrainDir(kind: string) {
    await api.openPath(kind);
  }

  // ----- setup center -----
  async refreshSetup(autoSelect = true) {
    try {
      this.setup = await api.getSetupInfo();
      if (autoSelect) {
        const sel: Record<string, boolean> = { ...this.selected };
        let any8gb = false;
        let anyFull = false;
        for (const g of this.setup.groups) {
          for (const c of g.components) {
            if (c.installed) {
              sel[c.id] = false;
              continue;
            }
            if (sel[c.id] === undefined) {
              // 默认：必装组件 + 8GB 套餐勾选
              sel[c.id] = c.required || c.preset === '8gb' || c.preset === 'both';
            }
            if (c.preset === '8gb') any8gb = true;
            if (c.preset === 'full') anyFull = true;
          }
        }
        void any8gb;
        void anyFull;
        this.selected = sel;
      }
    } catch (e) {
      console.error('setup info failed', e);
    }
  }

  selectPreset(preset: '8gb' | 'full') {
    if (!this.setup) return;
    const sel = { ...this.selected };
    for (const g of this.setup.groups) {
      for (const c of g.components) {
        if (c.installed) continue;
        if (c.preset === preset || c.preset === 'both') sel[c.id] = true;
        else if (c.preset === '8gb' || c.preset === 'full') sel[c.id] = false;
      }
    }
    this.selected = sel;
  }

  toggleSelect(id: string) {
    this.selected = { ...this.selected, [id]: !this.selected[id] };
  }

  async startInstall() {
    const ids = this.selectedIds;
    if (!ids.length) {
      this.toast('请先勾选要安装的组件');
      return;
    }
    try {
      await api.startInstall(ids);
      this.install = await api.getInstallState();
    } catch (e) {
      this.toast(String((e as Error)?.message ?? e));
    }
  }

  async cancelInstall() {
    try {
      await api.cancelInstall();
    } catch {
      /* ignore */
    }
  }

  /** 设置某通道下载源（'auto' 或候选 id）并保存 */
  async setSource(channel: keyof AppConfig['sources'], value: string) {
    this.config.sources = { ...this.config.sources, [channel]: value };
    await this.save();
    await this.refreshSetup(false);
  }

  /** 全通道测速并优选 */
  async testSources() {
    if (this.sourceTest.running) return;
    this.sourceTest = { running: true, rows: this.sourceTest.rows, error: '' };
    try {
      const r = await api.testSources();
      this.sourceTest = { running: false, rows: r.rows, error: '' };
      await this.refreshSetup(false);
      const okCount = r.rows.filter((x) => x.ok).length;
      this.toast(`测速完成：${okCount}/${r.rows.length} 个源可用，已自动优选`);
    } catch (e) {
      this.sourceTest = {
        running: false,
        rows: this.sourceTest.rows,
        error: String((e as Error)?.message ?? e)
      };
      this.toast('测速失败：' + this.sourceTest.error, 5000);
    }
  }

  async setDataRoot(root: string) {
    this.config.data_root = root;
    this.config.comfy_dir = '';
    this.config.ai_toolkit_dir = '';
    await this.save();
    await this.refreshSetup();
    await this.poll();
  }

  // ----- gallery -----
  async refreshOutputs() {
    try {
      this.outputs = await api.listOutputs(80);
    } catch {
      this.outputs = [];
    }
  }
}

export const store = new StudioStore();
