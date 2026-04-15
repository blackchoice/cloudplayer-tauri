//! 多源歌词解析：默认顺序为 **Lrc.cx**（`api.lrc.cx/lyrics`）→ 网易云（自托管时 YRC `/lyric/new` 优先）→ LRCLIB → pjmp3 页内 LRC。
//!
//! 自托管 [NeteaseCloudMusicApiEnhanced](https://github.com/NeteaseCloudMusicApiEnhanced/api-enhanced)
//! 时，在 `GET /lyric` 之前优先请求 `GET /lyric/new`，将返回的 **YRC（逐字）** 转为标准 LRC 供前端解析。
//!
//! 歌词文本经 [amll_lyric]（[Apple Music-like Lyrics](https://github.com/Steve-xmh/applemusic-like-lyrics) 子库）解析/序列化为统一 LRC；YRC / TTML 亦转为 LRC。
//!
//! **atlas** 源：内置请求 [amll-ttml-db](https://amlldb.bikonoo.com)（`GET …/ncm-lyrics/{网易云ID}.ttml|yrc|lrc`），与 Lyric-Atlas 仓库逻辑一致，无需自托管服务。

use std::io::Cursor;

use amll_lyric::lrc::{parse_lrc, stringify_lrc};
use amll_lyric::LyricLine;
use amll_lyric::ttml::parse_ttml;
use amll_lyric::yrc::parse_yrc;
use reqwest::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, REFERER, USER_AGENT};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::Settings;
use crate::lrc_format::has_lrc_timestamp_tags;
use crate::pjmp3::fetch_song_lrc_text;

fn lyrics_log(msg: impl AsRef<str>) {
    eprintln!("[lyrics] {}", msg.as_ref());
}

fn netease_portal_headers() -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(
        USER_AGENT,
        HeaderValue::from_static(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        ),
    );
    h.insert(REFERER, HeaderValue::from_static("https://music.163.com/"));
    h.insert(ACCEPT, HeaderValue::from_static("application/json, text/plain, */*"));
    h
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsFetchIn {
    pub pjmp3_source_id: Option<String>,
    pub title: String,
    pub artist: String,
    /// 可选，传给 [Lrc.cx](https://api.lrc.cx) 等源以提高匹配率
    #[serde(default)]
    pub album: String,
    #[serde(default)]
    #[allow(dead_code)] // 预留：本地元数据歌词
    pub local_path: Option<String>,
    /// 秒，可选，用于 LRCLIB 匹配
    #[serde(default)]
    pub duration_seconds: Option<f64>,
}

