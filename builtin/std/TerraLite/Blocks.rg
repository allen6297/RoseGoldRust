// Block registration helpers. import Blocks;
// Models prefer assets/ns/models/block/id.json or .bbmodel.
// Lives at builtin/std/TerraLite/Blocks.rg (same folder as ModAPI).
class Block {
    var id: String = "";
    var name: String = "";
    var description: String = "";
    var properties: BlockProperties;
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
/// Returns a Block whose properties mirror what was sent to the host.
fn defineBlock(namespace: String, path: String, model_path: String, texture_path: String, seconds: Float, glow: Int): Block {
    var id = block.register(namespace, path);
    var hardness_value: Float = 1.5;
    var light_value: Int = 0;
    if (model_path != "") {
        block.model(id, model_path);
    }
    if (texture_path != "") {
        block.texture(id, texture_path);
    }
    if (seconds > 0.0) {
        block.hardness(id, seconds);
        hardness_value = seconds;
    }
    if (glow > 0) {
        block.light(id, glow);
        light_value = glow;
    }
    return Block {
        id: id,
        name: path,
        description: "",
        properties: BlockProperties {
            model: model_path,
            texture: texture_path,
            hardness: hardness_value,
            light: light_value,
        },
    };
}

class BlockProperties {
    var model: String = "";
    var texture: String = "";
    var hardness: Float = 1.5;
    var light: Int = 0;
}
