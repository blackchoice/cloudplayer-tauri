//! 与 Python `services/share_link_importer.py` + `platforms/netease.py` + `platforms/qqmusic.py` 对齐：
//! 从网易云 / QQ 音乐分享链接拉取歌单曲目。

use std::time::Duration;

use regex::Regex;
use reqwest::Client;
use serde_json::Value;
use url::Url;

use crate::import_playlist::ImportedTrackDto;

const NETEASE_REFERER: &str = "https://music.163.com/";
const QQ_REFERER: &str = "https://y.qq.com/";

fn netease_headers() -> reqwest::header::HeaderMap {
    let mut h = reqwest::header::HeaderMap::new();
    h.insert(
        reqwest::header::USER_AGENT,
        reqwest::header::HeaderValue::from_static(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        ),
    );
    h.insert(
        reqwest::header::REFERER,
        reqwest::header::HeaderValue::from_static(NETEASE_REFERER),
    );
    h.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/json, text/plain, */*"),
    );
    h
}

fn qq_headers() -> reqwest::header::HeaderMap {
    let mut h = reqwest::header::HeaderMap::new();
    h.insert(
        reqwest::header::USER_AGENT,
        reqwest::header::HeaderValue::from_static(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        ),
    );
    h.insert(
        reqwest::header::REFERER,
        reqwest::header::HeaderValue::from_static(QQ_REFERER),
    );
    h.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"),
    );
    h.insert(
        "Accept-Language",
        reqwest::header::HeaderValue::from_static("zh-CN,zh;q=0.9"),
    );
    h
}

pub fn detect_platform(url: &str) -> Option<&'static str> {
    let u = url.to_lowercase();
    if u.contains("163.com") || u.contains("163cn") || u.contains("music.163") {
        return Some("netease");
    }
    if u.contains("qq.com") {
        return Some("qq");
    }
    None
}

fn extract_netease_playlist_id(raw: &str) -> Option<String> {
    let u = raw.trim();
    let normalized = if u.starts_with("http://") || u.starts_with("https://") {
        u.to_string()
    } else {
        format!("https://{}", u)
    };
    if let Ok(parsed) = Url::parse(&normalized) {
        for (k, v) in parsed.query_pairs() {
            if k == "id" && !v.is_empty() {
                return Some(v.into_owned());
            }
        }
        let path = parsed.path();
        let re = Regex::new(r"/playlist[/?](\d+)").ok()?;
        if let Some(c) = re.captures(path) {
            return Some(c[1].to_string());
        }
        if let Some(frag) = parsed.fragment() {
            let qpart = frag.split('?').last().unwrap_or(frag);
            for pair in qpart.split('&') {
                if let Some((k, v)) = pair.split_once('=') {
                    if k == "id" && !v.is_empty() {
                        return Some(v.to_string());
                    }
                }
            }
        }
    }
    let re = Regex::new(r"[?&]id=(\d+)").ok()?;
    re.captures(raw).map(|c| c[1].to_string())
}

fn track_from_netease_item(t: &Value) -> Option<ImportedTrackDto> {
    let name = t.get("name")?.as_str()?.trim();
    if name.is_empty() {
        return None;
    }
    let artist = t
        .get("ar")
        .and_then(|x| x.as_array())
        .map(|ars| {
            ars.iter()
                .filter_map(|a| {
                    a.get("name")
                        .and_then(|n| n.as_str())
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                })
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_default();
    let album = t
        .get("al")
        .and_then(|al| {
            if let Some(n) = al.get("name").and_then(|x| x.as_str()) {
                return Some(n.trim().to_string());
            }
            al.as_str().map(|s| s.trim().to_string())
        })
        .unwrap_or_default();
    Some(ImportedTrackDto {
        title: name.to_string(),
        artist,
        album,
    })
}

fn collect_track_ids(track_ids_raw: Option<&Value>) -> Vec<i64> {
    let mut out = Vec::new();
    let Some(arr) = track_ids_raw.and_then(|x| x.as_array()) else {
        return out;
    };
    for item in arr {
        if let Some(obj) = item.as_object() {
            if let Some(id) = obj.get("id").and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))) {
                out.push(id);
                continue;
            }
        }
        if let Some(n) = item.as_i64() {
            out.push(n);
            continue;
        }
        if let Some(f) = item.as_f64() {
            out.push(f as i64);
            continue;
        }
        if let Some(s) = item.as_str() {
            if let Ok(n) = s.parse::<i64>() {
                out.push(n);
            }
        }
    }
    out
}

async fn netease_song_detail_batch(client: &Client, ids: &[i64]) -> Result<Vec<ImportedTrackDto>, String> {
    let mut result = Vec::new();
    for chunk in ids.chunks(500) {
        let ids_str = format!(
            "[{}]",
            chunk
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        let body = client
            .get("https://music.163.com/api/song/detail")
            .timeout(Duration::from_secs(60))
            .headers(netease_headers())
            .query(&[("ids", ids_str.as_str())])
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .text()
            .await
            .map_err(|e| e.to_string())?;
        let data: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
        let songs = data
            .get("songs")
            .or_else(|| data.get("data"))
            .and_then(|x| x.as_array())
            .cloned()
            .unwrap_or_default();
        for s in songs {
            if let Some(tr) = track_from_netease_item(&s) {
                result.push(tr);
            }
        }
    }
    Ok(result)
}

async fn fetch_netease_playlist(client: &Client, url: &str) -> Result<(String, Vec<ImportedTrackDto>), String> {
    let pid = extract_netease_playlist_id(url).ok_or_else(|| {
        "无法从链接中识别网易云歌单 id（请使用 music.163.com 歌单分享链接）。".to_string()
    })?;

    let body = client
        .get("https://music.163.com/api/playlist/detail")
        .timeout(Duration::from_secs(60))
        .headers(netease_headers())
        .query(&[("id", pid.as_str()), ("n", "100000")])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;

    let data: Value = serde_json::from_str(&body).map_err(|e| format!("网易云返回非 JSON：{e}"))?;
    let code = data.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    if code != 200 {
        let msg = data
            .get("msg")
            .and_then(|m| m.as_str())
            .unwrap_or("");
        return Err(format!("网易云接口错误：code={code} msg={msg}"));
    }

    let pl = data.get("playlist").cloned().unwrap_or(Value::Null);
    let name = pl
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("未命名歌单")
        .trim()
        .to_string();

    let mut out: Vec<ImportedTrackDto> = Vec::new();
    if let Some(tracks_raw) = pl.get("tracks").and_then(|x| x.as_array()) {
        for t in tracks_raw {
            if let Some(tr) = track_from_netease_item(t) {
                out.push(tr);
            }
        }
    }

    if out.is_empty() {
        let ids = collect_track_ids(pl.get("trackIds"));
        if !ids.is_empty() {
            out = netease_song_detail_batch(client, &ids).await?;
        }
    }

    if out.is_empty() {
        return Err("歌单为空或接口未返回曲目（可能是隐私歌单或需登录）。".into());
    }
    Ok((name, out))
}

fn percent_decode_url(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(hs) = std::str::from_utf8(&bytes[i + 1..i + 3]) {
                if hs.as_bytes().iter().all(|b| b.is_ascii_hexdigit()) {
                    if let Ok(v) = u8::from_str_radix(hs, 16) {
                        out.push(v);
                        i += 3;
                        continue;
                    }
                }
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn extract_qq_disstid(raw: &str) -> Option<String> {
    let u = percent_decode_url(raw.trim());
    let normalized = if u.starts_with("http://") || u.starts_with("https://") {
        u
    } else {
        format!("https://{}", u)
    };
    let Ok(parsed) = Url::parse(&normalized) else {
        return Regex::new(r"[?&]id=(\d+)")
            .ok()
            .and_then(|re| re.captures(raw))
            .map(|c| c[1].to_string());
    };
    for key in ["id", "disstid", "playlistId", "songlistid"] {
        for (k, v) in parsed.query_pairs() {
            if k == key && !v.is_empty() {
                return Some(v.into_owned());
            }
        }
    }
    let path = parsed.path();
    for pat in [r"/playlist/(\d+)", r"(?i)playlist[/_](\d+)"] {
        if let Ok(re) = Regex::new(pat) {
            if let Some(c) = re.captures(path) {
                return Some(c[1].to_string());
            }
        }
    }
    Regex::new(r"[?&]id=(\d+)")
        .ok()
        .and_then(|re| re.captures(&normalized))
        .map(|c| c[1].to_string())
}

async fn resolve_qq_share_url(client: &Client, url: &str) -> Result<String, String> {
    let u = url.trim();
    let normalized = if u.starts_with("http://") || u.starts_with("https://") {
        u.to_string()
    } else {
        format!("https://{}", u)
    };
    let decoded = percent_decode_url(&normalized);
    if extract_qq_disstid(&decoded).is_some() {
        return Ok(decoded);
    }

    let resp = client
        .get(&decoded)
        .timeout(Duration::from_secs(45))
        .headers(qq_headers())
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?;
    let final_url = resp.url().to_string();
    let html = resp.text().await.map_err(|e| e.to_string())?;

    if extract_qq_disstid(&final_url).is_some() {
        return Ok(final_url);
    }

    let patterns = [
        r#""dissid"\s*:\s*"?(\d+)"?"#,
        r#""disstid"\s*:\s*"?(\d+)"?"#,
        r#""tid"\s*:\s*"?(\d+)"?\s*,\s*"dirid""#,
        r"dissid=(\d+)",
        r"disstid=(\d+)",
        r"/n/ryqq/playlist/(\d+)",
        r"ryqq/pl/(\d+)",
        r"/playlist/(\d+)(?:\?|$|/)",
        r"playlistId=(\d+)",
        r"songlistid=(\d+)",
    ];
    for pat in patterns {
        let re = Regex::new(pat).map_err(|e| e.to_string())?;
        if let Some(c) = re.captures(&html) {
            let pid = c.get(1).unwrap().as_str();
            return Ok(format!("https://y.qq.com/n/ryqq/playlist/{pid}"));
        }
    }

    let re_full = Regex::new(r"https?://[a-zA-Z0-9.]+/n/ryqq/playlist/(\d+)").unwrap();
    if let Some(c) = re_full.captures(&html) {
        let pid = c.get(1).unwrap().as_str();
        return Ok(format!("https://y.qq.com/n/ryqq/playlist/{pid}"));
    }

    if let Ok(parsed) = Url::parse(&final_url) {
        if let Some(frag) = parsed.fragment() {
            if frag.contains("id=") {
                let qpart = frag.split('?').last().unwrap_or(frag);
                for pair in qpart.split('&') {
                    if let Some((k, v)) = pair.split_once('=') {
                        if (k == "id" || k == "dissid" || k == "disstid") && v.chars().all(|c| c.is_ascii_digit()) {
                            return Ok(format!("https://y.qq.com/n/ryqq/playlist/{v}"));
                        }
                    }
                }
            }
        }
    }

    Err("无法解析 QQ 音乐短链（页面结构可能更新）。请在浏览器打开该链接，从地址栏复制带 playlist 数字 id 的长链接后再粘贴。".into())
}

fn parse_jsonp_or_json(text: &str) -> Result<Value, String> {
    let t = text.trim();
    let inner = if t.starts_with("MusicJsonCallback(") {
        t.strip_prefix("MusicJsonCallback(").unwrap_or(t)
    } else if t.starts_with("jsonCallback(") {
        t.strip_prefix("jsonCallback(").unwrap_or(t)
    } else {
        return serde_json::from_str(t).map_err(|e| e.to_string());
    };
    let mut s = inner;
    if s.ends_with(");") {
        s = &s[..s.len() - 2];
    } else if s.ends_with(')') {
        s = &s[..s.len() - 1];
    }
    serde_json::from_str(s.trim()).map_err(|e| e.to_string())
}

fn song_from_qq(item: &Value) -> Option<ImportedTrackDto> {
    let name = item
        .get("songname")
        .or_else(|| item.get("title"))
        .and_then(|x| x.as_str())?
        .trim();
    if name.is_empty() {
        return None;
    }
    let artist = item
        .get("singer")
        .and_then(|x| x.as_array())
        .map(|singers| {
            singers
                .iter()
                .filter_map(|s| s.get("name").and_then(|n| n.as_str()).map(|x| x.trim()))
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_default();
    let mut album = item
        .get("albumname")
        .or_else(|| item.get("album_name"))
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if album.is_empty() {
        if let Some(alb) = item.get("album").and_then(|x| x.as_object()) {
            album = alb
                .get("name")
                .or_else(|| alb.get("title"))
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
        }
    }
    Some(ImportedTrackDto {
        title: name.to_string(),
        artist,
        album,
    })
}

async fn fetch_qq_playlist(client: &Client, url: &str) -> Result<(String, Vec<ImportedTrackDto>), String> {
    let resolved = resolve_qq_share_url(client, url).await?;
    let disstid = extract_qq_disstid(&resolved).ok_or_else(|| "无法从链接中识别 QQ 音乐歌单 id。".to_string())?;

    let body = client
        .get("https://c.y.qq.com/qzone/fcg-bin/fcg_ucc_getcdinfo_byids_cp.fcg")
        .timeout(Duration::from_secs(60))
        .headers(qq_headers())
        .query(&[
            ("type", "1"),
            ("json", "1"),
            ("utf8", "1"),
            ("onlysong", "0"),
            ("new_format", "1"),
            ("disstid", disstid.as_str()),
            ("format", "json"),
            ("inCharset", "utf8"),
            ("outCharset", "utf-8"),
            ("notice", "0"),
            ("platform", "yqq.json"),
            ("needNewCode", "0"),
            ("g_tk", "5381"),
            ("loginUin", "0"),
            ("hostUin", "0"),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;

    let data = parse_jsonp_or_json(&body).map_err(|e| format!("QQ 音乐返回无法解析：{e}"))?;
    let cdlist = data.get("cdlist").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let pl = cdlist.first().cloned().unwrap_or(Value::Null);
    let name = pl
        .get("dissname")
        .or_else(|| pl.get("diss_name"))
        .and_then(|x| x.as_str())
        .unwrap_or("未命名歌单")
        .trim()
        .to_string();
    let songs = pl.get("songlist").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    let mut out = Vec::new();
    for s in songs {
        if let Some(tr) = song_from_qq(&s) {
            out.push(tr);
        }
    }
    if out.is_empty() {
        return Err("歌单曲目为空或接口限制。".into());
    }
    Ok((name, out))
}

/// 拉取分享链接歌单（网易云 / QQ）。
pub async fn fetch_playlist_from_share_url(
    client: &Client,
    url: &str,
) -> Result<(String, Vec<ImportedTrackDto>), String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("链接为空".into());
    }
    match detect_platform(url) {
        Some("netease") => fetch_netease_playlist(client, url).await,
        Some("qq") => fetch_qq_playlist(client, url).await,
        _ => Err("暂只支持网易云音乐、QQ 音乐的分享链接（music.163.com / y.qq.com）。".into()),
    }
}
