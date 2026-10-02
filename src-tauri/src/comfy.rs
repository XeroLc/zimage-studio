use crate::config::{log_dir, AppConfig};
use serde::Serialize;
use serde_json::Value;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

use crate::AppState;

pub struct ComfyState {
    pub child: Option<Child>,
    pub state: String, // stopped | starting | ready | stopping | error
    pub error: String,
    pub log_path: PathBuf,
    pub start_at: Option<Instant>,
}

impl ComfyState {
    pub fn new() -> Self {
        Self {
            child: None,
            state: "stopped".into(),
            error: String::new(),
            log_path: log_dir().join("comfy.log"),
            start_at: None,
        }
    }
}

pub fn python_exe(comfy_dir: &Path) -> PathBuf {
    comfy_dir.join(".venv").join("Scripts").join("python.exe")
}

pub fn probe_host(host: &str) -> &str {
    if host == "0.0.0.0" || host.is_empty() {
        "127.0.0.1"
    } else {
        host
    }
}

pub fn port_open(host: &str, port: u16) -> bool {
    format!("{}:{}", probe_host(host), port)
        .parse::<SocketAddr>()
        .ok()
        .and_then(|addr| TcpStream::connect_timeout(&addr, Duration::from_millis(600)).ok())
        .is_some()
}

/// strip ANSI escape sequences
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if let Some('[') = chars.peek() {
                chars.next();
                while let Some(&n) = chars.peek() {
                    chars.next();
                    if ('@'..='~').contains(&n) {
                        break;
                    }
                }
                continue;
            }
        }
        out.push(c);
    }
    out
}

pub fn read_tail_lines(path: &Path, n: usize) -> Vec<String> {
    let mut f = match File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let cap: u64 = 256 * 1024;
    if len > cap {
        let _ = f.seek(SeekFrom::Start(len - cap));
    }
    let mut buf = Vec::new();
    if f.read_to_end(&mut buf).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&buf);
    text.lines()
        .rev()
        .take(n)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|l| strip_ansi(l))
        .collect()
}

pub fn refresh(st: &mut AppState) {
    // child exit detection
    let mut exited: Option<Option<i32>> = None;
    if let Some(child) = st.comfy.child.as_mut() {
        if let Ok(Some(status)) = child.try_wait() {
            exited = Some(status.code());
        }
    }
    if let Some(code) = exited {
        st.comfy.child = None;
        match st.comfy.state.as_str() {
            "stopping" => st.comfy.state = "stopped".into(),
            "ready" => st.comfy.state = "stopped".into(),
            "starting" => {
                st.comfy.state = "error".into();
                if st.comfy.error.is_empty() {
                    st.comfy.error =
                        format!("ComfyUI 启动失败（退出代码 {}），请查看运行日志", code.unwrap_or(-1));
                }
            }
            _ => st.comfy.state = "stopped".into(),
        }
    }

    // readiness probing while starting
    if st.comfy.state == "starting" {
        if port_open(&st.config.host, st.config.port) {
            st.comfy.state = "ready".into();
            st.comfy.error.clear();
        } else if let Some(t0) = st.comfy.start_at {
            if t0.elapsed() > Duration::from_secs(300) {
                st.comfy.state = "error".into();
                st.comfy.error = "等待服务就绪超时（5 分钟）".into();
            }
        }
    }

    // adopt externally-started instance
    if st.comfy.state == "stopped"
        && st.comfy.child.is_none()
        && port_open(&st.config.host, st.config.port)
    {
        st.comfy.state = "ready".into();
    }
}

// ---------------- commands ----------------

#[derive(Serialize)]
pub struct StatusInfo {
    pub state: String,
    pub url: String,
    pub host: String,
    pub port: u16,
    pub pid: Option<u32>,
    pub error: String,
    pub autostart: bool,
    pub initial_tab: String,
    pub autotrain: bool,
    /// 是否已配置好（ComfyUI 环境存在）
    pub configured: bool,
    /// 运行中的实例不是本应用启动的
    pub external: bool,
    pub data_root: String,
    pub comfy_dir: String,
    pub open_target: String,
    pub auto_enter: bool,
}

#[tauri::command]
pub fn get_config(state: State<'_, Mutex<AppState>>) -> AppConfig {
    state.lock().unwrap().config.clone()
}

