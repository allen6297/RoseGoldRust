// Item registration helpers. import Items;
// places makes a held item put a block. stack sets the hotbar limit (1 to 64).
// Lives at builtin/std/TerraLite/Items.rg (same folder as ModAPI).
class Item {
    var id: String = "";
    var name: String = "";
    var description: String = "";
    var properties: ItemProperties;

    fn places(target: String) {
        if (target != "") {
            properties.placeable = true;
            item.places(id, target);
        }
    }

    fn stack(count: Int) {
        if (count > 0) {
            properties.stack_size = count;
            item.stack(id, count);
        }
    }
}

fn places(held: String, target: String) {
    item.places(held, target);
}

fn stack(id: String, count: Int) {
    item.stack(id, count);
}

/// Register an item. places may be "" when it does not place. count 0 keeps 64.
/// Returns an Item whose properties mirror what was sent to the host.
fn defineItem(namespace: String, path: String, places_id: String, count: Int): Item {
    var id = item.register(namespace, path);
    var placeable = false;
    var stack_size: Int = 64;
    if (places_id != "") {
        item.places(id, places_id);
        placeable = true;
    }
    if (count > 0) {
        item.stack(id, count);
        stack_size = count;
    }
    return Item {
        id: id,
        name: path,
        description: "",
        properties: ItemProperties {
            model: "",
            texture: "",
            stack_size: stack_size,
            placeable: placeable,
        },
    };
}

class ItemProperties {
    var model: String = "";
    var texture: String = "";
    var stack_size: Int = 64;
    var placeable: Bool = false;
}
