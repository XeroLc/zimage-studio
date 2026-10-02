use crate::config::AppConfig;
use crate::download::{build_client, download_file, file_ready, DownloadEvent};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// ---------------- manifest ----------------

#[derive(Deserialize, Clone)]
pub struct Manifest {
    pub version: u32,
    #[serde(default)]
    pub updated: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub sources: ManifestSources,
    pub groups: Vec<Group>,
}

/// 各通道候选下载源（数据驱动：改清单即可增删源）
#[derive(Deserialize, Clone, Default)]
pub struct ManifestSources {
    #[serde(default)]
    pub models: Vec<SourceCandidate>,
    #[serde(default)]
    pub github: Vec<SourceCandidate>,
    #[serde(default)]
    pub pypi: Vec<SourceCandidate>,
    #[serde(default)]
    pub torch: Vec<SourceCandidate>,
}

#[derive(Deserialize, Clone)]
pub struct SourceCandidate {
    pub id: String,
    pub name: String,
    /// 测速用的小文件地址
    #[serde(default)]
    pub probe: String,
    /// pypi / torch 通道：index 基址
    #[serde(default)]
    pub index: String,
    /// github 通道：URL 前缀
    #[serde(default)]
    pub prefix: String,
}

#[derive(Deserialize, Clone)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub components: Vec<Component>,
}

#[derive(Deserialize, Clone)]
pub struct Component {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub desc: String,
    #[serde(default)]
    pub size_mb: u64,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub preset: Option<String>,
    #[serde(default)]
    pub check: Option<Check>,
    #[serde(default)]
    pub steps: Vec<Step>,
}

#[derive(Deserialize, Clone)]
pub struct Check {
    #[serde(default)]
    pub all: Vec<Rule>,
    /// 任一满足即可（与 all 同时存在时取"与"）
    #[serde(default)]
    pub any: Vec<Rule>,
}

#[derive(Deserialize, Clone)]
pub struct Rule {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub min_size: u64,
    #[serde(default)]
    pub suffix: String,
    #[serde(default)]
    pub min_count: usize,
}

#[derive(Deserialize, Clone, Default)]
pub struct Step {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub files: Option<Vec<DownloadFile>>,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub strip_first: Option<bool>,
    #[serde(default)]
    pub exe: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub crlf: Option<bool>,
    #[serde(default)]
    pub resource: Option<String>,
    #[serde(default)]
    pub marker: Option<String>,
}

#[derive(Deserialize, Clone)]
pub struct DownloadFile {
    pub url: String,
    pub to: String,
    #[serde(default)]
    pub size: u64,
    /// 备用源（当前源失败时切换；auto 模式下也参与优选）
    #[serde(default)]
    pub alt: Option<String>,
}

// ---------------- install state ----------------

#[derive(Serialize, Clone, Default)]
pub struct InstallSnapshot {
    pub running: bool,
    pub comp_id: String,
    pub comp_name: String,
    pub phase: String, // idle | prepare | download | step | done | error | cancelled
    pub message: String,
    pub file: String,
    pub file_index: usize,
    pub file_count: usize,
    pub downloaded: u64,
    pub total: u64,
    pub speed: f64,
    pub done: Vec<String>,
    pub failed: Vec<String>,
    pub error: String,
}

pub struct InstallCtl {
    pub cancel: AtomicBool,
    pub snap: Mutex<InstallSnapshot>,
}

impl Default for InstallCtl {
    fn default() -> Self {
        Self {
            cancel: AtomicBool::new(false),
            snap: Mutex::new(InstallSnapshot::default()),
        }
    }
}

fn publish(app: &AppHandle, ctl: &InstallCtl) {
    let s = ctl.snap.lock().unwrap().clone();
    let _ = app.emit("setup://progress", s);
}

fn set_phase(ctl: &InstallCtl, phase: &str, message: &str) {
    let mut s = ctl.snap.lock().unwrap();
    s.phase = phase.into();
    s.message = message.into();
}

// ---------------- manifest loading ----------------

/// 同步副本位置：从 GitHub 拉取后存这里，优先于随安装包内置的清单
pub fn synced_manifest_path() -> PathBuf {
    crate::config::app_data_dir().join("manifest.json")
}

