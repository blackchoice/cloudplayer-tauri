use tauri::{AppHandle, Manager};

/// 由主窗口调用：在原生层对「lyrics」窗设置鼠标穿透（不依赖子 Webview 的 ACL）。
#[cfg(desktop)]
#[tauri::command]
pub fn set_desktop_lyrics_click_through(app: AppHandle, ignore_cursor_events: bool) -> Result<(), String> {
    let Some(w) = app.get_webview_window("lyrics") else {
        return Ok(());
    };
    w.set_ignore_cursor_events(ignore_cursor_events)
        .map_err(|e| e.to_string())
}

#[cfg(not(desktop))]
#[tauri::command]
pub fn set_desktop_lyrics_click_through(_app: AppHandle, _ignore_cursor_events: bool) -> Result<(), String> {
    Ok(())
}

/// 关闭到托盘：隐藏主窗口。
#[cfg(desktop)]
#[tauri::command]
pub fn hide_main_window(app: AppHandle) -> Result<(), String> {
    let Some(w) = app.get_webview_window("main") else {
        return Ok(());
    };
    w.hide().map_err(|e| e.to_string())
}

#[cfg(not(desktop))]
#[tauri::command]
pub fn hide_main_window(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

/// 从托盘恢复主窗口。
#[cfg(desktop)]
#[tauri::command]
pub fn show_main_window(app: AppHandle) -> Result<(), String> {
    let Some(w) = app.get_webview_window("main") else {
        return Ok(());
    };
    w.show().map_err(|e| e.to_string())?;
    w.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(not(desktop))]
#[tauri::command]
pub fn show_main_window(_app: AppHandle) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}
