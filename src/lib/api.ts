import { invoke } from '@tauri-apps/api/core';

export interface SourcesCfg {
  /** auto | modelscope | hfm */
  models: string;
  /** auto | proxy | direct */
  github: string;
  /** auto | tsinghua | aliyun | official */
  pypi: string;
  /** auto | official | sjtu | aliyun */
  torch: string;
}

export interface AppConfig {
  data_root: string;
  comfy_dir: string;
  ai_toolkit_dir: string;
  host: string;
  port: number;
  vram_mode: 'auto' | 'normal' | 'low' | 'none';
  preview: 'auto' | 'latent2rgb' | 'none';
  extra_args: string;
  open_target: 'workspace' | 'window' | 'browser';
  auto_enter: boolean;
  embed_support: boolean;
  manifest_url: string;
  sources: SourcesCfg;
  open_browser: boolean;
}

export type RunState = 'stopped' | 'starting' | 'ready' | 'stopping' | 'error';

export type TrainRunState = 'idle' | 'running' | 'stopping' | 'completed' | 'error';

export interface TrainInfo {
  state: TrainRunState;
  config_file: string;
  pid: number | null;
  error: string;
  ai_toolkit_dir: string;
}

export interface StatusInfo {
  state: RunState;
  url: string;
  host: string;
  port: number;
  pid: number | null;
  error: string;
  autostart: boolean;
  initial_tab: string;
  autotrain: boolean;
  configured: boolean;
  external: boolean;
  data_root: string;
  comfy_dir: string;
  open_target: 'workspace' | 'window' | 'browser';
  auto_enter: boolean;
}

export interface ComponentInfo {
  id: string;
  name: string;
  desc: string;
  size_mb: number;
  required: boolean;
  optional: boolean;
  preset: string | null;
  installed: boolean;
}

export interface GroupInfo {
  id: string;
  name: string;
  components: ComponentInfo[];
}

export interface ChannelChoice {
  channel: string;
  id: string;
  name: string;
  latency_ms: number;
  detail: string;
  auto: boolean;
}

export interface ChannelCandidates {
  channel: string;
  name: string;
  candidates: { id: string; name: string }[];
}

export interface SourceTestRow {
  channel: string;
  id: string;
  name: string;
  ok: boolean;
  latency_ms: number;
  speed_kbps: number;
}

export interface SourceTestResult {
  rows: SourceTestRow[];
  chosen: ChannelChoice[];
  tested_at: number;
}

export interface SetupInfo {
  data_root: string;
  suggested_root: string;
  manifest_version: number;
  manifest_updated: string;
  manifest_source: 'bundled' | 'synced';
  manifest_url: string;
  groups: GroupInfo[];
  effective: ChannelChoice[];
  source_test_at: number;
  source_candidates: ChannelCandidates[];
}

export interface ManifestSyncResult {
  version: number;
  updated: string;
  groups: number;
  components: number;
  source: string;
}

export interface InstallSnapshot {
  running: boolean;
  comp_id: string;
  comp_name: string;
  phase: 'idle' | 'prepare' | 'download' | 'step' | 'done' | 'error' | 'cancelled';
  message: string;
  file: string;
  file_index: number;
  file_count: number;
  downloaded: number;
  total: number;
  speed: number;
  done: string[];
  failed: string[];
  error: string;
}

export interface OutputItem {
  rel: string;
  filename: string;
  subfolder: string;
  size: number;
  mtime: number;
}

export interface OutputMeta {
  texts: string[];
  seed: number | null;
  steps: number | null;
  width: number | null;
  height: number | null;
  model: string;
}

export interface SysStats {
  available: boolean;
  name: string;
  mem_used_mb: number;
  mem_total_mb: number;
  util: number;
  temp: number;
}

export interface RemoteUpdate {
  available: boolean;
  version: string;
  notes: string;
  url: string;
  signature: string;
  source: string;
}

export interface UpdateProgress {
  phase: 'download' | 'verify' | 'launch';
  downloaded: number;
  total: number;
  speed: number;
  message: string;
}

export const api = {
  // config / status
  getConfig: () => invoke<AppConfig>('get_config'),
  saveConfig: (cfg: AppConfig) => invoke<void>('save_config', { cfg }),
  getStatus: () => invoke<StatusInfo>('get_status'),
  getLog: () => invoke<string[]>('get_log'),
  start: () => invoke<void>('start_comfy'),
  stop: () => invoke<void>('stop_comfy'),
  openBrowser: () => invoke<void>('open_browser'),
  openComfyWindow: () => invoke<void>('open_comfy_window'),
  enterWorkspace: () => invoke<void>('enter_workspace'),
  exitWorkspace: () => invoke<void>('exit_workspace'),
  comfyUrl: () => invoke<string>('comfy_url'),
  comfyApi: (method: 'GET' | 'POST' | 'DELETE', path: string, body?: unknown) =>
    invoke<unknown>('comfy_api', { method, path, body: body ?? null }),

  // sys / gallery
  sysStats: () => invoke<SysStats>('sys_stats'),
  listOutputs: (limit?: number) => invoke<OutputItem[]>('list_outputs', { limit: limit ?? null }),
  outputMeta: (rel: string) => invoke<OutputMeta>('output_meta', { rel }),
  deleteOutput: (rel: string) => invoke<void>('delete_output', { rel }),
  openPath: (kind: string) => invoke<void>('open_path', { kind }),

  // training
  getTrainInfo: () => invoke<TrainInfo>('get_train_info'),
  listTrainConfigs: () => invoke<string[]>('list_train_configs'),
  getTrainLog: () => invoke<string[]>('get_train_log'),
  startTraining: (configFile: string) => invoke<void>('start_training', { configFile }),
  stopTraining: () => invoke<void>('stop_training'),

  // setup center
  getSetupInfo: () => invoke<SetupInfo>('get_setup_info'),
  getInstallState: () => invoke<InstallSnapshot>('get_install_state'),
  startInstall: (ids: string[]) => invoke<void>('start_install', { ids }),
  cancelInstall: () => invoke<void>('cancel_install'),
  quickgenTemplate: () => invoke<string>('get_quickgen_template'),
  syncManifest: () => invoke<ManifestSyncResult>('sync_manifest'),
  testSources: () => invoke<SourceTestResult>('test_sources'),

  // 自研更新（绕 CDN 缓存 + 签名校验；安装时应用会退出并交由安装器接管）
  checkUpdateRemote: () => invoke<RemoteUpdate>('check_update_remote'),
  downloadInstallUpdate: (url: string, signature: string, version: string) =>
    invoke<void>('download_install_update', { url, signature, version })
};