fn bundled_manifest_path(resource_dir: &Path) -> Option<PathBuf> {
    let candidates = [
        resource_dir.join("resources").join("setup-manifest.json"),
        resource_dir.join("setup-manifest.json"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("setup-manifest.json"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

pub fn load_manifest(resource_dir: &Path) -> Result<Manifest, String> {
    // 已同步（GitHub）副本优先；损坏则回退内置
    let synced = synced_manifest_path();
    if synced.exists() {
        if let Ok(text) = fs::read_to_string(&synced) {
            if let Ok(m) = serde_json::from_str::<Manifest>(&text) {
                return Ok(m);
            }
        }
    }
    let path = bundled_manifest_path(resource_dir)
        .ok_or_else(|| "找不到资源清单 setup-manifest.json".to_string())?;
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("资源清单解析失败: {e}"))
}

fn resource_path(resource_dir: &Path, rel: &str) -> Option<PathBuf> {
    let candidates = [
        resource_dir.join("resources").join(rel),
        resource_dir.join(rel),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join(rel),
    ];
    candidates.into_iter().find(|p| p.exists())
}

// ---------------- detection ----------------

fn rule_ok(root: &Path, r: &Rule) -> bool {
    match r.kind.as_str() {
        "file" => file_ready(&root.join(&r.path), r.min_size),
        "dir_files" => {
            let dir = root.join(&r.path);
            let mut n = 0usize;
            if let Ok(rd) = fs::read_dir(&dir) {
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if r.suffix.is_empty() || name.ends_with(&r.suffix) {
                        n += 1;
                    }
                }
            }
            n >= r.min_count.max(1)
        }
        // 按 PATH 查找命令（如系统已装 uv：where uv 成功即视为可用）
        "where" => resolve_on_path(&r.path).is_some(),
        _ => false,
    }
}

/// 在 PATH 上解析命令（Windows `where`），返回第一个存在的路径
fn resolve_on_path(name: &str) -> Option<PathBuf> {
    if name.trim().is_empty() {
        return None;
    }
    let mut cmd = Command::new("where");
    cmd.arg(name);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .map(|l| PathBuf::from(l.trim()))
        .find(|p| p.exists())
}

pub fn component_installed(root: &Path, comp: &Component) -> bool {
    match &comp.check {
        Some(c) if !c.all.is_empty() || !c.any.is_empty() => {
            let all_ok = c.all.iter().all(|r| rule_ok(root, r));
            let any_ok = c.any.is_empty() || c.any.iter().any(|r| rule_ok(root, r));
            all_ok && any_ok
        }
        _ => false,
    }
}

// ---------------- download sources（分通道下载源：手动指定 / 自动测速优选） ----------------

#[derive(Serialize, Clone, Default)]
pub struct ChannelChoice {
    pub channel: String,
    pub id: String,
    pub name: String,
    pub latency_ms: u64,
    pub detail: String,
    pub auto: bool,
}

#[derive(Serialize, Clone)]
pub struct ChannelCandidatesOut {
    pub channel: String,
    pub name: String,
    pub candidates: Vec<CandidateOut>,
}

#[derive(Serialize, Clone)]
pub struct CandidateOut {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Clone)]
pub struct SourceTestRow {
    pub channel: String,
    pub id: String,
    pub name: String,
    pub ok: bool,
    pub latency_ms: u64,
    pub speed_kbps: f64,
}

#[derive(Serialize, Clone)]
pub struct SourceTestResult {
    pub rows: Vec<SourceTestRow>,
    pub chosen: Vec<ChannelChoice>,
    pub tested_at: i64,
}

pub struct EffectiveSources {
    pub channels: Vec<ChannelChoice>,
    pub github_prefix: String,
    pub pypi_index: String,
    pub torch_index: String,
    pub models_id: String,
    /// 模型通道为 auto 时允许下载失败后自动切换备用源
    pub allow_model_fallback: bool,
    /// 各通道全部候选（优选的放首位；用于命令行步骤失败后的换源重试）
    pub github_all: Vec<(String, String)>, // (id, prefix)
    pub pypi_all: Vec<(String, String)>,   // (id, index)
    pub torch_all: Vec<(String, String)>,  // (id, index)
}

impl EffectiveSources {
    fn channel_auto(&self, channel: &str) -> bool {
        self.channels
            .iter()
            .find(|c| c.channel == channel)
            .map(|c| c.auto)
            .unwrap_or(false)
    }
}

/// 命令替换变量（与 EffectiveSources 解耦，重试时按尝试项生成）
#[derive(Clone)]
pub struct SubstVars {
    pub root: String,
    pub pypi_index: String,
    pub torch_index: String,
    pub python_mirror: String,
}

fn vars_with(
    root: &Path,
    pypi_index: &str,
    torch_index: &str,
    github_prefix: &str,
) -> SubstVars {
    SubstVars {
        root: root.to_string_lossy().replace('\\', "/"),
        pypi_index: pypi_index.to_string(),
        torch_index: torch_index.to_string(),
        python_mirror: format!(
            "{}astral-sh/python-build-standalone/releases/download",
            github_prefix
        ),
    }
}

fn source_cache_path() -> PathBuf {
    crate::config::app_data_dir().join("sources.json")
}

#[derive(Serialize, Deserialize, Default)]
struct SourceCache {
    #[serde(default)]
    tested_at: i64,
    #[serde(default)]
    channels: HashMap<String, String>,
}

const SOURCE_CACHE_TTL_SECS: i64 = 24 * 3600;
pub const SOURCE_CHANNELS: [&str; 4] = ["models", "github", "pypi", "torch"];

fn load_source_cache() -> SourceCache {
    fs::read_to_string(source_cache_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_source_cache(cache: &SourceCache) {
    if let Ok(text) = serde_json::to_string_pretty(cache) {
        let _ = fs::write(source_cache_path(), text);
    }
}

fn pub_cache_tested_at() -> i64 {
    load_source_cache().tested_at
}

fn now_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn candidates_for<'a>(m: &'a ManifestSources, channel: &str) -> &'a [SourceCandidate] {
    match channel {
        "models" => &m.models,
        "github" => &m.github,
        "pypi" => &m.pypi,
        "torch" => &m.torch,
        _ => &[],
    }
}

fn configured_id<'a>(cfg: &'a AppConfig, channel: &str) -> &'a str {
    match channel {
        "models" => &cfg.sources.models,
        "github" => &cfg.sources.github,
        "pypi" => &cfg.sources.pypi,
        "torch" => &cfg.sources.torch,
        _ => "auto",
    }
}

