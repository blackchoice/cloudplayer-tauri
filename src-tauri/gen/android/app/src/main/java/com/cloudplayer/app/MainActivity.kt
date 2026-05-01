package com.cloudplayer.app

import android.os.Bundle
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {

  private var safeInsetTopPx: Int = 0
  private var safeInsetBottomPx: Int = 0

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  /**
   * 物理返回键：优先让 JS 消费，未消费则走系统默认（退到桌面）。
   * 使用 onBackPressed 而非 OnBackPressedCallback，避免与 TauriActivity 内部回调冲突。
   */
  @Suppress("DEPRECATION")
  override fun onBackPressed() {
    val wv = webViewRef
    if (wv == null) {
      super.onBackPressed()
      return
    }
    wv.evaluateJavascript("window.__cpAndroidBack && window.__cpAndroidBack()") { result ->
      if (result == "\"true\"" || result == "true") {
        // JS 已消费，不做任何事
      } else {
        super.onBackPressed()
      }
    }
  }

  /**
   * 避免系统「显示大小 / 字体大小」改变 WebView 内文字缩放，导致与 UI 设计比例不一致；
   * 关闭捏合缩放，行为接近常见原生 App（布局由 CSS 控制）。
   */
  override fun onWebViewCreate(webView: WebView) {
    webView.settings.apply {
      textZoom = 100
      setSupportZoom(false)
      builtInZoomControls = false
      displayZoomControls = false
      useWideViewPort = true
      loadWithOverviewMode = false
    }

    // 注册 JavascriptInterface 桥接
    webView.addJavascriptInterface(CpMediaBridge(applicationContext), "CpMediaBridge")

    // 保存 webView 引用供 dispatchMediaCallback / onBackPressed 使用
    webViewRef = webView

    // ① 立即从系统资源获取状态栏高度（不依赖 insets 分发，确保首帧就有值）
    val statusBarId = resources.getIdentifier("status_bar_height", "dimen", "android")
    if (statusBarId > 0) {
      val statusBarPx = resources.getDimensionPixelSize(statusBarId)
      val density = resources.displayMetrics.density
      safeInsetTopPx = (statusBarPx / density).toInt()
    }

    // ② insets 变化时动态更新（旋转、折叠屏等）
    ViewCompat.setOnApplyWindowInsetsListener(webView) { v, insets ->
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout()
      )
      val density = v.resources.displayMetrics.density
      safeInsetTopPx = (bars.top / density).toInt()
      safeInsetBottomPx = (bars.bottom / density).toInt()
      v.post {
        (v as WebView).evaluateJavascript(injectSafeAreaScript(), null)
      }
      insets
    }

    // ③ 请求系统分发 insets
    ViewCompat.requestApplyInsets(webView)

    // ④ 多次延迟注入，覆盖不同页面加载时机
    for (delay in longArrayOf(200, 500, 1000, 2000)) {
      webView.postDelayed({
        webView.evaluateJavascript(injectSafeAreaScript(), null)
      }, delay)
    }
  }

  /** 生成注入 CSS 变量 + 直接设置元素 padding 引用变量的 JS 代码 */
  private fun injectSafeAreaScript(): String {
    return "(function(){" +
      "var r=document.documentElement.style;" +
      "r.setProperty('--cp-safe-top','${safeInsetTopPx}px');" +
      "r.setProperty('--cp-safe-bottom','${safeInsetBottomPx}px');" +
      "var el=document.querySelector('.cp-mobile-library');" +
      "if(el)el.style.setProperty('padding-top','var(--cp-safe-top)');" +
      "})();"
  }

  companion object {
    @Volatile
    var webViewRef: WebView? = null

    /**
     * MediaPlaybackService 回调 → 通过 evaluateJavascript 触发 JS 端 window.__cpMediaCb
     */
    fun dispatchMediaCallback(action: String) {
      val wv = webViewRef ?: return
      val safe = action.replace("\\", "\\\\").replace("'", "\\'")
      wv.post {
        wv.evaluateJavascript(
          "window.__cpMediaCb && window.__cpMediaCb('$safe')",
          null
        )
      }
    }
  }
}
