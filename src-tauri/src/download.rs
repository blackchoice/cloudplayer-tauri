//! 与 Python `services/download_service.py` 对齐：顺序队列、captcha 链、流式落盘。

use std::path::PathBuf;
use std::time::Duration;

use rand::Rng;
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;

use crate::captcha_slider::guess_slider_offset;
use crate::config::{default_download_dir, Settings, BASE_URL};

const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

#[derive(Debug, Clone)]
pub struct DownloadJob {
    pub source_id: String,
    pub title: String,
    pub artist: String,
    pub quality: String,
}

#[derive(Clone, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTaskEvent {
    pub source_id: String,
    pub title: String,
    pub artist: String,
    pub quality: String,
    pub status: String,
    pub progress: f64,
    pub message: Option<String>,
}

fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
}

fn find_long_base64_strings(v: &Value, min_len: usize, out: &mut Vec<String>) {
    match v {
        Value::Object(map) => {
            for x in map.values() {
                find_long_base64_strings(x, min_len, out);
            }
        }
        Value::Array(a) => {
            for x in a {
                find_long_base64_strings(x, min_len, out);
            }
        }
        Value::String(s) => {
            let t = s.trim();
            if t.len() >= min_len && t.chars().take(200).all(|c| c.is_ascii_alphanumeric() || "+/=\n\r ".contains(c)) {
                out.push(s.clone());
            }
        }
        _ => {}
    }
}

fn extract_captcha_id(v: &Value) -> Option<String> {
    match v {
        Value::Object(map) => {
            for k in ["captchaId", "captcha_id", "token", "id", "uuid", "cid"] {
                if let Some(Value::String(s)) = map.get(k) {
                    if s.len() > 8 {
                        return Some(s.clone());
                    }
                }
            }
            for x in map.values() {
                if let Some(r) = extract_captcha_id(x) {
                    return Some(r);
                }
            }
        }
        Value::Array(a) => {
            for x in a {
                if let Some(r) = extract_captcha_id(x) {
                    return Some(r);
                }
            }
        }
        _ => {}
    }
    None
}

fn pick_two_images(blobs: &[String]) -> Option<(String, String)> {
    if blobs.len() >= 2 {
        return Some((blobs[0].clone(), blobs[1].clone()));
    }
    if blobs.len() == 1 {
        return Some((blobs[0].clone(), blobs[0].clone()));
    }
    None
}

fn dest_root() -> PathBuf {
    let s = Settings::load();
    let p = s.download_folder.trim();
    if p.is_empty() {
        default_download_dir()
    } else {
        PathBuf::from(p)
    }
}

/// 与 `run_one_job` 落盘规则一致：`{title} - {artist}.mp3` / `.flac`，用于播放时优先走已下载文件。
pub fn candidate_downloaded_audio_paths(title: &str, artist: &str) -> Vec<PathBuf> {
    let root = dest_root();
    let name_mp3 = sanitize_filename(&format!("{} - {}.mp3", title.trim(), artist.trim()));
    let name_flac = sanitize_filename(&format!("{} - {}.flac", title.trim(), artist.trim()));
    vec![root.join(name_mp3), root.join(name_flac)]
}

fn check_and_reserve_download_slot() -> Result<(), String> {
    let mut s = Settings::load();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    if s.downloads_today_date != today {
        s.downloads_today_date = today;
        s.downloads_today_count = 0;
    }
    if s.daily_download_limit > 0 && s.downloads_today_count >= s.daily_download_limit {
        return Err(format!(
            "已达到当日下载上限（{} 次）",
            s.daily_download_limit
        ));
    }
    Ok(())
}

fn record_download_success() -> Result<(), String> {
    let mut s = Settings::load();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    if s.downloads_today_date != today {
        s.downloads_today_date = today;
        s.downloads_today_count = 0;
    }
    s.downloads_today_count += 1;
    s.save()
}