/// 用小文件测速：返回 (首字节延迟 ms, 吞吐 KB/s)
async fn probe_candidate(client: &reqwest::Client, cand: &SourceCandidate) -> Option<(u64, f64)> {
    if cand.probe.is_empty() {
        return None;
    }
    let t0 = Instant::now();
    let resp = client
        .get(&cand.probe)
        .header("Range", "bytes=0-131071")
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let latency = t0.elapsed().as_millis() as u64;
    let t1 = Instant::now();
    let mut got: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(c) => {
                got += c.len() as u64;
                if got >= 131072 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let secs = t1.elapsed().as_secs_f64().max(0.001);
    Some((latency, got as f64 / 1024.0 / secs))
}

/// 解析各通道的生效源：手动指定直接用；auto 用缓存（24h）或并发测速选最快。
pub async fn resolve_sources(
    client: &reqwest::Client,
    cfg: &AppConfig,
    manifest: &Manifest,
    force: bool,
) -> EffectiveSources {
    let mut cache = load_source_cache();
    let stale = force || (now_ts() - cache.tested_at > SOURCE_CACHE_TTL_SECS);
    let mut channels: Vec<ChannelChoice> = Vec::new();

    for channel in SOURCE_CHANNELS {
        let cands = candidates_for(&manifest.sources, channel);
        if cands.is_empty() {
            continue;
        }
        let wanted = configured_id(cfg, channel);
        let auto = wanted == "auto";
        let mut chosen: Option<&SourceCandidate> = None;
        let mut latency = 0u64;
        let mut detail = if auto { "测速优选".to_string() } else { "手动指定".to_string() };

        if auto {
            if !stale {
                if let Some(id) = cache.channels.get(channel) {
                    if let Some(c) = cands.iter().find(|x| &x.id == id) {
                        chosen = Some(c);
                        detail = "测速优选（缓存）".into();
                    }
                }
            }
            if chosen.is_none() {
                let probes: Vec<_> = cands.iter().map(|c| probe_candidate(client, c)).collect();
                let results = futures_util::future::join_all(probes).await;
                let mut best: Option<(usize, u64)> = None;
                for (i, r) in results.iter().enumerate() {
                    if let Some((lat, _)) = r {
                        if best.map(|(_, bl)| *lat < bl).unwrap_or(true) {
                            best = Some((i, *lat));
                        }
                    }
                }
                if let Some((i, lat)) = best {
                    chosen = Some(&cands[i]);
                    latency = lat;
                    cache.channels.insert(channel.to_string(), cands[i].id.clone());
                    cache.tested_at = now_ts();
                } else {
                    chosen = cands.first();
                    detail = "所有源不可达，用默认".into();
                }
            }
        } else {
            chosen = cands.iter().find(|x| x.id == wanted);
            if chosen.is_none() {
                chosen = cands.first();
                detail = "指定源不存在，已回退".into();
            }
        }

        if let Some(c) = chosen {
            channels.push(ChannelChoice {
                channel: channel.to_string(),
                id: c.id.clone(),
                name: c.name.clone(),
                latency_ms: latency,
                detail,
                auto,
            });
        }
    }
    save_source_cache(&cache);

    let find_cand = |ch: &str| -> Option<&SourceCandidate> {
        let c = channels.iter().find(|x| x.channel == ch)?;
        candidates_for(&manifest.sources, ch).iter().find(|x| x.id == c.id)
    };
    let github_prefix = find_cand("github")
        .map(|c| c.prefix.clone())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "https://gh-proxy.com/https://github.com/".into());
    let pypi_index = find_cand("pypi")
        .map(|c| c.index.clone())
        .filter(|i| !i.is_empty())
        .unwrap_or_else(|| "https://pypi.tuna.tsinghua.edu.cn/simple".into());
    let torch_index = find_cand("torch")
        .map(|c| c.index.clone())
        .filter(|i| !i.is_empty())
        .unwrap_or_else(|| "https://download.pytorch.org/whl/cu130".into());
    let models_id = channels
        .iter()
        .find(|c| c.channel == "models")
        .map(|c| c.id.clone())
        .unwrap_or_else(|| "modelscope".into());

    // 全部候选（优选/指定者放首位），供命令行失败换源重试
    let ordered = |ch: &str, pick: &dyn Fn(&SourceCandidate) -> String| -> Vec<(String, String)> {
        let chosen_id = channels
            .iter()
            .find(|c| c.channel == ch)
            .map(|c| c.id.clone())
            .unwrap_or_default();
        let mut v: Vec<(String, String)> = candidates_for(&manifest.sources, ch)
            .iter()
            .map(|c| (c.id.clone(), pick(c)))
            .filter(|(_, val)| !val.is_empty())
            .collect();
        v.sort_by_key(|(id, _)| if *id == chosen_id { 0 } else { 1 });
        v
    };
    let github_all = ordered("github", &|c| {
        if c.prefix.is_empty() {
            "https://gh-proxy.com/https://github.com/".into()
        } else {
            c.prefix.clone()
        }
    });
    let pypi_all = ordered("pypi", &|c| c.index.clone());
    let torch_all = ordered("torch", &|c| c.index.clone());

    EffectiveSources {
        allow_model_fallback: cfg.sources.models == "auto",
        channels,
        github_prefix,
        pypi_index,
        torch_index,
        models_id,
        github_all,
        pypi_all,
        torch_all,
    }
}

/// 只读缓存（不测速），用于界面显示当前生效源
pub fn cached_choices(cfg: &AppConfig, manifest: &Manifest) -> Vec<ChannelChoice> {
    let cache = load_source_cache();
    let mut out = Vec::new();
    for channel in SOURCE_CHANNELS {
        let cands = candidates_for(&manifest.sources, channel);
        if cands.is_empty() {
            continue;
        }
        let wanted = configured_id(cfg, channel);
        let auto = wanted == "auto";
        let id = if auto {
            cache
                .channels
                .get(channel)
                .cloned()
                .unwrap_or_else(|| cands.first().map(|c| c.id.clone()).unwrap_or_default())
        } else {
            wanted.to_string()
        };
        let name = cands
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.clone())
            .unwrap_or_else(|| id.clone());
        out.push(ChannelChoice {
            channel: channel.to_string(),
            id,
            name,
            latency_ms: 0,
            detail: if auto {
                if cache.tested_at > 0 { "测速优选（缓存）".into() } else { "自动（安装时测速）".into() }
            } else {
                "手动指定".into()
            },
            auto,
        });
    }
    out
}

