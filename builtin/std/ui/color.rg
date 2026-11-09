enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    Rgb(r: Int, g: Int, b: Int),
    Argb(a: Int, r: Int, g: Int, b: Int)
}

fn rgb(r: Int, g: Int, b: Int): Color {
    return Color.Rgb(r, g, b);
}

fn argb(a: Int, r: Int, g: Int, b: Int): Color {
    return Color.Argb(a, r, g, b);
}

@ufcs
fn value(c: Color): Int {
    match c {
        Black { return 0; }
        Red { return 255 * 65536; }
        Green { return 255 * 256; }
        Yellow { return 255 * 65536 + 255 * 256; }
        Blue { return 255; }
        Magenta { return 255 * 65536 + 255; }
        Cyan { return 255 * 256 + 255; }
        White { return 255 * 65536 + 255 * 256 + 255; }
        Rgb(r, g, b) { return r * 65536 + g * 256 + b; }
        Argb(a, r, g, b) { return a * 65536 + r * 256 + g * 256 + b; }
    }
    return 0;
}

class Theme {
    var window_bg: Color = Color.Rgb(242, 242, 242);
    var button_fill: Color = Color.Rgb(47, 111, 196);
    var button_hover: Color = Color.Rgb(66, 133, 220);
    var button_pressed: Color = Color.Rgb(30, 80, 150);
    var button_ink: Color = Color.White;
    var label_ink: Color = Color.Rgb(32, 32, 32);
    var field_fill: Color = Color.White;
    var field_border: Color = Color.Rgb(160, 160, 160);
    var field_focus: Color = Color.Rgb(47, 111, 196);
    var accent: Color = Color.Rgb(47, 111, 196);
    var accent_hover: Color = Color.Rgb(66, 133, 220);
    var muted: Color = Color.Rgb(140, 140, 148);
    var track: Color = Color.Rgb(180, 180, 180);
    var select_fill: Color = Color.Rgb(200, 220, 255);
}

fn default_theme(): Theme {
    return Theme {};
}
