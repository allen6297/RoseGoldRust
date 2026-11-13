// TerraLite names a RoseGold mod can call. Hooks and the game modules are the
// list in docs/terralite-builtins.md. GUIAPI and SettingsAPI are the names in
// builtin/std/TerraLite/GUIAPI.rg, SettingsAPI.rg, and docs/guiapi.md.

/** @typedef {{ signature: string, doc: string, kind?: string }} Member */

/** @type {Record<string, { doc: string, members: Record<string, Member> }>} */
const MODULES = {
  hooks: {
    doc: "Register a hook with hooks.<name>(id, fn). The id is a non-empty string.",
    members: {
      before_break: member(
        "hooks.before_break(id: String, fn)",
        "Callback fn(block: String): Bool. False cancels the break. One argument. A throw also cancels."
      ),
      before_place: member(
        "hooks.before_place(id: String, fn)",
        "Same list as events.place_block. Callback fn(item: String, x: Int, y: Int, z: Int): Bool. False cancels. A throw also cancels."
      ),
      after_break: member(
        "hooks.after_break(id: String, fn)",
        "Callback fn(block: String, x: Int, y: Int, z: Int, left: String). Runs only when the cell changes. left is the block now in the cell."
      ),
      after_place: member(
        "hooks.after_place(id: String, fn)",
        "Callback fn(item: String, x: Int, y: Int, z: Int). Runs only when the cell changes."
      ),
      on_select: member(
        "hooks.on_select(id: String, fn)",
        "Callback fn(slot: Int, item: String). Slot is 1-based. An empty slot passes \"\"."
      ),
      on_screen: member(
        "hooks.on_screen(id: String, fn)",
        "Callback fn(screen: String, open: Bool)."
      ),
      break_speed: member(
        "hooks.break_speed(id: String, fn)",
        "Callback fn(block: String): Float. 0.5 breaks in half the time. Values multiply and clamp to 0.05..=8. Non-positive or non-finite is ignored."
      ),
    },
  },
  events: {
    doc: "Fabric-style hook names. events.<name>(id, fn). The id is a non-empty string.",
    members: {
      break_block: member(
        "events.break_block(id: String, fn)",
        "Callback fn(block: String, x: Int, y: Int, z: Int): Bool. False cancels. Separate from the one-argument hooks.before_break."
      ),
      place_block: member(
        "events.place_block(id: String, fn)",
        "Same list as hooks.before_place. Callback fn(item: String, x: Int, y: Int, z: Int): Bool. False cancels."
      ),
      broken: member(
        "events.broken(id: String, fn)",
        "Same list as hooks.after_break. Callback fn(block: String, x: Int, y: Int, z: Int, left: String)."
      ),
      placed: member(
        "events.placed(id: String, fn)",
        "Same list as hooks.after_place. Callback fn(item: String, x: Int, y: Int, z: Int)."
      ),
      select_slot: member(
        "events.select_slot(id: String, fn)",
        "Same list as hooks.on_select. Callback fn(slot: Int, item: String)."
      ),
      open_screen: member(
        "events.open_screen(id: String, fn)",
        "Same list as hooks.on_screen. Callback fn(screen: String, open: Bool)."
      ),
      break_speed: member(
        "events.break_speed(id: String, fn)",
        "Same list as hooks.break_speed. Callback fn(block: String): Float."
      ),
      pickup_item: member(
        "events.pickup_item(id: String, fn)",
        "Callback fn(item: String): Bool. False leaves the drop on the ground. No hooks.pickup_item name."
      ),
      break_drop: member(
        "events.break_drop(id: String, fn)",
        "Callback fn(block: String, left: String): String. The item id to drop, or \"\" for none. The last hook's string wins."
      ),
      entity_spawn: member(
        "events.entity_spawn(id: String, fn)",
        "Callback fn(kind: String, x: Int, y: Int, z: Int). Runs for \"player\" on join, then \"pig\" and \"zombie\"."
      ),
      entity_hurt: member(
        "events.entity_hurt(id: String, fn)",
        "Callback fn(kind: String, amount: Int): Bool or Int. False or an Int of 0 or less cancels. An Int replaces the amount. True or void keeps it. This hook cannot write the world."
      ),
      entity_death: member(
        "events.entity_death(id: String, fn)",
        "Callback fn(kind: String, x: Int, y: Int, z: Int): Bool. False cancels despawn and loot. A throw also cancels."
      ),
      entity_tick: member(
        "events.entity_tick(id: String, fn)",
        "Callback fn(kind: String). The client and the server tick \"player\", \"sheep\", \"pig\", and \"zombie\" every 50 ms. This hook cannot write the world."
      ),
      tick: member(
        "events.tick(id: String, fn)",
        "Callback fn(n: Int). Once a second. n starts at 1. ModAPI.bindTick and TLMod.listenTick register this."
      ),
      use_block: member(
        "events.use_block(id: String, fn)",
        "Callback fn(item: String, block: String, x: Int, y: Int, z: Int): Bool. False consumes the right click on a block."
      ),
      use_item: member(
        "events.use_item(id: String, fn)",
        "Callback fn(item: String): Bool. False consumes a right click with the held item."
      ),
      use_entity: member(
        "events.use_entity(id: String, fn)",
        "Callback fn(item: String, kind: String): Bool. False consumes a right click on a mob."
      ),
      structure_discovered: member(
        "events.structure_discovered(id: String, fn)",
        "Callback fn(structure: String): Bool. Fires once per named landmark on first approach. False cancels the built-in tip. chat.say lines still show."
      ),
    },
  },
  world: {
    doc: "Cell, pack, hotbar, and sky questions. Only available inside a hook. Writes work during break, place, and spawn.",
    members: {
      block: member(
        "world.block(x: Int, y: Int, z: Int): String",
        "Block id in that cell, or \"\" outside the world."
      ),
      get_block: member(
        "world.get_block(x: Int, y: Int, z: Int): String",
        "Same as world.block. Block id, or \"\" outside the world."
      ),
      is_air: member(
        "world.is_air(x: Int, y: Int, z: Int): Bool",
        "True only when the cell is in bounds and air. False outside the world."
      ),
      get_meta: member(
        "world.get_meta(x: Int, y: Int, z: Int): Int",
        "Metadata nibble, or 0 outside the world."
      ),
      set_meta: member(
        "world.set_meta(x: Int, y: Int, z: Int, meta: Int)",
        "Writes metadata 0..=15 on a non-air cell that is in bounds."
      ),
      write: member(
        "world.write(x: Int, y: Int, z: Int, id: String): Bool",
        "Sets a pack block or a block.register id. \"air\" clears the cell. An unknown id returns false and does not write. Refused during select, screen, break speed, pickup, hurt, and tick."
      ),
      set_block: member(
        "world.set_block(x: Int, y: Int, z: Int, id: String): Bool",
        "Same as world.write."
      ),
      hardness: member(
        "world.hardness(id: String): Float",
        "Break time in seconds. Missing is 0. Lookup uses metadata 0. item.hardness is the same call."
      ),
      light: member(
        "world.light(id: String): Int",
        "Light level. Missing is 0. Lookup uses metadata 0. item.light is the same call."
      ),
      replaces: member(
        "world.replaces(id: String): String",
        "Break-result block id. Lookup uses metadata 0. item.replaces is the same call."
      ),
      slot: member(
        "world.slot(index: Int): String",
        "Hotbar item id. Index is 1-based. Out of range is \"\". player.stack is the same call. The bar is the last synced client hotbar."
      ),
      count: member(
        "world.count(index: Int): Int",
        "Stack count in that hotbar slot. Out of range is 0. player.count is the same call."
      ),
      time_of_day: member(
        "world.time_of_day(): Float",
        "0 at midnight and 0.5 at noon. -1 when the hook has no sky. Server break, place, and spawn have no sky."
      ),
      is_day: member(
        "world.is_day(): Bool",
        "True when time is in [0.25, 0.75)."
      ),
      season: member(
        "world.season(): String",
        "Lowercased season, or \"\" when the hook has no sky."
      ),
      has_time: member(
        "world.has_time(): Bool",
        "True when the hook was given a clock."
      ),
    },
  },
  player: {
    doc: "The local player, from the hook's view. Only available inside a hook.",
    members: {
      stack: member(
        "player.stack(index: Int): String",
        "Same as world.slot. Hotbar item id. Index is 1-based. Out of range is \"\"."
      ),
      count: member(
        "player.count(index: Int): Int",
        "Same as world.count. Stack count. Out of range is 0."
      ),
      held: member(
        "player.held(): String",
        "Item in the selected hotbar slot, or \"\"."
      ),
      selected: member(
        "player.selected(): Int",
        "1-based selected slot, or 0 when there is no player."
      ),
      present: member(
        "player.present(): Bool",
        "True when the hook was given an eye position. False for server break, place, and spawn."
      ),
      x: member("player.x(): Float", "Eye x, or 0 when the hook has no eye."),
      y: member("player.y(): Float", "Eye y, or 0 when the hook has no eye."),
      z: member("player.z(): Float", "Eye z, or 0 when the hook has no eye."),
    },
  },
  item: {
    doc: "Pack items. register and places run in main or on_init. The queries run inside a hook.",
    members: {
      register: member(
        "item.register(namespace: String, path: String): String",
        "Adds namespace:path. Returns that id. A script item is not a block. Duplicate id fails."
      ),
      places: member(
        "item.places(item: String, block: String)",
        "That item places that block from the hotbar."
      ),
      stack: member(
        "item.stack(id: String, count: Int): Bool",
        "Hotbar stack limit, 1 to 64. Default is 64. item.max_count reads it."
      ),
      exists: member("item.exists(id: String): Bool", "True when the pack contains the item."),
      hardness: member(
        "item.hardness(id: String): Float",
        "Same lookup as world.hardness. Missing is 0. A non-block item reads as a missing block."
      ),
      light: member(
        "item.light(id: String): Int",
        "Same lookup as world.light. Missing is 0."
      ),
      replaces: member(
        "item.replaces(id: String): String",
        "Same lookup as world.replaces. Break-result block id at metadata 0."
      ),
      max_count: member(
        "item.max_count(id: String): Int",
        "64 if the item exists, otherwise 0. Every existing item reports 64."
      ),
    },
  },
  block: {
    doc: "Pack blocks added from main or on_init.",
    members: {
      register: member(
        "block.register(namespace: String, path: String): String",
        "Adds namespace:path. Returns that id. A registered block places from the hotbar and from world.set_block. Duplicate id fails."
      ),
      texture: member(
        "block.texture(id: String, path: String)",
        "Attaches a pack PNG, or another block's texture."
      ),
      model: member(
        "block.model(id: String, model: String)",
        "Pack shape or model. The default is cube."
      ),
      hardness: member(
        "block.hardness(id: String, seconds: Float): Bool",
        "Dig time in seconds. Missing stays the pack default."
      ),
      light: member(
        "block.light(id: String, level: Int): Bool",
        "Block light 0 to 15."
      ),
    },
  },
  recipe: {
    doc: "Crafting lines for the Items screen. Both ids must already exist.",
    members: {
      register: member(
        "recipe.register(result: String, ingredient: String)",
        "One crafting line."
      ),
      shapeless: member(
        "recipe.shapeless(result: String, ingredients: String)",
        "A pile of ids joined with +. Order on the grid does not matter."
      ),
      shaped: member(
        "recipe.shaped(result: String, grid: String)",
        "Nine cells separated by commas, left to right, top to bottom. An empty cell is blank. The shape can sit anywhere on the 3×3."
      ),
    },
  },
  loot: {
    doc: "Equal-weight string lists for mods. Weighted {id,weight,when} entries are datapack JSON only.",
    members: {
      block: member(
        "loot.block(block: String, list: String)",
        "Break drops for block. list is a+b(+c…); every id drops once. Overrides datapack loot/blocks/<id>.json."
      ),
      chest: member(
        "loot.chest(table: String, list: String)",
        "Replaces drops for a named chest table (camp_chest, mine_chest, …) with equal-weight ids. Datapack omen stays."
      ),
      entity: member(
        "loot.entity(id: String, list: String)",
        "Guest death drops. list is a+b(+c…); every id drops once at the body."
      ),
    },
  },
  entity: {
    doc: "A simple mob registered from main or on_init.",
    members: {
      register: member(
        "entity.register(namespace: String, path: String)",
        "A simple mob namespace:path. It stands just east of the oak stairs. The same script can register it again. ENTITY in TLModAPI is this same mob."
      ),
      model: member("entity.model(id: String, path: String): Bool", "Blockbench .bbmodel for that mob."),
      hostile: member(
        "entity.hostile(id: String, flag: Bool): Bool",
        "Default chase and attack when the host supports it."
      ),
      spawn_egg: member(
        "entity.spawn_egg(id: String, item: String): Bool",
        "Item that spawns this mob on use."
      ),
      hearts: member(
        "entity.hearts(id: String, n: Int): Bool",
        "Guest max hearts (1..=40, default 4)."
      ),
      hurtbox: member(
        "entity.hurtbox(id: String, radius: Float): Bool",
        "Aim sphere radius in blocks (0.2..=4.0, default 0.9)."
      ),
      speed: member(
        "entity.speed(id: String, scale: Float): Bool",
        "Hostile chase step scale (0.25..=4.0, default 1.0)."
      ),
      spawn: member(
        "entity.spawn(id: String, x: Int, y: Int, z: Int): Bool",
        "Place a body. During main it is recorded for load. Inside a hook the host places it."
      ),
      despawn: member(
        "entity.despawn(id: String, x: Int, y: Int, z: Int): Bool",
        "Remove a body at that cell."
      ),
    },
  },
  modapi: {
    doc: "The host calls TLModAPI records. note, registry, and content are the whole module.",
    members: {
      note: member(
        "modapi.note(message: String)",
        "TLMod.log calls this. The game prints rosegold: {message}."
      ),
      registry: member(
        "modapi.registry(kind: String)",
        "TLRegistry.open calls this. The game stores (\"registry\", kind) and does nothing else with it."
      ),
      content: member(
        "modapi.content(kind: String, namespace: String, path: String): String",
        "Used for registry kinds other than BLOCK and ITEM. kind is uppercase. namespace and path are lowercase."
      ),
    },
  },
  helpers: {
    doc: "import helpers. These calls use world and player, so they only work inside a hook.",
    members: {
      id: member("helpers.id(namespace: String, name: String): String", "Joins namespace:name."),
      same_id: member(
        "helpers.same_id(left: String, right: String): Bool",
        "True when the ids match, treating a missing namespace as the same name."
      ),
      holding: member("helpers.holding(id: String): Bool", "True when player.held() is that id."),
      block_at: member(
        "helpers.block_at(x: Int, y: Int, z: Int): String",
        "world.get_block(x, y, z)."
      ),
      air: member("helpers.air(x: Int, y: Int, z: Int): Bool", "world.is_air(x, y, z)."),
      replace_block: member(
        "helpers.replace_block(x: Int, y: Int, z: Int, was: String, next: String): Bool",
        "world.set_block when the cell id matches was."
      ),
      stack_at_least: member(
        "helpers.stack_at_least(slot: Int, n: Int): Bool",
        "player.count(slot) >= n."
      ),
    },
  },
  TLModAPI: {
    doc: "import TLModAPI. Fabric-style entry. on_init runs after main when the script defines it.",
    members: {
      createMod: member(
        "TLModAPI.createMod(name: String, version: String): TLMod",
        "Builds the mod. TLMod.log records a note."
      ),
      createRegister: member(
        "TLModAPI.createRegister(owner: TLMod): TLRegister",
        "Builds the registries. BLOCK and ITEM register real content. Other kinds call modapi.content."
      ),
    },
  },
  TLMod: {
    doc: "The mod object from TLModAPI.createMod.",
    members: {
      log: member("TLMod.log(message: String)", "Calls modapi.note. The game prints rosegold: {message}."),
      listenTick: member(
        "TLMod.listenTick(id: String, handler: fn(Int): Void)",
        "events.tick. Once a second. n starts at 1."
      ),
      listenEntityTick: member(
        "TLMod.listenEntityTick(id: String, handler: fn(String): Void)",
        "events.entity_tick."
      ),
      listenUseBlock: member(
        "TLMod.listenUseBlock(id: String, handler: fn(String, String, Int, Int, Int): Bool)",
        "events.use_block. False consumes the right click on a block."
      ),
      listenUseItem: member(
        "TLMod.listenUseItem(id: String, handler: fn(String): Bool)",
        "events.use_item. False consumes a right click with the held item."
      ),
      listenUseEntity: member(
        "TLMod.listenUseEntity(id: String, handler: fn(String, String): Bool)",
        "events.use_entity. False consumes a right click on a mob."
      ),
      listenStructureDiscovered: member(
        "TLMod.listenStructureDiscovered(id: String, handler: fn(String): Bool)",
        "events.structure_discovered. False cancels the built-in tip."
      ),
    },
  },
  TLRegistry: {
    doc: "One registry, such as Register.BLOCK.",
    members: {
      open: member("TLRegistry.open()", "Calls modapi.registry. The game stores the kind and does nothing else."),
      register: member(
        "TLRegistry.register(namespace: String, path: String): String",
        "BLOCK and ITEM call block.register and item.register. Any other kind calls modapi.content."
      ),
    },
  },
  TLRegister: {
    doc: "Registries from TLModAPI.createRegister. The report's Register.BLOCK is this object's BLOCK field.",
    members: {
      texture: member("TLRegister.texture(id: String, file: String)", "block.texture."),
      model: member("TLRegister.model(id: String, file: String)", "block.model."),
      places: member("TLRegister.places(held: String, target: String)", "item.places."),
      hardness: member("TLRegister.hardness(id: String, seconds: Float)", "block.hardness."),
      light: member("TLRegister.light(id: String, level: Int)", "block.light."),
      stack: member("TLRegister.stack(id: String, count: Int)", "item.stack. Count is 1 to 64."),
      recipe: member(
        "TLRegister.recipe(result: String, ingredient: String)",
        "recipe.register."
      ),
      ITEM: field("TLRegistry", "Item registry. register calls item.register."),
      BLOCK: field("TLRegistry", "Block registry. register calls block.register."),
      ENTITY: field("TLRegistry", "Same mob as entity.register."),
      BLOCK_ENTITY: field("TLRegistry", "Remembered by the pack. Does not create a working block entity."),
      ITEM_GROUP: field("TLRegistry", "Stored on the hook table. The game does not read it."),
      SCREEN: field("TLRegistry", "Added to the pack's script screen list. Does not open a UI."),
      EVENT: field("TLRegistry", "Stored on the hook table. The game does not read it."),
      COMMAND: field("TLRegistry", "Becomes a chat-row label. The command does not run."),
      CONFIG: field("TLRegistry", "Stored on the hook table. The game does not read it."),
      RESOURCE: field("TLRegistry", "Stored on the hook table. The game does not read it."),
      TEXTURE: field("TLRegistry", "Stored on the hook table. This is not block.texture."),
      PARTICLE: field("TLRegistry", "Picks a color. Does not spawn particles."),
    },
  },
  GUIAPI: {
    doc: "import GUIAPI. A screen TerraLite draws, plus one chat line.",
    members: {
      vec: member("GUIAPI.vec(x: Float, y: Float): Vec2", "A Vec2. x and y are Float."),
      color_bits: member(
        "GUIAPI.color_bits(c: Color): Int",
        "Packs a std.ui Color. White is 255 * 65536 + 255 * 256 + 255. Argb keeps red, green, and blue."
      ),
      text: member(
        "GUIAPI.text(name: String, label: String): TextElement",
        "A row. name is the row id. label is the words drawn. The plate starts dark gray and the glyphs start white."
      ),
      at_rest: member(
        "GUIAPI.at_rest(element: TextElement): Bool",
        "True when the element is still at x 16 and y 48."
      ),
      createScreen: member(
        "GUIAPI.createScreen(name: String, size: Vec2, position: Vec2): Screen",
        "Records the screen. position is pixels from the top left of the window. name is a lowercase id."
      ),
      addElement: member(
        "GUIAPI.addElement(screen: Screen, element: TextElement)",
        "Adds that row. At most 12. A row still at (16, 48) stacks 8 pixels under the lowest row."
      ),
      removeElement: member(
        "GUIAPI.removeElement(screen: Screen, element: TextElement)",
        "Drops the row with that name."
      ),
      on_press: member(
        "GUIAPI.on_press(screen: Screen, key: String, pressed: fn(String): Void)",
        "key is one lowercase letter. Enter or a click calls pressed with the row name."
      ),
      say: member(
        "GUIAPI.say(line: String): Bool",
        "Queues one chat line. Letters and spaces, at most 32 characters. It does not change the world."
      ),
      button: member(
        "GUIAPI.button(name: String, label: String): TextElement",
        "A button row. font is button."
      ),
      heading: member(
        "GUIAPI.heading(name: String, label: String): TextElement",
        "A heading row. font is heading."
      ),
      icon: member(
        "GUIAPI.icon(name: String, block: String): TextElement",
        "A 32 by 32 icon row. font is icon. text is the block id."
      ),
      field: member(
        "GUIAPI.field(name: String, label: String): TextElement",
        "A field row. font is field."
      ),
      read: member(
        "GUIAPI.read(screen: Screen, name: String): String",
        "gui.read. The current text of that row."
      ),
      place: member(
        "GUIAPI.place(element: TextElement, at: Vec2): TextElement",
        "Sets the element position and returns it."
      ),
      open: member(
        "GUIAPI.open(screen: Screen): Bool",
        "From a hook, open that registered panel. False when the name is unknown."
      ),
      openNamed: member(
        "GUIAPI.openNamed(name: String): Bool",
        "From a hook, open a registered GUIAPI screen by name. Calls gui.open."
      ),
      Vec2: type("class Vec2", "x and y are Float."),
      Element: type("abstract class Element", "name, size, and position."),
      TextElement: type(
        "class TextElement extends Element",
        "text, color, font, fontSize, and fontColor. color is the plate. fontColor is the glyph color."
      ),
      Screen: type("class Screen", "name, size, position, and elements."),
    },
  },
  SettingsAPI: {
    doc: "import SettingsAPI. A row on the settings page. TerraLite draws it above Back.",
    members: {
      createCategory: member(
        "SettingsAPI.createCategory(name: String, description: String): SettingsCategory",
        "Builds the category createOption reads. This call does not register a row."
      ),
      createOption: member(
        "SettingsAPI.createOption(category: SettingsCategory, name: String, description: String, default: String): SettingsOption",
        "Records the settings row and returns it."
      ),
      choices: member(
        "SettingsAPI.choices(option: SettingsOption, list: String)",
        "Sets the words for that option. list is space-separated."
      ),
      on_change: member(
        "SettingsAPI.on_change(option: SettingsOption, changed: fn(String): Void)",
        "Calls changed with the option name when that row changes."
      ),
      value: member(
        "SettingsAPI.value(name: String): String",
        "The current string for that setting name."
      ),
      SettingsCategory: type("class SettingsCategory", "name and description."),
      SettingsOption: type("class SettingsOption", "name, description, and default."),
      Settings: type("class Settings", "name, description, categories, and options."),
    },
  },
  Blocks: {
    doc: "import Blocks. Register a block, and optionally the item that places it.",
    members: {
      defineBlock: member(
        "Blocks.defineBlock(namespace: String, path: String, model: String, texture: String, seconds: Float, glow: Int): Block",
        "Registers the block and applies model, texture, dig time, and glow when set. .id is the registered id."
      ),
      defineBlockItem: member(
        "Blocks.defineBlockItem(namespace: String, block_path: String, item_path: String, model: String, texture: String, seconds: Float, glow: Int, stack: Int): Block",
        "Registers the block and a placeable item. The two paths must differ. Returns the Block."
      ),
      texture: member("Blocks.texture(id: String, file: String)", "block.texture."),
      model: member("Blocks.model(id: String, file: String)", "block.model."),
      hardness: member("Blocks.hardness(id: String, seconds: Float)", "block.hardness."),
      light: member("Blocks.light(id: String, level: Int)", "block.light."),
      Block: type("class Block", "id, name, description, and properties."),
      BlockProperties: type("class BlockProperties", "model, texture, hardness, and light."),
    },
  },
  Items: {
    doc: "import Items. Register an item, optionally the block it places and its stack size.",
    members: {
      defineItem: member(
        "Items.defineItem(namespace: String, path: String, places: String, count: Int): Item",
        "places may be empty. count 0 keeps 64. .id is the registered id."
      ),
      places: member("Items.places(held: String, target: String)", "item.places."),
      stack: member("Items.stack(id: String, count: Int)", "item.stack. Count is 1 to 64."),
      Item: type("class Item", "id, name, description, and properties. Methods places and stack."),
      ItemProperties: type("class ItemProperties", "model, texture, stack_size, and placeable."),
    },
  },
  Entities: {
    doc: "import Entities. Register a mob, its model, spawn egg, and whether it is hostile.",
    members: {
      defineEntity: member(
        "Entities.defineEntity(namespace: String, path: String, model: String, egg_path: String, hostile: Bool): Entity",
        "model and egg_path may be empty. Returns the Entity. .id is the registered id."
      ),
      setModel: member("Entities.setModel(id: String, path: String)", "entity.model."),
      setHostile: member("Entities.setHostile(id: String, flag: Bool)", "entity.hostile."),
      spawnEgg: member("Entities.spawnEgg(id: String, item_id: String)", "entity.spawn_egg."),
      setHearts: member("Entities.setHearts(id: String, n: Int)", "entity.hearts."),
      setHurtbox: member("Entities.setHurtbox(id: String, radius: Float)", "entity.hurtbox."),
      setSpeed: member("Entities.setSpeed(id: String, scale: Float)", "entity.speed."),
      spawn: member("Entities.spawn(id: String, x: Int, y: Int, z: Int): Bool", "entity.spawn."),
      despawn: member("Entities.despawn(id: String, x: Int, y: Int, z: Int): Bool", "entity.despawn."),
      spawnNear: member(
        "Entities.spawnNear(id: String, dx: Int, dy: Int, dz: Int): Bool",
        "Spawns at the player's cell plus the offset. Hook only."
      ),
      Entity: type(
        "class Entity",
        "id, name, description, and properties. Methods setModel, setHostile, spawnEgg, setHearts, setHurtbox, setSpeed, spawn, despawn, spawnNear."
      ),
      EntityProperties: type(
        "class EntityProperties",
        "model, spawn_egg, hostile, hearts, hurtbox, and speed."
      ),
    },
  },
  BlockEntities: {
    doc: "import BlockEntities. Per-cell fields on a block marked with bind. get and set need a hook.",
    members: {
      bind: member(
        "BlockEntities.bind(block_id: String): BlockEntity",
        "Marks that block id as a script tile."
      ),
      get: member(
        "BlockEntities.get(x: Int, y: Int, z: Int, key: String): String",
        "block_entity.get."
      ),
      set: member(
        "BlockEntities.set(x: Int, y: Int, z: Int, key: String, value: String): Bool",
        "block_entity.set."
      ),
      getInt: member(
        "BlockEntities.getInt(x: Int, y: Int, z: Int, key: String): Int",
        "block_entity.get_int."
      ),
      setInt: member(
        "BlockEntities.setInt(x: Int, y: Int, z: Int, key: String, n: Int): Bool",
        "block_entity.set_int."
      ),
      has: member("BlockEntities.has(x: Int, y: Int, z: Int): Bool", "block_entity.has."),
      clear: member("BlockEntities.clear(x: Int, y: Int, z: Int): Bool", "block_entity.clear."),
      openScreen: member(
        "BlockEntities.openScreen(name: String): Bool",
        "gui.open. False when that screen was never built."
      ),
      BlockEntity: type("class BlockEntity", "block_id, plus get, set, getInt, setInt, has, clear, and openScreen."),
    },
  },
  ModAPI: {
    doc: "import ModAPI. Fabric-style entry. on_init runs after main when the script defines it.",
    members: {
      createMod: member(
        "ModAPI.createMod(name: String, version: String): TLMod",
        "Builds the mod. TLMod.log records a note."
      ),
      createRegister: member(
        "ModAPI.createRegister(owner: TLMod): TLRegister",
        "Builds the registries. BLOCK and ITEM register real content. Other kinds call modapi.content."
      ),
      bindTick: member("ModAPI.bindTick(id: String, handler: fn(Int): Void)", "events.tick."),
      bindEntityTick: member(
        "ModAPI.bindEntityTick(id: String, handler: fn(String): Void)",
        "events.entity_tick."
      ),
      bindUseBlock: member(
        "ModAPI.bindUseBlock(id: String, handler: fn(String, String, Int, Int, Int): Bool)",
        "events.use_block."
      ),
      bindUseItem: member(
        "ModAPI.bindUseItem(id: String, handler: fn(String): Bool)",
        "events.use_item."
      ),
      bindUseEntity: member(
        "ModAPI.bindUseEntity(id: String, handler: fn(String, String): Bool)",
        "events.use_entity."
      ),
      Tickable: type("trait Tickable", "onTick(n: Int)."),
      Interactable: type(
        "trait Interactable",
        "interact, onUseBlock, onUseItem, and onUseEntity. False consumes the click."
      ),
      TickBehavior: type("class TickBehavior impl Tickable", "No-op onTick."),
      UseBehavior: type("class UseBehavior impl Interactable", "No-op use hooks. Each returns true."),
      TLMod: type("class TLMod", "name, version, log, and the listen hooks."),
      TLRegistry: type("class TLRegistry", "kind, open, and register."),
      TLRegister: type("class TLRegister", "The BLOCK, ITEM, ENTITY, BLOCK_ENTITY, and SCREEN registries."),
    },
  },
  chat: {
    doc: "One chat line. It does not change the world.",
    members: {
      say: member(
        "chat.say(line: String): Bool",
        "Queues one chat line. Letters and spaces, at most 32 characters."
      ),
    },
  },
  block_entity: {
    doc: "Per-cell string fields on a block id marked with bind. Reads and writes need a hook.",
    members: {
      bind: member("block_entity.bind(id: String): Bool", "Marks that block id as a script tile."),
      get: member(
        "block_entity.get(x: Int, y: Int, z: Int, key: String): String",
        "The string stored at that key, or empty."
      ),
      set: member(
        "block_entity.set(x: Int, y: Int, z: Int, key: String, value: String): Bool",
        "Stores a string on that cell."
      ),
      get_int: member(
        "block_entity.get_int(x: Int, y: Int, z: Int, key: String): Int",
        "The integer stored at that key."
      ),
      set_int: member(
        "block_entity.set_int(x: Int, y: Int, z: Int, key: String, n: Int): Bool",
        "Stores an integer on that cell."
      ),
      has: member("block_entity.has(x: Int, y: Int, z: Int): Bool", "True when that cell has tile data."),
      clear: member("block_entity.clear(x: Int, y: Int, z: Int): Bool", "Drops the tile data on that cell."),
    },
  },
  gui: {
    doc: "The host behind GUIAPI. open shows a screen built with GUIAPI.createScreen.",
    members: {
      screen: member(
        "gui.screen(name: String, w: Float, h: Float, x: Float, y: Float): Bool",
        "Records a screen. name is a lowercase id."
      ),
      element: member(
        "gui.element(screen: String, name: String, text: String, x: Float, y: Float, w: Float, h: Float, color: Int, font: String, fontSize: Int, fontColor: Int): Bool",
        "Adds a row. At most 12."
      ),
      remove: member("gui.remove(screen: String, name: String): Bool", "Drops that row."),
      read: member("gui.read(screen: String, name: String): String", "The current text of that row."),
      press: member(
        "gui.press(screen: String, key: String, pressed: fn(String): Void)",
        "key is one lowercase letter. Enter or a click calls pressed with the row name."
      ),
      open: member(
        "gui.open(name: String): Bool",
        "From a hook, open that registered panel. False when the name is unknown."
      ),
    },
  },
};