/// 歌词载荷：统一 LRC 文本 + 可选逐字时间轴（毫秒，与 amll `LyricWord` 一致）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LyricsPayload {
    pub lrc_text: String,
    pub word_lines: Option<Vec<WordLine>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordLine {
    pub start_ms: u64,
    pub end_ms: u64,
    pub words: Vec<WordTiming>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordTiming {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

/// 从 amll 解析结果生成载荷；仅当至少一行含多个词时视为「逐字」可用（行级 LRC 为每行一词，不启用）。
fn lyric_lines_to_payload(lines: &[LyricLine<'_>]) -> LyricsPayload {
    let lrc_text = pack_lyrics_for_ui(stringify_lrc(lines));
    let has_word_timing = lines.iter().any(|l| l.words.len() > 1);
    let word_lines = if has_word_timing {
        Some(
            lines
                .iter()
                .map(|l| WordLine {
                    start_ms: l.start_time,
                    end_ms: l.end_time,
                    words: l
                        .words
                        .iter()
                        .map(|w| WordTiming {
                            start_ms: w.start_time,
                            end_ms: w.end_time,
                            text: w.word.to_string(),
                        })
                        .collect(),
                })
                .collect(),
        )
    } else {
        None
    };
    LyricsPayload {
        lrc_text,
        word_lines,
    }
}

fn line_only_payload(raw: String) -> LyricsPayload {
    LyricsPayload {
        lrc_text: pack_lyrics_for_ui(raw),
        word_lines: None,
    }
}

#[inline]
fn is_word_level(p: &LyricsPayload) -> bool {
    p.word_lines.is_some()
}

enum Prov {
    LrcCx,
    Atlas,
    Pjmp3,
    Netease,
    Lrclib,
}

fn parse_order(s: &str) -> Vec<Prov> {
    let mut out = Vec::new();
    for p in s.split(',') {
        match p.trim().to_ascii_lowercase().as_str() {
            "lrccx" | "lrc_cx" => out.push(Prov::LrcCx),
            "atlas" | "lyric_atlas" => out.push(Prov::Atlas),
            "pjmp3" => out.push(Prov::Pjmp3),
            "netease" => out.push(Prov::Netease),
            "lrclib" => out.push(Prov::Lrclib),
            _ => {}
        }
    }
    if out.is_empty() {
        out.push(Prov::Netease);
        out.push(Prov::Atlas);
        out.push(Prov::LrcCx);
        out.push(Prov::Lrclib);
        out.push(Prov::Pjmp3);
    }
    out
}

#[inline]
fn looks_like_lrc(text: &str) -> bool {
    has_lrc_timestamp_tags(text)
}

fn polish_lyrics_with_amll(input: &str) -> String {
    let lines = parse_lrc(input);
    if !lines.is_empty() {
        let s = stringify_lrc(&lines);
        if !s.trim().is_empty() {
            return s;
        }
    }
    let y_lines = parse_yrc(input);
    if !y_lines.is_empty() {
        let s = stringify_lrc(&y_lines);
        if !s.trim().is_empty() {
            return s;
        }
    }
    let trimmed = input.trim();
    if trimmed.starts_with("<?xml")
        || trimmed.contains("<tt ")
        || trimmed.contains("<ttml")
        || trimmed.contains("xmlns=\"http://www.w3.org/ns/ttml")
    {
        if let Ok(ttml) = parse_ttml(Cursor::new(input.as_bytes())) {
            if !ttml.lines.is_empty() {
                let s = stringify_lrc(&ttml.lines);
                if !s.trim().is_empty() {
                    return s;
                }
            }
        }
    }
    input.to_string()
}

/// 供前端 `parseLrc` 使用：经 amll_lyric 归一化为标准 LRC；无法解析时保留原文。
pub fn pack_lyrics_for_ui(raw: String) -> String {
    let polished = polish_lyrics_with_amll(&raw);
    if looks_like_lrc(&polished) {
        polished
    } else if looks_like_lrc(&raw) {
        raw
    } else {
        raw
    }
}

const LRC_CX_LYRICS: &str = "https://api.lrc.cx/lyrics";
const LRC_CX_COVER: &str = "https://api.lrc.cx/cover";

/// Lrc.cx 歌词：`GET https://api.lrc.cx/lyrics`（`title` / `artist` / `album` 可选，空则省略）。
async fn lyric_lrc_cx(
    client: &Client,
    title: &str,
    artist: &str,
    album: &str,
) -> Result<Option<LyricsPayload>, String> {
    let title = title.trim();
    let artist = artist.trim();
    let album = album.trim();
    let mut q: Vec<(&str, &str)> = Vec::new();
    if !title.is_empty() {
        q.push(("title", title));
    }
    if !artist.is_empty() {
        q.push(("artist", artist));
    }
    if !album.is_empty() {
        q.push(("album", album));
    }
    if q.is_empty() {
        lyrics_log("lrc.cx lyrics: skip (no title/artist/album)");
        return Ok(None);
    }
    lyrics_log(format!(
        "lrc.cx GET lyrics title={title:?} artist={artist:?} album={album:?}"
    ));
    let r = client
        .get(LRC_CX_LYRICS)
        .query(&q)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        lyrics_log(format!("lrc.cx lyrics http {}", r.status()));
        return Ok(None);
    }
    let text = r.text().await.map_err(|e| e.to_string())?;
    let text = text.trim();
    if text.is_empty() {
        lyrics_log("lrc.cx lyrics: empty body");
        return Ok(None);
    }
    if text.eq_ignore_ascii_case("invalid http request") {
        lyrics_log("lrc.cx lyrics: invalid http request");
        return Ok(None);
    }
    if looks_like_lrc(text) {
        lyrics_log(format!("lrc.cx lyrics ok chars={}", text.len()));
        return Ok(Some(line_only_payload(text.to_string())));
    }
    let polished = polish_lyrics_with_amll(text);
    if looks_like_lrc(&polished) {
        lyrics_log(format!("lrc.cx lyrics ok (amll) chars={}", polished.len()));
        return Ok(Some(line_only_payload(polished)));
    }
    lyrics_log("lrc.cx lyrics: body not lrc-like");
    Ok(None)
}

/// Lrc.cx 封面：`GET https://api.lrc.cx/cover`（跟随重定向至 CDN 图片 URL）。
pub async fn fetch_lrc_cx_cover(
    client: &Client,
    title: &str,
    artist: &str,
    album: &str,
) -> Result<Option<String>, String> {
    let title = title.trim();
    let artist = artist.trim();
    let album = album.trim();
    let mut q: Vec<(&str, &str)> = Vec::new();
    if !title.is_empty() {
        q.push(("title", title));
    }
    if !artist.is_empty() {
        q.push(("artist", artist));
    }
    if !album.is_empty() {
        q.push(("album", album));
    }
    if q.is_empty() {
        return Ok(None);
    }
    let r = client
        .get(LRC_CX_COVER)
        .query(&q)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = r.status();
    let final_url = r.url().clone();
    let ct = r
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    let body = r.bytes().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        lyrics_log(format!("lrc.cx cover http {}", status));
        return Ok(None);
    }
    let host = final_url.host_str().unwrap_or("");
    let path = final_url.path();
    if host == "api.lrc.cx" && path.contains("/cover") {
        lyrics_log("lrc.cx cover: no redirect off /cover");
        return Ok(None);
    }
    if ct.starts_with("image/") && !body.is_empty() {
        let u = final_url.to_string();
        lyrics_log(format!("lrc.cx cover ok (image) url={u}"));
        return Ok(Some(u));
    }
    if final_url.as_str().starts_with("http://") || final_url.as_str().starts_with("https://") {
        let u = final_url.to_string();
        if u.contains(".jpg")
            || u.contains(".jpeg")
            || u.contains(".png")
            || u.contains(".webp")
            || u.contains("music.126.net")
            || u.contains("/pic/")
        {
            lyrics_log(format!("lrc.cx cover ok url={u}"));
            return Ok(Some(u));
        }
    }
    lyrics_log("lrc.cx cover: unrecognized response");
    Ok(None)
}

/// 从 `/lyric/new` 的 JSON 中取 YRC 原文（兼容 api-enhanced / Binaryify 多种嵌套）。
fn yrc_raw_from_lyric_new_json(v: &Value) -> Option<String> {
    let try_str = |s: &str| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    };
    let try_value_yrc = |node: &Value| -> Option<String> {
        if let Some(s) = node.as_str() {
            return try_str(s);
        }
        if let Some(s) = node.get("lyric").and_then(|x| x.as_str()) {
            return try_str(s);
        }
        None
    };
    for ptr in [
        "/yrc/lyric",
        "/body/yrc/lyric",
        "/data/yrc/lyric",
        "/result/yrc/lyric",
        "/body/data/yrc/lyric",
    ] {
        if let Some(s) = v.pointer(ptr).and_then(|x| x.as_str()) {
            if let Some(t) = try_str(s) {
                return Some(t);
            }
        }
    }
    if let Some(n) = v.get("yrc") {
        if let Some(t) = try_value_yrc(n) {
            return Some(t);
        }
    }
    if let Some(n) = v.pointer("/body/yrc") {
        if let Some(t) = try_value_yrc(n) {
            return Some(t);
        }
    }
    if let Some(n) = v.pointer("/data/yrc") {
        if let Some(t) = try_value_yrc(n) {
            return Some(t);
        }
    }
    None
}

