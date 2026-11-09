import math;
import str;

fn id(namespace: String, name: String): String {
    if str.contains(namespace, ":") {
        return namespace;
    }
    if str.is_empty(namespace) {
        return name;
    }
    return namespace + ":" + name;
}

fn namespace_of(id: String): String {
    var at = str.find(id, ":");
    if at < 0 {
        return "";
    }
    return str.slice(id, 0, at);
}

fn name_of(id: String): String {
    var at = str.find(id, ":");
    if at < 0 {
        return id;
    }
    return str.slice(id, at + 1, str.length(id));
}

fn same_id(left: String, right: String): Bool {
    if left == right {
        return true;
    }
    if name_of(left) != name_of(right) {
        return false;
    }
    var a = namespace_of(left);
    var b = namespace_of(right);
    if str.is_empty(a) || str.is_empty(b) {
        return true;
    }
    return a == b;
}

fn holding(id: String): Bool {
    return same_id(player.held(), id);
}

fn block_at(x: Int, y: Int, z: Int): String {
    return world.get_block(x, y, z);
}

fn air(x: Int, y: Int, z: Int): Bool {
    return world.is_air(x, y, z);
}

fn replace_block(x: Int, y: Int, z: Int, was: String, next: String): Bool {
    if same_id(world.get_block(x, y, z), was) {
        return world.set_block(x, y, z, next);
    }
    return false;
}

fn floor_block(n: Float): Int {
    return math.to_int(math.floor(n));
}

fn chunk_of(n: Float): Int {
    return math.to_int(math.floor(n / 16.0));
}

fn stack_at_least(slot: Int, n: Int): Bool {
    return player.count(slot) >= n;
}
