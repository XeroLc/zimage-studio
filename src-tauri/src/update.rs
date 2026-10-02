//! 自研更新检查/安装（替代插件内置检查，规避 gh-proxy/raw 的 CDN 缓存滞后）：
//! - 检查：请求 latest.json 时附加时间戳查询参数（缓存键随每次请求变化 → 永远拿到最新）
//! - 下载：复用下载引擎（进度事件 update://progress）
//! - 校验：minisign 签名（与 tauri.conf 中内置的公钥一致）
//! - 安装：以 NSIS 模板的 /UPDATE 静默模式拉起安装程序，随后退出本进程让安装器接管

use crate::config::app_data_dir;
use crate::download::{build_client, download_file};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

/// 与 tauri.conf.json plugins.updater.pubkey 保持一致
const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDI4MDQxRTZERjgxOEYwNDEKUldSQjhCajRiUjRFS1BtV3FkdC9HT1ZWL0hwUU9FSDZnYS9yYWZVbzJlQ0oxYkt2eHFmeGJKeEsK";

const ENDPOINTS: [&str; 3] = [
    "https://gh-proxy.com/https://raw.githubusercontent.com/XeroLc/zimage-studio/main/latest.json",
    "https://gh-proxy.com/https://github.com/XeroLc/zimage-studio/raw/main/latest.json",
    "https://raw.githubusercontent.com/XeroLc/zimage-studio/main/latest.json",
];

const TARGET: &str = "windows-x86_64";

#[derive(Serialize, Clone, Default)]
pub struct RemoteUpdate {
    pub available: bool,
    pub version: String,
    pub notes: String,
    pub url: String,
    pub signature: String,
    pub source: String,
}

#[derive(Serialize, Clone, Default)]
pub struct UpdateProgress {
    pub phase: String, // download | verify | launch
    pub downloaded: u64,
    pub total: u64,
    pub speed: f64,
    pub message: String,
}

#[derive(Deserialize)]
struct Manifest {
    version: String,
    #[serde(default)]
    notes: String,
    platforms: HashMap<String, PlatformEntry>,
}

#[derive(Deserialize, Clone)]
struct PlatformEntry {
    signature: String,
    url: String,
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 依次尝试各端点；每个都附加时间戳参数以绕过 CDN 缓存
async fn fetch_latest(client: &reqwest::Client) -> Result<(Manifest, PlatformEntry, String), String> {
    let mut last_err = String::from("无可用更新源");
    for ep in ENDPOINTS {
        let sep = if ep.contains('?') { '&' } else { '?' };
        let url = format!("{ep}{sep}t={}", now_ts());
        let resp = match client
            .get(&url)
            .header("Cache-Control", "no-cache")
            .timeout(Duration::from_secs(15))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                last_err = format!("更新源返回 {}", r.status());
                continue;
            }
            Err(e) => {
                last_err = format!("更新源不可达：{e}");
                continue;
            }
        };
        let text = match resp.text().await {
            Ok(t) => t,
            Err(e) => {
                last_err = e.to_string();
                continue;
            }
        };
        match serde_json::from_str::<Manifest>(&text) {
            Ok(m) => {
                if let Some(p) = m.platforms.get(TARGET).cloned() {
                    return Ok((m, p, ep.to_string()));
                }
                last_err = "清单缺少当前平台条目".into();
            }
            Err(e) => {
                last_err = format!("清单解析失败：{e}");
            }
        }
    }
    Err(last_err)
}

#[tauri::command]
pub async fn check_update_remote(app: AppHandle) -> Result<RemoteUpdate, String> {
    let client = build_client();
    let (m, p, source) = fetch_latest(&client).await?;
    let current = app.package_info().version.clone();
    let newer = match (
        semver::Version::parse(&m.version),
        semver::Version::parse(&current.to_string()),
    ) {
        (Ok(remote), Ok(cur)) => remote > cur,
        _ => m.version != current.to_string(),
    };
    Ok(RemoteUpdate {
        available: newer,
        version: m.version,
        notes: m.notes,
        url: p.url,
        signature: p.signature,
        source,
    })
}

fn verify_signature(
    data: &[u8],
    release_signature: &str,
    pub_key: &str,
) -> Result<(), String> {
    use base64::Engine;
    let dec = |s: &str| -> Result<String, String> {
        base64::engine::general_purpose::STANDARD
            .decode(s.trim())
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .map_err(|e| format!("签名/公钥解码失败：{e}"))
    };
    let pk_text = dec(pub_key)?;
    let public_key = minisign_verify::PublicKey::decode(&pk_text)
        .map_err(|e| format!("公钥格式错误：{e}"))?;
    let sig_text = dec(release_signature)?;
    let signature =
        minisign_verify::Signature::decode(&sig_text).map_err(|e| format!("签名格式错误：{e}"))?;
    public_key
        .verify(data, &signature, true)
        .map_err(|e| format!("更新包签名校验失败（已拒绝安装）：{e}"))
}

