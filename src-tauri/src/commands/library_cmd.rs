use std::path::PathBuf;

use serde::Serialize;
use tauri::State;
use walkdir::WalkDir;

use crate::db::DbState;

/// 本地库表中的一行（与 `songs` 表对应）。
#[derive(Serialize)]
pub struct LocalSongRow {
    pub id: i64,
    pub title: String,
    pub artist: String,
    pub file_path: String,
}

#[tauri::command]
pub fn list_local_songs(state: State<'_, DbState>) -> Result<Vec<LocalSongRow>, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare(
            "SELECT id, title, artist, file_path FROM songs ORDER BY title COLLATE NOCASE, id ASC",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok(LocalSongRow {
                id: r.get(0)?,
                title: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                artist: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                file_path: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
            })
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| e.to_string())?);
    }
    Ok(out)
}

#[derive(Serialize)]
pub struct ScanMusicFolderResult {
    /// 遍历到的音频文件数（去重前按扩展名计数）
    pub audio_files_seen: usize,
    /// INSERT/UPDATE 实际写入库的行数
    pub rows_written: usize,
}

fn is_audio_extension(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "mp3" | "flac" | "m4a" | "wav" | "ogg" | "aac" | "opus" | "wma"
    )
}

/// 递归扫描文件夹，将音频文件写入 `songs` 表（路径唯一，冲突则更新标题）。
#[tauri::command]
pub fn scan_music_folder(state: State<'_, DbState>, path: String) -> Result<ScanMusicFolderResult, String> {
    let root = PathBuf::from(path.trim());
    if !root.is_dir() {
        return Err("不是有效的文件夹路径".to_string());
    }
    let mut audio_files_seen = 0usize;
    let mut rows_written = 0usize;
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    for entry in WalkDir::new(&root).follow_links(false).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
        if !is_audio_extension(ext) {
            continue;
        }
        audio_files_seen += 1;
        let fp = p.to_string_lossy().to_string();
        let stem = p
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let title = if stem.is_empty() { fp.clone() } else { stem };
        let n = conn
            .execute(
                r#"INSERT INTO songs (title, artist, album, file_path)
                   VALUES (?1, '', '', ?2)
                   ON CONFLICT(file_path) DO UPDATE SET title = excluded.title"#,
                rusqlite::params![title, fp],
            )
            .map_err(|e| e.to_string())?;
        rows_written += n as usize;
    }
    Ok(ScanMusicFolderResult {
        audio_files_seen,
        rows_written,
    })
}
