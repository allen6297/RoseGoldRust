// Fabric-style mod entry. A script imports this and builds a mod in on_init.
// open records the registry. register stores real content.
// modapi.note, modapi.registry, and modapi.content are the host calls Terralite records.
// Lives at builtin/std/TerraLite/ModAPI.rg (same folder as GUIAPI and SettingsAPI).
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

    fn places(held: String, target: String) {
        item.places(held, target);
    }

    fn recipe(result: String, ingredient: String) {
        recipe.register(result, ingredient);
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