pub async fn run_one_job(client: Client, app: AppHandle, job: DownloadJob) {
    let emit = |task: DownloadTaskEvent| {
        let _ = app.emit("download-task-changed", &task);
    };

    let mut task = DownloadTaskEvent {
        source_id: job.source_id.clone(),
        title: job.title.clone(),
        artist: job.artist.clone(),
        quality: job.quality.clone(),
        status: "queued".to_string(),
        progress: 0.0,
        message: None,
    };

    if let Err(e) = check_and_reserve_download_slot() {
        task.status = "failed".to_string();
        task.message = Some(e.clone());
        emit(task);
        return;
    }

    task.status = "downloading".to_string();
    emit(task.clone());

    let base = BASE_URL.trim_end_matches('/');
    let song_page = format!("{}/song.php?id={}", base, job.source_id);

    // 1) captcha/gen
    let gen_r = match client
        .get(format!("{}/captcha/gen", base))
        .header("User-Agent", BROWSER_UA)
        .header("Referer", format!("{}/", base))
        .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            task.status = "failed".to_string();
            task.message = Some(format!("captcha/gen: {e}"));
            emit(task);
            return;
        }
    };
    if !gen_r.status().is_success() {
        task.status = "failed".to_string();
        task.message = Some(format!("captcha/gen HTTP {}", gen_r.status()));
        emit(task);
        return;
    }
    let gen_text = match gen_r.text().await {
        Ok(t) => t,
        Err(e) => {
            task.status = "failed".to_string();
            task.message = Some(e.to_string());
            emit(task);
            return;
        }
    };
    let payload: Value = match serde_json::from_str(&gen_text) {
        Ok(v) => v,
        Err(e) => {
            task.status = "failed".to_string();
            task.message = Some(format!("验证码响应非 JSON: {e}"));
            emit(task);
            return;
        }
    };
    let mut blobs = Vec::new();
    find_long_base64_strings(&payload, 500, &mut blobs);
    let Some((bg_b64, sl_b64)) = pick_two_images(&blobs) else {
        task.status = "failed".to_string();
        task.message = Some("无法解析验证码图片".to_string());
        emit(task);
        return;
    };
    let Some(captcha_id) = extract_captcha_id(&payload) else {
        task.status = "failed".to_string();
        task.message = Some("无法解析 captchaId".to_string());
        emit(task);
        return;
    };

    let x = match guess_slider_offset(&bg_b64, &sl_b64) {
        Some(v) => v,
        None => {
            task.status = "failed".to_string();
            task.message = Some("自动滑块匹配失败".to_string());
            emit(task);
            return;
        }
    };

    let chk = match client
        .get(format!("{}/captcha/check", base))
        .query(&[("id", captcha_id.as_str()), ("x", &x.to_string())])
        .header("User-Agent", BROWSER_UA)
        .header("Referer", song_page.as_str())
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            task.status = "failed".to_string();
            task.message = Some(format!("captcha/check: {e}"));
            emit(task);
            return;
        }
    };
    if !chk.status().is_success() {
        task.status = "failed".to_string();
        task.message = Some(format!("captcha/check HTTP {}", chk.status()));
        emit(task);
        return;
    }

    let jitter = {
        let mut rng = rand::thread_rng();
        rng.gen_range(0.5f64..2.0f64)
    };
    tokio::time::sleep(Duration::from_secs_f64(2.0 + jitter)).await;

    let url_r = match client
        .get(format!("{}/captcha/check/getMusicUrl", base))
        .query(&[
            ("captchaId", captcha_id.as_str()),
            ("id", job.source_id.as_str()),
            ("br", job.quality.as_str()),
        ])
        .header("User-Agent", BROWSER_UA)
        .header("Referer", song_page.as_str())
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            task.status = "failed".to_string();
            task.message = Some(format!("getMusicUrl: {e}"));
            emit(task);
            return;
        }
    };
    if !url_r.status().is_success() {
        task.status = "failed".to_string();
        task.message = Some(format!("getMusicUrl HTTP {}", url_r.status()));
        emit(task);
        return;
    }
    let url_json: Value = match url_r.json::<Value>().await {
        Ok(v) => v,
        Err(e) => {
            task.status = "failed".to_string();
            task.message = Some(format!("解析 getMusicUrl JSON: {e}"));
            emit(task);
            return;
        }
    };
    if url_json.get("code").and_then(|c| c.as_i64()) != Some(200) {
        task.status = "failed".to_string();
        task.message = Some(format!("获取下载链接失败: {url_json}"));
        emit(task);
        return;
    }
    let Some(music_url) = url_json.get("result").and_then(|r| r.as_str()) else {
        task.status = "failed".to_string();
        task.message = Some("响应无 result URL".to_string());
        emit(task);
        return;
    };

    let ext = if job.quality == "flac" { ".flac" } else { ".mp3" };
    let name = sanitize_filename(&format!("{} - {}{}", job.title, job.artist, ext));
    let root = dest_root();
    if let Err(e) = tokio::fs::create_dir_all(&root).await {
        task.status = "failed".to_string();
        task.message = Some(format!("创建目录: {e}"));
        emit(task);
        return;
    }
    let out_path = root.join(name);

    let resp = match client
        .get(music_url)
        .header("User-Agent", BROWSER_UA)
        .header("Referer", song_page.as_str())
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            task.status = "failed".to_string();
            task.message = Some(format!("下载音频: {e}"));
            emit(task);
            return;
        }
    };
    if !resp.status().is_success() {
        task.status = "failed".to_string();
        task.message = Some(format!("音频 HTTP {}", resp.status()));
        emit(task);
        return;
    }
    let total = resp.content_length().unwrap_or(0);
    let body = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            task.status = "failed".to_string();
            task.message = Some(format!("读取音频: {e}"));
            emit(task);
            return;
        }
    };
    let mut file = match tokio::fs::File::create(&out_path).await {
        Ok(f) => f,
        Err(e) => {
            task.status = "failed".to_string();
            task.message = Some(format!("创建文件: {e}"));
            emit(task);
            return;
        }
    };
    if let Err(e) = file.write_all(&body).await {
        task.status = "failed".to_string();
        task.message = Some(format!("写入: {e}"));
        emit(task);
        return;
    }
    let done = body.len() as u64;
    if total > 0 {
        task.progress = (done as f64) / (total as f64);
    } else {
        task.progress = 0.99;
    }
    emit(task.clone());
    if let Err(e) = file.flush().await {
        task.status = "failed".to_string();
        task.message = Some(e.to_string());
        emit(task);
        return;
    }

    if let Err(e) = record_download_success() {
        task.message = Some(format!("已保存但计数失败: {e}"));
    } else {
        task.message = Some(format!("已保存: {}", out_path.display()));
    }
    task.status = "completed".to_string();
    task.progress = 1.0;
    emit(task);
}
