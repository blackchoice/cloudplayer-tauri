use tauri::State;

use crate::db::DbState;

/// 获取或创建「我的喜欢」收藏歌单，返回其 ID。
#[tauri::command]
pub fn ensure_favorites_playlist(state: State<'_, DbState>) -> Result<i64, String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    // 查找已存在的收藏歌单
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM playlists WHERE is_favorites = 1 LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();
    if let Some(id) = existing {
        return Ok(id);
    }
    // 不存在则创建
    conn.execute(
        "INSERT INTO playlists (name, is_favorites) VALUES ('我的喜欢', 1)",
        [],
    )
    .map_err(|e| e.to_string())?;
    Ok(conn.last_insert_rowid())
}

/// 向收藏歌单添加一条曲目（按 source_id 去重）。
#[tauri::command]
pub fn add_to_favorites(
    state: State<'_, DbState>,
    title: String,
    artist: String,
    album: String,
    source_id: String,
    cover_url: String,
    play_url: String,
    duration_ms: i64,
) -> Result<(), String> {
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    // 获取收藏歌单 ID
    let fav_id: i64 = conn
        .query_row(
            "SELECT id FROM playlists WHERE is_favorites = 1 LIMIT 1",
            [],
            |r| r.get(0),
        )
        .map_err(|_| "收藏歌单不存在，请先调用 ensure_favorites_playlist".to_string())?;
    // 去重：如果已有相同 source_id 的条目则跳过
    let sid = source_id.trim();
    if !sid.is_empty() {
        let exists: bool = conn
            .query_row(
                "SELECT COUNT(1) FROM playlist_import_items WHERE playlist_id = ?1 AND pjmp3_source_id = ?2",
                rusqlite::params![fav_id, sid],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);
        if exists {
            return Ok(());
        }
    }
    // 获取当前最大 sort_order
    let max_order: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(sort_order), 0) FROM playlist_import_items WHERE playlist_id = ?1",
            [fav_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    conn.execute(
        "INSERT INTO playlist_import_items (playlist_id, sort_order, title, artist, album, pjmp3_source_id, cover_url, play_url, duration_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![fav_id, max_order + 1, title, artist, album, sid, cover_url, play_url, duration_ms],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 从收藏歌单移除指定 source_id 的曲目。
#[tauri::command]
pub fn remove_from_favorites(state: State<'_, DbState>, source_id: String) -> Result<(), String> {
    let sid = source_id.trim();
    if sid.is_empty() {
        return Ok(());
    }
    let conn = state.conn.lock().map_err(|e| e.to_string())?;
    let fav_id: i64 = conn
        .query_row(
            "SELECT id FROM playlists WHERE is_favorites = 1 LIMIT 1",
            [],
            |r| r.get(0),
        )
        .map_err(|_| "收藏歌单不存在".to_string())?;
    conn.execute(
        "DELETE FROM playlist_import_items WHERE playlist_id = ?1 AND pjmp3_source_id = ?2",
        rusqlite::params![fav_id, sid],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