function member(signature, doc) {
  return { signature, doc, kind: "function" };
}

function field(typeName, doc) {
  return { signature: typeName, doc, kind: "field" };
}

function type(signature, doc) {
  return { signature, doc, kind: "class" };
}

function moduleNames() {
  return Object.keys(MODULES);
}

function membersOf(name) {
  const mod = MODULES[name];
  if (!mod) {
    return [];
  }
  return Object.keys(mod.members).map((label) => {
    const item = mod.members[label];
    return { label, kind: item.kind, detail: item.signature, doc: item.doc };
  });
}

function hover(qualifier, name) {
  if (!name) {
    return null;
  }
  if (!qualifier) {
    const mod = MODULES[name];
    if (!mod) {
      return null;
    }
    return {
      name,
      kind: "module",
      signature: "module " + name,
      doc: mod.doc,
    };
  }
  const mod = MODULES[qualifier];
  if (!mod) {
    return null;
  }
  const item = mod.members[name];
  if (!item) {
    return null;
  }
  return {
    name: qualifier + "." + name,
    kind: item.kind,
    signature: item.signature,
    doc: item.doc,
  };
}

function complete(prefix) {
  if (closedContext(prefix)) {
    return [];
  }
  const recv = receiver(prefix);
  if (recv) {
    return membersOf(recv);
  }
  if (afterImport(prefix)) {
    return [
      { label: "helpers", kind: "module", detail: "import helpers", doc: MODULES.helpers.doc },
      { label: "TLModAPI", kind: "module", detail: "import TLModAPI", doc: MODULES.TLModAPI.doc },
      { label: "GUIAPI", kind: "module", detail: "import GUIAPI", doc: MODULES.GUIAPI.doc },
      { label: "SettingsAPI", kind: "module", detail: "import SettingsAPI", doc: MODULES.SettingsAPI.doc },
      { label: "ModAPI", kind: "module", detail: "import ModAPI", doc: MODULES.ModAPI.doc },
      { label: "Blocks", kind: "module", detail: "import Blocks", doc: MODULES.Blocks.doc },
      { label: "Items", kind: "module", detail: "import Items", doc: MODULES.Items.doc },
      { label: "Entities", kind: "module", detail: "import Entities", doc: MODULES.Entities.doc },
      { label: "BlockEntities", kind: "module", detail: "import BlockEntities", doc: MODULES.BlockEntities.doc },
    ];
  }
  if (endsWithDot(prefix)) {
    return [];
  }
  return moduleNames().map((name) => ({
    label: name,
    kind: "module",
    detail: "module " + name,
    doc: MODULES[name].doc,
  }));
}