/// `GET {api_base}/lyric/new?id=`（api-enhanced），YRC → LRC + 可选逐字时间轴。
async fn lyric_netease_api_yrc(
    client: &Client,
    api_base: &str,
    song_id: i64,
) -> Result<Option<LyricsPayload>, String> {
    let url = format!("{api_base}/lyric/new");
    lyrics_log(format!("netease api GET lyric/new id={song_id}"));
    let r = client
        .get(&url)
        .query(&[("id", song_id.to_string())])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        lyrics_log(format!("netease api lyric/new http {}", r.status()));
        return Ok(None);
    }
    let v: Value = r.json::<Value>().await.map_err(|e| e.to_string())?;
    let Some(raw) = yrc_raw_from_lyric_new_json(&v) else {
        lyrics_log("netease api lyric/new: no yrc text in json");
        return Ok(None);
    };
    let lines = parse_yrc(&raw);
    if lines.is_empty() {
        lyrics_log("netease api yrc: amll parse_yrc empty");
        return Ok(None);
    }
    let payload = lyric_lines_to_payload(&lines);
    if looks_like_lrc(&payload.lrc_text) {
        lyrics_log(format!(
            "netease api yrc->lrc ok chars={} (from yrc chars={}) word_level={}",
            payload.lrc_text.len(),
            raw.len(),
            payload.word_lines.is_some()
        ));
        Ok(Some(payload))
    } else {
        lyrics_log("netease api yrc->lrc: not lrc-like");
        Ok(None)
    }
}

