trait Widget {
    fn min_width(): Int;
    fn height(w: Int): Int;
    fn flex(): Int;
    fn paint(win_id: Int, x: Int, y: Int, w: Int);
    fn handle_click(lx: Int, ly: Int, w: Int, h: Int): Bool;
    fn handle_right_click(lx: Int, ly: Int, w: Int, h: Int): Bool;
    fn handle_key(code: Int, text: String): Bool;
    fn handle_scroll(dx: Int, dy: Int, lx: Int, ly: Int, w: Int, h: Int): Bool;
    fn clear_focus();
    fn append_focusables(out: Array[Widget]);
    fn focus_enter();
    fn has_focus(): Bool;
    fn hover_cursor(lx: Int, ly: Int, w: Int, h: Int): Int;
}

trait LazyRows {
    fn count(): Int;
    fn row(i: Int): Widget;
}

class Style {
    @optional
    var fill: Color;
    @optional
    var ink: Color;
    var pad: Int = 0;
}

class Pad impl Widget {
    var child: Widget;
    var amount: Int = 0;

    fn min_width(): Int {
        return child.min_width() + amount * 2;
    }

    fn height(w: Int): Int {
        var inner = w - amount * 2;
        if (inner < 0) {
            inner = 0;
        }
        return child.height(inner) + amount * 2;
    }

    fn flex(): Int {
        return child.flex();
    }

    fn paint(win_id: Int, x: Int, y: Int, w: Int) {
        var inner = w - amount * 2;
        if (inner < 0) {
            inner = 0;
        }
        child.paint(win_id, x + amount, y + amount, inner);
    }

    fn handle_click(lx: Int, ly: Int, w: Int, h: Int): Bool {
        if (lx < amount || ly < amount) {
            return false;
        }
        if (lx >= w - amount || ly >= h - amount) {
            return false;
        }
        var inner_w = w - amount * 2;
        var inner_h = h - amount * 2;
        if (inner_w < 0) {
            inner_w = 0;
        }
        if (inner_h < 0) {
            inner_h = 0;
        }
        return child.handle_click(lx - amount, ly - amount, inner_w, inner_h);
    }

    fn handle_right_click(lx: Int, ly: Int, w: Int, h: Int): Bool {
        if (lx < amount || ly < amount) {
            return false;
        }
        if (lx >= w - amount || ly >= h - amount) {
            return false;
        }
        var inner_w = w - amount * 2;
        var inner_h = h - amount * 2;
        if (inner_w < 0) {
            inner_w = 0;
        }
        if (inner_h < 0) {
            inner_h = 0;
        }
        return child.handle_right_click(lx - amount, ly - amount, inner_w, inner_h);
    }

    fn handle_key(code: Int, text: String): Bool {
        return child.handle_key(code, text);
    }

    fn handle_scroll(dx: Int, dy: Int, lx: Int, ly: Int, w: Int, h: Int): Bool {
        if (lx < amount || ly < amount) {
            return false;
        }
        if (lx >= w - amount || ly >= h - amount) {
            return false;
        }
        var inner_w = w - amount * 2;
        var inner_h = h - amount * 2;
        if (inner_w < 0) {
            inner_w = 0;
        }
        if (inner_h < 0) {
            inner_h = 0;
        }
        return child.handle_scroll(dx, dy, lx - amount, ly - amount, inner_w, inner_h);
    }

    fn clear_focus() {
        child.clear_focus();
    }

    fn append_focusables(out: Array[Widget]) {
        child.append_focusables(out);
    }

    fn focus_enter() {
        child.focus_enter();
    }

    fn has_focus(): Bool {
        return child.has_focus();
    }

    fn hover_cursor(lx: Int, ly: Int, w: Int, h: Int): Int {
        var inner_w = w - amount * 2;
        var inner_h = h - amount * 2;
        if (inner_w < 0) {
            inner_w = 0;
        }
        if (inner_h < 0) {
            inner_h = 0;
        }
        return child.hover_cursor(lx - amount, ly - amount, inner_w, inner_h);
    }
}

class Backdrop impl Widget {
    var child: Widget;
    var color: Color = Color.Black;

    fn min_width(): Int {
        return child.min_width();
    }

    fn height(w: Int): Int {
        return child.height(w);
    }

    fn flex(): Int {
        return child.flex();
    }

    fn paint(win_id: Int, x: Int, y: Int, w: Int) {
        __ui.fill(win_id, x, y, w, child.height(w), color.value());
        child.paint(win_id, x, y, w);
    }

    fn handle_click(lx: Int, ly: Int, w: Int, h: Int): Bool {
        return child.handle_click(lx, ly, w, h);
    }

    fn handle_right_click(lx: Int, ly: Int, w: Int, h: Int): Bool {
        return child.handle_right_click(lx, ly, w, h);
    }

    fn handle_key(code: Int, text: String): Bool {
        return child.handle_key(code, text);
    }

    fn handle_scroll(dx: Int, dy: Int, lx: Int, ly: Int, w: Int, h: Int): Bool {
        return child.handle_scroll(dx, dy, lx, ly, w, h);
    }

    fn clear_focus() {
        child.clear_focus();
    }

