// A mod screen. TerraLite draws it and calls the press function on Enter or a click.
// import GUIAPI;
// var screen = GUIAPI.createScreen("notes", GUIAPI.vec(320.0, 240.0), GUIAPI.vec(40.0, 48.0));
// GUIAPI.addElement(screen, GUIAPI.text("marker", "marker"));
// GUIAPI.on_press(screen, "n", picked);
import ui;
from ui import Color;

class Vec2 {
    var x: Float = 0.0;
    var y: Float = 0.0;
}

abstract class Element {
    var name: String = "";
    var size: Vec2 = Vec2 { x: 0.0, y: 0.0 };
    var position: Vec2 = Vec2 { x: 0.0, y: 0.0 };
}

class TextElement extends Element {
    var name: String = "";
    var size: Vec2 = Vec2 { x: 200.0, y: 32.0 };
    var position: Vec2 = Vec2 { x: 16.0, y: 48.0 };
    var text: String = "";
    var color: Color = Color.White;
    var font: String = "";
    var fontSize: Int = 16;
    var fontColor: Color = Color.White;
}

class SliderElement extends Element {
    var name: String = "";
    var size: Vec2 = Vec2 { x: 200.0, y: 32.0 };
    var position: Vec2 = Vec2 { x: 16.0, y: 48.0 };

    var min: Float = 0.0;
    var max: Float = 100.0;
    var value: Float = 0.0;
}

class ButtonElement extends Element {
    var name: String = "";
    var size: Vec2 = Vec2 { x: 200.0, y: 32.0 };
    var position: Vec2 = Vec2 { x: 16.0, y: 48.0 };
    var text: String = "";
}

class NineSliceElement extends Element {
    var name: String = "";
    var size: Vec2 = Vec2 { x: 200.0, y: 32.0 };
    var position: Vec2 = Vec2 { x: 16.0, y: 48.0 };
    var texture: String = "";
    var textureSize: Vec2 = Vec2 { x: 200.0, y: 32.0 };
    var texturePosition: Vec2 = Vec2 { x: 16.0, y: 48.0 };
}

class CheckboxElement extends Element {
    var name: String = "";
    var size: Vec2 = Vec2 { x: 200.0, y: 32.0 };
    var position: Vec2 = Vec2 { x: 16.0, y: 48.0 };
    var text: String = "";
    var checked: Bool = false;
}

class DropdownElement extends Element {
    var name: String = "";
    var size: Vec2 = Vec2 { x: 200.0, y: 32.0 };
    var position: Vec2 = Vec2 { x: 16.0, y: 48.0 };
    var text: String = "";
    var options: Array[String] = [];
}

class Screen {
    var name: String = "";
    var size: Vec2 = Vec2 { x: 0.0, y: 0.0 };
    var position: Vec2 = Vec2 { x: 0.0, y: 0.0 };
    var elements: Array[TextElement] = [];
}

fn vec(x: Float, y: Float): Vec2 {
    return Vec2 { x: x, y: y };
}

fn color_bits(c: Color): Int {
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
        Argb(a, r, g, b) { return r * 65536 + g * 256 + b; }
    }
}

fn text(name: String, label: String): TextElement {
    return TextElement { name: name, text: label, color: Color.Rgb(23, 26, 33) };
}

fn button(name: String, label: String): TextElement {
    return TextElement { name: name, text: label, size: vec(120.0, 32.0), color: Color.Rgb(196, 148, 58), font: "button", fontColor: Color.Rgb(32, 22, 8) };
}

fn heading(name: String, label: String): TextElement {
    return TextElement { name: name, text: label, size: vec(288.0, 20.0), color: Color.Rgb(23, 26, 33), font: "heading", fontColor: Color.Rgb(232, 196, 96) };
}

fn icon(name: String, block: String): TextElement {
    return TextElement { name: name, text: block, size: vec(32.0, 32.0), color: Color.Rgb(23, 26, 33), font: "icon" };
}

fn field(name: String, label: String): TextElement {
    return TextElement { name: name, text: label, size: vec(288.0, 32.0), color: Color.Rgb(16, 18, 24), font: "field", fontColor: Color.Rgb(236, 232, 214) };
}

fn read(screen: Screen, name: String): String {
    return gui.read(screen.name, name);
}

fn place(element: TextElement, at: Vec2): TextElement {
    element.position = at;
    return element;
}

fn createScreen(name: String, size: Vec2, position: Vec2): Screen {
    gui.screen(name, size.x, size.y, position.x, position.y);
    return Screen { name: name, size: size, position: position, elements: [] };
}

fn at_rest(element: TextElement): Bool {
    return element.position.x == 16.0 && element.position.y == 48.0;
}

fn addElement(screen: Screen, element: TextElement) {
    if at_rest(element) {
        var y = 48.0;
        var seen = false;
        for item in screen.elements {
            var below = item.position.y + item.size.y + 8.0;
            if !seen || below > y {
                y = below;
                seen = true;
            }
        }
        if seen {
            element.position = vec(16.0, y);
        }
    }
    screen.elements.push(element);
    gui.element(screen.name, element.name, element.text, element.position.x, element.position.y, element.size.x, element.size.y, color_bits(element.color), element.font, element.fontSize, color_bits(element.fontColor));
}

fn removeElement(screen: Screen, element: TextElement) {
    var kept: Array[TextElement] = [];
    for item in screen.elements {
        if item.name != element.name {
            kept.push(item);
        }
    }
    screen.elements = kept;
    gui.remove(screen.name, element.name);
}

fn on_press(screen: Screen, key: String, pressed: fn(String): Void) {
    gui.press(screen.name, key, pressed);
}

fn say(line: String): Bool {
    return chat.say(line);
}
