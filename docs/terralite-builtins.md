# TerraLite builtins for RoseGold mods

Read-only look at what a `.rg` mod can call inside TerraLite, and how that surface compares with the Java language host and the Rust interpreter. No code was changed.

TerraLite is `C:\CODE\TerraLite`. The language the game actually runs is the Rust interpreter in `C:\CODE\RoseGoldRust` (`rosegold-eval::Session`), loaded by `C:\CODE\TerraLite\crates\rosegold`. The Java tree at `C:\CODE\RoseGoldJava` is the language oracle. It has no game world, no hooks, and no `ModAPI`.

`Pack::builtin()` in TerraLite is the block and item pack, not a RoseGold module. A mod does not import it.

## What a mod can call

A script is real RoseGold. `main` registers hooks and content. If the file defines `on_init`, that runs after `main`. While a hook is running, `world`, `player`, and `item` ask the game. `print` from a hook is written to stdout.

The interpreter also loads the language hosts and the std modules below. Those are not TerraLite APIs. They work because the game embeds the full interpreter.

### Hooks

Register with `hooks.<name>(id, fn)` or the Fabric-style `events.<name>(id, fn)`. The id is a non-empty string. A duplicate id of the same kind fails the load. An unknown name is `unknown hook {name}`.

| Stored name | Older name | Fabric name | Function the game calls | Cancel |
| --- | --- | --- | --- | --- |
| `before_break` | `hooks.before_break` | none | `fn(block: String): Bool` | `false` cancels |
| `break_block` | none | `events.break_block` | `fn(block: String, x: Int, y: Int, z: Int): Bool` | `false` cancels |
| `before_place` | `hooks.before_place` | `events.place_block` | `fn(item: String, x: Int, y: Int, z: Int): Bool` | `false` cancels |
| `after_break` | `hooks.after_break` | `events.broken` | `fn(block: String, x, y, z, left: String)` | no |
| `after_place` | `hooks.after_place` | `events.placed` | `fn(item: String, x, y, z)` | no |
| `on_select` | `hooks.on_select` | `events.select_slot` | `fn(slot: Int, item: String)` | no |
| `on_screen` | `hooks.on_screen` | `events.open_screen` | `fn(screen: String, open: Bool)` | no |
| `break_speed` | `hooks.break_speed` | `events.break_speed` | `fn(block: String): Float` | scale, not cancel |
| `pickup_item` | none | `events.pickup_item` | `fn(item: String): Bool` | `false` leaves the drop |
| `break_drop` | none | `events.break_drop` | `fn(block: String, left: String): String` | `""` drops nothing |
| `entity_spawn` | none | `events.entity_spawn` | `fn(kind: String, x, y, z): Bool` | `false` cancels |
| `entity_hurt` | none | `events.entity_hurt` | `fn(kind: String, amount: Int): Bool` | `false` cancels |
| `entity_death` | none | `events.entity_death` | `fn(kind: String, x, y, z): Bool` | `false` cancels despawn/loot side effects when the host asks |
| `entity_tick` | none | `events.entity_tick` | `fn(kind: String)` | no |

`hooks.before_place` and `events.place_block` are one list. Both are called with four arguments. Only break keeps two lists: the one-argument `hooks.before_break` and the four-argument `events.break_block`. There is no `hooks.pickup_item`, `hooks.break_drop`, `hooks.entity_spawn`, `hooks.entity_hurt`, or `hooks.entity_tick`. Those exist only as `events.*`.

A hook that throws cancels break, place, pickup, and hurt. After-hooks, select, screen, speed, spawn, and tick log the error and continue. Break-speed values multiply and are clamped to `0.05..=8`. A non-positive or non-finite speed is ignored. The last break-drop string wins. `None` from the host (the function did not return a string) keeps the broken block.

Rust `Hooks` run before script hooks. One cancel is enough. After-break and after-place run only when the cell changes. Air, a cell outside the world, and a refused place do not call hooks.

`entity_spawn` runs for `"player"` on join, then `"pig"` at `(5, 1, 9)` and `"zombie"` at `(11, 1, 13)`. The client and the server both do this. `entity_tick` runs every 50 ms, at most 20 steps per catch-up, for `"player"`, `"sheep"`, `"pig"`, and `"zombie"`. `entity_hurt` runs for the player on a fall of four blocks or more, and from one other client hit path. Comments in `ScriptTable::entity_spawn`, `entity_hurt`, and `entity_tick` say the engine does not call hurt and tick. Those comments are stale.

### Queries inside a hook

These names are the ones `crates/rosegold` forwards. The game answers them in `crates/world/src/rosegold.rs`.

