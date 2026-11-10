# GUIAPI

`import GUIAPI` loads `C:\CODE\RoseGoldRust\builtin\std\TerraLite\GUIAPI.rg`. TerraLite draws the screen and calls the script when the player presses Enter or clicks a row. A mod that still registers `SCREEN` and ships `assets/<namespace>/gui/<path>.json` keeps that list.

## What a mod calls

```
import GUIAPI;

fn picked(row: String) {
    print(row);
}

fn main(): Int {
    var screen = GUIAPI.createScreen("notes", GUIAPI.vec(320.0, 240.0), GUIAPI.vec(40.0, 48.0));
    GUIAPI.addElement(screen, GUIAPI.text("marker", "marker"));
    GUIAPI.on_press(screen, "n", picked);
    return 0;
}
```

| Call | What it records |
| --- | --- |
| `GUIAPI.vec(x, y)` | A `Vec2`. `x` and `y` are `Float` |
| `GUIAPI.text(name, label)` | A `TextElement`. `name` is the row id. `label` is the words drawn |
| `GUIAPI.button(name, label)` | A `TextElement` drawn as a raised plate. Same row id and press as `text` |
| `GUIAPI.heading(name, label)` | A gold label with a rule. Up, down, and Enter skip it |
| `GUIAPI.icon(name, block)` | A square well showing that block's face. Up, down, and Enter skip it |
| `GUIAPI.field(name, label)` | A dark line you can type. The label may be empty. Enter commits it |
| `GUIAPI.read(screen, name)` | The words stored on that row, as a `String` |
| `GUIAPI.place(element, at)` | Sets that row's position. `addElement` then keeps the box |
| `GUIAPI.createScreen(name, size, position)` | The screen. `position` is pixels from the top left of the window |
| `GUIAPI.addElement(screen, element)` | Adds that row. At most 12. A row still at (16, 48) stacks 8 pixels under the lowest row. Any other position stays where it was set |
| `GUIAPI.removeElement(screen, element)` | Drops the row with that name |
| `GUIAPI.on_press(screen, key, fn)` | `key` is one lowercase letter, not `e` or `i`, and not a letter another screen already uses. Enter or a click calls `fn(row: String)` |
| `GUIAPI.open(screen)` / `GUIAPI.openNamed(name)` | From a hook (`events.use_block`, …), open that registered panel. False when the name is unknown |
| `GUIAPI.say(line)` | Queues one chat line. Letters and spaces, at most 32 characters. It does not change the world |

`TextElement` also has `size`, `position`, `color`, `font`, `fontSize`, and `fontColor`. `color` and `fontColor` are `std.ui`'s `Color` (`Color.White`, `Color.Black`, `Color.Rgb`, `Color.Argb`). `color` is the plate. `fontColor` is the glyph color, and `Color.Black` draws black. `text()` starts the plate at a dark gray and the glyphs at white. `font` is `""` or `bitmap` for the mixed face, `capital` for the tall capitals, `button` for the raised plate, `heading` for a gold label with a rule, `icon` for a block face, or `field` for a typed line. Anything else is refused. An empty label is allowed only on a field. `button()`, `heading()`, `icon()`, and `field()` set that `font`. A press that wants the typed words calls `read`. The script's own `TextElement` does not see the keystrokes. The client draws that face at `fontSize` (16 is the normal size). `Color.Argb` keeps the red, green, and blue and drops the alpha.

`name` on a screen or a row is a lowercase id: letters, digits, and underscores.

## What the game does

During `main`, those calls are stored on the pack. The client opens the screen on its key, or when a hook calls `gui.open` / `GUIAPI.open`. Escape, or that same key, closes it. Up and down move the row. Enter calls the press function with the row name. A click on the row does the same. The open and close still notify `hooks.on_screen` / `events.open_screen` with the screen name. The press function cannot write the world. `GUIAPI.say` shows a line in chat. `print` still goes to the process stdout.

`Register.SCREEN.register` still notes `assets/<namespace>/gui/<path>.json` list ids. Pair it with `GUIAPI.createScreen` when the panel should draw and open from a block entity.
