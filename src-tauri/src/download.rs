use futures_util::StreamExt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// 资源 URL 前缀解析：
///   ms:<repo>/<path>   → ModelScope（自动用 /resolve/master/ 直链，支持断点续传）
///   hf:<repo>/<path>   → hf-mirror.net
///   gh:<owner>/<repo>/...  → GitHub（经 gh-proxy.com 加速）
///   url:<完整地址>      → 原样
pub fn resolve_url(spec: &str) -> String {
    if let Some(rest) = spec.strip_prefix("ms:") {
        // ms:Owner/Repo/path/to/file → https://www.modelscope.cn/models/Owner/Repo/resolve/master/path/to/file
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
        format!("https://gh-proxy.com/https://github.com/{}", rest)
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

pub fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("Z-Image-Studio/0.2 (+local)")
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(600)) // 大文件整体限时，单块读取不受影响
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .expect("http client")
}

pub struct DownloadEvent {
    pub downloaded: u64,
    pub total: u64,
}

/// 下载到 dest（自动创建父目录、.part 断点续传）。
/// `on_progress(downloaded, total)` 返回 false 表示请求取消。
pub async fn download_file(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected: u64,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(DownloadEvent) -> bool,
) -> Result<u64, String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    let part = part_path(dest);
    let mut have: u64 = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);

    // 已有完整文件且大小符合预期 → 跳过
    if let Ok(meta) = std::fs::metadata(dest) {
        if expected > 0 && meta.len() >= expected {
            return Ok(meta.len());
        }
        if expected == 0 && meta.len() > 0 {
            return Ok(meta.len());
        }
    }

    let mut req = client.get(url);
    if have > 0 {
        req = req.header("Range", format!("bytes={have}-"));
    }
    let resp = req.send().await.map_err(|e| format!("请求失败: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("服务器返回 {}", status));
    }
    let resumed = status == reqwest::StatusCode::PARTIAL_CONTENT;
    if have > 0 && !resumed {
        // 服务器不支持续传 → 从头再来
        have = 0;
    }
    let total = if expected > 0 {
        expected
    } else {
        resp.content_length().unwrap_or(0) + have
    };

    let mut file = if have > 0 && resumed {
        OpenOptions::new()
            .append(true)
            .open(&part)
            .map_err(|e| e.to_string())?
    } else {
        File::create(&part).map_err(|e| e.to_string())?
    };

    let mut downloaded = have;
    let mut last_emit = Instant::now();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Err("已取消".into());
        }
        let chunk = chunk.map_err(|e| format!("网络中断: {e}"))?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if last_emit.elapsed() > Duration::from_millis(180) {
            last_emit = Instant::now();
            if !on_progress(DownloadEvent { downloaded, total }) {
                return Err("已取消".into());
            }
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    if !on_progress(DownloadEvent { downloaded, total }) {
        return Err("已取消".into());
    }
    std::fs::rename(&part, dest).map_err(|e| format!("重命名失败: {e}"))?;
    Ok(downloaded)
}

fn part_path(dest: &Path) -> PathBuf {
    let mut p = dest.as_os_str().to_os_string();
    p.push(".part");
    PathBuf::from(p)
}

/// 检测文件是否就绪（大小足够）
pub fn file_ready(path: &Path, min_size: u64) -> bool {
    match std::fs::metadata(path) {
        Ok(m) => m.is_file() && (min_size == 0 || m.len() >= min_size),
        Err(_) => false,
    }
}