| Call | Result |
| --- | --- |
| `world.block` / `world.get_block(x, y, z)` | Block id, or `""` outside the world |
| `world.is_air(x, y, z)` | True only when the cell is in bounds and air |
| `world.get_meta(x, y, z)` | Metadata nibble, or `0` outside |
| `world.set_meta(x, y, z, meta)` | Writes `0..=15` on a non-air in-bounds cell |
| `world.write` / `world.set_block(x, y, z, id)` | Bool. `"air"` clears. An unknown id returns false and does not write |
| `world.hardness(id)` / `item.hardness(id)` | Seconds as a float. Missing is `0`. Lookup is metadata `0` |
| `world.light(id)` / `item.light(id)` | Light level as an int. Missing is `0` |
| `world.replaces(id)` / `item.replaces(id)` | Break-result block id |
| `world.slot(index)` / `player.stack(index)` | Hotbar item id. Index is 1-based. Out of range is `""` |
| `world.count(index)` / `player.count(index)` | Stack count. Out of range is `0` |
| `player.held()` | Item in the selected slot, or `""` |
| `player.selected()` | 1-based slot, or `0` when there is no player |
| `player.present()` | True when the hook was given an eye position |
| `player.x()` / `player.y()` / `player.z()` | Eye components, or `0` |
| `world.time_of_day()` | `0` at midnight, `0.5` at noon, `-1` when the hook has no sky |
| `world.is_day()` | True when time is in `[0.25, 0.75)` |
| `world.season()` | Lowercased season, or `""` |
| `world.has_time()` | True when the hook was given a clock |
| `item.exists(id)` | Pack contains the item |
| `item.max_count(id)` | `64` if the item exists, otherwise `0` |

Anything else in those modules is `unknown function {name}`, including a call made outside a hook (`{module}.{name} is only available inside a hook`).

Writes are allowed for break, place, and spawn. They are refused for select, screen, break speed, pickup, hurt, and tick. A refused write returns false. `drain_writes` is the cells `world.write` changed, not counting the edit cell itself.

Break, place, and spawn on the authority build the view from the synced hotbar only. Eye, time, and season are empty, so `player.present` and `world.has_time` are false, time is `-1`, and `player.x/y/z` are `0`. Client select, screen, break speed, and pickup pass the real eye and clock.

The hotbar the server shows a hook is the last client hotbar (`set_inventory`), not a live inventory. `item.max_count` is a flat 64. Hardness, light, and replaces always look up the block of that id at metadata 0, so a non-block item reads as a missing block.

### Registration during `main` / `on_init`

| Call | What the game does |
| --- | --- |
| `block.register(namespace, path)` | Adds `namespace:path` to the pack. Returns that id. Duplicate id fails |
| `item.register(namespace, path)` | Same for an item. A script item is not a block |
| `block.texture(id, path)` | Pack PNG, or another block's texture |
| `block.model(id, model)` | Pack shape, pack model, or a mod file. Prefer `assets/<ns>/models/block/<id>.json` / `.bbmodel`. Default is `cube` |
| `block.hardness(id, seconds)` | Seconds to break. Positive float. Same script can set it again |
| `block.light(id, level)` | Emit light `1..=15` (warm orange). Same script can set it again |
| `item.places(item, block)` | That item places that block from the hotbar |
| `item.stack(id, count)` | Hotbar stack limit `1..=64`. Default is 64. `item.max_count` reads it |
| `recipe.register(result, ingredient)` | One crafting line. Both ids must already exist |
| `recipe.shapeless(result, ingredients)` | A pile, ids joined with `+`. Order on the grid does not matter |
| `recipe.shaped(result, grid)` | Nine cells separated by commas, left to right, top to bottom. An empty cell is blank. The shape can sit anywhere on the 3×3 |
| `entity.register(namespace, path)` | A simple mob `namespace:path`. It stands just east of the oak stairs. The same script can register it again |
| `entity.model(id, path)` | Blockbench `.bbmodel` for that mob (pack or mod assets) |
| `entity.hostile(id, flag)` | Default chase/attack AI when the host Guest supports it |
| `entity.spawn_egg(id, item)` | Creative/hotbar item that spawns this mob on use |
| `entity.spawn(id, x, y, z)` / `entity.despawn(...)` | Place or remove a body. During `main` it is recorded for load; inside a hook the host places it live |

`import ModAPI` is the Fabric-style entry. It lives at `C:\CODE\RoseGoldRust\builtin\std\TerraLite\ModAPI.rg` (same folder as `GUIAPI.rg` and `SettingsAPI.rg`). The old `builtin/std/TLModAPI/` path is gone.

