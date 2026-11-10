// Entity registration helpers. import Entities;
// Models prefer assets/ns/models/entity/id.bbmodel (Blockbench).
// Lives at builtin/std/TerraLite/Entities.rg (same folder as ModAPI).
import math;

class Entity {
    var id: String = "";
    var name: String = "";
    var description: String = "";
    var properties: EntityProperties;

    fn setModel(path: String) {
        if (path != "") {
            properties.model = path;
            entity.model(id, path);
        }
    }

    fn setHostile(flag: Bool) {
        properties.hostile = flag;
        entity.hostile(id, flag);
    }

    fn spawnEgg(item_id: String) {
        if (item_id != "") {
            properties.spawn_egg = item_id;
            entity.spawn_egg(id, item_id);
        }
    }

    fn spawn(x: Int, y: Int, z: Int): Bool {
        return entity.spawn(id, x, y, z);
    }

    fn despawn(x: Int, y: Int, z: Int): Bool {
        return entity.despawn(id, x, y, z);
    }

    /// Place one body near the local player. Only works inside a hook.
    fn spawnNear(dx: Int, dy: Int, dz: Int): Bool {
        var x = math.to_int(math.floor(player.x())) + dx;
        var y = math.to_int(math.floor(player.y())) + dy;
        var z = math.to_int(math.floor(player.z())) + dz;
        return entity.spawn(id, x, y, z);
    }
}

/// Free helpers use setModel / setHostile so they do not clash with Blocks.model
/// when the TerraLite folder is loaded as one module.
fn setModel(id: String, path: String) {
    entity.model(id, path);
}

fn setHostile(id: String, flag: Bool) {
    entity.hostile(id, flag);
}

fn spawnEgg(id: String, item_id: String) {
    entity.spawn_egg(id, item_id);
}

fn spawn(id: String, x: Int, y: Int, z: Int): Bool {
    return entity.spawn(id, x, y, z);
}

fn despawn(id: String, x: Int, y: Int, z: Int): Bool {
    return entity.despawn(id, x, y, z);
}

/// Spawn at the player's eye cell plus an offset. Hook-only (needs player.*).
fn spawnNear(id: String, dx: Int, dy: Int, dz: Int): Bool {
    var x = math.to_int(math.floor(player.x())) + dx;
    var y = math.to_int(math.floor(player.y())) + dy;
    var z = math.to_int(math.floor(player.z())) + dz;
    return entity.spawn(id, x, y, z);
}

/// Register a mob. model may be "". egg_path may be "" (no creative egg item).
/// hostile true means chase/attack defaults when the host supports Guest AI.
/// Returns an Entity whose properties mirror what was sent to the host.
fn defineEntity(namespace: String, path: String, model_path: String, egg_path: String, is_hostile: Bool): Entity {
    var id = entity.register(namespace, path);
    var egg_id = "";
    if (model_path != "") {
        entity.model(id, model_path);
    }
    entity.hostile(id, is_hostile);
    if (egg_path != "") {
        egg_id = item.register(namespace, egg_path);
        entity.spawn_egg(id, egg_id);
    }
    return Entity {
        id: id,
        name: path,
        description: "",
        properties: EntityProperties {
            model: model_path,
            spawn_egg: egg_id,
            hostile: is_hostile,
        },
    };
}

class EntityProperties {
    var model: String = "";
    var spawn_egg: String = "";
    var hostile: Bool = false;
}
