//! Software `__ui` host. Hidden windows always succeed. A visible window uses
//! the Win32 desktop when one exists and throws `"cannot create window"` when
//! it does not, matching `HostUi`. Drawing, input queues, clipboard, and image
//! decode follow the Java host.

#[path = "font.rs"]
mod font;
#[path = "svg.rs"]
mod svg;
#[path = "win32.rs"]
mod win32;

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;
use std::rc::Rc;

use super::{FrameJob, Halt, Interp};
use crate::value::{FutureData, Value};

const MAX_SIZE: i32 = 16384;
const MAX_IMAGE: i32 = 8192;

struct Clip {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

struct RgbImage {
    w: i32,
    h: i32,
    px: Vec<u32>,
}

struct Win {
    alive: bool,
    mapped: bool,
    needs_frame: bool,
    width: i32,
    height: i32,
    title: String,
    fb: Vec<u32>,
    fb_w: i32,
    fb_h: i32,
    mouse_x: i32,
    mouse_y: i32,
    mouse_down: bool,
    mouse_click: bool,
    mouse_right_click: bool,
    cursor: i32,
    key_pending: bool,
    key_code: i32,
    key_text: String,
    scroll_pending: bool,
    scroll_dx: i32,
    scroll_dy: i32,
    wheel_acc: i32,
    hwnd: isize,
    clips: Vec<Clip>,
}

pub(super) struct HostUi {
    wins: BTreeMap<i64, Win>,
    frame_fns: BTreeMap<i64, Value>,
    images: HashMap<String, RgbImage>,
    next_id: i64,
    clipboard: String,
    in_frame: bool,
}

impl HostUi {
    pub(super) fn new() -> Self {
        Self {
            wins: BTreeMap::new(),
            frame_fns: BTreeMap::new(),
            images: HashMap::new(),
            next_id: 1,
            clipboard: String::new(),
            in_frame: false,
        }
    }

    pub(super) fn is_alive(&self, id: i64) -> bool {
        self.wins.get(&id).is_some_and(|w| w.alive)
    }

