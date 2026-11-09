//! Win32 window for `__ui`, matching the Swing host in `HostUi` when a desktop exists.
//! Hidden windows stay invisible. `run` pumps until every native window is closed.

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::Once;

#[derive(Clone)]
pub(super) enum NativeEvent {
    Mouse {
        id: i64,
        x: i32,
        y: i32,
        kind: MouseKind,
    },
    Key {
        id: i64,
        code: i32,
        text: String,
    },
    Wheel {
        id: i64,
        x: i32,
        y: i32,
        delta: i32,
    },
    Resize {
        id: i64,
        w: i32,
        h: i32,
    },
    Close {
        id: i64,
    },
}

#[derive(Clone, Copy)]
pub(super) enum MouseKind {
    Move,
    LeftDown,
    LeftUp,
    RightUp,
}

pub(super) fn open(id: i64, title: &str, w: i32, h: i32, visible: bool) -> Option<isize> {
    if !cfg!(windows) {
        return None;
    }
    PENDING.with(|slot| slot.set(id));
    let hwnd = unsafe { create_window(title, w, h, visible) };
    PENDING.with(|slot| slot.set(0));
    hwnd
}

pub(super) fn destroy(hwnd: isize) {
    if hwnd != 0 && cfg!(windows) {
        unsafe {
            DestroyWindow(hwnd);
        }
    }
}

pub(super) fn show(hwnd: isize, visible: bool) {
    if hwnd == 0 || !cfg!(windows) {
        return;
    }
    unsafe {
        ShowWindow(hwnd, if visible { 5 } else { 0 });
        if visible {
            SetForegroundWindow(hwnd);
        }
    }
}

pub(super) fn set_title(hwnd: isize, title: &str) {
    if hwnd == 0 || !cfg!(windows) {
        return;
    }
    let wide = wide(title);
    unsafe {
        SetWindowTextW(hwnd, wide.as_ptr());
    }
}

pub(super) fn set_client_size(hwnd: isize, w: i32, h: i32) {
    if hwnd == 0 || !cfg!(windows) {
        return;
    }
    unsafe {
        let mut rect = Rect {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        let style = GetWindowLongPtrW(hwnd, -16) as u32;
        let ex = GetWindowLongPtrW(hwnd, -20) as u32;
        AdjustWindowRectEx(&mut rect, style, 0, ex);
        SetWindowPos(
            hwnd,
            0,
            0,
            0,
            rect.right - rect.left,
            rect.bottom - rect.top,
            0x0002 | 0x0004 | 0x0010,
        );
    }
}

pub(super) fn present(hwnd: isize, fb: &[u32], w: i32, h: i32) {
    if hwnd == 0 || !cfg!(windows) || w < 1 || h < 1 || fb.len() < (w as usize) * (h as usize) {
        return;
    }
    unsafe {
        blit(hwnd, fb, w, h);
    }
}

pub(super) fn set_cursor(hwnd: isize, kind: i32) {
    if hwnd == 0 || !cfg!(windows) {
        return;
    }
    let id = match kind {
        1 => 32649isize, // IDC_HAND
        2 => 32513isize, // IDC_IBEAM
        _ => 32512isize, // IDC_ARROW
    };
    unsafe {
        let cursor = LoadCursorW(0, id);
        if cursor != 0 {
            SetClassLongPtrW(hwnd, -12, cursor);
            SetCursor(cursor);
        }
    }
}

pub(super) fn pump() -> Vec<NativeEvent> {
    if !cfg!(windows) {
        return Vec::new();
    }
    let mut out = Vec::new();
    unsafe {
        let mut msg = Msg::default();
        while PeekMessageW(&mut msg, 0, 0, 0, 1) != 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    out.append(&mut EVENTS.with(|q| q.borrow_mut().drain(..).collect()));
    out
}

#[allow(dead_code)]
pub(super) fn desktop_available() -> bool {
    open(1, "RoseGold", 32, 32, false)
        .inspect(|hwnd| destroy(*hwnd))
        .is_some()
}

#[cfg(windows)]
unsafe fn create_window(title: &str, w: i32, h: i32, visible: bool) -> Option<isize> {
    register();
    let wide = wide(title);
    let mut rect = Rect {
        left: 0,
        top: 0,
        right: w.max(1),
        bottom: h.max(1),
    };
    let style = WS_OVERLAPPEDWINDOW;
    let ex = if visible { 0 } else { 0x00000080 }; // WS_EX_TOOLWINDOW
    AdjustWindowRectEx(&mut rect, style, 0, ex);
    let hwnd = CreateWindowExW(
        ex,
        CLASS.as_ptr(),
        wide.as_ptr(),
        style,
        0x80000000u32 as i32,
        0x80000000u32 as i32,
        rect.right - rect.left,
        rect.bottom - rect.top,
        0,
        0,
        instance(),
        std::ptr::null_mut(),
    );
    if hwnd == 0 {
        return None;
    }
    if visible {
        ShowWindow(hwnd, 5);
    }
    Some(hwnd)
}

#[cfg(not(windows))]
unsafe fn create_window(_title: &str, _w: i32, _h: i32, _visible: bool) -> Option<isize> {
    None
}

#[cfg(windows)]
fn register() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        let class = WndClassW {
            size: std::mem::size_of::<WndClassW>() as u32,
            style: 0x0002 | 0x0001,
            wnd_proc: wnd_proc,
            cls_extra: 0,
            wnd_extra: 0,
            instance: instance(),
            icon: 0,
            cursor: LoadCursorW(0, 32512),
            brush: 0,
            menu: std::ptr::null(),
            class_name: CLASS.as_ptr(),
            icon_sm: 0,
        };
        RegisterClassExW(&class);
    });
}

