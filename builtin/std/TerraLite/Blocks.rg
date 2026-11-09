// Block registration helpers. import Blocks;
// Models prefer assets/ns/models/block/id.json or .bbmodel.
// Lives at builtin/std/TerraLite/Blocks.rg (same folder as ModAPI).
class Block {
    var id: String = "";
    var name: String = "";
    var description: String = "";
    var icon: String = "";
    var model: String = "";
    var texture: String = "";
    var hardness: Float = 1.5;
    var light: Int = 0;
}

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

/// Register a block and apply model, texture, dig time, and glow when set.
fn defineBlock(namespace: String, path: String, model_path: String, texture_path: String, seconds: Float, glow: Int): String {
    var id = block.register(namespace, path);
    if (model_path != "") {
        block.model(id, model_path);
    }
    if (texture_path != "") {
        block.texture(id, texture_path);
    }
    if (seconds > 0.0) {
        block.hardness(id, seconds);
    }
    if (glow > 0) {
        block.light(id, glow);
    }
    return id;
}