/// 对全部候选源做一次测速（并发），写入缓存并返回明细
#[tauri::command]
pub async fn test_sources(
    app: AppHandle,
    state: tauri::State<'_, Mutex<crate::AppState>>,
) -> Result<SourceTestResult, String> {
    let (cfg, res) = {
        let st = state.lock().unwrap();
        let res = app
            .path()
            .resource_dir()
            .unwrap_or_else(|_| PathBuf::from("."));
        (st.config.clone(), res)
    };
    let manifest = load_manifest(&res)?;
    let client = build_client();

    // 并发测所有通道的所有候选
    let mut jobs = Vec::new();
    for channel in SOURCE_CHANNELS {
        for cand in candidates_for(&manifest.sources, channel) {
            jobs.push((channel, cand.clone()));
        }
    }
    let probes = jobs
        .iter()
        .map(|(_, c)| probe_candidate(&client, c));
    let results = futures_util::future::join_all(probes).await;

    let mut rows = Vec::new();
    let mut best_by_channel: HashMap<String, (String, u64)> = HashMap::new();
    for ((channel, cand), r) in jobs.iter().zip(results.iter()) {
        let (ok, lat, kbps) = match r {
            Some((lat, kbps)) => (true, *lat, *kbps),
            None => (false, 0, 0.0),
        };
        rows.push(SourceTestRow {
            channel: channel.to_string(),
            id: cand.id.clone(),
            name: cand.name.clone(),
            ok,
            latency_ms: lat,
            speed_kbps: kbps,
        });
        if ok {
            let e = best_by_channel
                .entry(channel.to_string())
                .or_insert((cand.id.clone(), lat));
            if lat < e.1 {
                *e = (cand.id.clone(), lat);
            }
        }
    }

    // 更新缓存（只对 auto 通道生效）
    let mut cache = load_source_cache();
    for channel in SOURCE_CHANNELS {
        if configured_id(&cfg, channel) == "auto" {
            if let Some((id, _)) = best_by_channel.get(channel) {
                cache.channels.insert(channel.to_string(), id.clone());
            }
        }
    }
    cache.tested_at = now_ts();
    save_source_cache(&cache);

    let chosen = cached_choices(&cfg, &manifest);
    Ok(SourceTestResult {
        rows,
        chosen,
        tested_at: cache.tested_at,
    })
}

#[derive(Serialize, Clone)]
pub struct ComponentInfo {
    pub id: String,
    pub name: String,
    pub desc: String,
    pub size_mb: u64,
    pub required: bool,
    pub optional: bool,
    pub preset: Option<String>,
    pub installed: bool,
}

#[derive(Serialize, Clone)]
pub struct GroupInfo {
    pub id: String,
    pub name: String,
    pub components: Vec<ComponentInfo>,
}

#[derive(Serialize, Clone)]
pub struct SetupInfo {
    pub data_root: String,
    pub suggested_root: String,
    pub manifest_version: u32,
    pub manifest_updated: String,
    /// "synced"（来自 GitHub 同步）或 "bundled"（随安装包内置）
    pub manifest_source: String,
    pub manifest_url: String,
    pub groups: Vec<GroupInfo>,
    /// 当前生效的下载源（来自缓存，不触发测速）
    pub effective: Vec<ChannelChoice>,
    /// 上次测速时间（unix 秒，0=从未）
    pub source_test_at: i64,
    /// 各通道候选源（界面下拉用）
    pub source_candidates: Vec<ChannelCandidatesOut>,
}

pub fn build_setup_info(cfg: &AppConfig, resource_dir: &Path) -> SetupInfo {
    let manifest = load_manifest(resource_dir).ok();
    let root = cfg.root();
    let (version, updated) = manifest
        .as_ref()
        .map(|m| (m.version, m.updated.clone()))
        .unwrap_or((0, String::new()));
    let (effective, source_candidates) = match &manifest {
        Some(m) => (
            cached_choices(cfg, m),
            SOURCE_CHANNELS
                .iter()
                .filter(|ch| !candidates_for(&m.sources, ch).is_empty())
                .map(|ch| ChannelCandidatesOut {
                    channel: ch.to_string(),
                    name: match *ch {
                        "models" => "模型".into(),
                        "github" => "GitHub".into(),
                        "pypi" => "PyPI".into(),
                        _ => "PyTorch".into(),
                    },
                    candidates: candidates_for(&m.sources, ch)
                        .iter()
                        .map(|c| CandidateOut {
                            id: c.id.clone(),
                            name: c.name.clone(),
                        })
                        .collect(),
                })
                .collect(),
        ),
        None => (Vec::new(), Vec::new()),
    };
    let groups: Vec<GroupInfo> = manifest
        .map(|m| {
            m.groups
                .into_iter()
                .map(|g| GroupInfo {
                    id: g.id,
                    name: g.name,
                    components: g
                        .components
                        .into_iter()
                        .map(|c| UserVisibleComponent {
                            installed: root
                                .as_ref()
                                .map(|r| component_installed(r, &c))
                                .unwrap_or(false),
                            c,
                        })
                        .map(Into::into)
                        .collect(),
                })
                .collect()
        })
        .unwrap_or_default();
    SetupInfo {
        data_root: cfg.data_root.clone(),
        suggested_root: crate::config::suggest_data_root(),
        manifest_version: version,
        manifest_updated: updated,
        manifest_source: if synced_manifest_path().exists() {
            "synced".into()
        } else {
            "bundled".into()
        },
        manifest_url: cfg.manifest_url.clone(),
        groups,
        effective,
        source_test_at: pub_cache_tested_at(),
        source_candidates,
    }
}

struct UserVisibleComponent {
    installed: bool,
    c: Component,
}