- `TLMod.log` calls `modapi.note`. The game prints `rosegold: {message}`.
- `TLRegistry.open` calls `modapi.registry`. The game stores `("registry", kind)` and does nothing else with it.
- `Register.BLOCK.register` and `Register.ITEM.register` are `block.register` and `item.register`.
- `Register.texture` / `model` / `hardness` / `light` / `places` / `stack` / `recipe` forward to the block and item calls above.
- `import Blocks` resolves to `builtin/std/TerraLite/Blocks.rg`. `Blocks.defineBlock(namespace, path, model, texture, seconds, glow)` registers a block, applies those fields when non-empty / positive, and returns a `Block` with a filled `BlockProperties` bag (`.id` is the registered id). Also `Blocks.texture` / `model` / `hardness` / `light`.
- `import Items` resolves to `builtin/std/TerraLite/Items.rg`. `Items.defineItem(namespace, path, places, count)` registers an item, optionally places a block, optionally sets stack size (`count` 0 keeps 64), and returns an `Item` with a filled `ItemProperties` bag (`.id` is the registered id). Free `Items.places` / `stack` still forward to the host. An `Item` also has `.places(target)` / `.stack(count)` methods that update its properties and call the host with that item's id.
- `import Entities` resolves to `builtin/std/TerraLite/Entities.rg`. `Entities.defineEntity(namespace, path, model, egg_path, hostile)` registers a mob, optional Blockbench model, optional spawn-egg item path, and hostile AI flag; returns an `Entity` with `EntityProperties`. Free `Entities.setModel` / `setHostile` / `spawnEgg` / `spawn` / `despawn` / `spawnNear` forward to the host. `spawnNear` needs `player.*` (hook only).
- `Tickable` / `Interactable` on ModAPI are empty trait stubs for later entity-style hooks.
- Any other kind calls `modapi.content(kind, namespace, path)`. The kind must be uppercase (`BLOCK_ENTITY`, `SCREEN`, `COMMAND`, …). The namespace and path must be lowercase.

Of those stored entries, the game uses two:

- `BLOCK_ENTITY` is remembered by the pack (`note_block_entity`). It does not create a working block entity.
- `SCREEN` is added to the pack's script screen list (`add_script_screen`). It does not open a UI.
- `COMMAND` becomes a chat-row label in the client (`command_rows`). The command does not run.
- `PARTICLE` picks a color (`particle_color`). It does not spawn particles.

`ENTITY` is the same mob as `entity.register`: one small body stands just east of the oak stairs. `ITEM_GROUP`, `EVENT`, `CONFIG`, `RESOURCE`, and `TEXTURE` are stored on the hook table and not read by the client or the server. `Register.texture`, `Register.model`, `Register.places`, and `Register.recipe` are the block and item calls above, not the `TEXTURE` registry.

`modapi` itself only has `note`, `registry`, and `content`.

### Std modules the interpreter loads

TerraLite does not ship its own std. `Session::load` uses the Rust tree.

Language hosts, same names as Java: `checks` (`eq`, `neq`, `eq_string`, `that`, `truthy`), `process` (`argv`, `argc`), `__math`, `__str`, `__io`, `__uuid`, `__time`, `__path`, `__json`, `__regex`, `__ui`. Top-level `print`, `len`, `assert`, `argv`, and `argv_len` are the interpreter, not TerraLite.

Std children Java also has: `std.math`, `std.str`, `std.io`, `std.vec`, `std.time`, `std.path`, `std.json`, `std.regex`, `std.ui`.

Std children only the Rust tree has: `std.collections`, `std.helpers`, `std.ModAPI`, `std.Blocks`, `std.Items`, `std.Entities`, `std.GUIAPI`, `std.SettingsAPI`.

`std.helpers` is the small mod helper (`id`, `same_id`, `holding`, `block_at`, `air`, `replace_block`, `stack_at_least`). It calls `player.held`, `world.get_block`, `world.is_air`, `world.set_block`, and `player.count`, so it only works inside a hook. `rose-kit.rg` imports it.

`std.ui` and `__ui` are the desktop window host (Win32, or a hidden window). They are not TerraLite screens. A mod can call them. The game will not show that window as a pack screen.

`__io` paths are sandboxed at the mod root. For `src/mod.rg` that root is the folder above `src`. Otherwise it is the script's folder. `Pack::builtin().load_mod_data` reads files from that root. That is pack data, not a RoseGold call.

### Shipped mods