    fn append_focusables(out: Array[Widget]) {
        child.append_focusables(out);
    }

    fn focus_enter() {
        child.focus_enter();
    }

    fn has_focus(): Bool {
        return child.has_focus();
    }

    fn hover_cursor(lx: Int, ly: Int, w: Int, h: Int): Int {
        return child.hover_cursor(lx, ly, w, h);
    }
}

@ufcs
fn padding(w: Widget, n: Int): Widget {
    return Pad { child: w, amount: n };
}

@ufcs
fn background(w: Widget, color: Color): Widget {
    return Backdrop { child: w, color: color };
}

@ufcs
fn style(w: Widget, s: Style): Widget {
    var out: Widget = w;
    if (s.fill) {
        out = Backdrop { child: out, color: s.fill };
    }
    if (s.pad > 0) {
        out = Pad { child: out, amount: s.pad };
    }
    return out;
}

fn drop_last(s: String): String {
    var n = len(s);
    if (n == 0) {
        return "";
    }
    return __str.slice(s, 0, n - 1);
}

fn str_slice(s: String, start: Int, end: Int): String {
    return __str.slice(s, start, end);
}

fn line_step(): Int {
    var h = font_height() + 2;
    if (h < 18) {
        return 18;
    }
    return h;
}

fn control_height(min_h: Int, pad: Int): Int {
    var h = font_height() + pad;
    if (h < min_h) {
        return min_h;
    }
    return h;
}

fn sb_width(): Int {
    return 14;
}

fn corner_r(): Int {
    return 8;
}

fn fill_round(win_id: Int, x: Int, y: Int, w: Int, h: Int, r: Int, color: Int) {
    __ui.fill_round(win_id, x, y, w, h, r, color);
}

fn stroke_round(win_id: Int, x: Int, y: Int, w: Int, h: Int, r: Int, color: Int) {
    __ui.stroke_round(win_id, x, y, w, h, r, color);
}

fn paint_chevron(win_id: Int, cx: Int, cy: Int, size: Int, up: Bool, color: Int) {
    var s = size;
    if (s < 4) {
        s = 4;
    }
    if (up) {
        __ui.line(win_id, cx - s, cy + s / 2, cx, cy - s / 2, color);
        __ui.line(win_id, cx + s, cy + s / 2, cx, cy - s / 2, color);
        __ui.line(win_id, cx - s + 1, cy + s / 2, cx, cy - s / 2 + 1, color);
        __ui.line(win_id, cx + s - 1, cy + s / 2, cx, cy - s / 2 + 1, color);
    } else {
        __ui.line(win_id, cx - s, cy - s / 2, cx, cy + s / 2, color);
        __ui.line(win_id, cx + s, cy - s / 2, cx, cy + s / 2, color);
        __ui.line(win_id, cx - s + 1, cy - s / 2, cx, cy + s / 2 - 1, color);
        __ui.line(win_id, cx + s - 1, cy - s / 2, cx, cy + s / 2 - 1, color);
    }
}

fn paint_vscroll(win_id: Int, x: Int, y: Int, w: Int, vh: Int, offset: Int, content_h: Int) {
    var max = content_h - vh;
    if (max <= 0 || vh < 1) {
        return;
    }
    var bw = sb_width();
    var tx = x + w - bw;
    fill_round(win_id, tx + 1, y + 1, bw - 2, vh - 2, 5, Color.Rgb(236, 236, 240).value());
    var thumb = (vh * vh) / content_h;
    if (thumb < 24) {
        thumb = 24;
    }
    if (thumb > vh - 4) {
        thumb = vh - 4;
    }
    var track = vh - 4 - thumb;
    var ty = y + 2;
    if (track > 0 && max > 0) {
        ty = y + 2 + (offset * track) / max;
    }
    fill_round(win_id, tx + 3, ty, bw - 6, thumb, 4, Color.Rgb(160, 160, 168).value());
}

fn vscroll_hit(lx: Int, ly: Int, w: Int, vh: Int): Bool {
    return lx >= w - sb_width() && lx < w && ly >= 0 && ly < vh;
}

fn vscroll_offset_at(ly: Int, vh: Int, content_h: Int): Int {
    var max = content_h - vh;
    if (max <= 0 || vh < 1) {
        return 0;
    }
    var thumb = (vh * vh) / content_h;
    if (thumb < 24) {
        thumb = 24;
    }
    if (thumb > vh - 4) {
        thumb = vh - 4;
    }
    var track = vh - 4 - thumb;
    if (track < 1) {
        return 0;
    }
    var y = ly - 2 - thumb / 2;
    if (y < 0) {
        y = 0;
    }
    if (y > track) {
        y = track;
    }
    return (y * max) / track;
}

