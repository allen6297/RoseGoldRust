//! Small SVG rasterizer for the shapes `load_image` tests draw.
//!
//! Covers the subset in `examples/assets/dot.svg`: an `<svg>` root with
//! `width` / `height` / `viewBox`, filled `<rect>`, and filled `<circle>`.
//! The result is composited onto white, matching `HostSvg`.

use std::collections::HashMap;

pub(super) struct Rgb {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
}

pub(super) fn rasterize(xml: &str) -> Option<Rgb> {
    let xml = xml.trim();
    if xml.is_empty() {
        return None;
    }
    let tags = parse_tags(xml);
    let root = tags.iter().find(|t| t.name == "svg")?;
    let mut view = [0f32; 4];
    let has_view = parse_view_box(root.attr("viewBox"), &mut view);
    let mut width = parse_len(root.attr("width"), if has_view { view[2] } else { 0.0 });
    let mut height = parse_len(root.attr("height"), if has_view { view[3] } else { 0.0 });
    if width <= 0.0 && has_view {
        width = view[2];
    }
    if height <= 0.0 && has_view {
        height = view[3];
    }
    if width <= 0.0 {
        width = 1.0;
    }
    if height <= 0.0 {
        height = 1.0;
    }
    let w = width.round().max(1.0) as i32;
    let h = height.round().max(1.0) as i32;
    if w > 8192 || h > 8192 {
        return None;
    }
    let (sx, sy, ox, oy) = if has_view && view[2] > 0.0 && view[3] > 0.0 {
        (w as f32 / view[2], h as f32 / view[3], view[0], view[1])
    } else {
        (1.0, 1.0, 0.0, 0.0)
    };
    let mut px = vec![0x00FF_FFFFu32; (w as usize) * (h as usize)];
    for tag in &tags {
        if tag.name == "svg" || tag.name == "g" || tag.name == "defs" {
            continue;
        }
        let fill = parse_color(tag.attr("fill")).unwrap_or(0x0000_0000);
        match tag.name.as_str() {
            "rect" => {
                let x = parse_len(tag.attr("x"), 0.0);
                let y = parse_len(tag.attr("y"), 0.0);
                let rw = parse_len(tag.attr("width"), 0.0);
                let rh = parse_len(tag.attr("height"), 0.0);
                if rw <= 0.0 || rh <= 0.0 {
                    continue;
                }
                fill_rect(&mut px, w, h, sx, sy, ox, oy, x, y, rw, rh, fill);
            }
            "circle" => {
                let cx = parse_len(tag.attr("cx"), 0.0);
                let cy = parse_len(tag.attr("cy"), 0.0);
                let r = parse_len(tag.attr("r"), 0.0);
                if r <= 0.0 {
                    continue;
                }
                fill_circle(&mut px, w, h, sx, sy, ox, oy, cx, cy, r, fill);
            }
            "ellipse" => {
                let cx = parse_len(tag.attr("cx"), 0.0);
                let cy = parse_len(tag.attr("cy"), 0.0);
                let rx = parse_len(tag.attr("rx"), 0.0);
                let ry = parse_len(tag.attr("ry"), 0.0);
                if rx <= 0.0 || ry <= 0.0 {
                    continue;
                }
                fill_ellipse(&mut px, w, h, sx, sy, ox, oy, cx, cy, rx, ry, fill);
            }
            _ => {}
        }
    }
    Some(Rgb { w, h, px })
}

fn fill_rect(
    px: &mut [u32],
    w: i32,
    h: i32,
    sx: f32,
    sy: f32,
    ox: f32,
    oy: f32,
    x: f32,
    y: f32,
    rw: f32,
    rh: f32,
    color: u32,
) {
    for row in 0..h {
        for col in 0..w {
            let ux = (col as f32 + 0.5) / sx + ox;
            let uy = (row as f32 + 0.5) / sy + oy;
            if ux >= x && uy >= y && ux < x + rw && uy < y + rh {
                px[(row as usize) * (w as usize) + col as usize] = color;
            }
        }
    }
}

fn fill_circle(
    px: &mut [u32],
    w: i32,
    h: i32,
    sx: f32,
    sy: f32,
    ox: f32,
    oy: f32,
    cx: f32,
    cy: f32,
    r: f32,
    color: u32,
) {
    let r2 = r * r;
    for row in 0..h {
        for col in 0..w {
            let ux = (col as f32 + 0.5) / sx + ox;
            let uy = (row as f32 + 0.5) / sy + oy;
            let dx = ux - cx;
            let dy = uy - cy;
            if dx * dx + dy * dy <= r2 {
                px[(row as usize) * (w as usize) + col as usize] = color;
            }
        }
    }
}

fn fill_ellipse(
    px: &mut [u32],
    w: i32,
    h: i32,
    sx: f32,
    sy: f32,
    ox: f32,
    oy: f32,
    cx: f32,
    cy: f32,
    rx: f32,
    ry: f32,
    color: u32,
) {
    for row in 0..h {
        for col in 0..w {
            let ux = (col as f32 + 0.5) / sx + ox;
            let uy = (row as f32 + 0.5) / sy + oy;
            let nx = (ux - cx) / rx;
            let ny = (uy - cy) / ry;
            if nx * nx + ny * ny <= 1.0 {
                px[(row as usize) * (w as usize) + col as usize] = color;
            }
        }
    }
}

struct Tag {
    name: String,
    attrs: HashMap<String, String>,
}

