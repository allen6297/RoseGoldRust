// Fabric-style mod entry. A script imports this and builds a mod in on_init.
// open records the registry. register stores real content.
// modapi.note, modapi.registry, and modapi.content are the host calls Terralite records.
// Lives at builtin/std/TerraLite/ModAPI.rg (same folder as GUIAPI and SettingsAPI).
// Block, item, and entity helpers live in Blocks.rg / Items.rg / Entities.rg.
class TLRegistry {
    var kind: String = "";

    fn open() {
        modapi.registry(kind);
    }

    fn register(namespace: String, path: String): String {
        if (kind == "BLOCK") {
            return block.register(namespace, path);
        }
        if (kind == "ITEM") {
            return item.register(namespace, path);
        }
        return modapi.content(kind, namespace, path);
    }
}

class TLMod {
    var name: String = "";
    var version: String = "";

    fn log(message: String) {
        modapi.note(name + ": " + message);
    }

    /// `events.tick` — once a second. `n` starts at 1.
    fn listenTick(id: String, handler: fn(Int): Void) {
        events.tick(id, handler);
    }

    /// `events.entity_tick` — every 50 ms for `"player"`, `"sheep"`, `"pig"`, `"zombie"`.
    fn listenEntityTick(id: String, handler: fn(String): Void) {
        events.entity_tick(id, handler);
    }

    /// `events.use_block` — false consumes the right click on a block.
    fn listenUseBlock(id: String, handler: fn(String, String, Int, Int, Int): Bool) {
        events.use_block(id, handler);
    }

    /// `events.use_item` — false consumes a right click with the held item.
    fn listenUseItem(id: String, handler: fn(String): Bool) {
        events.use_item(id, handler);
    }

    /// `events.use_entity` — false consumes a right click on a mob.
    fn listenUseEntity(id: String, handler: fn(String, String): Bool) {
        events.use_entity(id, handler);
    }

    /// `events.fill_place` — compound fill (`water` / `ember`) entered a host.
    fn listenFillPlace(id: String, handler: fn(String, Int, Int, Int): Void) {
        events.fill_place(id, handler);
    }

    /// `events.fill_remove` — compound fill left a host.
    fn listenFillRemove(id: String, handler: fn(String, Int, Int, Int): Void) {
        events.fill_remove(id, handler);
    }

    /// `events.structure_discovered` — first approach of a named landmark.
    /// `structure` is `village`, `ruined_portal`, `desert_temple`, …
    /// Return false to cancel the built-in tip; `chat.say` lines still show.
    fn listenStructureDiscovered(id: String, handler: fn(String): Bool) {
        events.structure_discovered(id, handler);
    }
}

class TLRegister {
    var owner: String = "";
    var ITEM: TLRegistry = TLRegistry {};
    var BLOCK: TLRegistry = TLRegistry {};
    var ENTITY: TLRegistry = TLRegistry {};
    var BLOCK_ENTITY: TLRegistry = TLRegistry {};
    var ITEM_GROUP: TLRegistry = TLRegistry {};
    var SCREEN: TLRegistry = TLRegistry {};
    var EVENT: TLRegistry = TLRegistry {};
    var COMMAND: TLRegistry = TLRegistry {};
    var CONFIG: TLRegistry = TLRegistry {};
    var RESOURCE: TLRegistry = TLRegistry {};
    var TEXTURE: TLRegistry = TLRegistry {};
    var PARTICLE: TLRegistry = TLRegistry {};

    fn texture(id: String, file: String) {
        block.texture(id, file);
    }

    fn model(id: String, file: String) {
        block.model(id, file);
    }

    fn hardness(id: String, seconds: Float) {
        block.hardness(id, seconds);
    }

    fn light(id: String, level: Int) {
        block.light(id, level);
    }

    fn places(held: String, target: String) {
        item.places(held, target);
    }

    fn stack(id: String, count: Int) {
        item.stack(id, count);
    }

    fn recipe(result: String, ingredient: String) {
        recipe.register(result, ingredient);
    }