fn wrap_lines(text: String, max_w: Int): Array[String] {
    var lines: Array[String] = [];
    var width = max_w;
    if (width < 1) {
        width = 1;
    }
    var para = "";
    var i = 0;
    var n = len(text);
    while (i <= n) {
        var at_end = i == n;
        var ch = "";
        if (!at_end) {
            ch = text[i];
        }
        if (at_end || ch == "\n") {
            if (len(para) == 0) {
                lines.push("");
            } else {
                var word = "";
                var line = "";
                var j = 0;
                while (j <= len(para)) {
                    var done = j == len(para);
                    var c = "";
                    if (!done) {
                        c = para[j];
                    }
                    if (done || c == " ") {
                        var candidate = word;
                        if (len(line) > 0) {
                            candidate = line + " " + word;
                        }
                        if (len(word) > 0 && text_width(candidate) <= width) {
                            line = candidate;
                        } else {
                            if (len(line) > 0) {
                                lines.push(line);
                            }
                            if (text_width(word) <= width) {
                                line = word;
                            } else {
                                var piece = "";
                                var k = 0;
                                while (k < len(word)) {
                                    var next = piece + word[k];
                                    if (len(piece) > 0 && text_width(next) > width) {
                                        lines.push(piece);
                                        piece = word[k];
                                    } else {
                                        piece = next;
                                    }
                                    k = k + 1;
                                }
                                line = piece;
                            }
                        }
                        word = "";
                    } else {
                        word = word + c;
                    }
                    j = j + 1;
                }
                if (len(line) > 0) {
                    lines.push(line);
                }
            }
            para = "";
            if (at_end) {
                break;
            }
        } else {
            para = para + ch;
        }
        i = i + 1;
    }
    if (len(lines) == 0) {
        lines.push("");
    }
    return lines;
}

fn wrapped_height(text: String, max_w: Int): Int {
    var lines = wrap_lines(text, max_w);
    var step = line_step();
    var h = len(lines) * step;
    if (h < step) {
        h = step;
    }
    return h;
}

class PopupMenu {
    var items: Array[String] = [];
    var at_x: Int = 0;
    var at_y: Int = 0;
    var open: Bool = false;
    var selected: Int = -1;
    var fill: Color = Color.White;
    var ink: Color = Color.Rgb(32, 32, 32);
    var border: Color = Color.Rgb(160, 160, 160);
    var select_fill: Color = Color.Rgb(200, 220, 255);
    signal chosen();

    fn item_h(): Int {
        return control_height(28, 8);
    }

    fn menu_w(): Int {
        var mw = 120;
        var i = 0;
        while (i < len(items)) {
            var tw = text_width(items[i]) + 24;
            if (tw > mw) {
                mw = tw;
            }
            i = i + 1;
        }
        return mw;
    }

    fn menu_h(): Int {
        var n = len(items);
        if (n < 1) {
            return item_h();
        }
        return n * item_h();
    }

    fn open_at(x: Int, y: Int) {
        at_x = x;
        at_y = y;
        open = true;
        if (len(items) > 0) {
            selected = 0;
        } else {
            selected = -1;
        }
    }

    fn dismiss() {
        open = false;
    }

    fn contains(mx: Int, my: Int): Bool {
        if (!open) {
            return false;
        }
        return hit_test(mx - at_x, my - at_y, menu_w(), menu_h());
    }

    fn paint(win_id: Int) {
        if (!open || len(items) < 1) {
            return;
        }
        var w = menu_w();
        var h = menu_h();
        var ih = item_h();
        var r = corner_r();
        fill_round(win_id, at_x, at_y, w, h, r, fill.value());
        stroke_round(win_id, at_x, at_y, w, h, r, Color.Rgb(47, 111, 196).value());
        var i = 0;
        while (i < len(items)) {
            var ry = at_y + i * ih;
            if (i == selected) {
                fill_round(win_id, at_x + 4, ry + 2, w - 8, ih - 4, 4, select_fill.value());
            }
            __ui.text(win_id, at_x + 10, ry + (ih - font_height()) / 2, items[i], ink.value());
            i = i + 1;
        }
    }

    fn handle_click(mx: Int, my: Int): Bool {
        if (!open) {
            return false;
        }
        if (!contains(mx, my)) {
            return false;
        }
        var ih = item_h();
        var i = (my - at_y) / ih;
        if (i < 0 || i >= len(items)) {
            return true;
        }
        selected = i;
        open = false;
        chosen.emit();
        return true;
    }

    fn handle_key(code: Int, text: String): Bool {
        if (!open) {
            return false;
        }
        if (code == 27) {
            open = false;
            return true;
        }
        if (code == 13 || code == 32) {
            if (selected >= 0 && selected < len(items)) {
                open = false;
                chosen.emit();
            }
            return true;
        }
        var n = len(items);
        if (n < 1) {
            return true;
        }
        if (code == 38) {
            if (selected <= 0) {
                selected = 0;
            } else {
                selected = selected - 1;
            }
            return true;
        }
        if (code == 40) {
            if (selected < 0) {
                selected = 0;
            } elif (selected >= n - 1) {
                selected = n - 1;
            } else {
                selected = selected + 1;
            }
            return true;
        }
        return false;
    }

    fn hover_cursor(mx: Int, my: Int): Int {
        if (contains(mx, my)) {
            return 1;
        }
        return 0;
    }
}

