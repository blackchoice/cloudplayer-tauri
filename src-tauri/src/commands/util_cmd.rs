use std::sync::Arc;

use log::{info, warn};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::db::DbState;

use super::AppState;

#[tauri::command]
pub fn db_status(state: State<'_, DbState>) -> Result<String, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let n_playlists: i64 = conn
        .query_row("SELECT COUNT(*) FROM playlists", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let n_songs: i64 = conn
        .query_row("SELECT COUNT(*) FROM songs", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    Ok(format!(
        "library.db OK — playlists: {}, local songs: {}",
        n_playlists, n_songs
    ))
}

/// 返回 `cloudplayer.log` 的绝对路径（与 `logging::init_from_app` 一致），便于 Android root / 排障对照。
#[tauri::command]
pub fn get_app_log_path(app: AppHandle) -> Result<String, String> {
    let log_dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    Ok(log_dir.join("cloudplayer.log").to_string_lossy().to_string())
}

/// 供移动端整文件读入后 `Blob` 播放（规避 Android 上 `convertFileSrc` 只缓冲开头、约 30s 停播，见 tauri#14776）。
#[tauri::command]
pub fn read_file_bytes(path: String) -> Result<Vec<u8>, String> {
    let p = path.trim();
    if p.is_empty() {
        return Err("空路径".to_string());
    }
    if p.contains("..") {
        return Err("非法路径".to_string());
    }
    let meta = std::fs::metadata(p).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("不是文件".to_string());
    }
    const MAX: u64 = 80 * 1024 * 1024;
    if meta.len() > MAX {
        return Err("文件过大".to_string());
    }
    std::fs::read(p).map_err(|e| e.to_string())
}

/// 播放前校验本地路径是否仍为可读文件（避免静默失败）。
#[tauri::command]
pub fn local_path_accessible(path: String) -> bool {
    let p = std::path::Path::new(path.trim());
    p.is_file()
}

/// 前端 `<audio>` 事件回写到 `cloudplayer.log`，与 Rust 侧 `pj-play` 日志对照排障。
#[tauri::command]
pub fn log_play_event(
    stage: String,
    url: Option<String>,
    error_code: Option<i32>,
    message: Option<String>,
    extra: Option<String>,
) -> Result<(), String> {
    let url_s = url.as_deref().map(log_url_160).unwrap_or_else(|| "-".to_string());
    let msg = message.as_deref().unwrap_or("-");
    let ex = extra.as_deref().unwrap_or("");
    let st = stage.trim();
    if st.contains("error") || st.ends_with("_err") {
        warn!(
            target: "pj-play",
            "webview stage={} url={} code={:?} msg={} extra={}",
            st,
            url_s,
            error_code,
            msg,
            ex
        );
    } else {
        info!(
            target: "pj-play",
            "webview stage={} url={} code={:?} msg={} extra={}",
            st,
            url_s,
            error_code,
            msg,
            ex
        );
    }
    Ok(())
}

fn log_url_160(s: &str) -> String {
    let t: String = s.chars().take(160).collect();
    if s.len() > 160 {
        format!("{t}…")
    } else {
        t
    }
}

#[tauri::command]
pub fn parse_import_text(
    text: String,
    fmt: String,
) -> Result<Vec<crate::import_playlist::ImportedTrackDto>, String> {
    crate::import_playlist::parse_playlist_text(&text, fmt.trim())
}

#[derive(Serialize)]
pub struct SharePlaylistResponse {
    pub playlist_name: String,
    pub tracks: Vec<crate::import_playlist::ImportedTrackDto>,
}

/// 网易云 / QQ 音乐分享链接 → 歌单名 + 曲目（与 Py `share_link_importer.fetch_playlist_from_share_url` 一致）。
#[tauri::command]
pub async fn fetch_share_playlist(
    state: State<'_, Arc<AppState>>,
    url: String,
) -> Result<SharePlaylistResponse, String> {
    let u = url.trim();
    if u.is_empty() {
        return Err("请先粘贴分享链接。".to_string());
    }
    state.limiter.acquire_slot().await;
    let (playlist_name, tracks) = crate::share_link::fetch_playlist_from_share_url(&state.client, u).await?;
    Ok(SharePlaylistResponse {
        playlist_name,
        tracks,
    })
}
