mod comfy;
mod config;
mod download;
mod setup;
mod train;
mod update;

use config::{load_or_migrate, AppConfig};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, WindowEvent};

pub struct AppState {
    pub config: AppConfig,
    pub config_path: PathBuf,
    pub comfy: comfy::ComfyState,
    pub train: train::TrainState,
    pub autostart: bool,
    pub initial_tab: String,
    pub autotrain: bool,
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// 无边框窗口启用 Win11 原生圆角（不透明窗口的硬件合成路径，比透明窗口流畅）
#[cfg(windows)]
fn apply_round_corners(window: &tauri::WebviewWindow) {
    use std::ffi::c_void;
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: *mut c_void,
            attr: u32,
            value: *const c_void,
            size: u32,
        ) -> i32;
    }
    if let Ok(h) = window.hwnd() {
        let hwnd = h.0 as isize as *mut c_void;
        let pref: u32 = 2; // DWMWCP_ROUND
        unsafe {
            DwmSetWindowAttribute(hwnd, 33, &pref as *const u32 as *const c_void, 4);
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (cfg, cpath) = load_or_migrate();
    let args: Vec<String> = std::env::args().collect();
    let autostart = args.iter().any(|a| a == "--autostart");
    let autotrain = args.iter().any(|a| a == "--autotrain");
    let initial_tab = args
        .iter()
        .position(|a| a == "--tab")
        .and_then(|i| args.get(i + 1).cloned())
        .unwrap_or_else(|| "image".into());

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(Mutex::new(AppState {
            config: cfg,
            config_path: cpath,
            comfy: comfy::ComfyState::new(),
            train: train::TrainState::new(),
            autostart,
            initial_tab,
            autotrain,
        }))
        .manage(Arc::new(setup::InstallCtl::default()))
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            // Win11 原生圆角（配合不透明窗口，保证硬件合成性能）
            #[cfg(windows)]
            if let Some(win) = app.get_webview_window("main") {
                apply_round_corners(&win);
            }

            // ---------- 系统托盘 ----------
            let show = MenuItem::with_id(app, "tray_show", "显示主界面", true, None::<&str>)?;
            let start = MenuItem::with_id(app, "tray_start", "启动 ComfyUI", true, None::<&str>)?;
            let stop = MenuItem::with_id(app, "tray_stop", "停止 ComfyUI", true, None::<&str>)?;
            let ws = MenuItem::with_id(app, "tray_workspace", "进入工作区", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "tray_quit", "退出", true, None::<&str>)?;
            let sep1 = PredefinedMenuItem::separator(app)?;
            let sep2 = PredefinedMenuItem::separator(app)?;
            let menu = Menu::with_items(app, &[&show, &sep1, &start, &stop, &ws, &sep2, &quit])?;
            let mut tray = TrayIconBuilder::with_id("main-tray")
                .tooltip("Z-Image Studio")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, ev| match ev.id().as_ref() {
                    "tray_show" => show_main(app),
                    "tray_start" => {
                        let st = app.state::<Mutex<AppState>>();
                        if let Err(e) = comfy::start_comfy(st) {
                            let _ = app.emit("app://notice", e);
                        }
                    }
                    "tray_stop" => {
                        let st = app.state::<Mutex<AppState>>();
                        let _ = comfy::stop_comfy(st);
                    }
                    "tray_workspace" => {
                        show_main(app);
                        let _ = app.emit("app://workspace", ());
                    }
                    "tray_quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            // 主窗口点关闭 → 收进托盘（ComfyUI 继续跑）；用托盘菜单「退出」真正退出
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            // comfy / general
            comfy::get_config,
            comfy::save_config,
            comfy::get_status,
            comfy::get_log,
            comfy::start_comfy,
            comfy::stop_comfy,
            comfy::open_browser,
            comfy::open_comfy_window,
            comfy::enter_workspace,
            comfy::exit_workspace,
            comfy::comfy_api,
            comfy::comfy_url,
            comfy::sys_stats,
            comfy::list_outputs,
            comfy::output_meta,
            comfy::delete_output,
            comfy::open_path,
            // train
            train::get_train_info,
            train::list_train_configs,
            train::get_train_log,
            train::start_training,
            train::stop_training,
            // setup / migration
            setup::get_setup_info,
            setup::get_install_state,
            setup::start_install,
            setup::cancel_install,
            setup::get_quickgen_template,
            setup::sync_manifest,
            setup::test_sources,
            // 更新（自研：绕缓存检查 + 签名校验 + 静默安装）
            update::check_update_remote,
            update::download_install_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while building tauri application");
}
