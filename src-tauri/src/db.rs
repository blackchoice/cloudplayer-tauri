//! 与 Python `core/database.py` 中 `library.db` 初始化对齐（同一路径：`~/.cloudplayer/library.db`）。

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;

use crate::config::config_dir;

pub struct DbState {
    pub conn: Mutex<Connection>,
}

pub fn db_path() -> PathBuf {
    config_dir().join("library.db")
}

pub fn open_and_init() -> Result<Connection, rusqlite::Error> {
    let path = db_path();
    let conn = Connection::open(&path)?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode=WAL;
        PRAGMA foreign_keys=ON;

        CREATE TABLE IF NOT EXISTS songs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL DEFAULT '',
            artist TEXT NOT NULL DEFAULT '',
            album TEXT NOT NULL DEFAULT '',
            file_path TEXT NOT NULL UNIQUE,
            cover TEXT,
            source_id TEXT,
            quality TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_songs_source ON songs(source_id);
        CREATE INDEX IF NOT EXISTS idx_songs_title_artist ON songs(title, artist);

        CREATE TABLE IF NOT EXISTS playlists (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS playlist_songs (
            playlist_id INTEGER NOT NULL,
            song_id INTEGER NOT NULL,
            position INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (playlist_id, song_id),
            FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE,
            FOREIGN KEY (song_id) REFERENCES songs(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_ps_playlist ON playlist_songs(playlist_id);

        CREATE TABLE IF NOT EXISTS playlist_import_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            playlist_id INTEGER NOT NULL,
            sort_order INTEGER NOT NULL DEFAULT 0,
            title TEXT NOT NULL DEFAULT '',
            artist TEXT NOT NULL DEFAULT '',
            album TEXT NOT NULL DEFAULT '',
            play_url TEXT NOT NULL DEFAULT '',
            pjmp3_source_id TEXT NOT NULL DEFAULT '',
            cover_url TEXT NOT NULL DEFAULT '',
            cover_cache_path TEXT NOT NULL DEFAULT '',
            duration_ms INTEGER NOT NULL DEFAULT 0,
            audio_cache_path TEXT NOT NULL DEFAULT '',
            FOREIGN KEY (playlist_id) REFERENCES playlists(id) ON DELETE CASCADE
        );
        CREATE INDEX IF NOT EXISTS idx_pii_playlist ON playlist_import_items(playlist_id);

        CREATE TABLE IF NOT EXISTS liked_tracks (
            key TEXT PRIMARY KEY NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            artist TEXT NOT NULL DEFAULT '',
            album TEXT NOT NULL DEFAULT '',
            pjmp3_source_id TEXT NOT NULL DEFAULT ''
        );

        CREATE TABLE IF NOT EXISTS recent_plays (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kind TEXT NOT NULL,
            title TEXT NOT NULL DEFAULT '',
            artist TEXT NOT NULL DEFAULT '',
            cover_url TEXT,
            pjmp3_source_id TEXT,
            file_path TEXT,
            played_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_recent_played_at ON recent_plays(played_at DESC);
        "#,
    )?;

    // 与 Python `_migrate_schema` 一致：忽略已存在列的错误
    for stmt in [
        "ALTER TABLE playlist_import_items ADD COLUMN album TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE playlist_import_items ADD COLUMN play_url TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE playlist_import_items ADD COLUMN pjmp3_source_id TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE playlist_import_items ADD COLUMN cover_url TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE playlist_import_items ADD COLUMN cover_cache_path TEXT NOT NULL DEFAULT ''",
        "ALTER TABLE playlist_import_items ADD COLUMN duration_ms INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE playlist_import_items ADD COLUMN audio_cache_path TEXT NOT NULL DEFAULT ''",
    ] {
        let _ = conn.execute(stmt, []);
    }

    Ok(conn)
}
