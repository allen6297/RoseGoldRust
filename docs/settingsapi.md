# SettingsAPI

`import SettingsAPI` loads `C:\CODE\RoseGoldRust\builtin\std\TerraLite\SettingsAPI.rg`. TerraLite adds each option to the settings page, above Back.

## What a mod calls

```
import SettingsAPI;
import GUIAPI;

fn main(): Int {
    var game = SettingsAPI.createCategory("game", "how it feels");
    var glow = SettingsAPI.createOption(game, "glow", "soft glow", "on");
    SettingsAPI.on_change(glow, fn(name: String) {
        GUIAPI.say("glow " + SettingsAPI.value(name));
    });
    return 0;
}
```

| Call | What it records |
| --- | --- |
| `SettingsAPI.createCategory(name, description)` | A group on the settings page. The description is the line under that group |
| `SettingsAPI.createOption(category, name, description, default)` | A settings row. `name` is the button. The line under it is `description` and the current value |
| `SettingsAPI.on_change(option, fn)` | Enter or Space on that row calls `fn(name: String)` |
| `SettingsAPI.choices(option, list)` | Words the row cycles. Two to four lowercase words. The current value must be one of them |
| `SettingsAPI.value(name)` | The current value. Inside the change function this is the value just stepped |

`name` is a lowercase id, at most 16 characters. It cannot be one of the built-in rows (`world`, `hemisphere`, `volume`, `view_bob`, `trace`, `back`, and the main-menu names). `description` is lowercase letters and spaces, at most 48 characters. `default` is the same kind of text, at most 16 characters. A script can add four options. The whole game keeps four.

## What the game does

During `main`, `createOption` stores the row on the pack, including the category description. Settings draws a heading for each category above Back, then that category's rows. The line under the heading is the category description. The line under a row names the category, the row, and the current value. Enter or Space on a heading does nothing. Enter or Space on a row steps to the next choice and wraps. A row with no choice list still flips `on` and `off`, and any other single value stays. The game then calls the `on_change` function with the row name. `SettingsAPI.value` reads the value after the step. While a world is being played, that function may place one light block into air within four blocks of the player, or turn a light block there back into air. A second write, a write with no player, a write farther away, and a write that is not a light all return false. `GUIAPI.say` from that function shows a chat line, including while the settings page is open.
