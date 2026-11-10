// Script tile data on a registered block. import BlockEntities;
// bind marks a block id; get/set need a hook. Lives next to Blocks.rg / ModAPI.rg.

class BlockEntity {
    var block_id: String = "";

    fn get(x: Int, y: Int, z: Int, key: String): String {
        return block_entity.get(x, y, z, key);
    }

    fn set(x: Int, y: Int, z: Int, key: String, value: String): Bool {
        return block_entity.set(x, y, z, key, value);
    }

    fn getInt(x: Int, y: Int, z: Int, key: String): Int {
        return block_entity.get_int(x, y, z, key);
    }

    fn setInt(x: Int, y: Int, z: Int, key: String, n: Int): Bool {
        return block_entity.set_int(x, y, z, key, n);
    }

    fn clear(x: Int, y: Int, z: Int): Bool {
        return block_entity.clear(x, y, z);
    }

    fn has(x: Int, y: Int, z: Int): Bool {
        return block_entity.has(x, y, z);
    }

    /// Open a GUIAPI screen from a use hook (false when the screen was never built).
    fn openScreen(name: String): Bool {
        return gui.open(name);
    }
}

fn bind(block_id: String): BlockEntity {
    block_entity.bind(block_id);
    return BlockEntity { block_id: block_id };
}

fn get(x: Int, y: Int, z: Int, key: String): String {
    return block_entity.get(x, y, z, key);
}

fn set(x: Int, y: Int, z: Int, key: String, value: String): Bool {
    return block_entity.set(x, y, z, key, value);
}

fn getInt(x: Int, y: Int, z: Int, key: String): Int {
    return block_entity.get_int(x, y, z, key);
}

fn setInt(x: Int, y: Int, z: Int, key: String, n: Int): Bool {
    return block_entity.set_int(x, y, z, key, n);
}

fn clear(x: Int, y: Int, z: Int): Bool {
    return block_entity.clear(x, y, z);
}

fn has(x: Int, y: Int, z: Int): Bool {
    return block_entity.has(x, y, z);
}

fn openScreen(name: String): Bool {
    return gui.open(name);
}