impl From<UserVisibleComponent> for ComponentInfo {
    fn from(u: UserVisibleComponent) -> Self {
        ComponentInfo {
            id: u.c.id,
            name: u.c.name,
            desc: u.c.desc,
            size_mb: u.c.size_mb,
            required: u.c.required,
            optional: u.c.optional,
            preset: u.c.preset,
            installed: u.installed,
        }
    }
}

// ---------------- install runner ----------------

pub async fn run_install(
    app: AppHandle,
    ctl: Arc<InstallCtl>,
    cfg: AppConfig,
    ids: Vec<String>,
    resource_dir: PathBuf,
) {
    let result = install_inner(&app, &ctl, &cfg, &ids, &resource_dir).await;
    let mut s = ctl.snap.lock().unwrap();
    s.running = false;
    match result {
        Ok(()) => {
            s.phase = "done".into();
            s.message = "全部完成".into();
            s.downloaded = 0;
            s.total = 0;
        }
        Err(e) => {
            if ctl.cancel.load(Ordering::Relaxed) {
                s.phase = "cancelled".into();
                s.message = "已取消".into();
            } else {
                let e = format!("{e}（应用 v{}）", env!("CARGO_PKG_VERSION"));
                s.phase = "error".into();
                s.error = e.clone();
                s.message = e;
                if !s.comp_id.is_empty() && !s.failed.contains(&s.comp_id) {
                    let id = s.comp_id.clone();
                    s.failed.push(id);
                }
            }
        }
    }
    let snap = s.clone();
    drop(s);
    let _ = app.emit("setup://progress", snap);
}

async fn install_inner(
    app: &AppHandle,
    ctl: &Arc<InstallCtl>,
    cfg: &AppConfig,
    ids: &[String],
    resource_dir: &Path,
) -> Result<(), String> {
    let root = cfg
        .root()
        .ok_or_else(|| "尚未设置数据目录".to_string())?;
    fs::create_dir_all(&root).map_err(|e| format!("无法创建数据目录: {e}"))?;
    let manifest = load_manifest(resource_dir)?;
    let client = build_client();

    // 解析下载源（auto 通道在此刻测速优选；结果缓存 24h）
    {
        let mut s = ctl.snap.lock().unwrap();
        s.phase = "prepare".into();
        s.message = "正在选择下载源…".into();
    }
    publish(app, ctl);
    let eff = resolve_sources(&client, cfg, &manifest, false).await;
    let summary = eff
        .channels
        .iter()
        .map(|c| {
            if c.latency_ms > 0 {
                format!("{}={}({}ms)", c.channel, c.name, c.latency_ms)
            } else {
                format!("{}={}", c.channel, c.name)
            }
        })
        .collect::<Vec<_>>()
        .join(" · ");
    {
        let mut s = ctl.snap.lock().unwrap();
        s.message = format!("下载源：{summary}");
    }
    publish(app, ctl);

    for group in &manifest.groups {
        for comp in &group.components {
            if !ids.iter().any(|i| i == &comp.id) {
                continue;
            }
            if ctl.cancel.load(Ordering::Relaxed) {
                return Err("已取消".into());
            }
            {
                let mut s = ctl.snap.lock().unwrap();
                s.comp_id = comp.id.clone();
                s.comp_name = comp.name.clone();
                s.phase = "prepare".into();
                s.message = format!("准备安装 {}", comp.name);
                s.file_index = 0;
                s.file_count = 0;
                s.downloaded = 0;
                s.total = 0;
                s.speed = 0.0;
                s.error.clear();
            }
            publish(app, ctl);

            for step in &comp.steps {
                if ctl.cancel.load(Ordering::Relaxed) {
                    return Err("已取消".into());
                }
                run_step(app, ctl, &client, &root, resource_dir, step, &eff).await?;
            }
            let mut s = ctl.snap.lock().unwrap();
            if !s.done.contains(&comp.id) {
                s.done.push(comp.id.clone());
            }
            drop(s);
            publish(app, ctl);
        }
    }
    Ok(())
}