| File | What it calls |
| --- | --- |
| `mods/keep-stone.rg` | `hooks.before_break` |
| `mods/log-place.rg` | `hooks.after_place` |
| `mods/log-break.rg` | `hooks.after_break` |
| `mods/log-player.rg` | `hooks.on_select`, `hooks.on_screen` |
| `mods/field-notes.rg` | `hooks.break_speed`, `hooks.on_select`, `hooks.after_break`, `world.slot`, `world.count`, `world.block`, `world.write` |
| `mods/red-wool.rg` | `events.placed`, `events.break_drop`, `events.pickup_item`, `world.get_block`, `world.get_meta`, `world.set_meta`, `player.held`, `world.has_time`, `world.is_day`, `world.season`, `player.present`, `player.x`, `item.exists`, `item.max_count` |
| `mods/rose-kit.rg` | `import helpers`, `block.register`, `block.texture`, `block.model`, `item.register`, `item.places`, `recipe.register`, `recipe.shapeless`, `recipe.shaped`, `entity.register`, `events.placed`, `events.entity_spawn`, `events.entity_hurt`, `events.entity_tick`, `world.set_block` |
| `mods/ember.rg` | `import ModAPI`, `import Blocks`, `import Items`, `import Entities`, `Blocks.defineBlock` / `Items.defineItem` / `Entities.defineEntity`, glow brick, shade mob (zombie.bbmodel + hostile + spawn egg) |
| `mods/k's-guide/src/mod.rg` | `import ModAPI`, `on_init`, registries for item, block, entity, block entity, item group, screen, event, command, config, resource, texture, particle |

## Compared with Java

Java `Types.isHostModule` is `checks`, `process`, `__math`, `__str`, `__io`, `__uuid`, `__time`, `__path`, `__json`, `__regex`, `__ui`. Java `Stdlib.CHILDREN` is `math`, `str`, `io`, `vec`, `time`, `path`, `json`, `regex`, `ui`.

Java has no `hooks`, `events`, `world`, `player`, `item`, `block`, `recipe`, or `modapi`. `import ModAPI` and `import helpers` do not resolve. `builtin/std` in the Java tree has no `TerraLite/ModAPI.rg`, `helpers`, or `collections`. A TerraLite mod does not run on the Java interpreter.

The Rust interpreter implements those Java host names. TerraLite therefore has the language hosts Java has, plus the game modules Java does not.

Java also has a Godot stub under `builtin/godot`. TerraLite does not load it. That is outside this host.

`std.ui` sources now live in the Rust tree. The Java checkout no longer finds `std.ui`. TerraLite uses the Rust copy. That desktop UI is still not a game screen.

## Compared with the Rust interpreter

The game and the interpreter agree on the hook names and the query names. `canonical_hook` and `host_arity` in `rosegold-eval` are the lists `crates/rosegold` forwards, and `answer` in `crates/world/src/rosegold.rs` covers every query that list can build.

What the interpreter accepts and the game does not honor:

- Registry kinds other than `BLOCK`, `ITEM`, `ENTITY`, `BLOCK_ENTITY`, and `SCREEN` do not change the world. `ENTITY` stands a mob beside the stairs. `COMMAND` and `PARTICLE` only affect a chat label and a color.
- `modapi.registry` is stored and unused.
- `std.ui` / `__ui` open a desktop window, not a TerraLite screen.
- `std.collections` is a language module. No shipped mod uses it, and the game has no collection API of its own.

What the game does that a bare `rg run` does not:

- Outside TerraLite, `world.*` and `player.*` fail with "only available inside a hook", and there is no hook caller.
- `block.register` and the other registry calls only append to the session. Nothing places a block until TerraLite's `register` writes the pack.
- The authority view is bare, so player position and the sky are empty during server break, place, and spawn. The interpreter itself does not invent an eye or a clock. TerraLite chooses not to pass them on that path.

## What is wrong or missing

1. `hooks.before_place` is always called as `(item, x, y, z)`. Only `hooks.before_break` is one argument. The world crate's module comment lists the two side by side and does not say that. A one-argument place function fails at the call. `hooks.before_place` and `events.place_block` are the same list.
2. Server break, place, and spawn hooks cannot see the player or the sky. `red-wool.rg` reads `world.has_time`, `player.present`, and `player.x` from a pickup hook, which is a client hook, so that one can see them. The same reads from `before_break` would be false and zero.
3. Hurt and tick cannot write the world. Spawn can. The comments that say hurt and tick are never called are wrong. The client and the server call both.
4. `item.stack` sets a per-item hotbar limit. `item.max_count` reads that value (default 64).
5. `item.hardness`, `item.light`, and `item.replaces` read the block table at metadata 0. An item that is not a block looks missing. Script `block.hardness` / `block.light` feed that table for registered blocks.
6. The Fabric-style registries in `k's-guide` mostly record names. Entities, item groups, events, config, resources, and the `TEXTURE` registry do not create game objects. Block entities are a list of ids. Screens are a list of ids. Commands are labels. Particles are a color.
7. The hotbar a hook sees is the last synced client hotbar, five slots in the current client, indexed from 1. There is no armor, offhand, or full inventory.
8. Java cannot load these mods. The game hosts and `ModAPI` / `helpers` exist only in the Rust interpreter.