async fn lyric_lrclib(
    client: &Client,
    title: &str,
    artist: &str,
    duration_seconds: Option<f64>,
) -> Result<Option<LyricsPayload>, String> {
    lyrics_log(format!(
        "lrclib request title={title:?} artist={artist:?} duration_s={duration_seconds:?}"
    ));
    let mut req = client
        .get("https://lrclib.net/api/get")
        .query(&[("track_name", title), ("artist_name", artist)]);
    if let Some(d) = duration_seconds {
        if d.is_finite() && d > 0.0 {
            req = req.query(&[("duration", &d.round().max(1.0).to_string())]);
        }
    }
    let r = req.send().await.map_err(|e| e.to_string())?;
    if !r.status().is_success() {
        lyrics_log(format!("lrclib http {}", r.status()));
        return Ok(None);
    }
    let v: Value = r.json::<Value>().await.map_err(|e| e.to_string())?;
    if let Some(s) = v.get("syncedLyrics").and_then(|x| x.as_str()) {
        if looks_like_lrc(s) {
            lyrics_log(format!("lrclib hit syncedLyrics chars={}", s.len()));
            return Ok(Some(line_only_payload(s.to_string())));
        }
    }
    if let Some(s) = v.get("plainLyrics").and_then(|x| x.as_str()) {
        if looks_like_lrc(s) {
            lyrics_log(format!("lrclib hit plainLyrics chars={}", s.len()));
            return Ok(Some(line_only_payload(s.to_string())));
        }
    }
    lyrics_log("lrclib: no usable lrc field");
    Ok(None)
}

/// 门户搜索首条歌曲 id（与 [`lyric_netease_music163_portal`] 搜索阶段一致）。
async fn netease_portal_search_song_id(
    client: &Client,
    title: &str,
    artist: &str,
) -> Result<Option<i64>, String> {
    let kw = format!("{artist} {title}");
    let search = client
        .get("https://music.163.com/api/search/get/web")
        .headers(netease_portal_headers())
        .query(&[("s", kw.as_str()), ("type", "1"), ("limit", "8")])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !search.status().is_success() {
        lyrics_log(format!(
            "netease portal search (id-only) http {}",
            search.status()
        ));
        return Ok(None);
    }
    let sj: Value = search.json::<Value>().await.map_err(|e| e.to_string())?;
    if sj.get("code").and_then(|x| x.as_i64()).unwrap_or(0) != 200 {
        return Ok(None);
    }
    Ok(sj
        .pointer("/result/songs/0/id")
        .and_then(|x| x.as_i64())
        .or_else(|| sj.pointer("/result/songs/0/song/id").and_then(|x| x.as_i64())))
}

/// 与 Lyric-Atlas `RepositoryFetcher` 等价：门户搜网易云 ID 后按 ttml → yrc → lrc 拉取并解析为 LRC。
const TTML_DB_BASE: &str = "https://amlldb.bikonoo.com/ncm-lyrics/";