class Dialog {
    var title: String = "";
    var message: String = "";
    var buttons: Array[String] = ["OK"];
    var field_placeholder: String = "";
    var field_text: String = "";
    var field_focused: Bool = false;
    var open: Bool = false;
    var selected: Int = -1;
    var btn_focus: Int = 0;
    var panel_x: Int = 0;
    var panel_y: Int = 0;
    var panel_w: Int = 320;
    var panel_h: Int = 160;
    var scrim: Color = Color.Rgb(32, 32, 40);
    var fill: Color = Color.White;
    var ink: Color = Color.Rgb(32, 32, 32);
    var muted: Color = Color.Rgb(90, 90, 100);
    var hover_btn: Int = -1;
    signal chosen();
    signal cancelled();

    fn pad(): Int {
        return 16;
    }

    fn btn_h(): Int {
        return control_height(32, 12);
    }

    fn field_h(): Int {
        return control_height(28, 10);
    }

    fn has_field(): Bool {
        return len(field_placeholder) > 0;
    }

    fn content_w(): Int {
        var w = panel_w - pad() * 2;
        if (w < 1) {
            return 1;
        }
        return w;
    }

    fn btn_w(i: Int): Int {
        var tw = text_width(buttons[i]) + 24;
        if (tw < 72) {
            return 72;
        }
        return tw;
    }

    fn layout(win_w: Int, win_h: Int) {
        panel_w = 360;
        if (panel_w > win_w - 40) {
            panel_w = win_w - 40;
        }
        if (panel_w < 220) {
            panel_w = 220;
        }
        var cw = content_w();
        var h = pad();
        if (len(title) > 0) {
            h = h + font_height() + 10;
        }
        if (len(message) > 0) {
            h = h + wrapped_height(message, cw) + 8;
        }
        if (has_field()) {
            h = h + field_h() + 12;
        }
        h = h + btn_h() + pad();
        panel_h = h;
        panel_x = (win_w - panel_w) / 2;
        panel_y = (win_h - panel_h) / 2;
        if (panel_x < 12) {
            panel_x = 12;
        }
        if (panel_y < 12) {
            panel_y = 12;
        }
    }

    fn open_centered(win_w: Int, win_h: Int) {
        layout(win_w, win_h);
        open = true;
        selected = -1;
        if (len(buttons) > 0) {
            btn_focus = len(buttons) - 1;
        } else {
            btn_focus = 0;
        }
        field_focused = has_field();
    }

    fn dismiss() {
        open = false;
        field_focused = false;
    }

    fn choose(i: Int) {
        if (i < 0 || i >= len(buttons)) {
            return;
        }
        selected = i;
        open = false;
        field_focused = false;
        chosen.emit();
    }

    fn cancel() {
        selected = -1;
        open = false;
        field_focused = false;
        cancelled.emit();
    }

    fn panel_contains(mx: Int, my: Int): Bool {
        return hit_test(mx - panel_x, my - panel_y, panel_w, panel_h);
    }

    fn field_rect_y(): Int {
        var y = panel_y + pad();
        if (len(title) > 0) {
            y = y + font_height() + 10;
        }
        if (len(message) > 0) {
            y = y + wrapped_height(message, content_w()) + 8;
        }
        return y;
    }

    fn buttons_y(): Int {
        return panel_y + panel_h - pad() - btn_h();
    }

    fn button_x(i: Int): Int {
        var x = panel_x + panel_w - pad();
        var j = len(buttons) - 1;
        while (j >= i) {
            x = x - btn_w(j);
            if (j > i) {
                x = x - 8;
            }
            j = j - 1;
        }
        return x;
    }

    fn button_at(mx: Int, my: Int): Int {
        var bi = 0;
        while (bi < len(buttons)) {
            if (hit_test(mx - button_x(bi), my - buttons_y(), btn_w(bi), btn_h())) {
                return bi;
            }
            bi = bi + 1;
        }
        return -1;
    }