function at(line, column) {
  const col = Math.max(0, Math.min(line.length, column));
  let start = col;
  let end = col;
  while (start > 0 && identChar(line[start - 1])) {
    start -= 1;
  }
  while (end < line.length && identChar(line[end])) {
    end += 1;
  }
  if (start === end) {
    return null;
  }
  const name = line.slice(start, end);
  let qualifier = "";
  let qEnd = start;
  while (qEnd > 0 && line[qEnd - 1] === " ") {
    qEnd -= 1;
  }
  if (qEnd > 0 && line[qEnd - 1] === ".") {
    let qStart = qEnd - 1;
    while (qStart > 0 && identChar(line[qStart - 1])) {
      qStart -= 1;
    }
    qualifier = line.slice(qStart, qEnd - 1);
  }
  return { qualifier, name, start, end };
}

function receiver(prefix) {
  const match = prefix.match(/([A-Za-z_][A-Za-z0-9_]*)\.[A-Za-z0-9_]*$/);
  if (!match || !MODULES[match[1]]) {
    return "";
  }
  return match[1];
}

function afterImport(prefix) {
  return /(?:^|\n)\s*(?:import|from)\s+[A-Za-z0-9_.]*$/.test(prefix);
}

function endsWithDot(prefix) {
  return /\.\s*$/.test(prefix);
}