#[tauri::command]
pub fn save_config(cfg: AppConfig, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut st = state.lock().unwrap();
    st.config = cfg.clone();
    let path = st.config_path.clone();
    crate::config::save_config_to(&path, &cfg)
}

#[tauri::command]
pub fn get_status(state: State<'_, Mutex<AppState>>) -> StatusInfo {
    let mut st = state.lock().unwrap();
    refresh(&mut st);
    let comfy_dir = st.config.comfy_dir();
    let configured = python_exe(&comfy_dir).exists();
    StatusInfo {
        state: st.comfy.state.clone(),
        url: format!("http://{}:{}", probe_host(&st.config.host), st.config.port),
        host: st.config.host.clone(),
        port: st.config.port,
        pid: st.comfy.child.as_ref().map(|c| c.id()),
        error: st.comfy.error.clone(),
        autostart: st.autostart,
        initial_tab: st.initial_tab.clone(),
        autotrain: st.autotrain,
        configured,
        external: st.comfy.state == "ready" && st.comfy.child.is_none(),
        data_root: st.config.data_root.clone(),
        comfy_dir: comfy_dir.to_string_lossy().into_owned(),
        open_target: st.config.open_target.clone(),
        auto_enter: st.config.auto_enter,
    }
}

#[tauri::command]
pub fn get_log(state: State<'_, Mutex<AppState>>) -> Vec<String> {
    let path = state.lock().unwrap().comfy.log_path.clone();
    read_tail_lines(&path, 400)
}

#[tauri::command]
pub fn start_comfy(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut st = state.lock().unwrap();
    refresh(&mut st);
    if st.comfy.state == "starting" || st.comfy.state == "ready" {
        return Err("ComfyUI 已在运行".into());
    }
    if st.train.child.is_some() {
        return Err("LoRA 训练正在运行中，请先停止训练再启动 ComfyUI（显存互斥）".into());
    }

    let cfg = st.config.clone();
    let comfy_dir = cfg.comfy_dir();
    let py = python_exe(&comfy_dir);
    if !py.exists() {
        st.comfy.state = "error".into();
        st.comfy.error = if cfg.data_root.is_empty() {
            "尚未配置环境。请先在「资源中心」选择数据目录并一键安装。".into()
        } else {
            format!("找不到 ComfyUI 环境：{}", py.display())
        };
        return Err(st.comfy.error.clone());
    }
    if port_open(&cfg.host, cfg.port) {
        st.comfy.state = "error".into();
        st.comfy.error = format!(
            "端口 {} 已被占用（可能已有 ComfyUI 在运行）。请先停止它，或更换端口。",
            cfg.port
        );
        return Err(st.comfy.error.clone());
    }

    let log_file = File::create(&st.comfy.log_path).map_err(|e| e.to_string())?;
    let log_file2 = log_file.try_clone().map_err(|e| e.to_string())?;

    let mut cmd = Command::new(&py);
    cmd.current_dir(&comfy_dir)
        .arg("-u")
        .arg("-X")
        .arg("utf8")
        .arg("main.py")
        .arg("--listen")
        .arg(&cfg.host)
        .arg("--port")
        .arg(cfg.port.to_string());

    match cfg.vram_mode.as_str() {
        "low" => {
            cmd.arg("--lowvram");
        }
        "normal" => {
            cmd.arg("--normalvram");
        }
        "none" => {
            cmd.arg("--novram");
        }
        _ => {}
    }
    let preview = if cfg.preview.is_empty() {
        "auto".to_string()
    } else {
        cfg.preview.clone()
    };
    cmd.arg("--preview-method").arg(preview);

    // 内嵌工作区需要放开 ComfyUI 的跨源限制（仅本机回环，代价可接受）
    if cfg.embed_support {
        cmd.arg("--enable-cors-header").arg("http://tauri.localhost");
    }
    for piece in cfg.extra_args.split_whitespace() {
        cmd.arg(piece);
    }

    cmd.stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file2));
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    match cmd.spawn() {
        Ok(child) => {
            st.comfy.child = Some(child);
            st.comfy.state = "starting".into();
            st.comfy.error.clear();
            st.comfy.start_at = Some(Instant::now());
            Ok(())
        }
        Err(e) => {
            st.comfy.state = "error".into();
            st.comfy.error = format!("启动失败：{}", e);
            Err(st.comfy.error.clone())
        }
    }
}

