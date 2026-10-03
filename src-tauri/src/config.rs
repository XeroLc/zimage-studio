use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// 下载源选择（分通道）。每个通道 "auto" = 测速优选（失败自动切换备用）。
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct SourcesCfg {
    /// auto | modelscope | hfm
    pub models: String,
    /// auto | proxy | direct
    pub github: String,
    /// auto | tsinghua | aliyun | official
    pub pypi: String,
    /// auto | official | sjtu | aliyun
    pub torch: String,
}

impl Default for SourcesCfg {
    fn default() -> Self {
        Self {
            models: "auto".into(),
            github: "auto".into(),
            pypi: "auto".into(),
            torch: "auto".into(),
        }
    }
}

/// 应用配置。除旧字段外新增数据目录 / 打开方式等字段，
/// 读取旧配置时新字段自动取默认值（serde(default)）。
#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AppConfig {
    /// 数据目录：整套环境（ComfyUI / models / ai-toolkit / outputs ...）的根
    pub data_root: String,
    /// ComfyUI 目录；留空 = {data_root}/ComfyUI
    pub comfy_dir: String,
    /// ai-toolkit 目录；留空 = {data_root}/ai-toolkit
    pub ai_toolkit_dir: String,
    pub host: String,
    pub port: u16,
    pub vram_mode: String,
    pub preview: String,
    pub extra_args: String,
    /// 就绪后的打开方式: workspace(内嵌工作区) | window(独立窗口) | browser(系统浏览器)
    pub open_target: String,
    /// 就绪后自动进入所选打开方式
    pub auto_enter: bool,
    /// 启动 ComfyUI 时附带 --enable-cors-header（内嵌工作区必需）
    pub embed_support: bool,
    /// 资源清单同步地址（GitHub 仓库里的 setup-manifest.json；
    /// 默认经 gh-proxy 走国内可用的 raw 地址。改仓库时只改这里）
    pub manifest_url: String,
    /// 启动时自动同步资源清单（带时间戳绕 CDN 缓存；失败静默保留本地）
    pub manifest_auto_sync: bool,
    /// 启动/安装时自动把外层 models/* 以目录链接（junction）挂进 ComfyUI/models/*
    pub auto_link_models: bool,
    /// 分通道下载源选择（见 SourcesCfg）
    pub sources: SourcesCfg,
    /// 保留旧字段以兼容旧配置文件（新版不再使用）
    pub open_browser: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            data_root: String::new(),
            comfy_dir: String::new(),
            ai_toolkit_dir: String::new(),
            host: "127.0.0.1".into(),
            port: 8188,
            vram_mode: "auto".into(),
            preview: "auto".into(),
            extra_args: String::new(),
            open_target: "workspace".into(),
            auto_enter: false,
            embed_support: true,
            manifest_url:
                "https://gh-proxy.com/https://raw.githubusercontent.com/XeroLc/zimage-studio/main/src-tauri/resources/setup-manifest.json".into(),
            manifest_auto_sync: true,
            auto_link_models: true,
            sources: SourcesCfg::default(),
            open_browser: false,
        }
    }
}

impl AppConfig {
    pub fn root(&self) -> Option<PathBuf> {
        if self.data_root.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(self.data_root.replace('\\', "/")))
        }
    }

    pub fn comfy_dir(&self) -> PathBuf {
        if !self.comfy_dir.trim().is_empty() {
            PathBuf::from(self.comfy_dir.replace('\\', "/"))
        } else if let Some(r) = self.root() {
            r.join("ComfyUI")
        } else {
            PathBuf::new()
        }
    }

    pub fn ai_toolkit_dir(&self) -> PathBuf {
        if !self.ai_toolkit_dir.trim().is_empty() {
            PathBuf::from(self.ai_toolkit_dir.replace('\\', "/"))
        } else if let Some(r) = self.root() {
            r.join("ai-toolkit")
        } else {
            derive_ai_toolkit_dir()
        }
    }

    /// 为旧版配置做迁移：旧版只有 comfy_dir（形如 <root>/ComfyUI），据此推断数据目录。
    pub fn migrate_legacy(&mut self) {
        if self.data_root.trim().is_empty() && !self.comfy_dir.trim().is_empty() {
            let p = PathBuf::from(self.comfy_dir.replace('\\', "/"));
            if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    self.data_root = parent.to_string_lossy().replace('\\', "/");
                }
            }
        }
    }
}

/// 从可执行文件向上寻找同级 `ai-toolkit` 目录（无数据目录配置时的兜底）
pub fn derive_ai_toolkit_dir() -> PathBuf {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return PathBuf::new(),
    };
    let mut dir = exe.parent().map(|d| d.to_path_buf());
    for _ in 0..8 {
        let d = match dir {
            Some(d) => d,
            None => break,
        };
        let candidate = d.join("ai-toolkit");
        if candidate.join("run.py").exists() {
            return candidate;
        }
        dir = d.parent().map(|p| p.to_path_buf());
    }
    PathBuf::new()
}

/// 应用数据目录：%APPDATA%\com.zimage.studio（配置与日志的家，
/// 装在 Program Files 下也能写；不受 360 清理 %TEMP% 影响）
pub fn app_data_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("com.zimage.studio")
}

pub fn config_path() -> PathBuf {
    app_data_dir().join("config.json")
}

pub fn log_dir() -> PathBuf {
    let dir = app_data_dir().join("logs");
    let _ = fs::create_dir_all(&dir);
    dir
}

/// 旧版配置的位置：exe 同目录 zimage-studio.config.json
pub fn legacy_config_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("zimage-studio.config.json")
}

pub fn load_config(path: &Path) -> AppConfig {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<AppConfig>(&s).ok())
        .unwrap_or_default()
}

/// 启动时载入配置：优先新位置；否则迁移旧位置（exe 同目录）
pub fn load_or_migrate() -> (AppConfig, PathBuf) {
    let cpath = config_path();
    if cpath.exists() {
        return (load_config(&cpath), cpath);
    }
    let legacy = legacy_config_path();
    if legacy.exists() {
        let mut cfg = load_config(&legacy);
        cfg.migrate_legacy();
        let _ = save_config_to(&cpath, &cfg);
        // 迁移后把旧文件改名，避免两处配置互相困惑
        let _ = fs::rename(&legacy, legacy.with_extension("json.migrated"));
        return (cfg, cpath);
    }
    // 全新安装：探测一个合理的默认数据目录建议（不写死，留给用户确认）
    let mut cfg = AppConfig::default();
    for candidate in ["D:/AI/image-gen", "C:/AI/image-gen"] {
        if Path::new(candidate).join("ComfyUI").join("main.py").exists() {
            cfg.data_root = candidate.into();
            break;
        }
    }
    cfg.host = "127.0.0.1".into();
    (cfg, cpath)
}

pub fn save_config_to(path: &Path, cfg: &AppConfig) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

/// 探测建议的数据目录（新机器首配时展示）
pub fn suggest_data_root() -> String {
    // 已有配置优先，其次常见位置，最后用户主目录
    for candidate in ["D:/AI/image-gen", "C:/AI/image-gen"] {
        if Path::new(candidate).join("ComfyUI").join("main.py").exists() {
            return candidate.into();
        }
    }
    if std::env::var("USERPROFILE")
        .map(|p| Path::new(&p).join("Z-Image-Studio").exists())
        .unwrap_or(false)
    {
        return format!(
            "{}/Z-Image-Studio",
            std::env::var("USERPROFILE").unwrap().replace('\\', "/")
        );
    }
    "D:/AI/image-gen".into()
}
