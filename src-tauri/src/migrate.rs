//! 数据目录迁移：整体移动（同盘秒级 rename / 跨盘复制）+ 修正内部绝对路径引用
//! 迁移对象：整个数据目录（ComfyUI / models / ai-toolkit / outputs / workflows ...）

use crate::config::AppConfig;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

#[derive(Serialize, Clone, Default)]
pub struct MigrateProgress {
    pub phase: String, // scan | copy | cleanup | rewrite | done
    pub copied: u64,
    pub total: u64,
    pub files: usize,
    pub message: String,
}

#[derive(Serialize, Clone)]
pub struct MigrateReport {
    pub mode: String, // rename | copy
    pub files: usize,
    pub bytes: u64,
    pub new_root: String,
}

/// 目录内字节数与文件数（用于复制进度）
fn scan_dir(dir: &Path) -> (u64, usize) {
    let mut bytes = 0u64;
    let mut files = 0usize;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        if let Ok(rd) = fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if let Ok(m) = e.metadata() {
                    bytes += m.len();
                    files += 1;
                }
            }
        }
    }
    (bytes, files)
}

fn copy_dir_recursive(
    src: &Path,
    dst: &Path,
    on_progress: &mut dyn FnMut(u64),
) -> Result<usize, String> {
    fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    let mut copied_files = 0usize;
    let mut stack = vec![(src.to_path_buf(), dst.to_path_buf())];
    while let Some((s, d)) = stack.pop() {
        let rd = fs::read_dir(&s).map_err(|e| format!("读取 {} 失败: {e}", s.display()))?;
        for e in rd.flatten() {
            let p = e.path();
            let target = d.join(e.file_name());
            if p.is_dir() {
                fs::create_dir_all(&target).map_err(|e| e.to_string())?;
                stack.push((p, target));
            } else {
                let n = fs::copy(&p, &target)
                    .map_err(|e| format!("复制 {} 失败: {e}", p.display()))?;
                copied_files += 1;
                on_progress(n);
            }
        }
    }
    Ok(copied_files)
}

/// 替换文件内容中的旧根路径（正斜杠与反斜杠两种形式都替换）
fn rewrite_file(path: &Path, old_fwd: &str, new_fwd: &str) -> bool {
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    let old_bwd = old_fwd.replace('/', "\\");
    let new_bwd = new_fwd.replace('/', "\\");
    if !text.contains(old_fwd) && !text.contains(&old_bwd) {
        return false;
    }
    let out = text.replace(old_fwd, new_fwd).replace(&old_bwd, &new_bwd);
    fs::write(path, out).is_ok()
}

/// 迁移后修正内部绝对路径引用（yaml / bat / 训练配置 / venv 的 pyvenv.cfg）
fn rewrite_references(root: &Path, old_fwd: &str, new_fwd: &str) {
    // extra_model_paths.yaml（base_path）
    let yaml = root.join("ComfyUI").join("extra_model_paths.yaml");
    if yaml.exists() {
        rewrite_file(&yaml, old_fwd, new_fwd);
    }
    // 启动脚本
    for bat in ["start_comfyui.bat", "train_zimage.bat"] {
        let p = root.join(bat);
        if p.exists() {
            rewrite_file(&p, old_fwd, new_fwd);
        }
    }
    // ai-toolkit 训练配置
    let cfg_dir = root.join("ai-toolkit").join("config");
    if let Ok(rd) = fs::read_dir(&cfg_dir) {
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_lowercase();
            if name.ends_with(".yml") || name.ends_with(".yaml") {
                rewrite_file(&p, old_fwd, new_fwd);
            }
        }
    }
    // 两个 venv 的 pyvenv.cfg（uv 托管 Python 位于数据目录内，路径需要随迁）
    for venv in ["ComfyUI/.venv/pyvenv.cfg", "ai-toolkit/.venv/pyvenv.cfg"] {
        let p = root.join(venv);
        if p.exists() {
            rewrite_file(&p, old_fwd, new_fwd);
        }
    }
}