fn find_pid_on_port(port: u16) -> Option<u32> {
    let mut cmd = Command::new("netstat");
    cmd.args(["-ano", "-p", "TCP"]);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd.output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let needle = format!(":{}", port);
    let mut pid = None;
    for line in text.lines() {
        if line.contains("LISTENING") && line.contains(&needle) {
            if let Some(p) = line.split_whitespace().last() {
                pid = p.parse::<u32>().ok();
            }
        }
    }
    pid
}

#[tauri::command]
pub fn kill_pid(pid: u32) {
    let mut cmd = Command::new("taskkill");
    cmd.args(["/PID", &pid.to_string(), "/T", "/F"]);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let _ = cmd.output();
}

#[tauri::command]
pub fn stop_comfy(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut st = state.lock().unwrap();
    match st.comfy.child.as_ref().map(|c| c.id()) {
        Some(pid) => {
            st.comfy.state = "stopping".into();
            kill_pid(pid);
            Ok(())
        }
        None => {
            // 外部实例：按端口找 PID 结束
            if port_open(&st.config.host, st.config.port) {
                if let Some(pid) = find_pid_on_port(st.config.port) {
                    st.comfy.state = "stopping".into();
                    kill_pid(pid);
                    return Ok(());
                }
                return Err("检测到外部实例但无法定位其进程".into());
            }
            st.comfy.state = "stopped".into();
            Ok(())
        }
    }
}

#[tauri::command]
pub fn open_browser(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let st = state.lock().unwrap();
    if st.comfy.state != "ready" {
        return Err("ComfyUI 尚未就绪".into());
    }
    let url = format!("http://{}:{}", probe_host(&st.config.host), st.config.port);
    let mut cmd = Command::new("cmd");
    cmd.args(["/c", "start", "", &url]);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

/// 打开独立的 ComfyUI 窗口（应用内独立窗口，非系统浏览器）
#[tauri::command]
pub fn open_comfy_window(app: AppHandle, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let url = {
        let st = state.lock().unwrap();
        if st.comfy.state != "ready" {
            return Err("ComfyUI 尚未就绪".into());
        }
        format!("http://{}:{}", probe_host(&st.config.host), st.config.port)
    };
    if let Some(w) = app.get_webview_window("comfy") {
        let _ = w.show();
        let _ = w.set_focus();
        return Ok(());
    }
    WebviewWindowBuilder::new(
        &app,
        "comfy",
        WebviewUrl::External(url.parse().map_err(|e| format!("URL 无效: {e}"))?),
    )
    .title("ComfyUI · Z-Image Studio")
    .inner_size(1320.0, 880.0)
    .min_inner_size(900.0, 600.0)
    .center()
    .build()
    .map_err(|e| format!("打开窗口失败: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn enter_workspace(app: AppHandle) -> Result<(), String> {
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "找不到主窗口".to_string())?;
    let scale = win.scale_factor().unwrap_or(1.0);
    let (sw, sh) = win
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| win.primary_monitor().ok().flatten())
        .map(|m| {
            let s = m.size();
            (s.width as f64 / scale, s.height as f64 / scale)
        })
        .unwrap_or((1920.0, 1080.0));
    let w = (sw * 0.9).clamp(1100.0, 1680.0);
    let h = (sh * 0.88).clamp(700.0, 1020.0);
    let _ = win.set_min_size(Some(tauri::LogicalSize::new(900.0, 600.0)));
    let _ = win.set_size(tauri::LogicalSize::new(w, h));
    let _ = win.center();
    Ok(())
}

#[tauri::command]
pub fn exit_workspace(app: AppHandle) -> Result<(), String> {
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "找不到主窗口".to_string())?;
    let _ = win.set_min_size(Some(tauri::LogicalSize::new(680.0, 560.0)));
    let _ = win.set_size(tauri::LogicalSize::new(820.0, 680.0));
    let _ = win.center();
    Ok(())
}

// ---------------- ComfyUI API passthrough ----------------