    fn paint(win_id: Int, win_w: Int, win_h: Int) {
        if (!open) {
            return;
        }
        layout(win_w, win_h);
        __ui.fill(win_id, 0, 0, win_w, win_h, scrim.value());
        var r = corner_r();
        fill_round(win_id, panel_x, panel_y, panel_w, panel_h, r, fill.value());
        stroke_round(win_id, panel_x, panel_y, panel_w, panel_h, r,
                     Color.Rgb(47, 111, 196).value());
        var cw = content_w();
        var x = panel_x + pad();
        var y = panel_y + pad();
        if (len(title) > 0) {
            __ui.text(win_id, x, y, title, ink.value());
            y = y + font_height() + 10;
        }
        if (len(message) > 0) {
            var lines = wrap_lines(message, cw);
            var step = line_step();
            var i = 0;
            while (i < len(lines)) {
                __ui.text(win_id, x, y + i * step, lines[i], muted.value());
                i = i + 1;
            }
            y = y + wrapped_height(message, cw) + 8;
        }
        if (has_field()) {
            var fh = field_h();
            fill_round(win_id, x, y, cw, fh, r, Color.White.value());
            var bcol = Color.Rgb(160, 160, 160);
            if (field_focused) {
                bcol = Color.Rgb(47, 111, 196);
            }
            stroke_round(win_id, x, y, cw, fh, r, bcol.value());
            var shown = field_text;
            var col = ink;
            if (len(shown) == 0) {
                shown = field_placeholder;
                col = Color.Rgb(140, 140, 150);
            }
            var ty = y + (fh - font_height()) / 2;
            __ui.text(win_id, x + 8, ty, shown, col.value());
            if (field_focused && len(field_text) > 0) {
                var cx = x + 8 + text_width(field_text) + 1;
                __ui.fill(win_id, cx, y + (fh - font_height()) / 2, 1, font_height(),
                          ink.value());
            }
            y = y + fh + 12;
        }
        var mx = __ui.mouse_x(win_id);
        var my = __ui.mouse_y(win_id);
        hover_btn = button_at(mx, my);
        var down = __ui.mouse_down(win_id);
        var bi = 0;
        while (bi < len(buttons)) {
            var bx = button_x(bi);
            var by = buttons_y();
            var bw = btn_w(bi);
            var bh = btn_h();
            var primary = bi == len(buttons) - 1;
            var hot = bi == hover_btn;
            var bg = Color.Rgb(230, 230, 236);
            var fg = ink;
            if (primary) {
                bg = Color.Rgb(47, 111, 196);
                fg = Color.White;
                if (hot && down) {
                    bg = Color.Rgb(30, 80, 150);
                } elif (hot) {
                    bg = Color.Rgb(66, 133, 220);
                }
            } else {
                if (hot && down) {
                    bg = Color.Rgb(200, 200, 210);
                } elif (hot) {
                    bg = Color.Rgb(242, 242, 248);
                }
            }
            fill_round(win_id, bx, by, bw, bh, r, bg.value());
            if (bi == btn_focus && !field_focused) {
                stroke_round(win_id, bx, by, bw, bh, r, Color.Rgb(20, 60, 120).value());
            }
            var tw = text_width(buttons[bi]);
            var tx = bx + (bw - tw) / 2;
            var ty = by + (bh - font_height()) / 2;
            __ui.text(win_id, tx, ty, buttons[bi], fg.value());
            bi = bi + 1;
        }
    }

    fn handle_click(mx: Int, my: Int, win_w: Int, win_h: Int): Bool {
        if (!open) {
            return false;
        }
        layout(win_w, win_h);
        if (!panel_contains(mx, my)) {
            return true;
        }
        if (has_field()) {
            var fy = field_rect_y();
            var fh = field_h();
            if (hit_test(mx - (panel_x + pad()), my - fy, content_w(), fh)) {
                field_focused = true;
                return true;
            }
        }
        var bi = button_at(mx, my);
        if (bi >= 0) {
            field_focused = false;
            btn_focus = bi;
            choose(bi);
            return true;
        }
        field_focused = false;
        return true;
    }

    fn handle_key(code: Int, text_in: String): Bool {
        if (!open) {
            return false;
        }
        if (code == 27) {
            cancel();
            return true;
        }
        if (field_focused && has_field()) {
            if (code == 9) {
                field_focused = false;
                if (len(buttons) > 0) {
                    btn_focus = len(buttons) - 1;
                }
                return true;
            }
            if (code == 8) {
                field_text = drop_last(field_text);
                return true;
            }
            if (code == 13) {
                if (len(buttons) > 0) {
                    choose(len(buttons) - 1);
                }
                return true;
            }
            if (len(text_in) > 0) {
                field_text = field_text + text_in;
                return true;
            }
            return true;
        }
        if (code == 9) {
            if (has_field() && (text_in == "shift" || btn_focus <= 0)) {
                field_focused = true;
                return true;
            }
            if (len(buttons) < 1) {
                return true;
            }
            if (text_in == "shift") {
                if (btn_focus <= 0) {
                    btn_focus = len(buttons) - 1;
                } else {
                    btn_focus = btn_focus - 1;
                }
            } else {
                if (btn_focus >= len(buttons) - 1) {
                    btn_focus = 0;
                } else {
                    btn_focus = btn_focus + 1;
                }
            }
            return true;
        }
        if (code == 37) {
            if (btn_focus > 0) {
                btn_focus = btn_focus - 1;
            }
            return true;
        }
        if (code == 39) {
            if (btn_focus < len(buttons) - 1) {
                btn_focus = btn_focus + 1;
            }
            return true;
        }
        if (code == 13 || code == 32) {
            choose(btn_focus);
            return true;
        }
        return true;
    }

    fn hover_cursor(mx: Int, my: Int, win_w: Int, win_h: Int): Int {
        if (!open) {
            return 0;
        }
        layout(win_w, win_h);
        if (has_field()) {
            var fy = field_rect_y();
            if (hit_test(mx - (panel_x + pad()), my - fy, content_w(), field_h())) {
                return 2;
            }
        }
        if (button_at(mx, my) >= 0) {
            return 1;
        }
        return 0;
    }
}

class Window {
    var id: Int = 0;
    var title: String = "RoseGold";
    var width: Int = 800;
    var height: Int = 600;
    var visible: Bool = true;
    var theme: Theme = Theme {};
    @optional
    var children: Array[Widget];
    var menus: Array[PopupMenu] = [];
    var dialogs: Array[Dialog] = [];
    var last_click_x: Int = 0;
    var last_click_y: Int = 0;
    var tip_text: String = "";
    var tip_x: Int = 0;
    var tip_y: Int = 0;
    var tip_on: Bool = false;
    var dirty: Bool = false;

