//! 与 Python `services/playlist_importer.py` 对齐：纯文本 / CSV / JSON。

use regex::Regex;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ImportedTrackDto {
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub album: String,
}

fn line_prefix_re() -> Regex {
    Regex::new(r"^\s*(?:\d+[.)]\s*|\d+\s+)").expect("regex")
}

fn strip_line_prefix(line: &str) -> String {
    line_prefix_re().replace(line, "").trim().to_string()
}

fn split_title_artist(line: &str) -> Option<ImportedTrackDto> {
    let s = strip_line_prefix(line);
    if s.is_empty() {
        return None;
    }
    for sep in [" - ", " – ", " — ", " / ", "\t", "|"] {
        if let Some(pos) = s.find(sep) {
            let a = s[..pos].trim();
            let b = s[pos + sep.len()..].trim();
            if !a.is_empty() && !b.is_empty() {
                return Some(ImportedTrackDto {
                    title: a.to_string(),
                    artist: b.to_string(),
                    album: String::new(),
                });
            }
        }
    }
    Some(ImportedTrackDto {
        title: s,
        artist: String::new(),
        album: String::new(),
    })
}

pub fn detect_format(text: &str) -> &'static str {
    let t = text.trim_start();
    if t.starts_with('{') || t.starts_with('[') {
        if serde_json::from_str::<serde_json::Value>(t).is_ok() {
            return "json";
        }
    }
    let first = text.lines().next().unwrap_or("");
    if first.contains(',') && (!first.contains('\t') || first.matches(',').count() >= 2) {
        return "csv";
    }
    "text"
}

pub fn parse_playlist_text(text: &str, fmt: &str) -> Result<Vec<ImportedTrackDto>, String> {
    let fmt = if fmt == "auto" {
        detect_format(text)
    } else {
        fmt
    };
    match fmt {
        "json" => parse_json(text),
        "csv" => parse_csv(text),
        "text" => Ok(parse_lines(text)),
        _ => Ok(parse_lines(text)),
    }
}

fn parse_lines(text: &str) -> Vec<ImportedTrackDto> {
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some(t) = split_title_artist(line) {
            out.push(t);
        }
    }
    out
}

/// 与 Python `csv.reader` 常见用例兼容：支持引号内逗号。
fn split_csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for c in line.chars() {
        match c {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    out.push(cur.trim().to_string());
    out
}

fn parse_csv(text: &str) -> Result<Vec<ImportedTrackDto>, String> {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return Ok(Vec::new());
    }

    let first_cells: Vec<String> = split_csv_line(lines[0]);
    let header: Vec<String> = first_cells
        .iter()
        .map(|c| c.to_lowercase().trim().to_string())
        .collect();
    let ti = header.iter().position(|h| h == "title");
    let ai = header.iter().position(|h| h == "artist");

    if let (Some(ti), Some(ai)) = (ti, ai) {
        let has_album = header.iter().position(|h| h == "album");
        let mut out = Vec::new();
        for line in lines.iter().skip(1) {
            let rec = split_csv_line(line);
            let n = rec.len();
            if n > ti.max(ai) {
                let title = rec.get(ti).map(|s| s.as_str()).unwrap_or("").trim().to_string();
                let artist = rec.get(ai).map(|s| s.as_str()).unwrap_or("").trim().to_string();
                let album = match has_album {
                    Some(ali) if n > ali => rec.get(ali).map(|s| s.as_str()).unwrap_or("").trim().to_string(),
                    _ => String::new(),
                };
                out.push(ImportedTrackDto {
                    title,
                    artist,
                    album,
                });
            }
        }
        return Ok(out);
    }

    let mut out = Vec::new();
    for line in lines {
        let rec = split_csv_line(line);
        if rec.len() >= 2 {
            out.push(ImportedTrackDto {
                title: rec[0].trim().to_string(),
                artist: rec[1].trim().to_string(),
                album: if rec.len() >= 3 {
                    rec[2].trim().to_string()
                } else {
                    String::new()
                },
            });
        } else if rec.len() == 1 {
            let cell = rec[0].as_str();
            if let Some(t) = split_title_artist(cell) {
                out.push(t);
            }
        }
    }
    Ok(out)
}

fn parse_json(text: &str) -> Result<Vec<ImportedTrackDto>, String> {
    let data: serde_json::Value = serde_json::from_str(text.trim()).map_err(|e| e.to_string())?;
    let mut out = Vec::new();

    if let Some(arr) = data.as_array() {
        for item in arr {
            if let Some(obj) = item.as_object() {
                let title = obj
                    .get("title")
                    .or_else(|| obj.get("name"))
                    .or_else(|| obj.get("song"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let artist = obj
                    .get("artist")
                    .or_else(|| obj.get("singer"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let album = obj
                    .get("album")
                    .or_else(|| obj.get("albumname"))
                    .or_else(|| obj.get("album_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if !title.is_empty() {
                    out.push(ImportedTrackDto {
                        title,
                        artist,
                        album,
                    });
                }
            } else if let Some(s) = item.as_str() {
                if let Some(t) = split_title_artist(s) {
                    out.push(t);
                }
            }
        }
        return Ok(out);
    }

    if let Some(obj) = data.as_object() {
        if let Some(tracks) = obj.get("tracks") {
            return parse_json(&serde_json::to_string(tracks).unwrap_or_default());
        }
    }

    Ok(out)
}