/// 通过 Rust 侧转发 ComfyUI HTTP API（绕过 WebView 的跨源/安全中间件限制）
#[tauri::command]
pub async fn comfy_api(
    state: State<'_, Mutex<AppState>>,
    method: String,
    path: String,
    body: Option<Value>,
) -> Result<Value, String> {
    let (host, port) = {
        let st = state.lock().unwrap();
        (probe_host(&st.config.host).to_string(), st.config.port)
    };
    let url = format!("http://{}:{}{}", host, port, path);
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    let client = CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("client")
    });
    let mut req = match method.to_uppercase().as_str() {
        "POST" => client.post(&url),
        "DELETE" => client.delete(&url),
        _ => client.get(&url),
    };
    if let Some(b) = &body {
        req = req.json(b);
    }
    let resp = req.send().await.map_err(|e| format!("请求失败: {e}"))?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("ComfyUI 返回 {}: {}", status, text.chars().take(300).collect::<String>()));
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(v) => Ok(v),
        Err(_) => Ok(serde_json::json!({ "raw": text })),
    }
}

#[tauri::command]
pub fn comfy_url(state: State<'_, Mutex<AppState>>) -> String {
    let st = state.lock().unwrap();
    format!("http://{}:{}", probe_host(&st.config.host), st.config.port)
}

// ---------------- system stats ----------------

#[derive(Serialize, Default)]
pub struct SysStats {
    pub available: bool,
    pub name: String,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    pub util: u64,
    pub temp: u64,
}

#[tauri::command]
pub fn sys_stats() -> SysStats {
    let mut cmd = Command::new("nvidia-smi");
    cmd.args([
        "--query-gpu=name,memory.used,memory.total,utilization.gpu,temperature.gpu",
        "--format=csv,noheader,nounits",
    ]);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    match cmd.output() {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            let line = text.lines().next().unwrap_or("");
            let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
            if parts.len() >= 5 {
                return SysStats {
                    available: true,
                    name: parts[0].to_string(),
                    mem_used_mb: parts[1].parse().unwrap_or(0),
                    mem_total_mb: parts[2].parse().unwrap_or(0),
                    util: parts[3].parse().unwrap_or(0),
                    temp: parts[4].parse().unwrap_or(0),
                };
            }
            SysStats::default()
        }
        _ => SysStats::default(),
    }
}

// ---------------- outputs gallery ----------------

#[derive(Serialize)]
pub struct OutputItem {
    pub rel: String,       // 相对 output 目录，正斜杠
    pub filename: String,
    pub subfolder: String,
    pub size: u64,
    pub mtime: u64,
}

fn output_dir(st: &AppState) -> PathBuf {
    st.config.comfy_dir().join("output")
}

fn is_image(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.ends_with(".png") || l.ends_with(".jpg") || l.ends_with(".jpeg") || l.ends_with(".webp")
}

#[tauri::command]
pub fn list_outputs(state: State<'_, Mutex<AppState>>, limit: Option<usize>) -> Vec<OutputItem> {
    let st = state.lock().unwrap();
    let dir = output_dir(&st);
    let mut items: Vec<OutputItem> = Vec::new();
    let mut stack = vec![dir.clone()];
    while let Some(d) = stack.pop() {
        if let Ok(rd) = fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if is_image(&e.file_name().to_string_lossy()) {
                    if let Ok(meta) = e.metadata() {
                        let rel = p
                            .strip_prefix(&dir)
                            .map(|r| r.to_string_lossy().replace('\\', "/"))
                            .unwrap_or_default();
                        let subfolder = Path::new(&rel)
                            .parent()
                            .map(|x| x.to_string_lossy().replace('\\', "/"))
                            .unwrap_or_default();
                        let mtime = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        items.push(OutputItem {
                            rel,
                            filename: e.file_name().to_string_lossy().into_owned(),
                            subfolder,
                            size: meta.len(),
                            mtime,
                        });
                    }
                }
            }
        }
    }
    items.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    items.truncate(limit.unwrap_or(80));
    items
}

#[derive(Serialize, Default)]
pub struct OutputMeta {
    pub texts: Vec<String>,
    pub seed: Option<i64>,
    pub steps: Option<u64>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub model: String,
}

/// 从 PNG tEXt 块解析生成参数
fn parse_png_prompt(path: &Path) -> Option<String> {
    let mut f = File::open(path).ok()?;
    let mut sig = [0u8; 8];
    f.read_exact(&mut sig).ok()?;
    if &sig != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    loop {
        let mut len_b = [0u8; 4];
        f.read_exact(&mut len_b).ok()?;
        let len = u32::from_be_bytes(len_b) as usize;
        let mut ctype = [0u8; 4];
        f.read_exact(&mut ctype).ok()?;
        if &ctype == b"tEXt" {
            let mut data = vec![0u8; len.min(8 * 1024 * 1024)];
            f.read_exact(&mut data).ok()?;
            let _ = f.seek(SeekFrom::Current(4)); // crc
            if let Some(pos) = data.iter().position(|&b| b == 0) {
                let key = String::from_utf8_lossy(&data[..pos]);
                if key == "prompt" {
                    return Some(String::from_utf8_lossy(&data[pos + 1..]).into_owned());
                }
            }
        } else if &ctype == b"IEND" {
            return None;
        } else {
            let _ = f.seek(SeekFrom::Current(len as i64 + 4));
        }
    }
}