    fn mark_dirty() {
        dirty = true;
    }

    fn mark_clean() {
        dirty = false;
    }

    fn clear_tip() {
        tip_on = false;
        tip_text = "";
    }

    fn offer_tip(text: String, x: Int, y: Int) {
        tip_on = true;
        tip_text = text;
        tip_x = x;
        tip_y = y;
    }

    fn paint_tip(win_id: Int) {
        if (!tip_on || len(tip_text) == 0) {
            return;
        }
        var pad = 6;
        var tw = text_width(tip_text);
        var th = font_height();
        var bw = tw + pad * 2;
        var bh = th + pad * 2;
        var bx = tip_x - bw / 2;
        var by = tip_y;
        if (bx < 4) {
            bx = 4;
        }
        if (bx + bw > width - 4) {
            bx = width - 4 - bw;
        }
        if (by + bh > height - 4) {
            by = tip_y - bh - 8;
        }
        if (by < 4) {
            by = 4;
        }
        fill_round(win_id, bx, by, bw, bh, 6, Color.Rgb(40, 40, 48).value());
        __ui.text(win_id, bx + pad, by + pad, tip_text, Color.White.value());
    }

    fn bind_frame() {
        __ui.set_frame(id, fn () { self.tick(); });
    }

    fn root_widget(): Widget {
        return VStack { spacing: 8, children: children }.padding(12);
    }

    fn dismiss_menus() {
        var i = 0;
        while (i < len(menus)) {
            menus[i].dismiss();
            i = i + 1;
        }
        menus = [];
    }

    fn dismiss_dialogs() {
        var i = 0;
        while (i < len(dialogs)) {
            dialogs[i].dismiss();
            i = i + 1;
        }
        dialogs = [];
    }

    fn show_menu(m: PopupMenu, x: Int, y: Int) {
        if (has_dialog()) {
            return;
        }
        dismiss_menus();
        var px = x;
        var py = y;
        var mw = m.menu_w();
        var mh = m.menu_h();
        if (px + mw > width) {
            px = width - mw;
        }
        if (py + mh > height) {
            py = height - mh;
        }
        if (px < 0) {
            px = 0;
        }
        if (py < 0) {
            py = 0;
        }
        m.open_at(px, py);
        menus.push(m);
    }

    fn show_menu_at_pointer(m: PopupMenu) {
        show_menu(m, last_click_x, last_click_y);
    }

    fn has_menu(): Bool {
        return len(menus) > 0;
    }

    fn show_dialog(d: Dialog) {
        dismiss_menus();
        dismiss_dialogs();
        d.open_centered(width, height);
        dialogs.push(d);
    }

    fn has_dialog(): Bool {
        return len(dialogs) > 0;
    }

    fn confirm(title: String, message: String, ok: String): Dialog {
        var d = Dialog {
            title: title,
            message: message,
            buttons: ["Cancel", ok]
        };
        show_dialog(d);
        return d;
    }

    fn alert(title: String, message: String): Dialog {
        var d = Dialog {
            title: title,
            message: message,
            buttons: ["OK"]
        };
        show_dialog(d);
        return d;
    }

    fn menu(items: Array[String]): PopupMenu {
        return PopupMenu { items: items };
    }

    fn request_close() {
        if (!dirty) {
            close();
            return;
        }
        var d = Dialog {
            title: "Close",
            message: "Discard unsaved changes?",
            buttons: ["Cancel", "Close"]
        };
        d.chosen.connect(fn () {
            if (d.selected == 1) {
                dirty = false;
                close();
            }
        });
        show_dialog(d);
    }

    fn show() throws {
        if (id == 0) {
            id = try __ui.open(title, width, height, visible);
            title = __ui.title(id);
            width = __ui.width(id);
            height = __ui.height(id);
            bind_frame();
            tick();
            return;
        }
        visible = true;
        __ui.show(id);
        tick();
    }

    fn hide() {
        visible = false;
        if (id == 0) {
            return;
        }
        __ui.hide(id);
    }

    fn close() {
        if (id == 0) {
            return;
        }
        __ui.close(id);
        id = 0;
    }

    fn poll(): Bool {
        if (id == 0) {
            return false;
        }
        var ok = __ui.poll(id);
        width = __ui.width(id);
        height = __ui.height(id);
        tick();
        return ok;
    }

    async fn next_frame(): Bool {
        if (id == 0) {
            return false;
        }
        var ok = await __ui.next_frame(id);
        width = __ui.width(id);
        height = __ui.height(id);
        tick();
        return ok;
    }

    fn alive(): Bool {
        if (id == 0) {
            return false;
        }
        return __ui.alive(id);
    }

    fn set_title(s: String) {
        title = s;
        if (id != 0) {
            __ui.set_title(id, s);
        }
    }

    fn set_size(w: Int, h: Int) {
        width = w;
        height = h;
        if (id != 0) {
            __ui.set_size(id, w, h);
            width = __ui.width(id);
            height = __ui.height(id);
            tick();
        }
    }

    fn add(w: Widget): Window {
        children.push(w);
        tick();
        return self;
    }