    /// Shapeless pile. `list` is `a+b(+c…)` — same as `recipe.shapeless`.
    fn shapeless(result: String, list: String) {
        recipe.shapeless(result, list);
    }

    /// Shaped 3×3. `grid` is nine comma-separated cells, blanks empty.
    fn shaped(result: String, grid: String) {
        recipe.shaped(result, grid);
    }

    /// Furnace cook line. `input` becomes `result` (also blast when not food/bricks).
    fn smelt(result: String, input: String) {
        recipe.smelt(result, input);
    }

    /// Smoker-only cook line.
    fn smoke(result: String, input: String) {
        recipe.smoke(result, input);
    }

    /// Blast-furnace-only cook line.
    fn blast(result: String, input: String) {
        recipe.blast(result, input);
    }

    /// Block-break loot. `list` is `a(+b…)` — every id drops once.
    fn blockLoot(block: String, list: String) {
        loot.block(block, list);
    }

    /// Chest table override. `table` is `camp_chest` / `mine_chest` / …; `list` replaces `drops`.
    fn chestLoot(table: String, list: String) {
        loot.chest(table, list);
    }
}

fn registry(kind: String): TLRegistry {
    return TLRegistry { kind: kind };
}

fn createMod(name: String, version: String): TLMod {
    return TLMod { name: name, version: version };
}

fn createRegister(owner: TLMod): TLRegister {
    return TLRegister {
        owner: owner.name,
        ITEM: registry("ITEM"),
        BLOCK: registry("BLOCK"),
        ENTITY: registry("ENTITY"),
        BLOCK_ENTITY: registry("BLOCK_ENTITY"),
        ITEM_GROUP: registry("ITEM_GROUP"),
        SCREEN: registry("SCREEN"),
        EVENT: registry("EVENT"),
        COMMAND: registry("COMMAND"),
        CONFIG: registry("CONFIG"),
        RESOURCE: registry("RESOURCE"),
        TEXTURE: registry("TEXTURE"),
        PARTICLE: registry("PARTICLE"),
    };
}

/// Once-a-second world pulse (`events.tick`). Implement and bridge with `bindTick`.
trait Tickable {
    /// `n` is the tick count starting at 1.
    fn onTick(n: Int);
}

/// Right-click use hooks (`events.use_block` / `use_item` / `use_entity`).
/// Return false to consume the click (Minecraft cancelable interact).
trait Interactable {
    /// Coarse hand use when a finer hook is not enough. Default: allow.
    fn interact(player: String, hand: String): Bool;
    /// Looking at a block. `item` may be "".
    fn onUseBlock(item: String, block: String, x: Int, y: Int, z: Int): Bool;
    /// Item-only / air click after block hooks pass.
    fn onUseItem(item: String): Bool;
    /// Looking at a mob (`zombie`, `ember:shade`, …).
    fn onUseEntity(item: String, kind: String): Bool;
}

/// No-op base for `Tickable`. Override `onTick`.
class TickBehavior impl Tickable {
    fn onTick(n: Int) {
    }
}

/// No-op base for `Interactable`. Override the use hooks you need.
class UseBehavior impl Interactable {
    fn interact(player: String, hand: String): Bool {
        return true;
    }

    fn onUseBlock(item: String, block: String, x: Int, y: Int, z: Int): Bool {
        return true;
    }

    fn onUseItem(item: String): Bool {
        return true;
    }

    fn onUseEntity(item: String, kind: String): Bool {
        return true;
    }
}

/// Wire a free function to `events.tick` (same as `TLMod.listenTick`).
fn bindTick(id: String, handler: fn(Int): Void) {
    events.tick(id, handler);
}

fn bindEntityTick(id: String, handler: fn(String): Void) {
    events.entity_tick(id, handler);
}

fn bindUseBlock(id: String, handler: fn(String, String, Int, Int, Int): Bool) {
    events.use_block(id, handler);
}

fn bindUseItem(id: String, handler: fn(String): Bool) {
    events.use_item(id, handler);
}

fn bindUseEntity(id: String, handler: fn(String, String): Bool) {
    events.use_entity(id, handler);
}
