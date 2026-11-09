// Item registration helpers. import Items;
// places makes a held item put a block. stack sets the hotbar limit (1 to 64).
// Lives at builtin/std/TerraLite/Items.rg (same folder as ModAPI).
class Item {
    var id: String = "";
    var name: String = "";
    var description: String = "";
    var icon: String = "";
    var model: String = "";
    var texture: String = "";
    var places: String = "";
    var stack: Int = 64;
}

fn places(held: String, target: String) {
    item.places(held, target);
}

fn stack(id: String, count: Int) {
    item.stack(id, count);
}

/// Register an item. places may be "" when it does not place. count 0 keeps 64.
fn defineItem(namespace: String, path: String, places_id: String, count: Int): String {
    var id = item.register(namespace, path);
    if (places_id != "") {
        item.places(id, places_id);
    }
    if (count > 0) {
        item.stack(id, count);
    }
    return id;
}