    fn focus_step(back: Bool) {
        if (len(children) == 0) {
            return;
        }
        var root = root_widget();
        var xs: Array[Widget] = [];
        root.append_focusables(xs);
        var n = len(xs);
        if (n < 1) {
            return;
        }
        var cur = -1;
        var i = 0;
        while (i < n) {
            if (xs[i].has_focus()) {
                cur = i;
            }
            i = i + 1;
        }
        var next = 0;
        if (back) {
            if (cur <= 0) {
                next = n - 1;
            } else {
                next = cur - 1;
            }
        } else {
            if (cur < 0 || cur >= n - 1) {
                next = 0;
            } else {
                next = cur + 1;
            }
        }
        root.clear_focus();
        xs[next].focus_enter();
    }

    fn tick() {
        if (id == 0) {
            return;
        }
        width = __ui.width(id);
        height = __ui.height(id);
        clear_tip();
        if (len(children) == 0 && len(menus) == 0 && len(dialogs) == 0) {
            return;
        }
        var mx = __ui.mouse_x(id);
        var my = __ui.mouse_y(id);
        if (len(children) == 0) {
            if (__ui.take_click(id)) {
                last_click_x = mx;
                last_click_y = my;
                if (len(dialogs) > 0) {
                    var dlg = dialogs[len(dialogs) - 1];
                    dlg.handle_click(mx, my, width, height);
                    if (!dlg.open) {
                        dismiss_dialogs();
                    }
                } elif (len(menus) > 0) {
                    var top = menus[len(menus) - 1];
                    if (top.handle_click(mx, my)) {
                        if (!top.open) {
                            dismiss_menus();
                        }
                    } else {
                        dismiss_menus();
                    }
                }
            }
            if (__ui.take_right_click(id)) {
                last_click_x = mx;
                last_click_y = my;
                if (len(dialogs) == 0) {
                    dismiss_menus();
                }
            }
            while (__ui.take_key(id)) {
                var code = __ui.key_code(id);
                var text = __ui.key_text(id);
                if (len(dialogs) > 0) {
                    var dlg = dialogs[len(dialogs) - 1];
                    dlg.handle_key(code, text);
                    if (!dlg.open) {
                        dismiss_dialogs();
                    }
                } elif (len(menus) > 0) {
                    var top = menus[len(menus) - 1];
                    if (code == 27) {
                        dismiss_menus();
                    } elif (top.handle_key(code, text)) {
                        if (!top.open) {
                            dismiss_menus();
                        }
                    }
                }
            }
            if (__ui.take_scroll(id)) {
                if (len(dialogs) == 0 && len(menus) > 0) {
                    dismiss_menus();
                }
            }
            __ui.clear(id, theme.window_bg.value());
            var mi = 0;
            while (mi < len(menus)) {
                menus[mi].paint(id);
                mi = mi + 1;
            }
            var di = 0;
            while (di < len(dialogs)) {
                dialogs[di].paint(id, width, height);
                di = di + 1;
            }
            paint_tip(id);
            var cur = 0;
            if (len(dialogs) > 0) {
                cur = dialogs[len(dialogs) - 1].hover_cursor(mx, my, width, height);
            } elif (len(menus) > 0) {
                cur = menus[len(menus) - 1].hover_cursor(mx, my);
            }
            __ui.cursor(id, cur);
            __ui.present(id);
            return;
        }
        var root = root_widget();
        var rh = root.height(width);
        if (__ui.take_click(id)) {
            last_click_x = mx;
            last_click_y = my;
            if (len(dialogs) > 0) {
                var dlg = dialogs[len(dialogs) - 1];
                dlg.handle_click(mx, my, width, height);
                if (!dlg.open) {
                    dismiss_dialogs();
                }
            } elif (len(menus) > 0) {
                var top = menus[len(menus) - 1];
                if (top.handle_click(mx, my)) {
                    if (!top.open) {
                        dismiss_menus();
                    }
                } else {
                    dismiss_menus();
                }
            } else {
                root.clear_focus();
                root.handle_click(mx, my, width, rh);
            }
        }
        if (__ui.take_right_click(id)) {
            last_click_x = mx;
            last_click_y = my;
            if (len(dialogs) == 0) {
                dismiss_menus();
                root.handle_right_click(mx, my, width, rh);
            }
        }
        while (__ui.take_key(id)) {
            var code = __ui.key_code(id);
            var text = __ui.key_text(id);
            if (len(dialogs) > 0) {
                var dlg = dialogs[len(dialogs) - 1];
                dlg.handle_key(code, text);
                if (!dlg.open) {
                    dismiss_dialogs();
                }
            } elif (len(menus) > 0) {
                var top = menus[len(menus) - 1];
                if (code == 27) {
                    dismiss_menus();
                } elif (top.handle_key(code, text)) {
                    if (!top.open) {
                        dismiss_menus();
                    }
                }
            } elif (code == 9) {
                focus_step(text == "shift");
            } else {
                root.handle_key(code, text);
            }
        }
        if (__ui.take_scroll(id)) {
            if (len(dialogs) > 0) {
            } elif (len(menus) > 0) {
                dismiss_menus();
            } else {
                root.handle_scroll(__ui.scroll_dx(id), __ui.scroll_dy(id),
                                   mx, my, width, rh);
            }
        }
        __ui.clear(id, theme.window_bg.value());
        root.paint(id, 0, 0, width);
        var mi = 0;
        while (mi < len(menus)) {
            menus[mi].paint(id);
            mi = mi + 1;
        }
        var di = 0;
        while (di < len(dialogs)) {
            dialogs[di].paint(id, width, height);
            di = di + 1;
        }
        paint_tip(id);
        var cur = 0;
        if (len(dialogs) > 0) {
            cur = dialogs[len(dialogs) - 1].hover_cursor(mx, my, width, height);
        } elif (len(menus) > 0) {
            cur = menus[len(menus) - 1].hover_cursor(mx, my);
        }
        if (cur == 0 && len(dialogs) == 0) {
            cur = root.hover_cursor(mx, my, width, rh);
        }
        __ui.cursor(id, cur);
        __ui.present(id);
    }