#[cfg(windows)]
fn instance() -> isize {
    unsafe { GetModuleHandleW(std::ptr::null()) }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

thread_local! {
    static EVENTS: RefCell<Vec<NativeEvent>> = const { RefCell::new(Vec::new()) };
    static PENDING: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
}

#[cfg(windows)]
fn push_event(event: NativeEvent) {
    EVENTS.with(|q| q.borrow_mut().push(event));
}

#[cfg(windows)]
unsafe extern "system" fn wnd_proc(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize {
    let stored = GetWindowLongPtrW(hwnd, -21);
    let id = if stored == 0 {
        let pending = PENDING.with(|slot| slot.get());
        if pending != 0 {
            SetWindowLongPtrW(hwnd, -21, pending as isize);
        }
        pending
    } else {
        stored as i64
    };
    match msg {
        0x0010 => {
            // WM_CLOSE
            if id != 0 {
                push_event(NativeEvent::Close { id });
            }
            DestroyWindow(hwnd);
            0
        }
        0x0002 => 0, // WM_DESTROY
        0x0005 => {
            // WM_SIZE
            if id != 0 {
                let w = (lparam & 0xffff) as i32;
                let h = ((lparam >> 16) & 0xffff) as i32;
                push_event(NativeEvent::Resize { id, w, h });
            }
            0
        }
        0x000F => {
            // WM_PAINT
            let mut ps = Paint::default();
            BeginPaint(hwnd, &mut ps);
            EndPaint(hwnd, &ps);
            0
        }
        0x0200 | 0x0201 | 0x0202 | 0x0204 | 0x0205 => {
            if id != 0 {
                let x = (lparam & 0xffff) as u16 as i16 as i32;
                let y = ((lparam >> 16) & 0xffff) as u16 as i16 as i32;
                let kind = match msg {
                    0x0201 => MouseKind::LeftDown,
                    0x0202 => MouseKind::LeftUp,
                    0x0205 => MouseKind::RightUp,
                    _ => MouseKind::Move,
                };
                if msg == 0x0201 {
                    SetCapture(hwnd);
                }
                if msg == 0x0202 || msg == 0x0205 {
                    ReleaseCapture();
                }
                push_event(NativeEvent::Mouse { id, x, y, kind });
            }
            0
        }
        0x020A => {
            if id != 0 {
                let x = (lparam & 0xffff) as u16 as i16 as i32;
                let y = ((lparam >> 16) & 0xffff) as u16 as i16 as i32;
                let delta = ((wparam >> 16) & 0xffff) as u16 as i16 as i32;
                push_event(NativeEvent::Wheel { id, x, y, delta });
            }
            0
        }
        0x0100 => {
            // WM_KEYDOWN
            if id != 0 {
                if let Some((code, text)) = key_down(wparam as i32) {
                    push_event(NativeEvent::Key { id, code, text });
                }
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        0x0102 => {
            // WM_CHAR
            if id != 0 {
                let ch = wparam as u32;
                if ch == 8 || ch == 13 {
                    push_event(NativeEvent::Key {
                        id,
                        code: ch as i32,
                        text: String::new(),
                    });
                } else if ch >= 32 && ch != 127 && ch != 9 {
                    if let Some(c) = char::from_u32(ch) {
                        push_event(NativeEvent::Key {
                            id,
                            code: ch as i32,
                            text: c.to_string(),
                        });
                    }
                }
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[cfg(windows)]
fn key_down(vk: i32) -> Option<(i32, String)> {
    let shift = unsafe { GetKeyState(0x10) } < 0;
    let ctrl = unsafe { GetKeyState(0x11) } < 0;
    let mods = if shift && ctrl {
        "ctrl+shift"
    } else if shift {
        "shift"
    } else if ctrl {
        "ctrl"
    } else {
        ""
    };
    const TAB: i32 = 0x09;
    const PRIOR: i32 = 0x21;
    const NEXT: i32 = 0x22;
    const END: i32 = 0x23;
    const HOME: i32 = 0x24;
    const LEFT: i32 = 0x25;
    const UP: i32 = 0x26;
    const RIGHT: i32 = 0x27;
    const DOWN: i32 = 0x28;
    const ESCAPE: i32 = 0x1B;
    const DELETE: i32 = 0x2E;
    if vk == TAB {
        return Some((
            vk,
            if shift {
                "shift".to_string()
            } else {
                String::new()
            },
        ));
    }
    if matches!(
        vk,
        LEFT | RIGHT | HOME | END | DELETE | UP | DOWN | PRIOR | NEXT | ESCAPE
    ) {
        return Some((vk, mods.to_string()));
    }
    if ctrl && matches!(vk, 0x41 | 0x43 | 0x58 | 0x56) {
        return Some((vk, "ctrl".to_string()));
    }
    None
}

#[cfg(windows)]
unsafe fn blit(hwnd: isize, fb: &[u32], w: i32, h: i32) {
    let hdc = GetDC(hwnd);
    if hdc == 0 {
        return;
    }
    let mut info = BitmapInfo::default();
    info.header.size = 40;
    info.header.width = w;
    info.header.height = -h;
    info.header.planes = 1;
    info.header.bit_count = 32;
    StretchDIBits(
        hdc,
        0,
        0,
        w,
        h,
        0,
        0,
        w,
        h,
        fb.as_ptr() as *const c_void,
        &info,
        0,
        0x00CC0020,
    );
    ReleaseDC(hwnd, hdc);
}

#[cfg(windows)]
const CLASS: &[u16] = &[
    b'R' as u16,
    b'o' as u16,
    b's' as u16,
    b'e' as u16,
    b'G' as u16,
    b'o' as u16,
    b'l' as u16,
    b'd' as u16,
    b'U' as u16,
    b'i' as u16,
    0,
];

const WS_OVERLAPPEDWINDOW: u32 = 0x00CF0000;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Msg {
    hwnd: isize,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt: Point,
}

impl Default for Msg {
    fn default() -> Self {
        Self {
            hwnd: 0,
            message: 0,
            wparam: 0,
            lparam: 0,
            time: 0,
            pt: Point::default(),
        }
    }
}

#[repr(C)]
struct WndClassW {
    size: u32,
    style: u32,
    wnd_proc: unsafe extern "system" fn(isize, u32, usize, isize) -> isize,
    cls_extra: i32,
    wnd_extra: i32,
    instance: isize,
    icon: isize,
    cursor: isize,
    brush: isize,
    menu: *const u16,
    class_name: *const u16,
    icon_sm: isize,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Paint {
    hdc: isize,
    erase: i32,
    rc: Rect,
    restore: i32,
    inc_update: i32,
    reserved: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct BitmapHeader {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bit_count: u16,
    compression: u32,
    size_image: u32,
    x_pels: i32,
    y_pels: i32,
    clr_used: u32,
    clr_important: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct BitmapInfo {
    header: BitmapHeader,
    colors: [u32; 3],
}

#[cfg(windows)]
#[link(name = "user32")]
#[link(name = "gdi32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
    fn RegisterClassExW(class: *const WndClassW) -> u16;
    fn CreateWindowExW(
        ex: u32,
        class: *const u16,
        window: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: isize,
        menu: isize,
        instance: isize,
        param: *mut c_void,
    ) -> isize;
    fn DestroyWindow(hwnd: isize) -> i32;
    fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
    fn SetForegroundWindow(hwnd: isize) -> i32;
    fn SetWindowTextW(hwnd: isize, text: *const u16) -> i32;
    fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
    fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
    fn SetClassLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
    fn AdjustWindowRectEx(rect: *mut Rect, style: u32, menu: i32, ex: u32) -> i32;
    fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32)
        -> i32;
    fn DefWindowProcW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn PeekMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32, remove: u32) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
    fn BeginPaint(hwnd: isize, paint: *mut Paint) -> isize;
    fn EndPaint(hwnd: isize, paint: *const Paint) -> i32;
    fn GetDC(hwnd: isize) -> isize;
    fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
    fn StretchDIBits(
        hdc: isize,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        sx: i32,
        sy: i32,
        sw: i32,
        sh: i32,
        bits: *const c_void,
        info: *const BitmapInfo,
        usage: u32,
        rop: u32,
    ) -> i32;
    fn LoadCursorW(instance: isize, id: isize) -> isize;
    fn SetCursor(cursor: isize) -> isize;
    fn GetKeyState(vk: i32) -> i16;
    fn SetCapture(hwnd: isize) -> isize;
    fn ReleaseCapture() -> i32;
}