async fn lyric_ttml_db(
    client: &Client,
    title: &str,
    artist: &str,
) -> Result<Option<LyricsPayload>, String> {
    let Some(nid) = netease_portal_search_song_id(client, title, artist).await? else {
        lyrics_log("atlas(ttml-db): no netease song id");
        return Ok(None);
    };
    lyrics_log(format!("atlas(ttml-db): song id={nid}"));
    for ext in ["ttml", "yrc", "lrc"] {
        let url = format!("{TTML_DB_BASE}{nid}.{ext}");
        lyrics_log(format!("atlas(ttml-db): GET {url}"));
        let r = client.get(&url).send().await.map_err(|e| e.to_string())?;
        let status = r.status();
        if status.as_u16() == 404 {
            continue;
        }
        if !status.is_success() {
            lyrics_log(format!("atlas(ttml-db): {ext} http {}", status));
            continue;
        }
        let body = r.text().await.map_err(|e| e.to_string())?;
        let content = body.trim();
        if content.is_empty() {
            continue;
        }
        let maybe_payload = match ext {
            "ttml" => match parse_ttml(Cursor::new(body.as_bytes())) {
                Ok(lyric) if !lyric.lines.is_empty() => Some(lyric_lines_to_payload(&lyric.lines)),
                Ok(_) => None,
                Err(e) => {
                    lyrics_log(format!("atlas(ttml-db): parse_ttml err {e}"));
                    None
                }
            },
            "yrc" => {
                let lines = parse_yrc(content);
                if lines.is_empty() {
                    None
                } else {
                    Some(lyric_lines_to_payload(&lines))
                }
            }
            "lrc" => {
                let lines = parse_lrc(content);
                if lines.is_empty() {
                    None
                } else {
                    Some(lyric_lines_to_payload(&lines))
                }
            }
            _ => unreachable!(),
        };
        if let Some(p) = maybe_payload {
            if !p.lrc_text.trim().is_empty() {
                lyrics_log(format!(
                    "atlas(ttml-db): ok from {ext} chars={} word_level={}",
                    p.lrc_text.len(),
                    p.word_lines.is_some()
                ));
                return Ok(Some(p));
            }
        }
    }
    Ok(None)
}