async fn run_step(
    app: &AppHandle,
    ctl: &Arc<InstallCtl>,
    client: &reqwest::Client,
    root: &Path,
    resource_dir: &Path,
    step: &Step,
    eff: &EffectiveSources,
) -> Result<(), String> {
    let label = step
        .name
        .clone()
        .unwrap_or_else(|| step.kind.clone());
    match step.kind.as_str() {
        "mkdir" => {
            let p = root.join(step.path.clone().unwrap_or_default());
            fs::create_dir_all(&p).map_err(|e| format!("创建目录 {} 失败: {e}", p.display()))?;
            Ok(())
        }
        "download" => {
            let files = step.files.clone().unwrap_or_default();
            let count = files.len();
            for (i, f) in files.iter().enumerate() {
                let dest = root.join(&f.to);
                {
                    let mut s = ctl.snap.lock().unwrap();
                    s.phase = "download".into();
                    s.file = f.to.clone();
                    s.file_index = i + 1;
                    s.file_count = count;
                    s.message = format!("下载 {}", short_name(&f.to));
                    s.downloaded = fs::metadata(dest.with_extension("part"))
                        .map(|m| m.len())
                        .unwrap_or(0);
                    s.total = f.size;
                    s.speed = 0.0;
                }
                publish(app, ctl);

                let urls = candidate_urls(f, eff);
                let mut last_err = String::new();
                let mut ok = false;
                for (attempt, url) in urls.iter().enumerate() {
                    if attempt > 0 {
                        set_phase(
                            ctl,
                            "download",
                            &format!("主源失败，切换备用源（{}）…", short_name(&f.to)),
                        );
                        publish(app, ctl);
                    }
                    let start = Instant::now();
                    let ctl2 = Arc::clone(ctl);
                    let app2 = app.clone();
                    let fsize = f.size;
                    let result = download_file(client, url, &dest, fsize, &ctl.cancel, move |ev: DownloadEvent| {
                        let mut s = ctl2.snap.lock().unwrap();
                        s.downloaded = ev.downloaded;
                        if ev.total > 0 {
                            s.total = ev.total;
                        }
                        let secs = start.elapsed().as_secs_f64();
                        if secs > 0.5 {
                            s.speed = ev.downloaded as f64 / secs;
                        }
                        let snap = s.clone();
                        drop(s);
                        let _ = app2.emit("setup://progress", snap);
                        true
                    })
                    .await;
                    match result {
                        Ok(_) => {
                            ok = true;
                            break;
                        }
                        Err(e) => {
                            if ctl.cancel.load(Ordering::Relaxed) {
                                return Err("已取消".into());
                            }
                            last_err = e;
                        }
                    }
                }
                if !ok {
                    return Err(format!(
                        "下载 {} 失败（已尝试 {} 个源）：{}",
                        short_name(&f.to),
                        urls.len(),
                        last_err
                    ));
                }
            }
            Ok(())
        }
        "unzip" => {
            let zip_path = root.join(step.from.clone().unwrap_or_default());
            let out_dir = root.join(step.to.clone().unwrap_or_default());
            let strip = step.strip_first.unwrap_or(false);
            set_phase(ctl, "step", &format!("解压 {}", short_name(&step.from.clone().unwrap_or_default())));
            publish(app, ctl);
            unzip_into(&zip_path, &out_dir, strip)
        }
        "cmd" => {
            let step2 = step.clone();
            let root2 = root.to_path_buf();
            let attempts = eff.cmd_attempts(root, step);
            set_phase(ctl, "step", &format!("执行：{}", label));
            publish(app, ctl);
            run_command(app, ctl, &step2, &root2, &attempts)
        }
        "write_file" => {
            let to = root.join(step.to.clone().unwrap_or_default());
            let content = if let Some(t) = &step.template {
                let tp = resource_path(resource_dir, t)
                    .ok_or_else(|| format!("找不到模板 {t}"))?;
                fs::read_to_string(&tp).map_err(|e| e.to_string())?
            } else {
                step.content.clone().unwrap_or_default()
            };
            let content = subst(&content, &eff.base_vars(root));
            let content = if step.crlf.unwrap_or(false) {
                content.replace("\r\n", "\n").replace('\n', "\r\n")
            } else {
                content
            };
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(&to, content).map_err(|e| format!("写入 {} 失败: {e}", to.display()))?;
            Ok(())
        }
        "copy_resource" => {
            let from_dir = resource_path(resource_dir, step.from.clone().unwrap_or_default().as_str())
                .ok_or_else(|| format!("找不到资源目录 {}", step.from.clone().unwrap_or_default()))?;
            let to_dir = root.join(step.to.clone().unwrap_or_default());
            fs::create_dir_all(&to_dir).map_err(|e| e.to_string())?;
            let rd = fs::read_dir(&from_dir).map_err(|e| e.to_string())?;
            for e in rd.flatten() {
                let p = e.path();
                if p.is_file() {
                    let dest = to_dir.join(e.file_name());
                    fs::copy(&p, &dest)
                        .map_err(|e2| format!("复制 {} 失败: {e2}", p.display()))?;
                }
            }
            Ok(())
        }
        "apply_patch" => {
            let res = step.resource.clone().unwrap_or_default();
            let to = root.join(step.to.clone().unwrap_or_default());
            let src = resource_path(resource_dir, &res)
                .ok_or_else(|| format!("找不到补丁资源 {res}"))?;
            let marker = step.marker.clone().unwrap_or_default();
            if to.exists() && !marker.is_empty() {
                let cur = fs::read_to_string(&to).unwrap_or_default();
                if cur.contains(&marker) {
                    set_phase(ctl, "step", &format!("{label}（已应用，跳过）"));
                    publish(app, ctl);
                    return Ok(());
                }
            }
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(&src, &to).map_err(|e| format!("应用补丁失败: {e}"))?;
            Ok(())
        }
        other => Err(format!("未知步骤类型: {other}")),
    }
}

fn short_name(p: &str) -> String {
    p.rsplit('/').next().unwrap_or(p).to_string()
}

/// 变量替换：{{ROOT}} / {{PYPI_INDEX}} / {{TORCH_INDEX}} / {{PYTHON_MIRROR}}
fn subst(s: &str, vars: &SubstVars) -> String {
    s.replace("{{ROOT}}", &vars.root)
        .replace("{{PYPI_INDEX}}", &vars.pypi_index)
        .replace("{{TORCH_INDEX}}", &vars.torch_index)
        .replace("{{PYTHON_MIRROR}}", &vars.python_mirror)
}

impl EffectiveSources {
    /// 基础替换变量（首选源）
    fn base_vars(&self, root: &Path) -> SubstVars {
        vars_with(root, &self.pypi_index, &self.torch_index, &self.github_prefix)
    }

    /// 命令行步骤的尝试序列：用到哪个模板变量，就按该通道的候选依次尝试（auto 通道才换源）
    fn cmd_attempts(&self, root: &Path, step: &Step) -> Vec<(String, SubstVars)> {
        let mut text = step.args.clone().unwrap_or_default().join(" ");
        if let Some(env) = &step.env {
            for v in env.values() {
                text.push(' ');
                text.push_str(v);
            }
        }
        if text.contains("{{TORCH_INDEX}}")
            && self.channel_auto("torch")
            && self.torch_all.len() > 1
        {
            return self
                .torch_all
                .iter()
                .map(|(id, idx)| {
                    (
                        format!("torch:{id}"),
                        vars_with(root, &self.pypi_index, idx, &self.github_prefix),
                    )
                })
                .collect();
        }
        if text.contains("{{PYPI_INDEX}}") && self.channel_auto("pypi") && self.pypi_all.len() > 1 {
            return self
                .pypi_all
                .iter()
                .map(|(id, idx)| {
                    (
                        format!("pypi:{id}"),
                        vars_with(root, idx, &self.torch_index, &self.github_prefix),
                    )
                })
                .collect();
        }
        if text.contains("{{PYTHON_MIRROR}}")
            && self.channel_auto("github")
            && self.github_all.len() > 1
        {
            return self
                .github_all
                .iter()
                .map(|(id, prefix)| {
                    (
                        format!("github:{id}"),
                        vars_with(root, &self.pypi_index, &self.torch_index, prefix),
                    )
                })
                .collect();
        }
        vec![("primary".into(), self.base_vars(root))]
    }
}