impl Tag {
    fn attr(&self, name: &str) -> &str {
        self.attrs.get(name).map(String::as_str).unwrap_or("")
    }
}

fn parse_tags(xml: &str) -> Vec<Tag> {
    let b = xml.as_bytes();
    let mut i = 0usize;
    let mut tags = Vec::new();
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        if b.get(i + 1) == Some(&b'/') || b.get(i + 1) == Some(&b'!') || b.get(i + 1) == Some(&b'?')
        {
            if let Some(end) = xml[i..].find('>') {
                i += end + 1;
            } else {
                break;
            }
            continue;
        }
        let start = i + 1;
        let Some(end_rel) = xml[start..].find('>') else {
            break;
        };
        let mut body = &xml[start..start + end_rel];
        i = start + end_rel + 1;
        if let Some(stripped) = body.strip_suffix('/') {
            body = stripped;
        }
        let mut parts = body.split_whitespace();
        let Some(name) = parts.next() else {
            continue;
        };
        let name = name.split(':').next_back().unwrap_or(name).to_string();
        let attr_src = if let Some(sp) = body.find(char::is_whitespace) {
            body[sp..].trim()
        } else {
            ""
        };
        tags.push(Tag {
            name,
            attrs: parse_attrs(attr_src),
        });
    }
    tags
}

fn parse_attrs(src: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let b = src.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        let key_start = i;
        while i < b.len() && b[i] != b'=' && !b[i].is_ascii_whitespace() {
            i += 1;
        }
        let key = src[key_start..i].trim().to_string();
        while i < b.len() && b[i] != b'=' {
            i += 1;
        }
        if i >= b.len() || key.is_empty() {
            break;
        }
        i += 1;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        let quote = b[i];
        let value = if quote == b'"' || quote == b'\'' {
            i += 1;
            let vstart = i;
            while i < b.len() && b[i] != quote {
                i += 1;
            }
            let v = src[vstart..i].to_string();
            if i < b.len() {
                i += 1;
            }
            v
        } else {
            let vstart = i;
            while i < b.len() && !b[i].is_ascii_whitespace() {
                i += 1;
            }
            src[vstart..i].to_string()
        };
        let key = key.split(':').next_back().unwrap_or(&key).to_string();
        out.insert(key, value);
    }
    out
}

fn parse_view_box(raw: &str, out: &mut [f32; 4]) -> bool {
    let nums: Vec<f32> = scan_numbers(raw);
    if nums.len() < 4 || nums[2] <= 0.0 || nums[3] <= 0.0 {
        return false;
    }
    out.copy_from_slice(&nums[..4]);
    true
}

fn parse_len(raw: &str, percent_of: f32) -> f32 {
    let s = raw.trim().to_ascii_lowercase();
    if s.is_empty() {
        return percent_of;
    }
    if let Some(num) = s.strip_suffix('%') {
        return percent_of * parse_f32(num) / 100.0;
    }
    let (num, scale) = if let Some(n) = s.strip_suffix("px") {
        (n, 1.0)
    } else if let Some(n) = s.strip_suffix("pt") {
        (n, 96.0 / 72.0)
    } else if let Some(n) = s.strip_suffix("in") {
        (n, 96.0)
    } else if let Some(n) = s.strip_suffix("cm") {
        (n, 96.0 / 2.54)
    } else if let Some(n) = s.strip_suffix("mm") {
        (n, 96.0 / 25.4)
    } else {
        (s.as_str(), 1.0)
    };
    parse_f32(num) * scale
}

fn parse_f32(raw: &str) -> f32 {
    raw.trim().parse::<f32>().unwrap_or(0.0)
}

fn scan_numbers(raw: &str) -> Vec<f32> {
    let mut out = Vec::new();
    let b = raw.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        while i < b.len() && !is_num_start(b[i]) {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        let start = i;
        if b[i] == b'+' || b[i] == b'-' {
            i += 1;
        }
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i < b.len() && b[i] == b'.' {
            i += 1;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
        if start < i {
            if let Ok(n) = raw[start..i].parse::<f32>() {
                out.push(n);
            }
        } else {
            i += 1;
        }
    }
    out
}

fn is_num_start(c: u8) -> bool {
    c == b'+' || c == b'-' || c == b'.' || c.is_ascii_digit()
}

fn parse_color(raw: &str) -> Option<u32> {
    let s = raw.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("none") || s.eq_ignore_ascii_case("transparent") {
        return None;
    }
    if let Some(hex) = s.strip_prefix('#') {
        let wide = if hex.len() == 3 || hex.len() == 4 {
            let mut w = String::new();
            for c in hex.chars() {
                w.push(c);
                w.push(c);
            }
            w
        } else {
            hex.to_string()
        };
        if wide.len() >= 6 {
            let v = u32::from_str_radix(&wide[..6], 16).ok()?;
            return Some(v & 0x00FF_FFFF);
        }
        return None;
    }
    match s.to_ascii_lowercase().as_str() {
        "black" => Some(0x0000_0000),
        "white" => Some(0x00FF_FFFF),
        "red" => Some(0x00FF_0000),
        "green" => Some(0x0000_8000),
        "blue" => Some(0x0000_00FF),
        "yellow" => Some(0x00FF_FF00),
        "cyan" | "aqua" => Some(0x0000_FFFF),
        "magenta" | "fuchsia" => Some(0x00FF_00FF),
        _ => None,
    }
}
