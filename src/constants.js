/** 常量与配置（不可变） */

export const NAV = [
  { id: "discover", label: "发现", key: "like" },
  { id: "recent", label: "最近播放", key: "recent" },
  { id: "download", label: "本地和下载", key: "local_download" },
  { id: "import", label: "导入歌单", key: "import" },
];

/** 与 Py 版 PlayMode 对应：序 → 循 → 单 → 随 */
export const PLAY_MODES = [
  { key: "sequential", label: "序", tip: "顺序播放（点击切换模式）" },
  { key: "loop_list", label: "循", tip: "列表循环" },
  { key: "one", label: "单", tip: "单曲循环" },
  { key: "shuffle", label: "随", tip: "随机播放" },
];

export const QUALITY_LABELS = { flac: "无损", "320": "HQ", "128": "标准" };

/** 与 Py RecentPlaysPage：本会话内最近播放上限 */
export const RECENT_SESSION_MAX = 100;

/** 不向用户展示后端/网络异常细节（仅控制台保留完整错误） */
export const MSG_REQUEST_FAILED = "请求失败";

/** 桌面歌词子窗口 label */
export const LYRICS_WW_TARGET = { kind: "WebviewWindow", label: "lyrics" };

/** 主导航页面（非歌单详情）：离开歌单上下文时清除侧栏选中与歌单 id */
export const MAIN_NAV_PAGE_IDS = new Set(["discover", "recent", "download", "import", "settings"]);
