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

async function openAddToPlaylistModal() {
  const modal = document.getElementById("add-to-playlist-modal");
  const trackEl = document.getElementById("add-to-pl-track");
  const listEl = document.getElementById("add-to-pl-list");
  if (!modal || !listEl) return;
  const cur = appState.playQueue[appState.playIndex];
  if (!cur) return;
  if (trackEl) trackEl.textContent = `${cur.title || "—"}${cur.artist ? ` — ${cur.artist}` : ""}`;
  listEl.innerHTML = "";
  const pls = await listPlaylistsCached();
  if (!pls.length) {
    const li = document.createElement("li");
    li.className = "add-to-pl-empty";
    li.textContent = "暂无歌单，请先新建";
    listEl.appendChild(li);
  } else {
    for (const p of pls) {
      const pid = p.id;
      if (pid == null) continue;
      const name = (p.name || "").trim() || `#${pid}`;
      const li = document.createElement("li");
      li.textContent = name;
      li.addEventListener("click", async () => {
        try {
          await invoke("append_playlist_import_items", {
            playlistId: pid,
            items: [buildPlaylistImportItem({ title: cur.title, artist: cur.artist || "", album: cur.album || "", sourceId: cur.source_id, coverUrl: cur.cover_url || "" })],
          });
          li.classList.add("is-added");
          await _deps.refreshSidebarPlaylists();
        } catch (err) {
          alertRequestFailed(err, "append_playlist_import_items");
        }
      });
      listEl.appendChild(li);
    }
  }
  modal.hidden = false;
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
      void invoke("save_settings", { patch: { last_play_mode_index: appState.playModeIndex } }).catch(() => {});
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
    await openAddToPlaylistModal();
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

  // 添加到歌单弹窗
  const addToPlModal = document.getElementById("add-to-playlist-modal");
  document.getElementById("add-to-pl-cancel")?.addEventListener("click", () => {
    if (addToPlModal) addToPlModal.hidden = true;
  });
  addToPlModal?.addEventListener("click", (e) => {
    if (e.target === addToPlModal) addToPlModal.hidden = true;
  });
  document.getElementById("add-to-pl-new")?.addEventListener("click", async () => {
    const name = window.prompt("歌单名称", "新歌单");
    if (!name || !name.trim()) return;
    const cur = appState.playQueue[appState.playIndex];
    if (!cur) return;
    try {
      const pid = await invoke("create_playlist", { name: name.trim() });
      await invoke("append_playlist_import_items", {
        playlistId: pid,
        items: [buildPlaylistImportItem({ title: cur.title, artist: cur.artist || "", album: cur.album || "", sourceId: cur.source_id, coverUrl: cur.cover_url || "" })],
      });
      await _deps.refreshSidebarPlaylists();
      if (addToPlModal) addToPlModal.hidden = true;
    } catch (err) {
      alertRequestFailed(err, "create_playlist");
    }
  });
  document.addEventListener("keydown", (e) => {
    if (!addToPlModal || addToPlModal.hidden) return;
    if (e.key === "Escape") {
      e.preventDefault();
      addToPlModal.hidden = true;
    }
  });

  wireLyricsReplaceModal();
}
