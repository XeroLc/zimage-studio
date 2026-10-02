use crate::config::AppConfig;
use crate::download::{build_client, download_file, file_ready, resolve_url, DownloadEvent};
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
    pub groups: Vec<Group>,
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
        _ => false,
    }
}

pub fn component_installed(root: &Path, comp: &Component) -> bool {
    match &comp.check {
        Some(c) if !c.all.is_empty() => c.all.iter().all(|r| rule_ok(root, r)),
        _ => false,
    }
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
}

pub fn build_setup_info(cfg: &AppConfig, resource_dir: &Path) -> SetupInfo {
    let manifest = load_manifest(resource_dir).ok();
    let root = cfg.root();
    let (version, updated) = manifest
        .as_ref()
        .map(|m| (m.version, m.updated.clone()))
        .unwrap_or((0, String::new()));
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
                run_step(app, ctl, &client, &root, resource_dir, step).await?;
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
                let url = resolve_url(&f.url);
                let start = Instant::now();
                let mut last_bytes = 0u64;
                let ctl2 = Arc::clone(ctl);
                let app2 = app.clone();
                let fsize = f.size;
                download_file(client, &url, &dest, fsize, &ctl.cancel, move |ev: DownloadEvent| {
                    let mut s = ctl2.snap.lock().unwrap();
                    s.downloaded = ev.downloaded;
                    if ev.total > 0 {
                        s.total = ev.total;
                    }
                    let secs = start.elapsed().as_secs_f64();
                    if secs > 0.5 {
                        s.speed = (ev.downloaded.saturating_sub(last_bytes)) as f64
                            / start.elapsed().as_secs_f64();
                    }
                    last_bytes = 0;
                    let snap = s.clone();
                    drop(s);
                    let _ = app2.emit("setup://progress", snap);
                    true
                })
                .await
                .map_err(|e| format!("下载 {} 失败: {e}", short_name(&f.to)))?;
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
            set_phase(ctl, "step", &format!("执行：{}", label));
            publish(app, ctl);
            run_command(app, ctl, &step2, &root2)
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
            let content = substitute_root(&content, root);
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

fn substitute_root(s: &str, root: &Path) -> String {
    let r = root.to_string_lossy().replace('\\', "/");
    s.replace("{{ROOT}}", &r)
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
) -> Result<(), String> {
    let exe_raw = step.exe.clone().unwrap_or_default();
    let exe = {
        let p = PathBuf::from(exe_raw.replace('\\', "/"));
        if p.is_absolute() {
            p
        } else {
            root.join(step.exe.clone().unwrap_or_default())
        }
    };
    if !exe.exists() {
        return Err(format!("找不到程序 {}", exe.display()));
    }
    let mut cmd = Command::new(&exe);
    if let Some(args) = &step.args {
        for a in args {
            cmd.arg(substitute_root(a, root));
        }
    }
    cmd.current_dir(root);
    if let Some(env) = &step.env {
        for (k, v) in env {
            cmd.env(k, substitute_root(v, root));
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
