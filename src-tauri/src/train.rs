use crate::config::log_dir;
use crate::comfy::read_tail_lines;
use serde::Serialize;
use std::fs::{self, File};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Instant;
use tauri::State;

#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

use crate::AppState;

pub struct TrainState {
    pub child: Option<Child>,
    pub state: String, // idle | running | stopping | completed | error
    pub config_file: String,
    pub log_path: PathBuf,
    pub start_at: Option<Instant>,
    pub error: String,
}

impl TrainState {
    pub fn new() -> Self {
        Self {
            child: None,
            state: "idle".into(),
            config_file: String::new(),
            log_path: log_dir().join("train.log"),
            start_at: None,
            error: String::new(),
        }
    }
}

pub fn refresh_train(st: &mut AppState) {
    let mut exited: Option<Option<i32>> = None;
    if let Some(child) = st.train.child.as_mut() {
        if let Ok(Some(status)) = child.try_wait() {
            exited = Some(status.code());
        }
    }
    if let Some(code) = exited {
        st.train.child = None;
        match st.train.state.as_str() {
            "stopping" => st.train.state = "idle".into(),
            "running" => {
                if code == Some(0) {
                    st.train.state = "completed".into();
                } else {
                    st.train.state = "error".into();
                    st.train.error = format!("训练进程退出（代码 {}），请查看日志", code.unwrap_or(-1));
                }
            }
            _ => st.train.state = "idle".into(),
        }
    }
}

#[derive(Serialize)]
pub struct TrainInfo {
    pub state: String,
    pub config_file: String,
    pub pid: Option<u32>,
    pub error: String,
    pub ai_toolkit_dir: String,
}

#[tauri::command]
pub fn get_train_info(state: State<'_, Mutex<AppState>>) -> TrainInfo {
    let mut st = state.lock().unwrap();
    refresh_train(&mut st);
    let dir = st.config.ai_toolkit_dir();
    TrainInfo {
        state: st.train.state.clone(),
        config_file: st.train.config_file.clone(),
        pid: st.train.child.as_ref().map(|c| c.id()),
        error: st.train.error.clone(),
        ai_toolkit_dir: dir.to_string_lossy().into_owned(),
    }
}

#[tauri::command]
pub fn list_train_configs(state: State<'_, Mutex<AppState>>) -> Vec<String> {
    let st = state.lock().unwrap();
    let dir = st.config.ai_toolkit_dir().join("config");
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(".yml") || name.ends_with(".yaml") {
                out.push(name);
            }
        }
    }
    out.sort();
    out
}

#[tauri::command]
pub fn get_train_log(state: State<'_, Mutex<AppState>>) -> Vec<String> {
    let path = state.lock().unwrap().train.log_path.clone();
    read_tail_lines(&path, 400)
}

#[tauri::command]
pub fn start_training(config_file: String, state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut st = state.lock().unwrap();
    crate::comfy::refresh(&mut st);
    refresh_train(&mut st);
    if st.train.child.is_some() {
        return Err("训练已在运行".into());
    }
    if st.comfy.state == "starting" || st.comfy.state == "ready" {
        return Err("ComfyUI 正在运行，请先停止它再启动训练（显存互斥）".into());
    }
    let toolkit = st.config.ai_toolkit_dir();
    let cfg_path = toolkit.join("config").join(&config_file);
    if !cfg_path.exists() {
        st.train.state = "error".into();
        st.train.error = format!("找不到训练配置：{}", cfg_path.display());
        return Err(st.train.error.clone());
    }
    let py = toolkit.join(".venv").join("Scripts").join("python.exe");
    if !py.exists() {
        st.train.state = "error".into();
        st.train.error = if st.config.data_root.is_empty() {
            "尚未配置训练环境。请先在「资源中心」安装 LoRA 训练环境。".into()
        } else {
            format!("找不到训练环境：{}", py.display())
        };
        return Err(st.train.error.clone());
    }

    let log_file = File::create(&st.train.log_path).map_err(|e| e.to_string())?;
    let log_file2 = log_file.try_clone().map_err(|e| e.to_string())?;

    let mut cmd = Command::new(&py);
    cmd.current_dir(&toolkit)
        .arg("-u")
        .arg("-X")
        .arg("utf8")
        .arg("run.py")
        .arg(format!("config/{}", config_file));
    cmd.env("PYTHONIOENCODING", "utf-8");
    if let Some(parent) = toolkit.parent() {
        cmd.env("MODELS_PATH", parent.join("models"));
    }
    cmd.env("HF_ENDPOINT", "https://hf-mirror.net");
    cmd.stdout(Stdio::from(log_file))
        .stderr(Stdio::from(log_file2));
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    match cmd.spawn() {
        Ok(child) => {
            st.train.child = Some(child);
            st.train.state = "running".into();
            st.train.error.clear();
            st.train.config_file = config_file;
            st.train.start_at = Some(Instant::now());
            Ok(())
        }
        Err(e) => {
            st.train.state = "error".into();
            st.train.error = format!("启动失败：{}", e);
            Err(st.train.error.clone())
        }
    }
}

#[tauri::command]
pub fn stop_training(state: State<'_, Mutex<AppState>>) -> Result<(), String> {
    let mut st = state.lock().unwrap();
    let pid = st.train.child.as_ref().map(|c| c.id());
    match pid {
        Some(pid) => {
            st.train.state = "stopping".into();
            let mut cmd = Command::new("taskkill");
            cmd.args(["/PID", &pid.to_string(), "/T", "/F"]);
            #[cfg(windows)]
            cmd.creation_flags(CREATE_NO_WINDOW);
            let _ = cmd.output();
            Ok(())
        }
        None => {
            st.train.state = "idle".into();
            Ok(())
        }
    }
}
