/** 沉浸播放页：歌词渲染、同步、控制 */
import { appState } from "./state.js";
import { PLAY_MODES } from "./constants.js";
import { formatTime, formatNowPlayingSubtitle, saveLikedSet } from "./utils.js";
import { refreshFavButton, setPlayerNavEnabled } from "./player.js";
import { invoke } from "@tauri-apps/api/core";

let _refreshSidebarPlaylists = () => {};

export function initImmersiveOverlay(deps) {
  _refreshSidebarPlaylists = deps.refreshSidebarPlaylists || (() => {});
}

function audioEl() {
  return document.getElementById("audio-player");
}

export function renderImmersiveLyrics() {
  const container = document.getElementById("immersive-lyrics");
  if (!container) return;
  container.innerHTML = "";
  if (!appState.lrcEntries.length) {
    const p = document.createElement("div");
    p.className = "immersive-lyrics__line";
    p.textContent = "暂无歌词";
    p.style.textAlign = "center";
    p.style.paddingTop = "40px";
    container.appendChild(p);
    return;
  }
  for (let i = 0; i < appState.lrcEntries.length; i++) {
    const el = document.createElement("div");
    el.className = "immersive-lyrics__line";
    el.textContent = appState.lrcEntries[i].text || " ";
    el.dataset.idx = String(i);
    el.addEventListener("click", () => {
      const a = audioEl();
      if (a && appState.lrcEntries[i]) {
        a.currentTime = appState.lrcEntries[i].t;
      }
    });
    container.appendChild(el);
  }
  appState.immersiveLyricsIdx = -1;
}

export function syncImmersiveLyrics() {
  const ov = document.getElementById("immersive-player");
  if (!ov || ov.hidden) return;
  const container = document.getElementById("immersive-lyrics");
  if (!container) return;
  if (container.children.length !== appState.lrcEntries.length) {
    renderImmersiveLyrics();
  }
  const a = audioEl();
  const t = a ? Number(a.currentTime) || 0 : 0;
  let idx = -1;
  for (let k = 0; k < appState.lrcEntries.length; k++) {
    if (appState.lrcEntries[k].t <= t + 0.12) idx = k;
    else break;
  }
  if (idx === appState.immersiveLyricsIdx) return;
  appState.immersiveLyricsIdx = idx;
  const lines = container.children;
  for (let i = 0; i < lines.length; i++) {
    lines[i].classList.remove("is-active", "is-past");
    if (i === idx) {
      lines[i].classList.add("is-active");
      lines[i].scrollIntoView({ behavior: "smooth", block: "center" });
    } else if (i < idx) {
      lines[i].classList.add("is-past");
    }
  }
}

export function syncImmersivePlayBtn() {
  const btn = document.getElementById("immersive-play");
  if (!btn) return;
  const a = audioEl();
  btn.textContent = a && !a.paused ? "⏸" : "▶";
}

export function syncImmersiveSeek() {
  const ov = document.getElementById("immersive-player");
  if (!ov || ov.hidden) return;
  const a = audioEl();
  const seek = document.getElementById("immersive-seek");
  const cur = document.getElementById("immersive-time-current");
  const tot = document.getElementById("immersive-time-total");
  if (!a || !seek || !cur || !tot) return;
  const d = a.duration;
  if (d && isFinite(d) && d > 0) {
    tot.textContent = formatTime(d);
    if (!appState.immersiveSeekDragging) {
      seek.value = String(Math.min(1000, Math.floor((a.currentTime / d) * 1000)));
    }
    cur.textContent = formatTime(a.currentTime);
    seek.disabled = false;
  } else {
    cur.textContent = "0:00";
    tot.textContent = "0:00";
    seek.value = "0";
    seek.disabled = !a.src;
  }
}