/// 直连 music.163.com 网页 API（不依赖自托管 NeteaseCloudMusicApi）。
async fn lyric_netease_music163_portal(
    client: &Client,
    title: &str,
    artist: &str,
) -> Result<Option<LyricsPayload>, String> {
    let kw = format!("{artist} {title}");
    let search = client
        .get("https://music.163.com/api/search/get/web")
        .headers(netease_portal_headers())
        .query(&[("s", kw.as_str()), ("type", "1"), ("limit", "8")])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !search.status().is_success() {
        lyrics_log(format!("netease portal search http {}", search.status()));
        return Ok(None);
    }
    let sj: Value = search.json::<Value>().await.map_err(|e| e.to_string())?;
    if sj.get("code").and_then(|x| x.as_i64()).unwrap_or(0) != 200 {
        lyrics_log("netease portal search: code != 200");
        return Ok(None);
    }
    let id = sj
        .pointer("/result/songs/0/id")
        .and_then(|x| x.as_i64())
        .or_else(|| sj.pointer("/result/songs/0/song/id").and_then(|x| x.as_i64()));
    let Some(nid) = id else {
        lyrics_log("netease portal: no song id in search result");
        return Ok(None);
    };
    lyrics_log(format!("netease portal song id={nid}"));
    let lr = client
        .get("https://music.163.com/api/song/lyric")
        .headers(netease_portal_headers())
        .query(&[
            ("id", nid.to_string()),
            ("lv", "-1".to_string()),
            ("kv", "-1".to_string()),
            ("tv", "-1".to_string()),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !lr.status().is_success() {
        lyrics_log(format!("netease portal lyric http {}", lr.status()));
        return Ok(None);
    }
    let lj: Value = lr.json::<Value>().await.map_err(|e| e.to_string())?;
    if let Some(ly) = lj.pointer("/lrc/lyric").and_then(|x| x.as_str()) {
        if looks_like_lrc(ly) {
            lyrics_log(format!("netease portal hit /lrc/lyric chars={}", ly.len()));
            return Ok(Some(line_only_payload(ly.to_string())));
        }
    }
    if let Some(ly) = lj.get("lrc").and_then(|x| x.as_str()) {
        if looks_like_lrc(ly) {
            lyrics_log(format!("netease portal hit lrc str chars={}", ly.len()));
            return Ok(Some(line_only_payload(ly.to_string())));
        }
    }
    lyrics_log("netease portal: lyric payload not lrc-like");
    Ok(None)
}

async fn lyric_netease(
    client: &Client,
    api_base: &str,
    title: &str,
    artist: &str,
) -> Result<Option<LyricsPayload>, String> {
    let base = api_base.trim().trim_end_matches('/');
    if base.is_empty() {
        return Ok(None);
    }
    lyrics_log(format!("netease api search base={base} title={title:?} artist={artist:?}"));
    let kw = format!("{artist} {title}");
    let search = client
        .get(format!("{base}/cloudsearch"))
        .query(&[("keywords", kw.as_str()), ("type", "1"), ("limit", "5")])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !search.status().is_success() {
        lyrics_log(format!("netease api cloudsearch http {}", search.status()));
        return Ok(None);
    }
    let sj: Value = search.json::<Value>().await.map_err(|e| e.to_string())?;
    let id = sj
        .pointer("/result/songs/0/id")
        .and_then(|x| x.as_i64())
        .or_else(|| sj.pointer("/songs/0/id").and_then(|x| x.as_i64()));
    let Some(nid) = id else {
        lyrics_log("netease api: no song id");
        return Ok(None);
    };
    lyrics_log(format!("netease api song id={nid}"));
    // api-enhanced：`/lyric/new` → 逐字 YRC，转成 LRC + 可选 word_lines
    match lyric_netease_api_yrc(client, base, nid).await {
        Ok(Some(payload)) => return Ok(Some(payload)),
        Ok(None) => {}
        Err(e) => lyrics_log(format!("netease api lyric/new: {e}, fallback /lyric")),
    }
    let lr = client
        .get(format!("{base}/lyric"))
        .query(&[("id", nid.to_string())])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !lr.status().is_success() {
        lyrics_log(format!("netease api lyric http {}", lr.status()));
        return Ok(None);
    }
    let lj: Value = lr.json::<Value>().await.map_err(|e| e.to_string())?;
    if let Some(ly) = lj.pointer("/lrc/lyric").and_then(|x| x.as_str()) {
        if looks_like_lrc(ly) {
            lyrics_log(format!("netease api hit /lrc/lyric chars={}", ly.len()));
            return Ok(Some(line_only_payload(ly.to_string())));
        }
    }
    if let Some(ly) = lj.get("lrc").and_then(|x| x.as_str()) {
        if looks_like_lrc(ly) {
            lyrics_log(format!("netease api hit lrc str chars={}", ly.len()));
            return Ok(Some(line_only_payload(ly.to_string())));
        }
    }
    lyrics_log("netease api: lyric not lrc-like");
    Ok(None)
}

fn consider_payload(
    fallback: &mut Option<LyricsPayload>,
    payload: LyricsPayload,
    provider: &str,
) -> Option<LyricsPayload> {
    if is_word_level(&payload) {
        lyrics_log(format!(
            "result ok provider={provider} chars={} word_level=true",
            payload.lrc_text.len()
        ));
        return Some(payload);
    }
    if fallback.is_none() {
        lyrics_log(format!(
            "result hold fallback provider={provider} chars={} word_level=false",
            payload.lrc_text.len()
        ));
        *fallback = Some(payload);
    }
    None
}

pub async fn fetch_song_lrc_enriched(
    client: &Client,
    settings: &Settings,
    req: &LyricsFetchIn,
) -> Result<Option<LyricsPayload>, String> {
    let order = parse_order(&settings.lyrics_provider_order);
    lyrics_log(format!(
        "fetch_song_lrc_enriched start title={:?} artist={:?} album={:?} pjmp3_source_id={:?} local_path={:?} duration_s={:?} order={}",
        req.title,
        req.artist,
        req.album.as_str(),
        req.pjmp3_source_id.as_deref(),
        req.local_path.as_deref(),
        req.duration_seconds,
        settings.lyrics_provider_order
    ));
    let mut fallback: Option<LyricsPayload> = None;
    for p in order {
        match p {
            Prov::LrcCx => {
                lyrics_log("try provider=lrc.cx");
                match lyric_lrc_cx(client, &req.title, &req.artist, &req.album).await {
                    Ok(Some(payload)) => {
                        if let Some(done) =
                            consider_payload(&mut fallback, payload, "lrc.cx")
                        {
                            return Ok(Some(done));
                        }
                    }
                    Ok(None) => lyrics_log("lrc.cx: miss"),
                    Err(e) => lyrics_log(format!("lrc.cx error: {e}")),
                }
            }
            Prov::Atlas => {
                lyrics_log("try provider=atlas (amlldb)");
                match lyric_ttml_db(client, &req.title, &req.artist).await {
                    Ok(Some(payload)) => {
                        if let Some(done) =
                            consider_payload(&mut fallback, payload, "atlas")
                        {
                            return Ok(Some(done));
                        }
                    }
                    Ok(None) => lyrics_log("atlas: miss"),
                    Err(e) => lyrics_log(format!("atlas error: {e}")),
                }
            }
            Prov::Pjmp3 => {
                lyrics_log("try provider=pjmp3");
                if let Some(ref sid) = req.pjmp3_source_id {
                    let t = sid.trim();
                    if !t.is_empty() {
                        match fetch_song_lrc_text(client, t).await {
                            Ok(Some(txt)) if looks_like_lrc(&txt) => {
                                let payload = line_only_payload(txt);
                                lyrics_log(format!(
                                    "result ok provider=pjmp3 chars={} (after looks_like_lrc)",
                                    payload.lrc_text.len()
                                ));
                                if let Some(done) =
                                    consider_payload(&mut fallback, payload, "pjmp3")
                                {
                                    return Ok(Some(done));
                                }
                            }
                            Ok(Some(_)) => {
                                lyrics_log("pjmp3 returned text but not lrc-like, continue");
                            }
                            Ok(None) => {
                                lyrics_log("pjmp3: no lrc");
                            }
                            Err(e) => {
                                lyrics_log(format!("pjmp3 error: {e}"));
                                return Err(e);
                            }
                        }
                    } else {
                        lyrics_log("pjmp3: empty source id");
                    }
                } else {
                    lyrics_log("pjmp3: skip (no pjmp3_source_id)");
                }
            }
            Prov::Netease => {
                lyrics_log("try provider=netease");
                let mut net: Option<LyricsPayload> = None;
                if !settings.lyrics_netease_api_base.trim().is_empty() {
                    match lyric_netease(
                        client,
                        settings.lyrics_netease_api_base.trim(),
                        &req.title,
                        &req.artist,
                    )
                    .await
                    {
                        Ok(t) => net = t,
                        Err(e) => lyrics_log(format!("netease api (self-hosted) error: {e}")),
                    }
                } else {
                    lyrics_log("netease: no custom api base, use portal only");
                }
                if net.is_none() {
                    match lyric_netease_music163_portal(client, &req.title, &req.artist).await {
                        Ok(t) => net = t,
                        Err(e) => lyrics_log(format!("netease portal error: {e}")),
                    }
                }
                if let Some(payload) = net {
                    lyrics_log(format!(
                        "result ok provider=netease chars={}",
                        payload.lrc_text.len()
                    ));
                    if let Some(done) = consider_payload(&mut fallback, payload, "netease") {
                        return Ok(Some(done));
                    }
                } else {
                    lyrics_log("netease: miss");
                }
            }
            Prov::Lrclib => {
                if settings.lyrics_lrclib_enabled {
                    lyrics_log("try provider=lrclib");
                    match lyric_lrclib(
                        client,
                        &req.title,
                        &req.artist,
                        req.duration_seconds,
                    )
                    .await
                    {
                        Ok(Some(payload)) => {
                            if let Some(done) =
                                consider_payload(&mut fallback, payload, "lrclib")
                            {
                                return Ok(Some(done));
                            }
                        }
                        Ok(None) => lyrics_log("lrclib: miss"),
                        Err(e) => lyrics_log(format!("lrclib error: {e}")),
                    }
                } else {
                    lyrics_log("lrclib: disabled in settings");
                }
            }
        }
    }
    if let Some(f) = fallback.take() {
        lyrics_log(format!(
            "fetch_song_lrc_enriched: return line-only fallback chars={}",
            f.lrc_text.len()
        ));
        return Ok(Some(f));
    }
    lyrics_log("fetch_song_lrc_enriched: no lyrics from any provider");
    Ok(None)
}