    fn hover_at(x: Int, y: Int) {
        if (id == 0) {
            return;
        }
        __ui.feed_mouse(id, x, y);
        tick();
    }

    fn drag_to(x: Int, y: Int) {
        if (id == 0) {
            return;
        }
        __ui.feed_down(id, true);
        __ui.feed_mouse(id, x, y);
        tick();
    }

    fn drag_end(x: Int, y: Int) {
        if (id == 0) {
            return;
        }
        __ui.feed_mouse(id, x, y);
        __ui.feed_down(id, false);
        tick();
    }

    fn click_at(x: Int, y: Int) {
        if (id == 0) {
            return;
        }
        __ui.feed_click(id, x, y);
        tick();
    }

    fn right_click_at(x: Int, y: Int) {
        if (id == 0) {
            return;
        }
        __ui.feed_right_click(id, x, y);
        tick();
    }

    fn key_at(code: Int, text: String) {
        if (id == 0) {
            return;
        }
        __ui.feed_key(id, code, text);
        tick();
    }

    fn scroll_at(dx: Int, dy: Int, x: Int, y: Int) {
        if (id == 0) {
            return;
        }
        __ui.feed_click(id, x, y);
        __ui.take_click(id);
        __ui.feed_scroll(id, dx, dy);
        tick();
    }

    fn run() {
        while (alive()) {
            __ui.wait();
            poll();
        }
    }
}

fn open(title: String, width: Int, height: Int) throws: Window {
    var id = try __ui.open(title, width, height, true);
    var w = Window {
        id: id,
        title: __ui.title(id),
        width: __ui.width(id),
        height: __ui.height(id)
    };
    w.bind_frame();
    return w;
}

fn open_theme(title: String, width: Int, height: Int, theme: Theme) throws: Window {
    var w = try open(title, width, height);
    w.theme = theme;
    return w;
}

fn open_hidden(title: String, width: Int, height: Int) throws: Window {
    var id = try __ui.open(title, width, height, false);
    var w = Window {
        id: id,
        title: __ui.title(id),
        width: __ui.width(id),
        height: __ui.height(id)
    };
    w.bind_frame();
    return w;
}

fn open_hidden_theme(title: String, width: Int, height: Int, theme: Theme) throws: Window {
    var w = try open_hidden(title, width, height);
    w.theme = theme;
    return w;
}

@ufcs
fn confirm(w: Window, title: String, message: String, ok: String): Dialog {
    return w.confirm(title, message, ok);
}

@ufcs
fn alert(w: Window, title: String, message: String): Dialog {
    return w.alert(title, message);
}

@ufcs
fn menu(w: Window, items: Array[String]): PopupMenu {
    return w.menu(items);
}

@ufcs
fn request_close(w: Window) {
    w.request_close();
}

fn cursor_arrow(win_id: Int) {
    __ui.cursor(win_id, 0);
}

fn cursor_hand(win_id: Int) {
    __ui.cursor(win_id, 1);
}

fn cursor_ibeam(win_id: Int) {
    __ui.cursor(win_id, 2);
}

fn pointer_over(win_id: Int, x: Int, y: Int, w: Int, h: Int): Bool {
    var mx = __ui.mouse_x(win_id);
    var my = __ui.mouse_y(win_id);
    return hit_test(mx - x, my - y, w, h);
}

fn hit_test(lx: Int, ly: Int, w: Int, h: Int): Bool {
    // 2px slop so borders / AA edges keep hover.
    return lx >= -2 && lx < w + 2 && ly >= -2 && ly < h + 2;
}

fn font_height(): Int {
    return __ui.font_height();
}

fn text_width(s: String): Int {
    return __ui.text_width(s);
}

fn clipboard_get(): String {
    return __ui.clipboard_get();
}

fn clipboard_set(s: String) {
    __ui.clipboard_set(s);
}

fn run() {
    __ui.run();
}

fn count(): Int {
    return __ui.count();
}

fn backend(): String {
    return __ui.backend();
}

fn platform(): String {
    return __ui.platform();
}

fn kind(): String {
    var p = platform();
    if (p == "android" || p == "ios") {
        return "mobile";
    }
    if (p == "web") {
        return "web";
    }
    return "desktop";
}