    fn alive_mut(&mut self, id: i64) -> Option<&mut Win> {
        let win = self.wins.get_mut(&id)?;
        if win.alive {
            Some(win)
        } else {
            None
        }
    }
}

impl Interp {
    pub(super) fn call_ui(
        &mut self,
        name: &str,
        args: &[Value],
        line: u32,
        col: u32,
    ) -> Result<Value, Halt> {
        match name {
            "backend" => {
                self.ui_arity(name, args, 0, line, col)?;
                Ok(Value::String(backend_name()))
            }
            "platform" => {
                self.ui_arity(name, args, 0, line, col)?;
                Ok(Value::String(platform_name()))
            }
            "open" => self.ui_open(args, line, col),
            "close" => {
                self.ui_arity(name, args, 1, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                if self.ui.is_alive(id) {
                    self.ui.frame_fns.remove(&id);
                    let hwnd = self.ui.wins.get(&id).map(|w| w.hwnd).unwrap_or(0);
                    if let Some(win) = self.ui.wins.get_mut(&id) {
                        win.alive = false;
                        win.hwnd = 0;
                    }
                    win32::destroy(hwnd);
                    self.ui_native_poll();
                }
                Ok(Value::Void)
            }
            "alive" => {
                self.ui_arity(name, args, 1, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                Ok(Value::Bool(self.ui.is_alive(id)))
            }
            "show" => {
                self.ui_arity(name, args, 1, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                if let Some(win) = self.ui.alive_mut(id) {
                    win.mapped = true;
                    let hwnd = win.hwnd;
                    win32::show(hwnd, true);
                }
                Ok(Value::Void)
            }
            "hide" => {
                self.ui_arity(name, args, 1, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                if let Some(win) = self.ui.alive_mut(id) {
                    win.mapped = false;
                    let hwnd = win.hwnd;
                    win32::show(hwnd, false);
                }
                Ok(Value::Void)
            }
            "title" => {
                self.ui_arity(name, args, 1, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let title = self
                    .ui
                    .wins
                    .get(&id)
                    .filter(|w| w.alive)
                    .map(|w| w.title.clone())
                    .unwrap_or_default();
                Ok(Value::String(title))
            }
            "set_title" => {
                self.ui_arity(name, args, 2, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let title = self.ui_str(name, args, 1, line, col)?;
                if let Some(win) = self.ui.alive_mut(id) {
                    win.title = if title.is_empty() {
                        "RoseGold".to_string()
                    } else {
                        title
                    };
                    let hwnd = win.hwnd;
                    let title = win.title.clone();
                    win32::set_title(hwnd, &title);
                }
                Ok(Value::Void)
            }
            "width" => self.ui_dim(name, args, line, col, true),
            "height" => self.ui_dim(name, args, line, col, false),
            "set_size" => {
                self.ui_arity(name, args, 3, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let w = self.ui_int(name, args, 1, line, col)? as i32;
                let h = self.ui_int(name, args, 2, line, col)? as i32;
                if w < 1 || h < 1 || w > MAX_SIZE || h > MAX_SIZE {
                    return Err(self.runtime("window size must be between 1 and 16384", line, col));
                }
                if let Some(win) = self.ui.alive_mut(id) {
                    win.width = w;
                    win.height = h;
                    let hwnd = win.hwnd;
                    ensure_fb(win);
                    win32::set_client_size(hwnd, w, h);
                }
                Ok(Value::Void)
            }
            "clear" => {
                self.ui_arity(name, args, 2, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let color = self.ui_int(name, args, 1, line, col)?;
                if let Some(win) = self.ui.alive_mut(id) {
                    fb_clear(win, color);
                }
                Ok(Value::Void)
            }
            "fill" => {
                self.ui_arity(name, args, 6, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let x = self.ui_int(name, args, 1, line, col)? as i32;
                let y = self.ui_int(name, args, 2, line, col)? as i32;
                let w = self.ui_int(name, args, 3, line, col)? as i32;
                let h = self.ui_int(name, args, 4, line, col)? as i32;
                let color = self.ui_int(name, args, 5, line, col)?;
                if let Some(win) = self.ui.alive_mut(id) {
                    fb_fill(win, x, y, w, h, color);
                }
                Ok(Value::Void)
            }
            "line" => {
                self.ui_arity(name, args, 6, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let x0 = self.ui_int(name, args, 1, line, col)? as i32;
                let y0 = self.ui_int(name, args, 2, line, col)? as i32;
                let x1 = self.ui_int(name, args, 3, line, col)? as i32;
                let y1 = self.ui_int(name, args, 4, line, col)? as i32;
                let color = self.ui_int(name, args, 5, line, col)?;
                if let Some(win) = self.ui.alive_mut(id) {
                    fb_line(win, x0, y0, x1, y1, color);
                }
                Ok(Value::Void)
            }
            "stroke_rect" => {
                self.ui_arity(name, args, 6, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let x = self.ui_int(name, args, 1, line, col)? as i32;
                let y = self.ui_int(name, args, 2, line, col)? as i32;
                let w = self.ui_int(name, args, 3, line, col)? as i32;
                let h = self.ui_int(name, args, 4, line, col)? as i32;
                let color = self.ui_int(name, args, 5, line, col)?;
                if let Some(win) = self.ui.alive_mut(id) {
                    fb_stroke_rect(win, x, y, w, h, color);
                }
                Ok(Value::Void)
            }
            "fill_round" => {
                self.ui_arity(name, args, 7, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let x = self.ui_int(name, args, 1, line, col)? as i32;
                let y = self.ui_int(name, args, 2, line, col)? as i32;
                let w = self.ui_int(name, args, 3, line, col)? as i32;
                let h = self.ui_int(name, args, 4, line, col)? as i32;
                let radius = self.ui_int(name, args, 5, line, col)? as i32;
                let color = self.ui_int(name, args, 6, line, col)?;
                if let Some(win) = self.ui.alive_mut(id) {
                    fb_fill_round(win, x, y, w, h, radius, color);
                }
                Ok(Value::Void)
            }
            "stroke_round" => {
                self.ui_arity(name, args, 7, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let x = self.ui_int(name, args, 1, line, col)? as i32;
                let y = self.ui_int(name, args, 2, line, col)? as i32;
                let w = self.ui_int(name, args, 3, line, col)? as i32;
                let h = self.ui_int(name, args, 4, line, col)? as i32;
                let radius = self.ui_int(name, args, 5, line, col)? as i32;
                let color = self.ui_int(name, args, 6, line, col)?;
                if let Some(win) = self.ui.alive_mut(id) {
                    fb_stroke_round(win, x, y, w, h, radius, color);
                }
                Ok(Value::Void)
            }
            "image_rgb" => self.ui_image_rgb(args, line, col),
            "image" => {
                self.ui_arity(name, args, 4, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let x = self.ui_int(name, args, 1, line, col)? as i32;
                let y = self.ui_int(name, args, 2, line, col)? as i32;
                let path = self.ui_str(name, args, 3, line, col)?;
                let file = self.sandbox_path(&path, line, col)?;
                let copied = self
                    .ui_cached_image(&file)
                    .map(|img| (img.w, img.h, img.px.clone()));
                if let Some((iw, ih, px)) = copied {
                    if let Some(win) = self.ui.alive_mut(id) {
                        fb_blit(win, x, y, iw, ih, &px);
                    }
                }
                Ok(Value::Void)
            }
            "image_width" => self.ui_image_size(name, args, line, col, true),
            "image_height" => self.ui_image_size(name, args, line, col, false),
            "clip_push" => {
                self.ui_arity(name, args, 5, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let x = self.ui_int(name, args, 1, line, col)? as i32;
                let y = self.ui_int(name, args, 2, line, col)? as i32;
                let w = self.ui_int(name, args, 3, line, col)? as i32;
                let h = self.ui_int(name, args, 4, line, col)? as i32;
                if let Some(win) = self.ui.alive_mut(id) {
                    let mut c = Clip {
                        x0: x,
                        y0: y,
                        x1: x + w,
                        y1: y + h,
                    };
                    if let Some(p) = win.clips.last() {
                        c.x0 = c.x0.max(p.x0);
                        c.y0 = c.y0.max(p.y0);
                        c.x1 = c.x1.min(p.x1);
                        c.y1 = c.y1.min(p.y1);
                    }
                    win.clips.push(c);
                }
                Ok(Value::Void)
            }
            "clip_pop" => {
                self.ui_arity(name, args, 1, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                if let Some(win) = self.ui.alive_mut(id) {
                    win.clips.pop();
                }
                Ok(Value::Void)
            }
            "text" => {
                self.ui_arity(name, args, 5, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let x = self.ui_int(name, args, 1, line, col)? as i32;
                let y = self.ui_int(name, args, 2, line, col)? as i32;
                let s = self.ui_str(name, args, 3, line, col)?;
                let color = self.ui_int(name, args, 4, line, col)?;
                if let Some(win) = self.ui.alive_mut(id) {
                    fb_text(win, x, y, &s, color);
                }
                Ok(Value::Void)
            }
            "text_width" => {
                self.ui_arity(name, args, 1, line, col)?;
                let s = self.ui_str(name, args, 0, line, col)?;
                Ok(Value::Int(text_width(&s) as i64))
            }
            "font_height" => {
                self.ui_arity(name, args, 0, line, col)?;
                Ok(Value::Int(font_height() as i64))
            }
            "present" | "cursor" => {
                let n = if name == "present" { 1 } else { 2 };
                self.ui_arity(name, args, n, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                if name == "cursor" {
                    let kind = self.ui_int(name, args, 1, line, col)? as i32;
                    if let Some(win) = self.ui.alive_mut(id) {
                        win.cursor = if !(0..=2).contains(&kind) { 0 } else { kind };
                        let hwnd = win.hwnd;
                        let cursor = win.cursor;
                        win32::set_cursor(hwnd, cursor);
                    }
                } else if let Some(win) = self.ui.wins.get(&id).filter(|w| w.alive) {
                    let hwnd = win.hwnd;
                    let w = win.fb_w;
                    let h = win.fb_h;
                    let fb = win.fb.clone();
                    win32::present(hwnd, &fb, w, h);
                }
                Ok(Value::Void)
            }
            "wait" => {
                self.ui_arity(name, args, 0, line, col)?;
                Ok(Value::Void)
            }
            "run" => {
                self.ui_arity(name, args, 0, line, col)?;
                self.ui_native_run();
                Ok(Value::Void)
            }
            "mouse_x" => self.ui_mouse_i(name, args, line, col, |w| w.mouse_x),
            "mouse_y" => self.ui_mouse_i(name, args, line, col, |w| w.mouse_y),
            "mouse_down" => {
                self.ui_arity(name, args, 1, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let down = self
                    .ui
                    .wins
                    .get(&id)
                    .is_some_and(|w| w.alive && w.mouse_down);
                Ok(Value::Bool(down))
            }
            "take_click" => self.ui_take_flag(name, args, line, col, true),
            "take_right_click" => self.ui_take_flag(name, args, line, col, false),
            "feed_click" => self.ui_feed_click(name, args, line, col, false),
            "feed_right_click" => self.ui_feed_click(name, args, line, col, true),
            "feed_mouse" => {
                self.ui_arity(name, args, 3, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let x = self.ui_int(name, args, 1, line, col)? as i32;
                let y = self.ui_int(name, args, 2, line, col)? as i32;
                if let Some(win) = self.ui.alive_mut(id) {
                    win.mouse_x = x;
                    win.mouse_y = y;
                }
                Ok(Value::Void)
            }
            "feed_down" => {
                self.ui_arity(name, args, 2, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let down = self.ui_bool(name, args, 1, line, col)?;
                if let Some(win) = self.ui.alive_mut(id) {
                    win.mouse_down = down;
                    if !win.mouse_down {
                        win.mouse_click = false;
                    }
                }
                Ok(Value::Void)
            }
            "take_key" => {
                self.ui_arity(name, args, 1, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let Some(win) = self.ui.alive_mut(id) else {
                    return Ok(Value::Bool(false));
                };
                let pending = win.key_pending;
                win.key_pending = false;
                Ok(Value::Bool(pending))
            }
            "key_code" => {
                self.ui_arity(name, args, 1, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let code = self
                    .ui
                    .wins
                    .get(&id)
                    .filter(|w| w.alive)
                    .map(|w| w.key_code)
                    .unwrap_or(0);
                Ok(Value::Int(code as i64))
            }
            "key_text" => {
                self.ui_arity(name, args, 1, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let text = self
                    .ui
                    .wins
                    .get(&id)
                    .filter(|w| w.alive)
                    .map(|w| w.key_text.clone())
                    .unwrap_or_default();
                Ok(Value::String(text))
            }
            "feed_key" => {
                self.ui_arity(name, args, 3, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let code = self.ui_int(name, args, 1, line, col)? as i32;
                let text = self.ui_str(name, args, 2, line, col)?;
                if let Some(win) = self.ui.alive_mut(id) {
                    win.key_code = code;
                    win.key_text = text;
                    win.key_pending = true;
                }
                Ok(Value::Void)
            }
            "take_scroll" => {
                self.ui_arity(name, args, 1, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let Some(win) = self.ui.alive_mut(id) else {
                    return Ok(Value::Bool(false));
                };
                let pending = win.scroll_pending;
                win.scroll_pending = false;
                Ok(Value::Bool(pending))
            }
            "scroll_dx" => self.ui_scroll_i(name, args, line, col, true),
            "scroll_dy" => self.ui_scroll_i(name, args, line, col, false),
            "feed_scroll" => {
                self.ui_arity(name, args, 3, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let dx = self.ui_int(name, args, 1, line, col)? as i32;
                let dy = self.ui_int(name, args, 2, line, col)? as i32;
                if let Some(win) = self.ui.alive_mut(id) {
                    if !win.scroll_pending {
                        win.scroll_dx = 0;
                        win.scroll_dy = 0;
                    }
                    win.scroll_dx += dx;
                    win.scroll_dy += dy;
                    win.scroll_pending = true;
                }
                Ok(Value::Void)
            }
            "set_frame" => {
                self.ui_arity(name, args, 2, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                if !matches!(args[1], Value::FnRef { .. }) {
                    return Err(self.runtime("__ui.set_frame expects a function", line, col));
                }
                if self.ui.is_alive(id) {
                    self.ui.frame_fns.insert(id, args[1].clone());
                }
                Ok(Value::Void)
            }
            "clipboard_get" => {
                self.ui_arity(name, args, 0, line, col)?;
                Ok(Value::String(self.ui.clipboard.clone()))
            }
            "clipboard_set" => {
                self.ui_arity(name, args, 1, line, col)?;
                self.ui.clipboard = self.ui_str(name, args, 0, line, col)?;
                Ok(Value::Void)
            }
            "poll" => {
                self.ui_arity(name, args, 1, line, col)?;
                self.ui_native_poll();
                let id = self.ui_int(name, args, 0, line, col)?;
                Ok(Value::Bool(self.ui.is_alive(id)))
            }
            "next_frame" => {
                self.ui_arity(name, args, 1, line, col)?;
                let id = self.ui_int(name, args, 0, line, col)?;
                let fut = Rc::new(RefCell::new(FutureData::pending()));
                self.frame_jobs.push(FrameJob {
                    win_id: id,
                    fut: fut.clone(),
                });
                Ok(Value::Future(fut))
            }
            "count" => {
                self.ui_arity(name, args, 0, line, col)?;
                let n = self.ui.wins.values().filter(|w| w.alive).count() as i64;
                Ok(Value::Int(n))
            }
            _ => Err(self.runtime(&format!("unknown function __ui.{name}"), line, col)),
        }
    }

    fn ui_open(&mut self, args: &[Value], line: u32, col: u32) -> Result<Value, Halt> {
        if args.len() != 4 {
            return Err(self.runtime("__ui.open takes 4 arguments", line, col));
        }
        let mut title = self.ui_str("open", args, 0, line, col)?;
        let w = self.ui_int("open", args, 1, line, col)?;
        let h = self.ui_int("open", args, 2, line, col)?;
        let visible = self.ui_bool("open", args, 3, line, col)?;
        if w < 1 || h < 1 || w > MAX_SIZE as i64 || h > MAX_SIZE as i64 {
            return Err(self.ui_throw("window size must be between 1 and 16384", line, col));
        }
        if title.is_empty() {
            title = "RoseGold".to_string();
        }
        let id = self.ui.next_id;
        self.ui.next_id += 1;
        let hwnd = if visible {
            win32::open(id, &title, w as i32, h as i32, true).unwrap_or(0)
        } else {
            0
        };
        self.ui.wins.insert(
            id,
            Win {
                alive: true,
                mapped: visible,
                needs_frame: false,
                width: w as i32,
                height: h as i32,
                title,
                fb: Vec::new(),
                fb_w: 0,
                fb_h: 0,
                mouse_x: 0,
                mouse_y: 0,
                mouse_down: false,
                mouse_click: false,
                mouse_right_click: false,
                cursor: 0,
                key_pending: false,
                key_code: 0,
                key_text: String::new(),
                scroll_pending: false,
                scroll_dx: 0,
                scroll_dy: 0,
                wheel_acc: 0,
                hwnd,
                clips: Vec::new(),
            },
        );
        if visible && hwnd == 0 {
            self.ui.wins.remove(&id);
            return Err(self.ui_throw("cannot create window", line, col));
        }
        if let Some(win) = self.ui.wins.get_mut(&id) {
            ensure_fb(win);
        }
        self.ui_native_poll();
        Ok(Value::Int(id))
    }

    fn ui_image_rgb(&mut self, args: &[Value], line: u32, col: u32) -> Result<Value, Halt> {
        if args.len() != 6 {
            return Err(self.runtime("__ui.image_rgb takes 6 arguments", line, col));
        }
        let id = self.ui_int("image_rgb", args, 0, line, col)?;
        let x = self.ui_int("image_rgb", args, 1, line, col)? as i32;
        let y = self.ui_int("image_rgb", args, 2, line, col)? as i32;
        let iw = self.ui_int("image_rgb", args, 3, line, col)? as i32;
        let ih = self.ui_int("image_rgb", args, 4, line, col)? as i32;
        let Value::Array(items) = &args[5] else {
            return Err(self.runtime("__ui.image_rgb expects Array[Int] pixels", line, col));
        };
        let items = items.borrow().clone();
        if iw < 1 || ih < 1 || iw > MAX_IMAGE || ih > MAX_IMAGE {
            return Err(self.runtime("__ui.image_rgb size out of range", line, col));
        }
        let need = (iw as usize).saturating_mul(ih as usize);
        if items.len() < need {
            return Err(self.runtime("__ui.image_rgb pixel array too short", line, col));
        }
        if self.ui.is_alive(id) {
            let mut px = Vec::with_capacity(need);
            for item in items.iter().take(need) {
                let Value::Int(c) = item else {
                    return Err(self.runtime("__ui.image_rgb pixels must be Int", line, col));
                };
                px.push(pack_rgb(*c));
            }
            if let Some(win) = self.ui.alive_mut(id) {
                fb_blit(win, x, y, iw, ih, &px);
            }
        }
        Ok(Value::Void)
    }

    fn ui_image_size(
        &mut self,
        name: &str,
        args: &[Value],
        line: u32,
        col: u32,
        width: bool,
    ) -> Result<Value, Halt> {
        self.ui_arity(name, args, 1, line, col)?;
        let path = self.ui_str(name, args, 0, line, col)?;
        let file = self.sandbox_path(&path, line, col)?;
        let n = self
            .ui_cached_image(&file)
            .map(|img| if width { img.w } else { img.h })
            .unwrap_or(0);
        Ok(Value::Int(n as i64))
    }

    fn ui_cached_image(&mut self, path: &Path) -> Option<&RgbImage> {
        let key = path.to_string_lossy().to_string();
        if self.ui.images.contains_key(&key) {
            return self.ui.images.get(&key);
        }
        let decoded = decode_image(path)?;
        self.ui.images.insert(key.clone(), decoded);
        self.ui.images.get(&key)
    }

    fn ui_dim(
        &mut self,
        name: &str,
        args: &[Value],
        line: u32,
        col: u32,
        width: bool,
    ) -> Result<Value, Halt> {
        self.ui_arity(name, args, 1, line, col)?;
        let id = self.ui_int(name, args, 0, line, col)?;
        let n = self
            .ui
            .wins
            .get(&id)
            .filter(|w| w.alive)
            .map(|w| if width { w.width } else { w.height })
            .unwrap_or(0);
        Ok(Value::Int(n as i64))
    }

    fn ui_mouse_i(
        &mut self,
        name: &str,
        args: &[Value],
        line: u32,
        col: u32,
        pick: impl FnOnce(&Win) -> i32,
    ) -> Result<Value, Halt> {
        self.ui_arity(name, args, 1, line, col)?;
        let id = self.ui_int(name, args, 0, line, col)?;
        let n = self
            .ui
            .wins
            .get(&id)
            .filter(|w| w.alive)
            .map(pick)
            .unwrap_or(0);
        Ok(Value::Int(n as i64))
    }

    fn ui_scroll_i(
        &mut self,
        name: &str,
        args: &[Value],
        line: u32,
        col: u32,
        dx: bool,
    ) -> Result<Value, Halt> {
        self.ui_arity(name, args, 1, line, col)?;
        let id = self.ui_int(name, args, 0, line, col)?;
        let n = self
            .ui
            .wins
            .get(&id)
            .filter(|w| w.alive)
            .map(|w| if dx { w.scroll_dx } else { w.scroll_dy })
            .unwrap_or(0);
        Ok(Value::Int(n as i64))
    }

    fn ui_take_flag(
        &mut self,
        name: &str,
        args: &[Value],
        line: u32,
        col: u32,
        left: bool,
    ) -> Result<Value, Halt> {
        self.ui_arity(name, args, 1, line, col)?;
        let id = self.ui_int(name, args, 0, line, col)?;
        let Some(win) = self.ui.alive_mut(id) else {
            return Ok(Value::Bool(false));
        };
        let click = if left {
            let c = win.mouse_click;
            win.mouse_click = false;
            c
        } else {
            let c = win.mouse_right_click;
            win.mouse_right_click = false;
            c
        };
        Ok(Value::Bool(click))
    }

    fn ui_feed_click(
        &mut self,
        name: &str,
        args: &[Value],
        line: u32,
        col: u32,
        right: bool,
    ) -> Result<Value, Halt> {
        self.ui_arity(name, args, 3, line, col)?;
        let id = self.ui_int(name, args, 0, line, col)?;
        let x = self.ui_int(name, args, 1, line, col)? as i32;
        let y = self.ui_int(name, args, 2, line, col)? as i32;
        if let Some(win) = self.ui.alive_mut(id) {
            win.mouse_x = x;
            win.mouse_y = y;
            win.mouse_down = false;
            if right {
                win.mouse_right_click = true;
            } else {
                win.mouse_click = true;
            }
        }
        Ok(Value::Void)
    }

    fn ui_native_run(&mut self) {
        let started = std::time::Instant::now();
        let mut next_tick = started;
        loop {
            let alive_native = self.ui.wins.values().any(|w| w.alive && w.hwnd != 0);
            if !alive_native {
                self.ui_native_poll();
                return;
            }
            if next_tick <= std::time::Instant::now() {
                for win in self.ui.wins.values_mut() {
                    if win.alive && win.hwnd != 0 {
                        win.needs_frame = true;
                    }
                }
                next_tick = std::time::Instant::now() + std::time::Duration::from_millis(500);
            }
            self.ui_native_poll();
            if !self.ui.wins.values().any(|w| w.alive && w.hwnd != 0) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
    }

    fn ui_apply_native(&mut self, events: Vec<win32::NativeEvent>) {
        for event in events {
            match event {
                win32::NativeEvent::Mouse { id, x, y, kind } => {
                    if let Some(win) = self.ui.alive_mut(id) {
                        win.mouse_x = x;
                        win.mouse_y = y;
                        match kind {
                            win32::MouseKind::LeftDown => win.mouse_down = true,
                            win32::MouseKind::LeftUp => {
                                win.mouse_down = false;
                                win.mouse_click = true;
                            }
                            win32::MouseKind::RightUp => win.mouse_right_click = true,
                            win32::MouseKind::Move => {}
                        }
                        win.needs_frame = true;
                    }
                }
                win32::NativeEvent::Key { id, code, text } => {
                    if let Some(win) = self.ui.alive_mut(id) {
                        win.key_code = code;
                        win.key_text = text;
                        win.key_pending = true;
                        win.needs_frame = true;
                    }
                }
                win32::NativeEvent::Wheel { id, x, y, delta } => {
                    if let Some(win) = self.ui.alive_mut(id) {
                        win.mouse_x = x;
                        win.mouse_y = y;
                        let px = wheel_pixels(win, delta);
                        if px != 0 {
                            if !win.scroll_pending {
                                win.scroll_dx = 0;
                                win.scroll_dy = 0;
                            }
                            win.scroll_dy += px;
                            win.scroll_pending = true;
                            win.needs_frame = true;
                        }
                    }
                }
                win32::NativeEvent::Resize { id, w, h } => {
                    if let Some(win) = self.ui.alive_mut(id) {
                        if w >= 1 && h >= 1 {
                            win.width = w.min(MAX_SIZE);
                            win.height = h.min(MAX_SIZE);
                            ensure_fb(win);
                            win.needs_frame = true;
                        }
                    }
                }
                win32::NativeEvent::Close { id } => {
                    if let Some(win) = self.ui.wins.get_mut(&id) {
                        win.alive = false;
                        win.hwnd = 0;
                    }
                    self.ui.frame_fns.remove(&id);
                }
            }
        }
    }

    fn ui_native_poll(&mut self) {
        if self.ui.in_frame {
            return;
        }
        let events = win32::pump();
        self.ui_apply_native(events);
        let due: Vec<i64> = self
            .ui
            .wins
            .iter_mut()
            .filter_map(|(id, win)| {
                if win.alive && win.needs_frame {
                    win.needs_frame = false;
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        for id in due {
            self.ui_run_frame(id);
        }
    }

    fn ui_run_frame(&mut self, id: i64) {
        if self.ui.in_frame || id <= 0 {
            return;
        }
        let Some(func) = self.ui.frame_fns.get(&id).cloned() else {
            return;
        };
        if !matches!(func, Value::FnRef { .. }) {
            return;
        }
        self.ui.in_frame = true;
        let _ = self.call_fn_value(func, Vec::new(), 1, 1);
        self.ui.in_frame = false;
    }

    fn ui_arity(
        &self,
        name: &str,
        args: &[Value],
        n: usize,
        line: u32,
        col: u32,
    ) -> Result<(), Halt> {
        if args.len() != n {
            let plural = if n == 1 { "" } else { "s" };
            return Err(self.runtime(
                &format!("__ui.{name} takes {n} argument{plural}"),
                line,
                col,
            ));
        }
        Ok(())
    }

    fn ui_int(
        &self,
        name: &str,
        args: &[Value],
        i: usize,
        line: u32,
        col: u32,
    ) -> Result<i64, Halt> {
        match args.get(i) {
            Some(Value::Int(n)) => Ok(*n),
            _ => Err(self.runtime(&format!("__ui.{name} expects Int"), line, col)),
        }
    }

    fn ui_str(
        &self,
        name: &str,
        args: &[Value],
        i: usize,
        line: u32,
        col: u32,
    ) -> Result<String, Halt> {
        match args.get(i) {
            Some(Value::String(s)) => Ok(s.clone()),
            _ => Err(self.runtime(&format!("__ui.{name} expects String"), line, col)),
        }
    }

    fn ui_bool(
        &self,
        name: &str,
        args: &[Value],
        i: usize,
        line: u32,
        col: u32,
    ) -> Result<bool, Halt> {
        match args.get(i) {
            Some(Value::Bool(b)) => Ok(*b),
            _ => Err(self.runtime(&format!("__ui.{name} expects Bool"), line, col)),
        }
    }

    fn ui_throw(&self, msg: &str, line: u32, col: u32) -> Halt {
        Halt::Throw {
            value: Value::String(msg.to_string()),
            line,
            col,
        }
    }
}

fn wheel_pixels(win: &mut Win, delta: i32) -> i32 {
    const UNIT: i32 = 120;
    const PIXELS: i32 = 48;
    win.wheel_acc += delta;
    let px = win.wheel_acc * PIXELS / UNIT;
    if px != 0 {
        win.wheel_acc -= px * UNIT / PIXELS;
    }
    px
}

fn backend_name() -> String {
    if cfg!(windows) {
        return "win32".to_string();
    }
    if cfg!(target_os = "macos") {
        return "cocoa".to_string();
    }
    if std::env::var("WAYLAND_DISPLAY")
        .ok()
        .is_some_and(|v| !v.is_empty())
    {
        return "wayland".to_string();
    }
    if std::env::var("DISPLAY").ok().is_some_and(|v| !v.is_empty()) {
        return "x11".to_string();
    }
    "none".to_string()
}

fn platform_name() -> String {
    if cfg!(windows) {
        "windows".to_string()
    } else if cfg!(target_os = "macos") {
        "macos".to_string()
    } else {
        "linux".to_string()
    }
}

fn text_width(s: &str) -> i32 {
    font::width(s)
}

fn font_height() -> i32 {
    font::height()
}

fn pack_rgb(color: i64) -> u32 {
    let r = ((color >> 16) & 255) as u32;
    let g = ((color >> 8) & 255) as u32;
    let b = (color & 255) as u32;
    b | (g << 8) | (r << 16)
}

fn ensure_fb(win: &mut Win) {
    let mut w = if win.width > 0 { win.width } else { 1 };
    let mut h = if win.height > 0 { win.height } else { 1 };
    if w > MAX_SIZE {
        w = MAX_SIZE;
    }
    if h > MAX_SIZE {
        h = MAX_SIZE;
    }
    let n = (w as usize).saturating_mul(h as usize);
    if win.fb_w == w && win.fb_h == h && win.fb.len() == n {
        return;
    }
    let fill = pack_rgb(0xF2F2F2);
    win.fb = vec![fill; n];
    win.fb_w = w;
    win.fb_h = h;
}

fn fb_clear(win: &mut Win, color: i64) {
    ensure_fb(win);
    win.fb.fill(pack_rgb(color));
}

fn clip_bounds(win: &Win, boxs: &mut [i32; 4]) {
    if boxs[0] < 0 {
        boxs[0] = 0;
    }
    if boxs[1] < 0 {
        boxs[1] = 0;
    }
    if boxs[2] > win.fb_w {
        boxs[2] = win.fb_w;
    }
    if boxs[3] > win.fb_h {
        boxs[3] = win.fb_h;
    }
    if let Some(c) = win.clips.last() {
        boxs[0] = boxs[0].max(c.x0);
        boxs[1] = boxs[1].max(c.y0);
        boxs[2] = boxs[2].min(c.x1);
        boxs[3] = boxs[3].min(c.y1);
    }
}

fn fb_fill(win: &mut Win, x: i32, y: i32, w: i32, h: i32, color: i64) {
    ensure_fb(win);
    if w < 1 || h < 1 {
        return;
    }
    let mut boxs = [x, y, x.saturating_add(w), y.saturating_add(h)];
    clip_bounds(win, &mut boxs);
    if boxs[0] >= boxs[2] || boxs[1] >= boxs[3] {
        return;
    }
    let p = pack_rgb(color);
    let fw = win.fb_w as usize;
    for row in boxs[1]..boxs[3] {
        let dest = (row as usize) * fw + boxs[0] as usize;
        let len = (boxs[2] - boxs[0]) as usize;
        if dest + len <= win.fb.len() {
            win.fb[dest..dest + len].fill(p);
        }
    }
}

fn fb_plot(win: &mut Win, x: i32, y: i32, p: u32) {
    if x < 0 || y < 0 || x >= win.fb_w || y >= win.fb_h {
        return;
    }
    if let Some(c) = win.clips.last() {
        if x < c.x0 || y < c.y0 || x >= c.x1 || y >= c.y1 {
            return;
        }
    }
    let i = (y as usize) * (win.fb_w as usize) + x as usize;
    if i < win.fb.len() {
        win.fb[i] = p;
    }
}

fn fb_plot_cover(win: &mut Win, x: i32, y: i32, src: u32, cover: i32) {
    if cover <= 0 || x < 0 || y < 0 || x >= win.fb_w || y >= win.fb_h {
        return;
    }
    if let Some(c) = win.clips.last() {
        if x < c.x0 || y < c.y0 || x >= c.x1 || y >= c.y1 {
            return;
        }
    }
    let i = (y as usize) * (win.fb_w as usize) + x as usize;
    if i >= win.fb.len() {
        return;
    }
    if cover >= 256 {
        win.fb[i] = src;
        return;
    }
    let dst = win.fb[i];
    let inv = 256 - cover;
    let src = src as i32;
    let dst = dst as i32;
    let r = ((((src >> 16) & 255) * cover) + (((dst >> 16) & 255) * inv)) >> 8;
    let g = ((((src >> 8) & 255) * cover) + (((dst >> 8) & 255) * inv)) >> 8;
    let b = (((src & 255) * cover) + ((dst & 255) * inv)) >> 8;
    win.fb[i] = (b as u32) | ((g as u32) << 8) | ((r as u32) << 16);
}

fn fb_line(win: &mut Win, x0: i32, y0: i32, x1: i32, y1: i32, color: i64) {
    ensure_fb(win);
    let p = pack_rgb(color);
    let dx = x1.wrapping_sub(x0);
    let dy = y1.wrapping_sub(y0);
    let steps = iabs(dx).max(iabs(dy));
    if steps == 0 {
        fb_plot(win, x0, y0, p);
        return;
    }
    for i in 0..=steps {
        let x = x0 as i64 + (dx as i64) * (i as i64) / (steps as i64);
        let y = y0 as i64 + (dy as i64) * (i as i64) / (steps as i64);
        fb_plot(win, x as i32, y as i32, p);
    }
}

fn iabs(v: i32) -> i32 {
    if v == i32::MIN {
        i32::MAX
    } else {
        v.abs()
    }
}

fn fb_stroke_rect(win: &mut Win, x: i32, y: i32, w: i32, h: i32, color: i64) {
    if w < 1 || h < 1 {
        return;
    }
    fb_fill(win, x, y, w, 1, color);
    fb_fill(win, x, y + h - 1, w, 1, color);
    fb_fill(win, x, y, 1, h, color);
    fb_fill(win, x + w - 1, y, 1, h, color);
}

fn clamp_round_radius(w: i32, h: i32, radius: i32) -> i32 {
    let r = radius.max(0);
    let lim = w.min(h) / 2;
    r.min(lim)
}

fn sd_round_box(px: f32, py: f32, hw: f32, hh: f32, radius: f32) -> f32 {
    let max_r = hw.min(hh);
    let r = radius.max(0.0).min(max_r);
    let ax = px.abs();
    let ay = py.abs();
    let qx = ax - hw + r;
    let qy = ay - hh + r;
    let mx = qx.max(0.0);
    let my = qy.max(0.0);
    let outside = (mx * mx + my * my).sqrt();
    let inside = qx.max(qy).min(0.0);
    outside + inside - r
}

fn fb_fill_round(win: &mut Win, x: i32, y: i32, w: i32, h: i32, radius: i32, color: i64) {
    ensure_fb(win);
    if w < 1 || h < 1 {
        return;
    }
    let r = clamp_round_radius(w, h, radius);
    if r == 0 {
        fb_fill(win, x, y, w, h, color);
        return;
    }
    let p = pack_rgb(color);
    let cx = x as f32 + w as f32 * 0.5;
    let cy = y as f32 + h as f32 * 0.5;
    let hw = w as f32 * 0.5;
    let hh = h as f32 * 0.5;
    let mut boxs = [x, y, x + w, y + h];
    clip_bounds(win, &mut boxs);
    for row in boxs[1]..boxs[3] {
        for col in boxs[0]..boxs[2] {
            let sd = sd_round_box(
                col as f32 + 0.5 - cx,
                row as f32 + 0.5 - cy,
                hw,
                hh,
                r as f32,
            );
            let cov = 0.5 - sd;
            if cov <= 0.0 {
                continue;
            }
            if cov >= 1.0 {
                fb_plot(win, col, row, p);
            } else {
                fb_plot_cover(win, col, row, p, (cov * 256.0) as i32);
            }
        }
    }
}

fn fb_stroke_round(win: &mut Win, x: i32, y: i32, w: i32, h: i32, radius: i32, color: i64) {
    ensure_fb(win);
    if w < 1 || h < 1 {
        return;
    }
    let r = clamp_round_radius(w, h, radius);
    if r == 0 {
        fb_stroke_rect(win, x, y, w, h, color);
        return;
    }
    let p = pack_rgb(color);
    let cx = x as f32 + w as f32 * 0.5;
    let cy = y as f32 + h as f32 * 0.5;
    let hw = w as f32 * 0.5 - 0.5;
    let hh = h as f32 * 0.5 - 0.5;
    if hw < 1.0 || hh < 1.0 {
        fb_stroke_rect(win, x, y, w, h, color);
        return;
    }
    let rf = (r as f32 - 0.5).max(0.0);
    let mut boxs = [x, y, x + w, y + h];
    clip_bounds(win, &mut boxs);
    for row in boxs[1]..boxs[3] {
        for col in boxs[0]..boxs[2] {
            let sd = sd_round_box(col as f32 + 0.5 - cx, row as f32 + 0.5 - cy, hw, hh, rf);
            let d = sd.abs();
            let cov = 1.0 - (d - 0.25) / 0.75;
            if cov <= 0.0 {
                continue;
            }
            if cov >= 1.0 {
                fb_plot(win, col, row, p);
            } else {
                fb_plot_cover(win, col, row, p, (cov * 256.0) as i32);
            }
        }
    }
}

fn fb_blit(win: &mut Win, x: i32, y: i32, iw: i32, ih: i32, src: &[u32]) {
    ensure_fb(win);
    if src.is_empty() || iw < 1 || ih < 1 {
        return;
    }
    let need = (iw as usize).saturating_mul(ih as usize);
    if src.len() < need {
        return;
    }
    for row in 0..ih {
        for col in 0..iw {
            let p = src[(row as usize) * (iw as usize) + col as usize];
            fb_plot(win, x + col, y + row, p);
        }
    }
}

fn fb_text(win: &mut Win, x: i32, y: i32, s: &str, color: i64) {
    ensure_fb(win);
    if s.is_empty() {
        return;
    }
    let rgb = pack_rgb(color);
    font::draw(s, x, y, rgb, |px, py, p| fb_plot(win, px, py, p));
}

fn decode_image(path: &Path) -> Option<RgbImage> {
    if !path.is_file() {
        return None;
    }
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if name.ends_with(".svg") {
        let xml = fs::read_to_string(path).ok()?;
        let img = svg::rasterize(&xml)?;
        if img.w < 1 || img.h < 1 || img.w > MAX_IMAGE || img.h > MAX_IMAGE {
            return None;
        }
        return Some(RgbImage {
            w: img.w,
            h: img.h,
            px: img.px,
        });
    }
    #[cfg(windows)]
    if let Some(img) = gdiplus_load(path) {
        return Some(img);
    }
    let bytes = fs::read(path).ok()?;
    let (w, h) = header_size(&bytes, &name)?;
    if w < 1 || h < 1 || w > MAX_IMAGE || h > MAX_IMAGE {
        return None;
    }
    let px = manual_pixels(&bytes, &name, w, h)
        .unwrap_or_else(|| vec![0x00FF_FFFF; (w as usize) * (h as usize)]);
    Some(RgbImage { w, h, px })
}

fn header_size(bytes: &[u8], name: &str) -> Option<(i32, i32)> {
    if name.ends_with(".png") || bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        if bytes.len() < 24 {
            return None;
        }
        let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?) as i32;
        let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?) as i32;
        return Some((w, h));
    }
    if name.ends_with(".bmp") || bytes.starts_with(b"BM") {
        if bytes.len() < 26 {
            return None;
        }
        let w = i32::from_le_bytes(bytes[18..22].try_into().ok()?);
        let h = i32::from_le_bytes(bytes[22..26].try_into().ok()?).abs();
        return Some((w, h));
    }
    if name.ends_with(".jpg") || name.ends_with(".jpeg") || bytes.starts_with(b"\xFF\xD8") {
        return jpeg_size(bytes);
    }
    None
}

fn jpeg_size(bytes: &[u8]) -> Option<(i32, i32)> {
    let mut i = 2usize;
    while i + 8 < bytes.len() {
        if bytes[i] != 0xFF {
            i += 1;
            continue;
        }
        while i < bytes.len() && bytes[i] == 0xFF {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let marker = bytes[i];
        i += 1;
        if marker == 0xD8 || marker == 0xD9 || (0xD0..=0xD7).contains(&marker) {
            continue;
        }
        if i + 1 >= bytes.len() {
            break;
        }
        let len = u16::from_be_bytes([bytes[i], bytes[i + 1]]) as usize;
        if marker == 0xC0 || marker == 0xC1 || marker == 0xC2 {
            if i + 7 >= bytes.len() {
                return None;
            }
            let h = u16::from_be_bytes([bytes[i + 3], bytes[i + 4]]) as i32;
            let w = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as i32;
            return Some((w, h));
        }
        if len < 2 || i + len > bytes.len() {
            break;
        }
        i += len;
    }
    None
}

fn manual_pixels(bytes: &[u8], name: &str, w: i32, h: i32) -> Option<Vec<u32>> {
    if !(name.ends_with(".bmp") || bytes.starts_with(b"BM")) {
        return None;
    }
    decode_bmp(bytes, w, h)
}

fn decode_bmp(bytes: &[u8], w: i32, h: i32) -> Option<Vec<u32>> {
    if bytes.len() < 54 || w < 1 || h < 1 {
        return None;
    }
    let offset = u32::from_le_bytes(bytes[10..14].try_into().ok()?) as usize;
    let bpp = u16::from_le_bytes(bytes[28..30].try_into().ok()?);
    let height_raw = i32::from_le_bytes(bytes[22..26].try_into().ok()?);
    let top_down = height_raw < 0;
    let n = (w as usize) * (h as usize);
    let mut px = vec![0u32; n];
    if bpp == 24 {
        let stride = ((w as usize) * 3 + 3) & !3;
        for row in 0..h as usize {
            let src_row = if top_down { row } else { h as usize - 1 - row };
            let start = offset + src_row * stride;
            for col in 0..w as usize {
                let i = start + col * 3;
                if i + 2 >= bytes.len() {
                    return None;
                }
                let b = bytes[i] as u32;
                let g = bytes[i + 1] as u32;
                let r = bytes[i + 2] as u32;
                px[row * (w as usize) + col] = b | (g << 8) | (r << 16);
            }
        }
        return Some(px);
    }
    if bpp == 32 {
        let stride = (w as usize) * 4;
        for row in 0..h as usize {
            let src_row = if top_down { row } else { h as usize - 1 - row };
            let start = offset + src_row * stride;
            for col in 0..w as usize {
                let i = start + col * 4;
                if i + 2 >= bytes.len() {
                    return None;
                }
                let b = bytes[i] as u32;
                let g = bytes[i + 1] as u32;
                let r = bytes[i + 2] as u32;
                px[row * (w as usize) + col] = b | (g << 8) | (r << 16);
            }
        }
        return Some(px);
    }
    None
}

#[cfg(windows)]
fn gdiplus_load(path: &Path) -> Option<RgbImage> {
    use std::os::windows::ffi::OsStrExt;
    let token = gdiplus_token()?;
    let _ = token;
    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut image = std::ptr::null_mut();
    let status = unsafe { GdipLoadImageFromFile(wide.as_ptr(), &mut image) };
    if status != 0 || image.is_null() {
        return None;
    }
    let mut w = 0u32;
    let mut h = 0u32;
    let ok =
        unsafe { GdipGetImageWidth(image, &mut w) == 0 && GdipGetImageHeight(image, &mut h) == 0 };
    if !ok {
        unsafe { GdipDisposeImage(image) };
        return None;
    }
    let w = w as i32;
    let h = h as i32;
    if w < 1 || h < 1 || w > MAX_IMAGE || h > MAX_IMAGE {
        unsafe { GdipDisposeImage(image) };
        return None;
    }
    let mut px = Vec::with_capacity((w as usize) * (h as usize));
    for y in 0..h {
        for x in 0..w {
            let mut color = 0u32;
            if unsafe { GdipBitmapGetPixel(image, x, y, &mut color) } != 0 {
                unsafe { GdipDisposeImage(image) };
                return None;
            }
            px.push(color & 0x00FF_FFFF);
        }
    }
    unsafe { GdipDisposeImage(image) };
    Some(RgbImage { w, h, px })
}

#[cfg(windows)]
fn gdiplus_token() -> Option<usize> {
    use std::sync::OnceLock;
    static TOKEN: OnceLock<usize> = OnceLock::new();
    if let Some(token) = TOKEN.get() {
        return Some(*token);
    }
    let input = GdiplusStartupInput {
        version: 1,
        debug: std::ptr::null_mut(),
        suppress_bg: 0,
        suppress_ext: 0,
    };
    let mut token = 0usize;
    let status = unsafe { GdiplusStartup(&mut token, &input, std::ptr::null_mut()) };
    if status != 0 {
        return None;
    }
    let _ = TOKEN.set(token);
    Some(token)
}

#[cfg(windows)]
#[repr(C)]
struct GdiplusStartupInput {
    version: u32,
    debug: *mut std::ffi::c_void,
    suppress_bg: i32,
    suppress_ext: i32,
}

#[cfg(windows)]
#[link(name = "gdiplus")]
extern "system" {
    fn GdiplusStartup(
        token: *mut usize,
        input: *const GdiplusStartupInput,
        output: *mut std::ffi::c_void,
    ) -> i32;
    fn GdipLoadImageFromFile(name: *const u16, image: *mut *mut std::ffi::c_void) -> i32;
    fn GdipGetImageWidth(image: *mut std::ffi::c_void, w: *mut u32) -> i32;
    fn GdipGetImageHeight(image: *mut std::ffi::c_void, h: *mut u32) -> i32;
    fn GdipBitmapGetPixel(image: *mut std::ffi::c_void, x: i32, y: i32, color: *mut u32) -> i32;
    fn GdipDisposeImage(image: *mut std::ffi::c_void) -> i32;
}
