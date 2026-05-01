package com.cloudplayer.app

import android.content.Context
import android.webkit.JavascriptInterface
import org.json.JSONObject

/**
 * JavaScript 桥接：供 WebView 中的 mobile-media-notif.js 调用，
 * 将媒体播放状态同步到 Android MediaPlaybackService（前台服务 + 通知栏）。
 *
 * 仅暴露 update / setState / clear / requestPostNotifications 四个方法，
 * 不暴露任意反射能力。
 */
class CpMediaBridge(private val ctx: Context) {

  @JavascriptInterface
  fun update(json: String) {
    try {
      val obj = JSONObject(json)
      val title = obj.optString("title", "")
      val artist = obj.optString("artist", "")
      val coverUrl = if (obj.has("coverUrl") && !obj.isNull("coverUrl")) obj.optString("coverUrl") else null
      val durationMs = obj.optLong("durationMs", 0)
      MediaPlaybackService.update(ctx, title, artist, coverUrl, durationMs)
    } catch (_: Exception) {
      // 解析失败静默忽略，不崩溃
    }
  }

  @JavascriptInterface
  fun setState(json: String) {
    try {
      val obj = JSONObject(json)
      val playing = obj.optBoolean("playing", false)
      val positionMs = obj.optLong("positionMs", 0)
      MediaPlaybackService.setPlayState(ctx, playing, positionMs)
    } catch (_: Exception) {
    }
  }

  @JavascriptInterface
  fun clear() {
    try {
      MediaPlaybackService.clear(ctx)
    } catch (_: Exception) {
    }
  }

  @JavascriptInterface
  fun requestPostNotifications() {
    try {
      val activity = MainActivity.webViewRef?.context
      if (activity is android.app.Activity) {
        androidx.core.app.ActivityCompat.requestPermissions(
          activity,
          arrayOf(android.Manifest.permission.POST_NOTIFICATIONS),
          1001
        )
      }
    } catch (_: Exception) {
    }
  }
}