#[tauri::command]
pub fn output_meta(state: State<'_, Mutex<AppState>>, rel: String) -> OutputMeta {
    let st = state.lock().unwrap();
    let base = output_dir(&st);
    let path = base.join(rel.replace('\\', "/"));
    let mut meta = OutputMeta::default();
    let Some(text) = parse_png_prompt(&path) else {
        return meta;
    };
    let Ok(v) = serde_json::from_str::<Value>(&text) else {
        return meta;
    };
    let Some(obj) = v.as_object() else { return meta };
    for (_id, node) in obj {
        let ct = node.get("class_type").and_then(|x| x.as_str()).unwrap_or("");
        let inputs = node.get("inputs");
        match ct {
            "CLIPTextEncode" => {
                if let Some(t) = inputs.and_then(|i| i.get("text")).and_then(|x| x.as_str()) {
                    if !t.trim().is_empty() {
                        meta.texts.push(t.to_string());
                    }
                }
            }
            "KSampler" | "KSamplerAdvanced" => {
                if let Some(i) = inputs {
                    meta.seed = i.get("seed").and_then(|x| x.as_i64());
                    meta.steps = i.get("steps").and_then(|x| x.as_u64());
                }
            }
            "EmptySD3LatentImage" | "EmptyLatentImage" => {
                if let Some(i) = inputs {
                    meta.width = i.get("width").and_then(|x| x.as_u64());
                    meta.height = i.get("height").and_then(|x| x.as_u64());
                }
            }
            "UNETLoader" | "CheckpointLoaderSimple" => {
                if let Some(i) = inputs {
                    if let Some(n) = i
                        .get("unet_name")
                        .or_else(|| i.get("ckpt_name"))
                        .and_then(|x| x.as_str())
                    {
                        meta.model = n.to_string();
                    }
                }
            }
            _ => {}
        }
    }
    meta
}

#[tauri::command]
pub fn delete_output(state: State<'_, Mutex<AppState>>, rel: String) -> Result<(), String> {
    let st = state.lock().unwrap();
    let base = output_dir(&st);
    let path = base.join(rel.replace('\\', "/"));
    // 防目录穿越：确保最终路径仍在 output 目录内
    let canon_base = base.canonicalize().map_err(|e| e.to_string())?;
    let canon = path.canonicalize().map_err(|e| e.to_string())?;
    if !canon.starts_with(&canon_base) {
        return Err("路径越界，拒绝删除".into());
    }
    fs::remove_file(&canon).map_err(|e| e.to_string())
}

// ---------------- open paths ----------------

#[tauri::command]
pub fn open_path(state: State<'_, Mutex<AppState>>, kind: String) -> Result<(), String> {
    let st = state.lock().unwrap();
    let cfg = &st.config;
    let root = cfg.root().unwrap_or_else(|| {
        cfg.comfy_dir()
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default()
    });
    let toolkit = cfg.ai_toolkit_dir();
    let path: PathBuf = match kind.as_str() {
        "data_root" => root.clone(),
        "comfy" => cfg.comfy_dir(),
        "models" => root.join("models"),
        "outputs" => cfg.comfy_dir().join("output"),
        "workflows" => root.join("workflows"),
        "loras" => root.join("models").join("loras"),
        "aitk" => toolkit.clone(),
        "aitk_dataset" => toolkit.join("datasets").join("character"),
        "aitk_configs" => toolkit.join("config"),
        "aitk_out" => root.join("outputs").join("training"),
        "logs" => log_dir(),
        other => PathBuf::from(other),
    };
    if path.as_os_str().is_empty() || !path.exists() {
        return Err(format!("目录不存在：{}", path.display()));
    }
    let mut cmd = Command::new("explorer");
    cmd.arg(&path);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    let _ = cmd.spawn();
    Ok(())
}
