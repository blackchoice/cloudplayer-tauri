use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use tauri::State;

use crate::db::DbState;

use super::AppState;

/// 「下载歌曲」Tab：库中已记录的本地下载文件（含重启后持久化）。
/// 返回前先清理磁盘上已不存在的记录，与资源管理器手动删文件后的列表一致。
#[tauri::command]
pub fn list_downloaded_songs(state: State<'_, DbState>) -> Result<Vec<crate::db::DownloadedSongRow>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let _ = crate::db::prune_downloaded_tracks_missing_files(&conn);
    crate::db::list_downloaded_tracks(&conn).map_err(|e| e.to_string())
}

/// 删除已下载音频文件，并移除库中对应记录。
#[tauri::command]
pub fn delete_downloaded_song(state: State<'_, DbState>, file_path: String) -> Result<(), String> {
    let fp = file_path.trim();
    if fp.is_empty() {
        return Err("路径为空".to_string());
    }
    let p = PathBuf::from(fp);
    if p.is_file() {
        std::fs::remove_file(&p).map_err(|e| format!("删除文件失败: {}", e))?;
    }
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let n = crate::db::delete_downloaded_track_by_path(&conn, fp).map_err(|e| e.to_string())?;
    if n == 0 {
        return Err("库中无此下载记录".to_string());
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct DownloadEnqueueIn {
    pub source_id: String,
    pub title: String,
    pub artist: String,
    pub quality: String,
}

#[tauri::command]
pub async fn enqueue_download(state: State<'_, Arc<AppState>>, job: DownloadEnqueueIn) -> Result<(), String> {
    let q = job.quality.trim().to_ascii_lowercase();
    let quality = match q.as_str() {
        "flac" => "flac",
        "320" | "hq" => "320",
        _ => "128",
    };
    let j = crate::download::DownloadJob {
        source_id: job.source_id.trim().to_string(),
        title: job.title,
        artist: job.artist,
        quality: quality.to_string(),
    };
    state
        .download_tx
        .send(j)
        .await
        .map_err(|e| format!("下载队列异常: {e}"))
}
