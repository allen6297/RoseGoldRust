// A row on the settings page. TerraLite draws it above Back.
// import SettingsAPI;
// var game = SettingsAPI.createCategory("game", "how it feels");
// SettingsAPI.createOption(game, "glow", "soft glow", "on");
class SettingsCategory {
    var name: String = "";
    var description: String = "";
}

class SettingsOption {
    var name: String = "";
    var description: String = "";
    var default: String = "";
}

class Settings {
    var name: String = "";
    var description: String = "";
    var categories: Array[SettingsCategory] = [];
    var options: Array[SettingsOption] = [];
}

fn createCategory(name: String, description: String): SettingsCategory {
    return SettingsCategory { name: name, description: description };
}

fn createOption(category: SettingsCategory, name: String, description: String, default: String): SettingsOption {
    settings.option(category.name, category.description, name, description, default);
    return SettingsOption { name: name, description: description, default: default };
}

fn choices(option: SettingsOption, list: String) {
    settings.choices(option.name, list);
}

fn on_change(option: SettingsOption, changed: fn(String): Void) {
    settings.watch(option.name, changed);
}

fn value(name: String): String {
    return settings.value(name);
}