export function openImmersivePlayer() {
  const overlay = document.getElementById("immersive-player");
  if (!overlay) return;
  const it = appState.playQueue[appState.playIndex];
  const cover = document.getElementById("dock-cover");
  const ic = document.getElementById("immersive-cover");
  const it2 = document.getElementById("immersive-title");
  const is2 = document.getElementById("immersive-sub");
  if (ic && cover) ic.src = cover.src;
  if (it2) it2.textContent = it?.title || document.getElementById("dock-title")?.textContent || "未播放";
  if (is2) is2.textContent = it ? formatNowPlayingSubtitle(it) : (document.getElementById("dock-sub")?.textContent || "");
  renderImmersiveLyrics();
  syncImmersivePlayBtn();
  syncImmersiveSeek();
  // 同步喜欢按钮
  const favBtn = document.getElementById("immersive-fav");
  if (favBtn) {
    const sid = (it?.source_id || "").trim();
    const canFav = !!sid && !it?.local_path;
    favBtn.disabled = !canFav;
    const on = canFav && appState.likedIds.has(sid);
    favBtn.textContent = on ? "♥" : "♡";
  }
  overlay.hidden = false;
  overlay.setAttribute("aria-hidden", "false");
  // 队列面板切换沉浸主题
  document.getElementById("queue-panel")?.classList.add("is-immersive");
  // 同步播放模式按钮
  const modeBtn = document.getElementById("immersive-mode");
  if (modeBtn) {
    const m = PLAY_MODES[appState.playModeIndex];
    modeBtn.textContent = m.label;
    modeBtn.title = m.tip;
  }
}

export function closeImmersivePlayer() {
  const overlay = document.getElementById("immersive-player");
  if (!overlay) return;
  overlay.hidden = true;
  overlay.setAttribute("aria-hidden", "true");
  document.getElementById("queue-panel")?.classList.remove("is-immersive");
}