/// NSIS 参数转义（包裹引号并转义 $ 与 "）
fn escape_nsis_arg(a: &str) -> String {
    let mut s = String::from("\"");
    for c in a.chars() {
        match c {
            '"' => s.push_str("$\\\""),
            '$' => s.push_str("$$"),
            _ => s.push(c),
        }
    }
    s.push('"');
    s
}

#[tauri::command]
pub async fn download_install_update(
    app: AppHandle,
    url: String,
    signature: String,
    version: String,
) -> Result<(), String> {
    let emit = |p: UpdateProgress| {
        let _ = app.emit("update://progress", p);
    };
    let dir = app_data_dir().join("updates");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest: PathBuf = dir.join(format!("Z-Image-Studio-{version}-setup.exe"));

    emit(UpdateProgress {
        phase: "download".into(),
        message: "正在下载更新包…".into(),
        ..Default::default()
    });
    let client = build_client();
    let cancel = AtomicBool::new(false);
    let app2 = app.clone();
    let start = Instant::now();
    let mut last_emit = Instant::now();
    download_file(&client, &url, &dest, 0, &cancel, move |ev| {
        if last_emit.elapsed() > Duration::from_millis(200) {
            last_emit = Instant::now();
            let secs = start.elapsed().as_secs_f64().max(0.001);
            let _ = app2.emit(
                "update://progress",
                UpdateProgress {
                    phase: "download".into(),
                    downloaded: ev.downloaded,
                    total: ev.total,
                    speed: ev.downloaded as f64 / secs,
                    message: "正在下载更新包…".into(),
                },
            );
        }
        true
    })
    .await
    .map_err(|e| format!("下载更新包失败：{e}"))?;

    emit(UpdateProgress {
        phase: "verify".into(),
        message: "正在校验更新包签名…".into(),
        ..Default::default()
    });
    let bytes = std::fs::read(&dest).map_err(|e| e.to_string())?;
    verify_signature(&bytes, &signature, PUBKEY)?;

    emit(UpdateProgress {
        phase: "launch".into(),
        message: "签名校验通过，正在启动安装程序…".into(),
        ..Default::default()
    });
    // 镜像官方插件的 NSIS 调用：/UPDATE（静默更新模式）+ /ARGS（更新完成后带原参数重启）
    let current_args: Vec<String> = std::env::args().skip(1).collect();
    let mut params = String::from("/UPDATE /ARGS");
    for a in &current_args {
        params.push(' ');
        params.push_str(&escape_nsis_arg(a));
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new(&dest)
            .raw_arg(params)
            .spawn()
            .map_err(|e| format!("启动安装程序失败：{e}"))?;
    }
    #[cfg(not(windows))]
    {
        let _ = params;
        return Err("当前平台不支持自动安装".into());
    }
    // 稍候让安装器完成初始化，然后退出本进程（安装器接管文件替换与重启）
    std::thread::sleep(Duration::from_millis(800));
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真实网络端到端：拉取 latest.json → 下载更新包 → 校验签名
    #[test]
    fn fetch_download_and_verify() {
        tauri::async_runtime::block_on(async {
            let client = build_client();
            let (m, p, src) = fetch_latest(&client).await.expect("fetch latest.json");
            println!("remote version: {} (via {})", m.version, src);
            let dir = std::env::temp_dir().join("zimg-update-test");
            std::fs::create_dir_all(&dir).unwrap();
            let dest = dir.join("setup-test.exe");
            let _ = std::fs::remove_file(&dest);
            let _ = std::fs::remove_file(format!("{}.part", dest.display()));
            let cancel = std::sync::atomic::AtomicBool::new(false);
            download_file(&client, &p.url, &dest, 0, &cancel, |_| true)
                .await
                .expect("download installer");
            let bytes = std::fs::read(&dest).unwrap();
            println!("downloaded {} bytes", bytes.len());
            verify_signature(&bytes, &p.signature, PUBKEY).expect("signature must verify");
            println!("signature OK");
            // 篡改一个字节必须校验失败
            let mut bad = bytes.clone();
            bad[0] ^= 0xFF;
            assert!(verify_signature(&bad, &p.signature, PUBKEY).is_err());
            println!("tamper detection OK");
            let _ = std::fs::remove_file(&dest);
        });
    }
}
