// Script tile data on a registered block. import BlockEntities;
// bind marks a block id; get/set need a hook. Lives next to Blocks.rg / ModAPI.rg.
//
// Machine contract (furnace-like stations): tile keys `in` / `in_n`, `fuel` / `fuel_n`,
// `out` / `out_n`, plus `progress` / `burn`. Use Tickable for cook ticks, openAt + GUIAPI
// for the panel, deposit / withdraw for hotbar ↔ slot moves. No new events.*.

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

    /// Open a screen and remember this cell as the machine focus for press handlers.
    fn openAt(x: Int, y: Int, z: Int, name: String): Bool {
        return block_entity.open_at(x, y, z, name);
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

/// Open `name` and set the machine focus to this cell (for deposit/withdraw from presses).
fn openAt(x: Int, y: Int, z: Int, name: String): Bool {
    return block_entity.open_at(x, y, z, name);
}

fn focusX(): Int {
    return block_entity.focus_x();
}

fn focusY(): Int {
    return block_entity.focus_y();
}

fn focusZ(): Int {
    return block_entity.focus_z();
}

/// Item id in a named machine slot (`in` / `fuel` / `out`), or "".
fn slotId(x: Int, y: Int, z: Int, name: String): String {
    return block_entity.get(x, y, z, name);
}

/// Stack count for that slot (`in_n` / `fuel_n` / `out_n`).
fn slotCount(x: Int, y: Int, z: Int, name: String): Int {
    return block_entity.get_int(x, y, z, name + "_n");
}

/// Write a machine slot id + count. `count` 0 clears the id.
fn setSlot(x: Int, y: Int, z: Int, name: String, id: String, count: Int): Bool {
    if (count <= 0 || id == "") {
        var cleared = block_entity.set(x, y, z, name, "");
        var zero = block_entity.set_int(x, y, z, name + "_n", 0);
        return cleared && zero;
    }
    var wrote = block_entity.set(x, y, z, name, id);
    var numbered = block_entity.set_int(x, y, z, name + "_n", count);
    return wrote && numbered;
}

/// Move one from the held hotbar stack into slot `name` (`in` / `fuel` / `out`).
fn deposit(x: Int, y: Int, z: Int, name: String): Bool {
    var held = player.held();
    if (held == "") {
        return false;
    }
    var cur = block_entity.get(x, y, z, name);
    var n = block_entity.get_int(x, y, z, name + "_n");
    if (cur != "" && cur != held) {
        return false;
    }
    if (n >= 64) {
        return false;
    }
    if (!player.take(1)) {
        return false;
    }
    return setSlot(x, y, z, name, held, n + 1);
}

/// Move one from slot `name` into the hotbar.
fn withdraw(x: Int, y: Int, z: Int, name: String): Bool {
    var id = block_entity.get(x, y, z, name);
    var n = block_entity.get_int(x, y, z, name + "_n");
    if (id == "" || n <= 0) {
        return false;
    }
    if (!player.give(id, 1)) {
        return false;
    }
    return setSlot(x, y, z, name, id, n - 1);
}

/// Deposit into the focused machine (set by `openAt`).
fn depositFocus(name: String): Bool {
    return deposit(focusX(), focusY(), focusZ(), name);
}

/// Withdraw from the focused machine.
fn withdrawFocus(name: String): Bool {
    return withdraw(focusX(), focusY(), focusZ(), name);
}