function identChar(ch) {
  return (ch >= "A" && ch <= "Z") || (ch >= "a" && ch <= "z") || (ch >= "0" && ch <= "9") || ch === "_";
}

function closedContext(prefix) {
  let i = 0;
  while (i < prefix.length) {
    const ch = prefix[i];
    if (ch === "#") {
      const nl = prefix.indexOf("\n", i);
      if (nl < 0) {
        return true;
      }
      i = nl + 1;
      continue;
    }
    if (ch === "/" && prefix[i + 1] === "/") {
      const nl = prefix.indexOf("\n", i);
      if (nl < 0) {
        return true;
      }
      i = nl + 1;
      continue;
    }
    if (ch === "/" && prefix[i + 1] === "#") {
      const end = prefix.indexOf("#/", i + 2);
      if (end < 0) {
        return true;
      }
      i = end + 2;
      continue;
    }
    if (ch === '"') {
      i += 1;
      let closed = false;
      while (i < prefix.length) {
        if (prefix[i] === "\\") {
          i += 2;
          continue;
        }
        if (prefix[i] === '"') {
          i += 1;
          closed = true;
          break;
        }
        i += 1;
      }
      if (!closed) {
        return true;
      }
      continue;
    }
    i += 1;
  }
  return false;
}

module.exports = {
  MODULES,
  moduleNames,
  membersOf,
  hover,
  complete,
  at,
};
