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
        "Callback fn(kind: String, amount: Int): Bool. False cancels. A fall of four blocks or more calls this for \"player\". This hook cannot write the world."
      ),
      entity_tick: member(
        "events.entity_tick(id: String, fn)",
        "Callback fn(kind: String). The client and the server tick \"player\", \"sheep\", \"pig\", and \"zombie\" every 50 ms. This hook cannot write the world."
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
  entity: {
    doc: "A simple mob registered from main or on_init.",
    members: {
      register: member(
        "entity.register(namespace: String, path: String)",
        "A simple mob namespace:path. It stands just east of the oak stairs. The same script can register it again. ENTITY in TLModAPI is this same mob."
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
