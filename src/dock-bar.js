/** Dock 底栏交互：播放模式、音质、收藏、下载、更多菜单、歌词、队列 */
import { appState } from "./state.js";
import { PLAY_MODES, QUALITY_LABELS } from "./constants.js";
import {
  alertRequestFailed,
  listPlaylistsCached,
  buildPlaylistImportItem,
  enqueueDownloadForTrack,
  saveLikedSet,
} from "./utils.js";
import {
  closeContextMenu,
  buildAddToSubmenu,
  mountContextMenuAt,
  cmBtn,
} from "./context-menu.js";
import {
  refreshLyricsLockMenuLabel,
  broadcastDesktopLyricsLock,
  toggleDesktopLyrics,
  wireLyricsReplaceModal,
  openLyricsReplaceModal,
} from "./lyrics.js";
import {
  audioEl,
  setPlayerNavEnabled,
  removeCurrentFromQueue,
  toggleQueuePanel,
  renderQueuePanel,
  refreshFavButton,
} from "./player.js";
import { setPage } from "./pages.js";
import { invoke } from "@tauri-apps/api/core";

/** @type {{ refreshSidebarPlaylists: Function }} */
let _deps = {};

export function initDockBar(deps) {
  _deps = deps;
}

function closeAllDockMenus() {
  document.querySelectorAll(".dock-menu").forEach((el) => {
    el.hidden = true;
  });
}

function toggleDockMenu(menuEl) {
  const willOpen = menuEl.hidden;
  closeAllDockMenus();
  menuEl.hidden = !willOpen;
}