/// 迁移核心（与 Tauri 解耦，便于测试）
pub fn migrate_core(
    old_root: &Path,
    new_root: &Path,
    on_progress: &mut dyn FnMut(MigrateProgress),
) -> Result<MigrateReport, String> {
    let old_fwd = old_root.to_string_lossy().replace('\\', "/");
    let new_fwd = new_root.to_string_lossy().replace('\\', "/");

    if old_fwd == new_fwd {
        return Err("新旧目录相同".into());
    }
    if !old_root.exists() {
        return Err(format!("原数据目录不存在：{}", old_root.display()));
    }
    if new_fwd.starts_with(&format!("{}/", old_fwd)) || old_fwd.starts_with(&format!("{}/", new_fwd))
    {
        return Err("新旧目录不能互相包含（请选择完全独立的位置）".into());
    }
    if new_root.exists() {
        let empty = fs::read_dir(new_root)
            .map(|mut rd| rd.next().is_none())
            .unwrap_or(false);
        if !empty {
            return Err(format!("目标目录已存在且非空：{}", new_root.display()));
        }
    }

    on_progress(MigrateProgress {
        phase: "scan".into(),
        message: "正在统计文件…".into(),
        ..Default::default()
    });
    let (total_bytes, total_files) = scan_dir(old_root);
    on_progress(MigrateProgress {
        phase: "scan".into(),
        total: total_bytes,
        files: total_files,
        message: format!("共 {total_files} 个文件 / {:.1} GB", total_bytes as f64 / 1073741824.0),
        ..Default::default()
    });

    if let Some(parent) = new_root.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目标父目录失败: {e}"))?;
    }

    let mode;
    let mut copied = 0u64;
    let mut files = 0usize;
    match fs::rename(old_root, new_root) {
        Ok(()) => {
            mode = "rename".to_string();
            copied = total_bytes;
            files = total_files;
        }
        Err(_) => {
            // 跨盘等情况：复制 + 删除
            mode = "copy".to_string();
            let start = Instant::now();
            let mut last = Instant::now();
            let mut acc = 0u64;
            files = copy_dir_recursive(old_root, new_root, &mut |n| {
                acc += n;
                if last.elapsed() > Duration::from_millis(200) {
                    last = Instant::now();
                    let secs = start.elapsed().as_secs_f64().max(0.001);
                    on_progress(MigrateProgress {
                        phase: "copy".into(),
                        copied: acc,
                        total: total_bytes,
                        files: total_files,
                        message: format!(
                            "复制中 {:.1}/{:.1} GB（{:.0} MB/s）",
                            acc as f64 / 1073741824.0,
                            total_bytes as f64 / 1073741824.0,
                            acc as f64 / 1048576.0 / secs
                        ),
                    });
                }
            })?;
            copied = acc;
            on_progress(MigrateProgress {
                phase: "cleanup".into(),
                copied,
                total: total_bytes,
                files,
                message: "正在清理原目录…".into(),
            });
            fs::remove_dir_all(old_root).map_err(|e| format!("删除原目录失败: {e}"))?;
        }
    }

    on_progress(MigrateProgress {
        phase: "rewrite".into(),
        copied,
        total: total_bytes,
        files,
        message: "正在修正内部路径引用…".into(),
    });
    rewrite_references(new_root, &old_fwd, &new_fwd);

    on_progress(MigrateProgress {
        phase: "done".into(),
        copied,
        total: total_bytes,
        files,
        message: "迁移完成".into(),
    });

    Ok(MigrateReport {
        mode,
        files,
        bytes: copied,
        new_root: new_fwd,
    })
}

