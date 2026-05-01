/**
 * 媒体通知桥接模块：将播放状态同步到 Android 通知栏 MediaSession。
 * 内部 guard：仅在 Android WebView（CpMediaBridge 可用）时执行。
 */

/**
 * 更新通知栏媒体信息（标题、歌手、封面、时长）。
 * 同时启动前台服务 + 显示通知。
 */
export function mediaNotifUpdate({ title, artist, coverUrl, durationMs }) {
  if (!window.CpMediaBridge) return;
  try {
    window.CpMediaBridge.update(
      JSON.stringify({
        title: title || "",
        artist: artist || "",
        coverUrl: coverUrl || null,
        durationMs: durationMs || 0,
      })
    );
  } catch (_) {}
}

/**
 * 更新播放/暂停状态 + 当前播放位置。
 */
export function mediaNotifSetState({ playing, positionMs }) {
  if (!window.CpMediaBridge) return;
  try {
    window.CpMediaBridge.setState(
      JSON.stringify({
        playing: !!playing,
        positionMs: positionMs || 0,
      })
    );
  } catch (_) {}
}

/**
 * 清除通知栏媒体信息并停止前台服务。
 */
export function mediaNotifClear() {
  if (!window.CpMediaBridge) return;
  try {
    window.CpMediaBridge.clear();
  } catch (_) {}
}

/**
 * 请求 Android 13+ 通知权限（结果不阻塞，首次播放前调用一次）。
 */
export function mediaNotifRequestPermission() {
  if (!window.CpMediaBridge) return;
  try {
    window.CpMediaBridge.requestPostNotifications();
  } catch (_) {}
}