/// 资源 URL 前缀解析（gh: 走当前 GitHub 通道前缀）
fn resolve_url(spec: &str, eff: &EffectiveSources) -> String {
    if let Some(rest) = spec.strip_prefix("ms:") {
        let mut it = rest.splitn(3, '/');
        let owner = it.next().unwrap_or("");
        let repo = it.next().unwrap_or("");
        let path = it.next().unwrap_or("");
        format!(
            "https://www.modelscope.cn/models/{}/{}/resolve/master/{}",
            owner, repo, path
        )
    } else if let Some(rest) = spec.strip_prefix("hf:") {
        let (repo, path) = split_repo_path(rest);
        format!("https://hf-mirror.net/{}/resolve/main/{}", repo, path)
    } else if let Some(rest) = spec.strip_prefix("gh:") {
        format!("{}{}", eff.github_prefix, rest)
    } else if let Some(rest) = spec.strip_prefix("url:") {
        rest.to_string()
    } else {
        spec.to_string()
    }
}

fn split_repo_path(rest: &str) -> (String, String) {
    let mut it = rest.splitn(3, '/');
    let owner = it.next().unwrap_or("");
    let repo = it.next().unwrap_or("");
    let path = it.next().unwrap_or("");
    (format!("{}/{}", owner, repo), path.to_string())
}

/// 单个文件的可尝试 URL 列表：
/// - auto 模式：主源优先（模型通道选了 HF-Mirror 时备用源提前），失败可切换
/// - GitHub 前缀类（gh:）在 auto 下附加另一种前缀（gh-proxy ↔ 直连）
/// - 手动指定：只用首选源（忠实用户选择）
fn candidate_urls(f: &DownloadFile, eff: &EffectiveSources) -> Vec<String> {
    let primary = resolve_url(&f.url, eff);
    let mut list = vec![primary.clone()];
    if let Some(alt) = &f.alt {
        let alt_url = resolve_url(alt, eff);
        if eff.models_id == "hfm" && f.url.starts_with("ms:") {
            list = vec![alt_url, primary];
        } else {
            list.push(alt_url);
        }
    }
    if f.url.starts_with("gh:") && eff.channel_auto("github") && eff.github_all.len() > 1 {
        let rest = &f.url[3..];
        for (_id, prefix) in eff.github_all.iter().skip(1) {
            list.push(format!("{prefix}{rest}"));
        }
    }
    if (f.url.starts_with("ms:") || f.url.starts_with("hf:")) && !eff.allow_model_fallback {
        list.truncate(1);
    }
    let mut seen = std::collections::HashSet::new();
    list.retain(|u| seen.insert(u.clone()));
    list
}

