/** 窗口标题栏控制（最小化/最大化/关闭） */

export async function wireWindowChrome() {
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const ctrls = document.querySelector(".window-titlebar__controls");
  if (!ctrls) return;
  if (typeof window.__TAURI_INTERNALS__ === "undefined") {
    ctrls.hidden = true;
    return;
  }
  let appWin;
  try {
    appWin = WebviewWindow.getCurrent();
  } catch {
    ctrls.hidden = true;
    return;
  }
  const minBtn = document.getElementById("win-btn-minimize");
  const maxBtn = document.getElementById("win-btn-maximize");
  const closeBtn = document.getElementById("win-btn-close");

  async function syncMaxIcon() {
    try {
      const m = await appWin.isMaximized();
      if (maxBtn) {
        const normal = maxBtn.querySelector(".win-max--normal");
        const restore = maxBtn.querySelector(".win-max--restore");
        normal?.toggleAttribute("hidden", m);
        restore?.toggleAttribute("hidden", !m);
        maxBtn.title = m ? "向下还原" : "最大化";
        maxBtn.setAttribute("aria-label", m ? "向下还原" : "最大化");
      }
    } catch {
      /* ignore */
    }
  }

  minBtn?.addEventListener("click", (e) => {
    e.preventDefault();
    e.stopPropagation();
    void appWin.minimize();
  });
  maxBtn?.addEventListener("click", (e) => {
    e.preventDefault();
    e.stopPropagation();
    void appWin.toggleMaximize().then(() => syncMaxIcon());
  });
  closeBtn?.addEventListener("click", (e) => {
    e.preventDefault();
    e.stopPropagation();
    void appWin.close();
  });

  await syncMaxIcon();
  try {
    const unResize = await appWin.onResized(() => {
      void syncMaxIcon();
    });
    window.addEventListener("beforeunload", () => {
      try {
        unResize();
      } catch {
        /* ignore */
      }
    });
  } catch {
    await syncMaxIcon();
  }
}