#[tauri::command]
pub async fn migrate_data_root(
    app: AppHandle,
    state: tauri::State<'_, Mutex<crate::AppState>>,
    ctl: tauri::State<'_, std::sync::Arc<crate::setup::InstallCtl>>,
    new_root: String,
) -> Result<MigrateReport, String> {
    // 守卫：安装中 / ComfyUI 运行中 / 训练中 都拒绝
    {
        let s = ctl.snap.lock().unwrap();
        if s.running {
            return Err("资源中心正在安装，暂不能迁移".into());
        }
    }
    let old_root = {
        let mut st = state.lock().unwrap();
        crate::comfy::refresh(&mut st);
        crate::train::refresh_train(&mut st);
        if st.comfy.child.is_some() || st.comfy.state == "starting" || st.comfy.state == "ready" {
            return Err("ComfyUI 正在运行，请先停止再迁移".into());
        }
        if st.train.child.is_some() {
            return Err("LoRA 训练正在运行，请先停止再迁移".into());
        }
        let cfg: AppConfig = st.config.clone();
        cfg.root().ok_or_else(|| "尚未设置数据目录".to_string())?
    };
    let target = PathBuf::from(new_root.replace('\\', "/"));
    let old_root_c = old_root.clone();
    let app2 = app.clone();
    let report = tauri::async_runtime::spawn_blocking(move || {
        migrate_core(&old_root_c, &target, &mut |p| {
            let _ = app2.emit("migrate://progress", p);
        })
    })
    .await
    .map_err(|e| e.to_string())??;

    // 更新配置
    {
        let mut st = state.lock().unwrap();
        st.config.data_root = report.new_root.clone();
        let path = st.config_path.clone();
        crate::config::save_config_to(&path, &st.config.clone())?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_fake_root_rewrites_references() {
        let base = std::env::temp_dir().join("zimg-migrate-test");
        let _ = fs::remove_dir_all(&base);
        let old = base.join("old-root");
        // 造一个迷你数据目录
        fs::create_dir_all(old.join("ComfyUI/.venv")).unwrap();
        fs::create_dir_all(old.join("ai-toolkit/config")).unwrap();
        fs::write(old.join("ComfyUI/main.py"), "print('hi')").unwrap();
        let old_fwd = old.to_string_lossy().replace('\\', "/");
        fs::write(
            old.join("ComfyUI/extra_model_paths.yaml"),
            format!("comfyui:\n    base_path: {old_fwd}/\n"),
        )
        .unwrap();
        fs::write(
            old.join("ComfyUI/.venv/pyvenv.cfg"),
            format!("home = {}\n", old_fwd.replace('/', "\\")),
        )
        .unwrap();
        fs::write(
            old.join("ai-toolkit/config/x.yml"),
            format!("training_folder: \"{old_fwd}/outputs/training\"\n"),
        )
        .unwrap();
        fs::write(old.join("start_comfyui.bat"), format!("cd /d \"{old_fwd}/ComfyUI\"\r\n")).unwrap();

        let new = base.join("new-root");
        let report = migrate_core(&old, &new, &mut |_| {}).expect("migrate ok");
        assert!(!old.exists());
        assert!(new.join("ComfyUI/main.py").exists());

        let new_fwd = new.to_string_lossy().replace('\\', "/");
        let yaml = fs::read_to_string(new.join("ComfyUI/extra_model_paths.yaml")).unwrap();
        assert!(yaml.contains(&format!("base_path: {new_fwd}/")), "yaml rewritten: {yaml}");
        let cfg = fs::read_to_string(new.join("ComfyUI/.venv/pyvenv.cfg")).unwrap();
        assert!(cfg.contains(&new_fwd.replace('/', "\\")), "pyvenv rewritten (backslash): {cfg}");
        let yml = fs::read_to_string(new.join("ai-toolkit/config/x.yml")).unwrap();
        assert!(yml.contains(&format!("{new_fwd}/outputs/training")), "yml rewritten: {yml}");
        let bat = fs::read_to_string(new.join("start_comfyui.bat")).unwrap();
        assert!(bat.contains(&new_fwd), "bat rewritten: {bat}");
        assert!(report.files >= 5);
        let _ = fs::remove_dir_all(&base);
        println!("migrate test ok: mode={} files={}", report.mode, report.files);
    }
}