fn unzip_into(zip_path: &Path, out_dir: &Path, strip_first: bool) -> Result<(), String> {
    let f = fs::File::open(zip_path).map_err(|e| format!("打开压缩包失败: {e}"))?;
    let mut archive = zip::ZipArchive::new(f).map_err(|e| format!("解析压缩包失败: {e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let enclosed = match entry.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => continue,
        };
        let rel = if strip_first {
            let mut comps = enclosed.components();
            comps.next();
            comps.as_path().to_path_buf()
        } else {
            enclosed
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let dest = out_dir.join(&rel);
        if entry.is_dir() {
            fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out = fs::File::create(&dest).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn run_command(
    app: &AppHandle,
    ctl: &Arc<InstallCtl>,
    step: &Step,
    root: &Path,
    attempts: &[(String, SubstVars)],
) -> Result<(), String> {
    let mut last_err = String::new();
    for (i, (label, vars)) in attempts.iter().enumerate() {
        if ctl.cancel.load(Ordering::Relaxed) {
            return Err("已取消".into());
        }
        if i > 0 {
            set_phase(ctl, "step", &format!("上一步失败，更换下载源重试（{label}）…"));
            publish(app, ctl);
        }
        match run_command_once(app, ctl, step, root, vars) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = e;
                if attempts.len() > 1 && i + 1 < attempts.len() {
                    continue;
                }
            }
        }
    }
    Err(last_err)
}

fn run_command_once(
    app: &AppHandle,
    ctl: &Arc<InstallCtl>,
    step: &Step,
    root: &Path,
    vars: &SubstVars,
) -> Result<(), String> {
    let exe_raw = step.exe.clone().unwrap_or_default();
    let exe = {
        let p = PathBuf::from(exe_raw.replace('\\', "/"));
        if p.is_absolute() {
            p
        } else {
            let local = root.join(&exe_raw);
            if local.exists() {
                local
            } else {
                // 数据目录里没有 → 退回按 PATH 解析（如系统已安装的 uv）
                resolve_on_path(&exe_raw).unwrap_or(local)
            }
        }
    };
    if !exe.exists() {
        return Err(format!("找不到程序 {}", exe.display()));
    }
    let mut cmd = Command::new(&exe);
    if let Some(args) = &step.args {
        for a in args {
            cmd.arg(subst(a, vars));
        }
    }
    cmd.current_dir(root);
    if let Some(env) = &step.env {
        for (k, v) in env {
            cmd.env(k, subst(v, vars));
        }
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let mut child = cmd.spawn().map_err(|e| format!("启动失败: {e}"))?;
    let (tx, rx) = mpsc::channel::<String>();
    if let Some(out) = child.stdout.take() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
    }
    if let Some(err) = child.stderr.take() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
    }

    let timeout = Duration::from_secs(step.timeout_secs.unwrap_or(1800));
    let started = Instant::now();
    let mut tail: Vec<String> = Vec::new();
    let mut last_pub = Instant::now();
    loop {
        while let Ok(line) = rx.try_recv() {
            tail.push(line);
            if tail.len() > 8 {
                tail.remove(0);
            }
        }
        if last_pub.elapsed() > Duration::from_millis(400) {
            last_pub = Instant::now();
            let mut s = ctl.snap.lock().unwrap();
            if let Some(l) = tail.last() {
                s.message = l.chars().take(160).collect();
            }
            let snap = s.clone();
            drop(s);
            let _ = app.emit("setup://progress", snap);
        }
        if ctl.cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            return Err("已取消".into());
        }
        if started.elapsed() > timeout {
            let _ = child.kill();
            return Err(format!("步骤超时（{}s）", timeout.as_secs()));
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                std::thread::sleep(Duration::from_millis(120));
                while let Ok(line) = rx.try_recv() {
                    tail.push(line);
                    if tail.len() > 8 {
                        tail.remove(0);
                    }
                }
                if status.success() {
                    return Ok(());
                }
                return Err(format!(
                    "命令失败（退出码 {:?}）：\n{}",
                    status.code().unwrap_or(-1),
                    tail.join("\n")
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(150)),
            Err(e) => return Err(format!("等待进程失败: {e}")),
        }
    }
}

// ---------------- commands ----------------

#[tauri::command]
pub fn get_setup_info(
    app: AppHandle,
    state: tauri::State<'_, Mutex<crate::AppState>>,
) -> SetupInfo {
    let cfg = state.lock().unwrap().config.clone();
    let res = app
        .path()
        .resource_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    build_setup_info(&cfg, &res)
}

#[tauri::command]
pub fn get_install_state(ctl: tauri::State<'_, Arc<InstallCtl>>) -> InstallSnapshot {
    ctl.snap.lock().unwrap().clone()
}

#[tauri::command]
pub async fn start_install(
    app: AppHandle,
    ctl: tauri::State<'_, Arc<InstallCtl>>,
    ids: Vec<String>,
) -> Result<(), String> {
    {
        let s = ctl.snap.lock().unwrap();
        if s.running {
            return Err("已有安装任务在运行".into());
        }
    }
    if ids.is_empty() {
        return Err("未选择任何组件".into());
    }
    let cfg = {
        let state = app.state::<Mutex<crate::AppState>>();
        let cfg = state.lock().unwrap().config.clone();
        cfg
    };
    if cfg.root().is_none() {
        return Err("请先设置数据目录".into());
    }
    let res = app
        .path()
        .resource_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    let ctl = Arc::clone(&ctl);
    {
        let mut s = ctl.snap.lock().unwrap();
        *s = InstallSnapshot {
            running: true,
            phase: "prepare".into(),
            message: "准备中…".into(),
            ..Default::default()
        };
    }
    ctl.cancel.store(false, Ordering::Relaxed);
    publish(&app, &ctl);
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        run_install(app2, ctl, cfg, ids, res).await;
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_install(ctl: tauri::State<'_, Arc<InstallCtl>>) -> Result<(), String> {
    ctl.cancel.store(true, Ordering::Relaxed);
    {
        let mut s = ctl.snap.lock().unwrap();
        s.message = "正在取消…".into();
    }
    Ok(())
}

/// 读取内嵌的快速生图 API 模板
#[tauri::command]
pub fn get_quickgen_template(app: AppHandle) -> Result<String, String> {
    let res = app
        .path()
        .resource_dir()
        .unwrap_or_else(|_| PathBuf::from("."));
    let p = resource_path(&res, "quickgen/txt2img-api.json")
        .ok_or_else(|| "找不到快速生图模板".to_string())?;
    fs::read_to_string(&p).map_err(|e| e.to_string())
}

// ---------------- manifest sync (GitHub) ----------------

#[derive(Serialize)]
pub struct ManifestSyncResult {
    pub version: u32,
    pub updated: String,
    pub groups: usize,
    pub components: usize,
    pub source: String,
}

/// 从配置的 GitHub 地址拉取最新资源清单并落盘（此后优先生效）。
/// 组件增删、下载源调整、模型版本更新都不需要发新版本应用。
#[tauri::command]
pub async fn sync_manifest(
    state: tauri::State<'_, Mutex<crate::AppState>>,
) -> Result<ManifestSyncResult, String> {
    let url = {
        let st = state.lock().unwrap();
        st.config.manifest_url.clone()
    };
    if url.trim().is_empty() {
        return Err("未配置清单同步地址".into());
    }
    let client = build_client();
    let resp = client
        .get(&url)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("拉取清单失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("拉取清单失败：服务器返回 {}", resp.status()));
    }
    let text = resp.text().await.map_err(|e| e.to_string())?;
    let manifest: Manifest = serde_json::from_str(&text)
        .map_err(|e| format!("清单格式无效（拒绝覆盖本地副本）: {e}"))?;
    let groups = manifest.groups.len();
    let components = manifest.groups.iter().map(|g| g.components.len()).sum();
    let path = synced_manifest_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, &text).map_err(|e| format!("写入同步副本失败: {e}"))?;
    Ok(ManifestSyncResult {
        version: manifest.version,
        updated: manifest.updated,
        groups,
        components,
        source: url,
    })
}
