/** Audio DOM 事件绑定 */
import { appState } from "./state.js";
import { PLAY_MODES, MSG_REQUEST_FAILED } from "./constants.js";
import {
  alertRequestFailed,
  audioDiagPayload,
  logPlayEventDesktop,
  randomNextIndex,
} from "./utils.js";
import { syncDesktopLyrics } from "./lyrics.js";
import { syncImmersiveLyrics, syncImmersiveSeek, syncImmersivePlayBtn } from "./immersive.js";
import {
  audioEl,
  playFromQueueIndex,
  syncSeekUi,
  togglePlayPauseFromHotkey,
  adjustPlayerVolumeDelta,
} from "./player.js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export function wireAudio() {
  const a = audioEl();
  const playBtn = document.getElementById("btn-player-play");
  const seek = document.getElementById("seek");

  a.addEventListener("timeupdate", () => {
    syncSeekUi();
    void syncDesktopLyrics();
    syncImmersiveLyrics();
    syncImmersiveSeek();
  });
  a.addEventListener("loadedmetadata", () => {
    syncSeekUi();
    if (appState.audioSourceGeneration === appState.playLoadGeneration) {
      void logPlayEventDesktop("audio_loadedmetadata", {
        url: a.src || null,
        extra: audioDiagPayload(a),
      });
    }
  });
  a.addEventListener("durationchange", () => syncSeekUi());
  a.addEventListener("canplay", () => syncSeekUi());
  a.addEventListener("progress", () => {
    if (appState.audioSourceGeneration !== appState.playLoadGeneration) return;
    const now = Date.now();
    if (now - appState.audioProgressLogLastTs < 1000) return;
    appState.audioProgressLogLastTs = now;
    void logPlayEventDesktop("audio_progress", {
      url: a.src || null,
      extra: audioDiagPayload(a),
    });
  });
  a.addEventListener("stalled", () => {
    if (appState.audioSourceGeneration !== appState.playLoadGeneration) return;
    void logPlayEventDesktop("audio_stalled", {
      url: a.src || null,
      extra: audioDiagPayload(a),
    });
  });
  a.addEventListener("ended", () => {
    if (appState.audioSourceGeneration === appState.playLoadGeneration) {
      void logPlayEventDesktop("audio_ended", {
        url: a.src || null,
        extra: audioDiagPayload(a),
      });
    }
    const n = appState.playQueue.length;
    const mode = PLAY_MODES[appState.playModeIndex].key;
    if (!n) {
      syncSeekUi();
      return;
    }
    if (mode === "one") {
      a.currentTime = 0;
      a.play().catch(() => {});
      return;
    }
    if (mode === "loop_list") {
      const nxt = (appState.playIndex + 1) % n;
      playFromQueueIndex(nxt);
      return;
    }
    if (mode === "shuffle") {
      playFromQueueIndex(randomNextIndex());
      return;
    }
    if (appState.playIndex < n - 1) {
      playFromQueueIndex(appState.playIndex + 1);
    } else if (playBtn) {
      playBtn.textContent = "▶";
    }
    syncSeekUi();
  });
  a.addEventListener("play", () => {
    if (playBtn) playBtn.textContent = "⏸";
    syncImmersivePlayBtn();
  });
  a.addEventListener("pause", () => {
    if (playBtn) playBtn.textContent = "▶";
    syncImmersivePlayBtn();
  });
  a.addEventListener("error", () => {
    const err = a.error;
    if (err && err.code === 1) return;
    if (appState.audioSourceGeneration !== appState.playLoadGeneration) return;
    void logPlayEventDesktop("audio_error", {
      url: a.src || null,
      error_code: err ? err.code : null,
      message: err && err.message ? err.message : null,
      extra: audioDiagPayload(a),
    });
    const sub = document.getElementById("dock-sub");
    if (sub && err) {
      sub.textContent = MSG_REQUEST_FAILED;
    }
  });

  if (seek) {
    seek.addEventListener("pointerdown", () => {
      appState.seekDragging = true;
    });
    seek.addEventListener("pointerup", () => {
      appState.seekDragging = false;
      syncSeekUi();
    });
    seek.addEventListener("input", () => {
      const d = a.duration;
      if (d && isFinite(d) && d > 0) {
        a.currentTime = (Number(seek.value) / 1000) * d;
      }
    });
  }

  playBtn?.addEventListener("click", async () => {
    if (!a.src) return;
    try {
      if (a.paused) {
        await a.play();
      } else {
        a.pause();
      }
    } catch (err) {
      alertRequestFailed(err, "audio play()");
    }
  });
  document.getElementById("btn-player-prev")?.addEventListener("click", () => {
    const n = appState.playQueue.length;
    if (!n) return;
    const mode = PLAY_MODES[appState.playModeIndex].key;
    if (mode === "shuffle") {
      playFromQueueIndex((appState.playIndex - 1 + n) % n);
      return;
    }
    if (mode === "loop_list" && appState.playIndex === 0) {
      playFromQueueIndex(n - 1);
      return;
    }
    if (appState.playIndex > 0) playFromQueueIndex(appState.playIndex - 1);
  });
  document.getElementById("btn-player-next")?.addEventListener("click", () => {
    const n = appState.playQueue.length;
    if (!n) return;
    const mode = PLAY_MODES[appState.playModeIndex].key;
    if (mode === "shuffle") {
      playFromQueueIndex(randomNextIndex());
      return;
    }
    if (mode === "loop_list" && appState.playIndex === n - 1) {
      playFromQueueIndex(0);
      return;
    }
    if (appState.playIndex < n - 1) playFromQueueIndex(appState.playIndex + 1);
  });
}

export function wireVolume() {
  const vol = document.getElementById("volume");
  const persist = async () => {
    const v = Number(vol.value) / 100;
    try {
      await invoke("save_settings", { patch: { volume: v } });
    } catch (e) {
      console.warn("save_settings", e);
    }
  };
  vol.addEventListener("input", () => {
    const v = Number(vol.value) / 100;
    const a = audioEl();
    if (a) a.volume = v;
  });
  vol.addEventListener("change", persist);
}

export function wireGlobalHotkeyListener() {
  void listen("global-hotkey", (e) => {
    const a = e?.payload;
    if (a === "play_pause") void togglePlayPauseFromHotkey();
    else if (a === "prev") document.getElementById("btn-player-prev")?.click();
    else if (a === "next") document.getElementById("btn-player-next")?.click();
    else if (a === "volume_up") void adjustPlayerVolumeDelta(0.05);
    else if (a === "volume_down") void adjustPlayerVolumeDelta(-0.05);
  });
}
