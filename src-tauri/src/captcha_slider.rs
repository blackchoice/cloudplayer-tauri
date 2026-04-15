//! 滑块验证码偏移：与 Python `captcha_solver.guess_slider_offset` 类似的边缘 + 模板匹配（纯 Rust）。

use base64::Engine;
use image::GrayImage;
use imageproc::edges::canny;
use imageproc::template_matching::{match_template, MatchTemplateMethod};

const MIN_CONF: f32 = 0.25f32;

fn strip_data_url(b64: &str) -> &str {
    let s = b64.trim();
    if let Some(i) = s.find(',') {
        if s[..i].contains("base64") {
            return s[i + 1..].trim();
        }
    }
    s
}

fn decode_image_b64(b64: &str) -> Option<GrayImage> {
    let raw = strip_data_url(b64);
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(raw.as_bytes())
        .ok()?;
    let img = image::load_from_memory(&bytes).ok()?.to_luma8();
    Some(img)
}

/// 返回滑块在背景上的水平偏移（像素），失败则 `None`。
pub fn guess_slider_offset(background_b64: &str, slider_b64: &str) -> Option<i32> {
    let bg = decode_image_b64(background_b64)?;
    let fg = decode_image_b64(slider_b64)?;
    if bg.width() < fg.width() + 8 || bg.height() < fg.height() + 2 {
        return None;
    }
    let bg_gray = canny(&bg, 100.0, 200.0);
    let fg_gray = canny(&fg, 100.0, 200.0);
    let map = match_template(&bg_gray, &fg_gray, MatchTemplateMethod::CrossCorrelationNormalized);
    let (w, h) = map.dimensions();
    let mut best = 0.0f32;
    let mut best_x = 0u32;
    for y in 0..h {
        for x in 0..w {
            let v = map.get_pixel(x, y).0[0];
            if v > best {
                best = v;
                best_x = x;
            }
        }
    }
    if !best.is_finite() || best < MIN_CONF {
        return None;
    }
    Some(best_x as i32)
}