/** 绑定沉浸播放页所有控件事件 */
export function wireImmersiveOverlay() {
  const btnMap = [
    ["immersive-prev", "btn-player-prev"],
    ["immersive-play", "btn-player-play"],
    ["immersive-next", "btn-player-next"],
    ["immersive-quality", "dock-quality"],
    ["immersive-lyrics-btn", "btn-dock-lyrics"],
  ];

  for (const [immId, mainId] of btnMap) {
    const immBtn = document.getElementById(immId);
    const mainBtn = document.getElementById(mainId);
    if (immBtn && mainBtn) {
      immBtn.addEventListener("click", () => mainBtn.click());
    }
  }

  // 播放顺序按钮（直接处理）
  const modeBtn = document.getElementById("immersive-mode");
  if (modeBtn) {
    const syncModeBtn = () => {
      const m = PLAY_MODES[appState.playModeIndex];
      modeBtn.textContent = m.label;
      modeBtn.title = m.tip;
      // 同步主界面按钮
      const mainMode = document.getElementById("btn-play-mode");
      if (mainMode) { mainMode.textContent = m.label; mainMode.title = m.tip; }
    };
    syncModeBtn();
    modeBtn.addEventListener("click", () => {
      appState.playModeIndex = (appState.playModeIndex + 1) % PLAY_MODES.length;
      syncModeBtn();
      setPlayerNavEnabled();
    });
  }

  // 播放列表按钮（直接处理）
  document.getElementById("immersive-queue")?.addEventListener("click", () => {
    const panel = document.getElementById("queue-panel");
    const btn = document.getElementById("queue-toggle");
    if (!panel) return;
    panel.classList.toggle("collapsed");
    if (btn) btn.textContent = panel.classList.contains("collapsed") ? "展开" : "收起";
  });

  // 队列面板点击外部收起
  document.addEventListener("click", (e) => {
    const panel = document.getElementById("queue-panel");
    if (!panel || panel.classList.contains("collapsed")) return;
    if (e.target.closest("#queue-panel") || e.target.closest("#btn-dock-queue") || e.target.closest("#immersive-queue")) return;
    panel.classList.add("collapsed");
    const btn = document.getElementById("queue-toggle");
    if (btn) btn.textContent = "展开";
  });

  // 喜欢按钮（直接处理，不委托）
  const favBtn = document.getElementById("immersive-fav");
  if (favBtn) {
    favBtn.addEventListener("click", async () => {
      const cur = appState.playQueue[appState.playIndex];
      if (!cur) return;
      const sid = (cur.source_id || "").trim();
      if (!sid || cur.local_path) return;
      const wasLiked = appState.likedIds.has(sid);
      if (wasLiked) {
        appState.likedIds.delete(sid);
        try { await invoke("remove_from_favorites", { sourceId: sid }); } catch (_) {}
      } else {
        appState.likedIds.add(sid);
        try {
          await invoke("add_to_favorites", {
            title: cur.title || "", artist: cur.artist || "", album: cur.album || "",
            sourceId: sid, coverUrl: cur.cover_url || "", playUrl: cur.play_url || "", durationMs: cur.duration_ms || 0,
          });
        } catch (_) {}
      }
      saveLikedSet(appState.likedIds);
      favBtn.textContent = appState.likedIds.has(sid) ? "♥" : "♡";
      refreshFavButton();
      void _refreshSidebarPlaylists();
    });
  }

  // 标题栏按钮：最小化 / 最大化 / 关闭
  document.getElementById("immersive-close")?.addEventListener("click", closeImmersivePlayer);
  document.getElementById("immersive-exit")?.addEventListener("click", closeImmersivePlayer);
  document.getElementById("immersive-win-min")?.addEventListener("click", async () => {
    try {
      const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
      await WebviewWindow.getCurrent().minimize();
    } catch (_) {}
  });
  document.getElementById("immersive-win-max")?.addEventListener("click", async () => {
    try {
      const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
      await WebviewWindow.getCurrent().toggleMaximize();
    } catch (_) {}
  });

  // 标题栏拖拽移动窗口
  const topBar = document.querySelector(".immersive-overlay__top");
  if (topBar) {
    topBar.addEventListener("mousedown", async (e) => {
      if (e.target.closest(".immersive-overlay__top-controls")) return;
      try {
        const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
        await WebviewWindow.getCurrent().startDragging();
      } catch (_) {}
    });
  }

  // 背景点击关闭
  document.querySelector(".immersive-overlay__bg")?.addEventListener("click", closeImmersivePlayer);

  // 歌词区域拖拽滚动
  const lyricsWrap = document.querySelector(".immersive-overlay__right");
  if (lyricsWrap) {
    let dragScrolling = false;
    let dragStartY = 0;
    let dragStartScroll = 0;
    lyricsWrap.addEventListener("mousedown", (e) => {
      if (e.target.closest(".immersive-lyrics__line")) return;
      dragScrolling = true;
      dragStartY = e.clientY;
      dragStartScroll = lyricsWrap.scrollTop;
      lyricsWrap.style.cursor = "grabbing";
      e.preventDefault();
    });
    window.addEventListener("mousemove", (e) => {
      if (!dragScrolling) return;
      lyricsWrap.scrollTop = dragStartScroll - (e.clientY - dragStartY);
    });
    window.addEventListener("mouseup", () => {
      if (dragScrolling) {
        dragScrolling = false;
        lyricsWrap.style.cursor = "";
      }
    });
  }

  // 进度条
  const seek = document.getElementById("immersive-seek");
  if (seek) {
    seek.addEventListener("pointerdown", () => { appState.immersiveSeekDragging = true; });
    seek.addEventListener("pointerup", () => { appState.immersiveSeekDragging = false; syncImmersiveSeek(); });
    seek.addEventListener("input", () => {
      const a = audioEl();
      const d = a?.duration;
      if (d && isFinite(d) && d > 0) {
        a.currentTime = (Number(seek.value) / 1000) * d;
      }
    });
  }

  // 音量
  const immVol = document.getElementById("immersive-volume");
  const mainVol = document.getElementById("volume");
  if (immVol && mainVol) {
    immVol.addEventListener("input", () => {
      const v = Number(immVol.value);
      mainVol.value = String(v);
      mainVol.dispatchEvent(new Event("input"));
    });
    immVol.addEventListener("change", () => {
      mainVol.dispatchEvent(new Event("change"));
    });
    const syncVol = () => { immVol.value = mainVol.value; };
    mainVol.addEventListener("input", syncVol);
    syncVol();
  }

  // Escape 关闭
  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      const ov = document.getElementById("immersive-player");
      if (ov && !ov.hidden) { e.preventDefault(); closeImmersivePlayer(); }
    }
  });
}