export function wireDockBar() {
  // 播放模式
  const modeBtn = document.getElementById("btn-play-mode");
  if (modeBtn) {
    const syncModeBtn = () => {
      const m = PLAY_MODES[appState.playModeIndex];
      modeBtn.textContent = m.label;
      modeBtn.title = m.tip;
    };
    syncModeBtn();
    modeBtn.addEventListener("click", () => {
      appState.playModeIndex = (appState.playModeIndex + 1) % PLAY_MODES.length;
      syncModeBtn();
      setPlayerNavEnabled();
    });
  }

  // 音质偏好弹出菜单
  const qBtn = document.getElementById("dock-quality");
  const qPop = document.getElementById("popover-quality");
  if (qBtn && qPop) {
    qBtn.textContent = QUALITY_LABELS[appState.qualityPref] || "标准";
    qBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      toggleDockMenu(qPop);
    });
    qPop.querySelectorAll("[data-quality]").forEach((b) => {
      b.addEventListener("click", (e) => {
        e.stopPropagation();
        appState.qualityPref = b.getAttribute("data-quality") || "128";
        qBtn.textContent = QUALITY_LABELS[appState.qualityPref] || "标准";
        closeAllDockMenus();
      });
    });
  }

  // 收藏
  document.getElementById("btn-dock-fav")?.addEventListener("click", async (e) => {
    e.stopPropagation();
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
          title: cur.title || "",
          artist: cur.artist || "",
          album: cur.album || "",
          sourceId: sid,
          coverUrl: cur.cover_url || "",
          playUrl: cur.play_url || "",
          durationMs: cur.duration_ms || 0,
        });
      } catch (_) {}
    }
    saveLikedSet(appState.likedIds);
    refreshFavButton();
    void _deps.refreshSidebarPlaylists();
  });

  // 下载弹出菜单
  document.getElementById("btn-dock-dl")?.addEventListener("click", (e) => {
    e.stopPropagation();
    toggleDockMenu(document.getElementById("popover-dl"));
  });
  document.getElementById("popover-dl")?.querySelectorAll("[data-dlq]").forEach((b) => {
    b.addEventListener("click", (e) => {
      e.stopPropagation();
      const q = b.getAttribute("data-dlq") || "128";
      closeAllDockMenus();
      const cur = appState.playQueue[appState.playIndex];
      if (!cur) { alert("当前没有播放曲目。"); return; }
      void enqueueDownloadForTrack(
        { sourceId: cur.source_id, title: cur.title, artist: cur.artist },
        q
      );
    });
  });

  // 更多菜单
  document.getElementById("btn-dock-more")?.addEventListener("click", (e) => {
    e.stopPropagation();
    toggleDockMenu(document.getElementById("popover-more"));
  });

  // 更多菜单 → 添加到歌单
  document.querySelector('[data-more="add-pl"]')?.addEventListener("click", async (e) => {
    e.stopPropagation();
    closeAllDockMenus();
    const cur = appState.playQueue[appState.playIndex];
    if (!cur) return;
    const pls = await listPlaylistsCached();
    const root = document.createElement("div");
    const { addRow, fly, sub } = buildAddToSubmenu({
      title: cur.title,
      artist: cur.artist,
      album: cur.album,
      sourceId: cur.source_id,
      coverUrl: cur.cover_url,
    });
    let any = false;
    for (const p of pls) {
      const pid = p.id;
      if (pid == null) continue;
      any = true;
      const name = (p.name || "").trim() || `#${pid}`;
      sub.appendChild(
        cmBtn(name, async () => {
          await invoke("append_playlist_import_items", {
            playlistId: pid,
            items: [buildPlaylistImportItem({ title: cur.title, artist: cur.artist || "", album: cur.album || "", sourceId: cur.source_id, coverUrl: cur.cover_url || "" })],
          });
          await _deps.refreshSidebarPlaylists();
        })
      );
    }
    if (!any) sub.appendChild(cmBtn("（暂无其它歌单）", () => {}, true));
    addRow.appendChild(fly);
    addRow.appendChild(sub);
    root.appendChild(addRow);
    const btnEl = e.currentTarget;
    const rect = btnEl.getBoundingClientRect();
    mountContextMenuAt(rect.right, rect.top, root);
  });

  // 更多菜单 → 从播放列表删除
  document.querySelector('[data-more="rm-queue"]')?.addEventListener("click", (e) => {
    e.stopPropagation();
    closeAllDockMenus();
    removeCurrentFromQueue();
  });

  // 更多菜单 → 桌面歌词锁定
  document.getElementById("btn-more-lyrics-lock")?.addEventListener("click", async (e) => {
    e.stopPropagation();
    closeAllDockMenus();
    appState.desktopLyricsLocked = !appState.desktopLyricsLocked;
    refreshLyricsLockMenuLabel();
    try {
      await invoke("save_settings", { patch: { desktop_lyrics_locked: appState.desktopLyricsLocked } });
    } catch (err) {
      console.warn("save_settings desktop_lyrics_locked", err);
    }
    await broadcastDesktopLyricsLock();
  });

  // 桌面歌词按钮
  document.getElementById("btn-dock-lyrics")?.addEventListener("click", async (e) => {
    e.stopPropagation();
    try {
      await toggleDesktopLyrics();
    } catch (err) {
      alertRequestFailed(err, "toggleDesktopLyrics");
    }
  });

  // 替换歌词按钮
  document.getElementById("btn-dock-lyrics-replace")?.addEventListener("click", (e) => {
    e.stopPropagation();
    void openLyricsReplaceModal();
  });

  // 播放列表按钮
  document.getElementById("btn-dock-queue")?.addEventListener("click", (e) => {
    e.stopPropagation();
    toggleQueuePanel();
  });

  // 偏好设置按钮
  document.getElementById("btn-dock-settings")?.addEventListener("click", (e) => {
    e.stopPropagation();
    closeAllDockMenus();
    setPage("settings");
  });

  // 点击空白关闭菜单
  document.addEventListener("click", (e) => {
    if (e.target.closest(".dock-menu-anchor") || e.target.closest(".dock-menu")) return;
    closeAllDockMenus();
  });

  wireLyricsReplaceModal();
}
