use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use rosegold_ide::*;

fn caret(src: &str) -> (String, usize) {
    let at = src.find('|').expect("caret");
    let text = format!("{}{}", &src[..at], &src[at + 1..]);
    (text, at)
}

fn java_path() -> String {
    r"C:\CODE\RoseGoldJava\t.rg".to_string()
}

fn godot_file(name: &str) -> String {
    Path::new(r"C:\CODE\RoseGoldJava\godot\demo")
        .join(name)
        .display()
        .to_string()
}

fn project(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rg-ide-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("util")).unwrap();
    fs::write(
        dir.join("project.toml"),
        format!(
            "[project]\nname = \"{label}\"\nentry = \"main.rg\"\n\n[modules]\nutil = \"util\"\n"
        ),
    )
    .unwrap();
    fs::write(dir.join("main.rg"), "fn main(): Int {\n    return 0;\n}\n").unwrap();
    fs::write(
        dir.join("util").join("lib.rg"),
        "fn hello(): Int {\n    return 1;\n}\n",
    )
    .unwrap();
    dir
}

#[test]
fn check_clean_main() {
    let res = handle(
        "{\"id\":1,\"method\":\"check\",\"path\":\"t.rg\",\"source\":\"fn main(): Int {\\n    return 0;\\n}\\n\"}",
    );
    assert!(res.contains("\"ok\":true"), "{res}");
    assert!(res.contains("\"diagnostics\":[]"), "{res}");
}

#[test]
fn check_reports_parse_error() {
    let res = handle("{\"id\":4,\"method\":\"check\",\"path\":\"t.rg\",\"source\":\"fn\"}");
    assert!(res.contains("\"ok\":true"), "{res}");
    assert!(res.contains("\"line\":"), "{res}");
    assert!(res.contains("\"message\":"), "{res}");
    assert!(res.contains("\"kind\":\"parse error\""), "{res}");
    assert!(res.contains("\"col\":3"), "{res}");
    assert!(res.contains("\"endCol\":4"), "{res}");
}

#[test]
fn check_marks_an_unused_import_and_hints_a_close_name() {
    let unused = handle(
        "{\"id\":5,\"method\":\"check\",\"path\":\"C:\\\\__rg_no_stdlib__\\\\mods\\\\probe.rg\",\"source\":\"import math;\\nfn main(): Int {\\n    return 0;\\n}\\n\"}",
    );
    assert!(unused.contains("\"severity\":\"warning\""), "{unused}");
    assert!(unused.contains("\"kind\":\"unused\""), "{unused}");
    assert!(unused.contains("unused import"), "{unused}");

    let typo = handle(
        "{\"id\":6,\"method\":\"check\",\"path\":\"C:\\\\__rg_no_stdlib__\\\\mods\\\\probe.rg\",\"source\":\"import math;\\nfn main(): Int {\\n    return math.abss(-1);\\n}\\n\"}",
    );
    assert!(typo.contains("has no export"), "{typo}");
    assert!(typo.contains("\"hint\":\"did you mean 'abs'?\""), "{typo}");
    assert!(typo.contains("\"kind\":\"type error\""), "{typo}");
}

#[test]
fn hover_print_protocol() {
    let source = "fn main(): Int {\\n    print(1);\\n    return 0;\\n}\\n";
    let text = "fn main(): Int {\n    print(1);\n    return 0;\n}\n";
    let offset = text.find("print").unwrap();
    let res = handle(&format!(
        "{{\"id\":2,\"method\":\"hover\",\"path\":\"t.rg\",\"offset\":{offset},\"source\":\"{source}\"}}"
    ));
    assert!(res.contains("\"ok\":true"), "{res}");
    assert!(res.contains("print"), "{res}");
}

#[test]
fn format_rewrites_source() {
    let res = handle(
        "{\"id\":3,\"method\":\"format\",\"path\":\"t.rg\",\"source\":\"fn main():Int{return 0;}\"}",
    );
    assert!(res.contains("\"ok\":true"), "{res}");
    assert!(res.contains("fn main()"), "{res}");
}

#[test]
fn unknown_method_and_two_lines() {
    let input = "{\"id\":1,\"method\":\"nope\"}\n\n{\"id\":2,\"method\":\"ping\"}\n";
    let mut out = Vec::new();
    serve(Cursor::new(input), &mut out);
    let text = String::from_utf8(out).unwrap();
    let lines: Vec<_> = text.split('\n').filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert!(lines[0].contains("\"ok\":false"), "{}", lines[0]);
    assert!(lines[1].contains("\"ok\":true"), "{}", lines[1]);
}

#[test]
fn rename_rejects_keyword() {
    let res = handle(
        "{\"id\":9,\"method\":\"rename\",\"path\":\"t.rg\",\"offset\":3,\"newName\":\"fn\",\"source\":\"fn main(): Int { return 0; }\"}",
    );
    assert!(res.contains("\"ok\":false"), "{res}");
    assert!(res.contains("invalid name"), "{res}");
}

#[test]
fn function_signature_and_docs() {
    let (text, at) = caret(
        "\
/// add two numbers
fn add(a: Int, b: Int): Int {
    return a + b;
}
fn main(): Int {
    return ad|d(1, 2);
}
",
    );
    let info = hover_at(&text, "t.rg", at).unwrap();
    assert_eq!(info.name, "add");
    assert_eq!(info.kind, "function");
    assert_eq!(info.signature, "fn add(a: Int, b: Int): Int");
    assert_eq!(info.doc, "add two numbers");
}

#[test]
fn deprecated_fn() {
    let (text, at) = caret(
        "\
@deprecated
fn boom() throws {
    throw \"x\";
}
fn main(): Int {
    bo|om();
    return 0;
}
",
    );
    let info = hover_at(&text, "t.rg", at).unwrap();
    assert!(info.deprecated);
    assert!(info.signature.contains("throws"), "{}", info.signature);
}

#[test]
fn builtin_print() {
    let (text, at) = caret(
        "\
fn main(): Int {
    pri|nt(\"hi\");
    return 0;
}
",
    );
    let info = hover_at(&text, "t.rg", at).unwrap();
    assert_eq!(info.signature, "print(...)");
}

#[test]
fn math_member_hover() {
    let (text, at) = caret(
        "\
fn main(): Int {
    return math.ab|s(-1);
}
",
    );
    let info = hover_at(&text, &java_path(), at).unwrap();
    assert_eq!(info.signature, "fn abs(n: Int): Int");
    assert_eq!(info.crate_name, "math");
}

#[test]
fn import_crate_hover() {
    let (text, at) = caret(
        "\
import u|i;
fn main(): Int {
    return 0;
}
",
    );
    let info = hover_at(&text, &java_path(), at).unwrap();
    assert_eq!(info.kind, "module");
    assert_eq!(info.signature, "module ui");
    assert_eq!(info.crate_name, "ui");
}

#[test]
fn from_import_enum_hover() {
    let (text, at) = caret(
        "\
from ui import Col|or;
fn main(): Int {
    return 0;
}
",
    );
    let info = hover_at(&text, &java_path(), at).unwrap();
    assert_eq!(info.kind, "enum");
    assert_eq!(info.signature, "enum Color");
    assert_eq!(info.crate_name, "ui");
}

#[test]
fn project_module_member_hover() {
    let dir = project("hover-mod");
    let (text, at) = caret(
        "\
import util;
fn main(): Int {
    return util.hel|lo();
}
",
    );
    let info = hover_at(&text, &dir.join("main.rg").display().to_string(), at).unwrap();
    assert_eq!(info.name, "hello");
    assert_eq!(info.signature, "fn hello(): Int");
    assert_eq!(info.crate_name, "util");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn project_module_alias_hover() {
    let dir = project("hover-alias");
    let (text, at) = caret(
        "\
import util as u;
fn main(): Int {
    return u.hel|lo();
}
",
    );
    let info = hover_at(&text, &dir.join("main.rg").display().to_string(), at).unwrap();
    assert_eq!(info.name, "hello");
    assert_eq!(info.signature, "fn hello(): Int");
    assert_eq!(info.crate_name, "util");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn from_import_alias_hover() {
    let dir = project("hover-from");
    let (text, at) = caret(
        "\
from util import hello as hi;
fn main(): Int {
    return h|i();
}
",
    );
    let info = hover_at(&text, &dir.join("main.rg").display().to_string(), at).unwrap();
    assert_eq!(info.signature, "fn hello(): Int");
    assert_eq!(info.crate_name, "util");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn local_var_hover() {
    let (text, at) = caret(
        "\
fn main(): Int {
    var sum: Int = 1;
    return su|m;
}
",
    );
    let info = hover_at(&text, "t.rg", at).unwrap();
    assert_eq!(info.signature, "var sum: Int");
}

#[test]
fn method_on_guessed_type() {
    let (text, at) = caret(
        "\
class Dog {
    fn speak(): String {
        return \"woof\";
    }
}
fn main(): Int {
    var d = Dog {};
    print(d.spe|ak());
    return 0;
}
",
    );
    let info = hover_at(&text, "t.rg", at).unwrap();
    assert_eq!(info.signature, "fn speak(): String");
}

#[test]
fn class_decl_hover() {
    let (text, at) = caret(
        "\
abstract class Ani|mal {
    abstract fn speak(): String;
}
",
    );
    let info = hover_at(&text, "t.rg", at).unwrap();
    assert_eq!(info.signature, "abstract class Animal");
}

#[test]
fn block_comment_docs() {
    let (text, at) = caret(
        "\
/#
 this is a bird
#/
class bi|rd extends animal {
}
",
    );
    let info = hover_at(&text, "t.rg", at).unwrap();
    assert_eq!(info.signature, "class bird extends animal");
    assert_eq!(info.doc, "this is a bird");
}

#[test]
fn nothing_inside_string_hover() {
    let (text, at) = caret(
        "\
fn main(): Int {
    print(\"ad|d\");
    return 0;
}
",
    );
    assert!(hover_at(&text, "t.rg", at).is_none());
}

fn labels(src: &str) -> Vec<String> {
    labels_at(src, "t.rg")
}

fn labels_at(src: &str, path: &str) -> Vec<String> {
    let (text, at) = caret(src);
    let mut labels: Vec<_> = suggest(&text, path, at)
        .into_iter()
        .map(|item| item.label)
        .collect();
    labels.sort();
    labels.dedup();
    labels
}

fn has(labels: &[String], name: &str) -> bool {
    labels.iter().any(|l| l == name)
}

#[test]
fn attributes_after_at() {
    let labels = labels(
        "\
@|
fn t() {
}
",
    );
    for name in ["test", "deprecated", "constexpr", "ufcs", "optional"] {
        assert!(has(&labels, name), "{labels:?}");
    }
    assert!(!has(&labels, "fn"), "{labels:?}");
}

#[test]
fn std_members() {
    let labels = labels(
        "\
fn main(): Int {
    std.|
    return 0;
}
",
    );
    for name in ["math", "str", "io", "v4", "UUID"] {
        assert!(has(&labels, name), "{labels:?}");
    }
    assert!(!has(&labels, "fn"), "{labels:?}");
}

#[test]
fn math_members() {
    let labels = labels(
        "\
fn main(): Int {
    math.|
    return 0;
}
",
    );
    assert!(has(&labels, "abs"), "{labels:?}");
    assert!(has(&labels, "sqrt"), "{labels:?}");
}

#[test]
fn types_after_colon() {
    let labels = labels(
        "\
fn main(): |
",
    );
    assert!(has(&labels, "Int"), "{labels:?}");
    assert!(has(&labels, "String"), "{labels:?}");
    assert!(!has(&labels, "fn"), "{labels:?}");
}

#[test]
fn default_keywords_and_builtins() {
    let labels = labels(
        "\
fn main(): Int {
    |
    return 0;
}
",
    );
    assert!(has(&labels, "fn"));
    assert!(has(&labels, "print"));
    assert!(has(&labels, "main"));
    assert!(has(&labels, "Int"));
}

#[test]
fn locals_and_params() {
    let labels = labels(
        "\
fn add(a: Int, b: Int): Int {
    var sum = a + b;
    |
    return sum;
}
",
    );
    assert!(has(&labels, "a"), "{labels:?}");
    assert!(has(&labels, "b"), "{labels:?}");
    assert!(has(&labels, "sum"), "{labels:?}");
}

#[test]
fn guessed_type_members() {
    let labels = labels(
        "\
class Dog {
    pub var hp: Int = 1;
    fn speak(): String {
        return \"woof\";
    }
}
fn main(): Int {
    var d = Dog {};
    d.|
    return 0;
}
",
    );
    assert!(has(&labels, "speak"), "{labels:?}");
    assert!(has(&labels, "hp"), "{labels:?}");
}

#[test]
fn enum_variants_after_dot() {
    let labels = labels(
        "\
enum te {
    one,
    two,
    three
}
fn main(): Int {
    var a: te = te.|
    return 0;
}
",
    );
    for name in ["one", "two", "three"] {
        assert!(has(&labels, name), "{labels:?}");
    }
    assert!(!has(&labels, "fn"));
}

#[test]
fn stdlib_color_variants_after_dot() {
    let labels = labels(
        "\
fn main(): Int {
    var b : Color = Color.|
    return 0;
}
",
    );
    for name in ["Red", "Rgb", "Argb", "Black", "White"] {
        assert!(has(&labels, name), "{labels:?}");
    }
    assert!(!has(&labels, "fn"));
}

#[test]
fn color_in_type_position() {
    let labels = labels(
        "\
fn main(): Int {
    var b : Co|
    return 0;
}
",
    );
    assert!(has(&labels, "Color"), "{labels:?}");
}

#[test]
fn enum_payload_variants() {
    let labels = labels(
        "\
enum Shape {
    Circle(Int),
    Rect(width: Int, height: Int),
}
fn main(): Int {
    var s = Shape.|
    return 0;
}
",
    );
    assert!(has(&labels, "Circle"), "{labels:?}");
    assert!(has(&labels, "Rect"), "{labels:?}");
}

#[test]
fn signal_members() {
    let labels = labels(
        "\
signal clicked();
fn main(): Int {
    clicked.|
    return 0;
}
",
    );
    for name in ["connect", "emit", "emit_deferred", "disconnect"] {
        assert!(has(&labels, name), "{labels:?}");
    }
}

#[test]
fn nothing_inside_string_complete() {
    let labels = labels(
        "\
fn main(): Int {
    print(\"hel|lo\");
    return 0;
}
",
    );
    assert!(labels.is_empty(), "{labels:?}");
}

#[test]
fn nothing_inside_comment() {
    let labels = labels(
        "\
fn main(): Int {
    // pri|
    return 0;
}
",
    );
    assert!(labels.is_empty(), "{labels:?}");
}

#[test]
fn crates_after_import() {
    let import_labels = labels(
        "\
import |
fn main(): Int {
    return 0;
}
",
    );
    for name in ["std", "ui", "math", "str"] {
        assert!(has(&import_labels, name), "{import_labels:?}");
    }
    assert!(!has(&import_labels, "fn"));
    assert!(!has(&import_labels, "print"));
    let from_labels = labels(
        "\
from |
fn main(): Int {
    return 0;
}
",
    );
    assert!(has(&from_labels, "ui"), "{from_labels:?}");
    assert!(!has(&from_labels, "fn"));
}

#[test]
fn std_children_after_import_std_dot() {
    let labels = labels(
        "\
import std.|
fn main(): Int {
    return 0;
}
",
    );
    for name in ["ui", "math", "json"] {
        assert!(has(&labels, name), "{labels:?}");
    }
    assert!(!has(&labels, "v4"));
    assert!(!has(&labels, "fn"));
}

#[test]
fn names_after_from_import() {
    let labels = labels_at(
        "\
from ui import |
fn main(): Int {
    return 0;
}
",
        &java_path(),
    );
    assert!(has(&labels, "Color"), "{labels:?}");
    assert!(!has(&labels, "fn"));
    assert!(!has(&labels, "std"));
}

#[test]
fn project_module_after_import() {
    let dir = project("comp-mod");
    let path = dir.join("main.rg").display().to_string();
    let labels = labels_at(
        "\
import |
fn main(): Int {
    return 0;
}
",
        &path,
    );
    assert!(has(&labels, "util"), "{labels:?}");
    assert!(has(&labels, "ui"), "{labels:?}");
    let labels = labels_at(
        "\
from util import |
fn main(): Int {
    return 0;
}
",
        &path,
    );
    assert!(has(&labels, "hello"), "{labels:?}");
    let labels = labels_at(
        "\
import util;
fn main(): Int {
    return util.|
}
",
        &path,
    );
    assert!(has(&labels, "hello"), "{labels:?}");
    assert!(!has(&labels, "fn"), "{labels:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn project_module_after_alias() {
    let dir = project("comp-alias");
    let labels = labels_at(
        "\
import util as u;
fn main(): Int {
    return u.|
}
",
        &dir.join("main.rg").display().to_string(),
    );
    assert!(has(&labels, "hello"), "{labels:?}");
    assert!(!has(&labels, "fn"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn math_after_alias() {
    let labels = labels_at(
        "\
import math as m;
fn main(): Int {
    return m.|
}
",
        &java_path(),
    );
    assert!(has(&labels, "abs"), "{labels:?}");
}

fn go(src: &str) -> Option<Loc> {
    go_at(src, "t.rg")
}

fn go_at(src: &str, path: &str) -> Option<Loc> {
    let (text, at) = caret(src);
    let loc = goto_at(&text, path, at);
    if let Some(loc) = &loc {
        if loc.path == path || path == "t.rg" {
            let slice: String = text
                .chars()
                .skip(loc.start)
                .take(loc.end - loc.start)
                .collect();
            assert_eq!(slice, loc.name, "path {} line {}", loc.path, loc.line);
        }
    }
    loc
}

#[test]
fn jumps_to_function() {
    let loc = go("\
fn add(a: Int, b: Int): Int {
    return a + b;
}
fn main(): Int {
    return ad|d(1, 2);
}
")
    .unwrap();
    assert_eq!(loc.name, "add");
    assert_eq!(loc.line, 1);
}

#[test]
fn jumps_to_local() {
    let loc = go("\
fn main(): Int {
    var sum: Int = 1;
    return su|m;
}
")
    .unwrap();
    assert_eq!(loc.name, "sum");
    assert_eq!(loc.line, 2);
}

#[test]
fn jumps_to_param() {
    let loc = go("\
fn add(a: Int, b: Int): Int {
    return a| + b;
}
")
    .unwrap();
    assert_eq!(loc.name, "a");
    assert_eq!(loc.line, 1);
}

#[test]
fn jumps_to_method() {
    let loc = go("\
class Dog {
    fn speak(): String {
        return \"woof\";
    }
}
fn main(): Int {
    var d = Dog {};
    print(d.spe|ak());
    return 0;
}
")
    .unwrap();
    assert_eq!(loc.name, "speak");
    assert_eq!(loc.line, 2);
}

#[test]
fn jumps_to_class() {
    let loc = go("\
class Dog {
    fn speak(): String {
        return \"woof\";
    }
}
fn main(): Int {
    var d = Do|g {};
    return 0;
}
")
    .unwrap();
    assert_eq!(loc.name, "Dog");
    assert_eq!(loc.line, 1);
}

#[test]
fn jumps_to_field() {
    let loc = go("\
class Dog {
    pub var hp: Int = 1;
    fn hit(): Int {
        return h|p;
    }
}
")
    .unwrap();
    assert_eq!(loc.name, "hp");
    assert_eq!(loc.line, 2);
}

#[test]
fn builtin_has_no_target() {
    assert!(go("\
fn main(): Int {
    pri|nt(\"hi\");
    return 0;
}
",)
    .is_none());
}

#[test]
fn jumps_to_import_crate() {
    let loc = go_at(
        "\
import u|i;
fn main(): Int {
    return 0;
}
",
        &java_path(),
    )
    .unwrap();
    assert!(
        loc.path
            .replace('\\', "/")
            .contains("builtin/std/ui/lib.rg"),
        "{}",
        loc.path
    );
}

#[test]
fn jumps_to_from_import_name() {
    let loc = go_at(
        "\
from ui import Col|or;
fn main(): Int {
    return 0;
}
",
        &java_path(),
    )
    .unwrap();
    assert_eq!(loc.name, "Color");
    assert!(
        loc.path
            .replace('\\', "/")
            .contains("builtin/std/ui/color.rg"),
        "{}",
        loc.path
    );
}

#[test]
fn jumps_to_stdlib_member() {
    let loc = go_at(
        "\
fn main(): Int {
    return math.ab|s(-1);
}
",
        &java_path(),
    )
    .unwrap();
    assert_eq!(loc.name, "abs");
    assert!(
        loc.path.replace('\\', "/").contains("builtin/std/math/"),
        "{}",
        loc.path
    );
}

#[test]
fn jumps_to_project_module() {
    let dir = project("goto-mod");
    let main = dir.join("main.rg").display().to_string();
    let loc = go_at(
        "\
import ut|il;
fn main(): Int {
    return util.hello();
}
",
        &main,
    )
    .unwrap();
    assert!(
        loc.path.replace('\\', "/").contains("util/lib.rg"),
        "{}",
        loc.path
    );
    let loc = go_at(
        "\
import util;
fn main(): Int {
    return util.hel|lo();
}
",
        &main,
    )
    .unwrap();
    assert_eq!(loc.name, "hello");
    assert!(
        loc.path.replace('\\', "/").contains("util/lib.rg"),
        "{}",
        loc.path
    );
    let loc = go_at(
        "\
import util as u;
fn main(): Int {
    return u.hel|lo();
}
",
        &main,
    )
    .unwrap();
    assert_eq!(loc.name, "hello");
    assert!(
        loc.path.replace('\\', "/").contains("util/lib.rg"),
        "{}",
        loc.path
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn jumps_to_from_import_alias() {
    let dir = project("goto-from");
    let main = dir.join("main.rg").display().to_string();
    let loc = go_at(
        "\
from util import hello as hi;
fn main(): Int {
    return h|i();
}
",
        &main,
    )
    .unwrap();
    assert_eq!(loc.name, "hello");
    assert!(
        loc.path.replace('\\', "/").contains("util/lib.rg"),
        "{}",
        loc.path
    );
    let loc = go_at(
        "\
from util import hello as h|i;
fn main(): Int {
    return hi();
}
",
        &main,
    )
    .unwrap();
    assert_eq!(loc.name, "hello");
    assert!(
        loc.path.replace('\\', "/").contains("util/lib.rg"),
        "{}",
        loc.path
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn nothing_inside_string_goto() {
    assert!(go("\
fn add(a: Int, b: Int): Int {
    return a + b;
}
fn main(): Int {
    print(\"ad|d\");
    return 0;
}
",)
    .is_none());
}

fn assert_godot(loc: &Loc, name: &str, class_line: &str) {
    assert_eq!(loc.name, name);
    let path = loc.path.replace('\\', "/");
    assert!(path.ends_with("builtin/godot/lib.rg"), "{}", loc.path);
    let text = fs::read_to_string(&loc.path).unwrap();
    let chars: Vec<char> = text.chars().collect();
    assert!(loc.end <= chars.len() && loc.end >= loc.start);
    let slice: String = chars[loc.start..loc.end].iter().collect();
    assert_eq!(slice, name);
    let lines: Vec<&str> = text.split('\n').collect();
    let mut owner = "";
    for line in lines.iter().take(loc.line as usize).rev() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("class ") || trimmed.starts_with("struct ") {
            owner = trimmed;
            break;
        }
    }
    assert!(owner.starts_with(class_line), "{owner}");
}

#[test]
fn jumps_from_face_to_control() {
    let file = godot_file("face.rg");
    let src = fs::read_to_string(&file).unwrap();
    let caret = src.find("Control").unwrap();
    let loc = goto_at(&src, &file, caret).unwrap();
    assert_godot(&loc, "Control", "class Control");
}

#[test]
fn jumps_to_godot_class() {
    let loc = go_at(
        "\
class Player extends Nod|e2D {
    fn _ready() {}
}
",
        &godot_file("player.rg"),
    )
    .unwrap();
    assert_godot(&loc, "Node2D", "class Node2D");
}

#[test]
fn jumps_to_inherited_node_field() {
    let loc = go_at(
        "\
class Player extends Node2D {
    fn _process(delta: Float) {
        posi|tion.x = position.x + 1.0;
    }
}
",
        &godot_file("player.rg"),
    )
    .unwrap();
    assert_godot(&loc, "position", "class Node2D");
}

#[test]
fn jumps_to_inherited_node_method() {
    let loc = go_at(
        "\
class Player extends Node2D {
    fn _ready() {
        get_|node(\"A\");
    }
}
",
        &godot_file("player.rg"),
    )
    .unwrap();
    assert_godot(&loc, "get_node", "class Node");
}

#[test]
fn jumps_to_field_on_parent_of_node2d() {
    let loc = go_at(
        "\
class Player extends Node2D {
    fn _ready() {
        visi|ble = false;
    }
}
",
        &godot_file("player.rg"),
    )
    .unwrap();
    assert_godot(&loc, "visible", "class CanvasItem");
}

#[test]
fn jumps_to_member_of_inherited_field() {
    let loc = go_at(
        "\
class Player extends Node2D {
    fn _process(delta: Float) {
        position.|x = position.x + 1.0;
    }
}
",
        &godot_file("player.rg"),
    )
    .unwrap();
    assert_godot(&loc, "x", "class Vec2");
}

#[test]
fn jumps_to_color_component() {
    let loc = go_at(
        "\
class Player extends Node2D {
    fn _ready() {
        modulate.|a = 0.5;
    }
}
",
        &godot_file("player.rg"),
    )
    .unwrap();
    assert_godot(&loc, "a", "class Color");
}

#[test]
fn jumps_to_engine_signal() {
    let loc = go_at(
        "\
class Player extends Node2D {
    fn _ready() {
        visibility_|changed.emit();
    }
}
",
        &godot_file("player.rg"),
    )
    .unwrap();
    assert_godot(&loc, "visibility_changed", "class CanvasItem");
}

#[test]
fn jumps_through_self_and_type_qualifier() {
    let self_pos = go_at(
        "\
class Player extends Node2D {
    fn _ready() {
        self.posi|tion.x = 1.0;
    }
}
",
        &godot_file("player.rg"),
    )
    .unwrap();
    assert_godot(&self_pos, "position", "class Node2D");
    let typed = go_at(
        "\
class Player extends Node2D {
    fn _ready() {
        Node2D.queue_|redraw();
    }
}
",
        &godot_file("player.rg"),
    )
    .unwrap();
    assert_godot(&typed, "queue_redraw", "class CanvasItem");
}

#[test]
fn script_member_still_wins_in_godot_project() {
    let path = "godot/demo/player.rg";
    let (text, at) = caret(
        "\
class Player extends Node2D {
    var position: Int = 0;
    fn _ready() {
        posi|tion = 1;
    }
}
",
    );
    let loc = goto_at(&text, path, at).unwrap();
    assert_eq!(loc.name, "position");
    assert_eq!(loc.path, path);
    let slice: String = text
        .chars()
        .skip(loc.start)
        .take(loc.end - loc.start)
        .collect();
    assert_eq!(slice, "position");
}

#[test]
fn plain_file_does_not_use_godot_stubs() {
    assert!(go("\
class Player extends Nod|e2D {
    fn _ready() {}
}
",)
    .is_none());
}

fn usages_of(src: &str) -> (String, Vec<Hit>) {
    let (text, at) = caret(src);
    let hits = usages_in_file(&text, "t.rg", at);
    (text, hits)
}

#[test]
fn function_decl_and_call() {
    let (text, hits) = usages_of(
        "\
fn add(a: Int, b: Int): Int {
    return a + b;
}
fn main(): Int {
    return ad|d(1, 2);
}
",
    );
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].name, "add");
    assert!(hits[0].write);
    assert_eq!(hits[0].line, 1);
    assert!(!hits[1].write);
    assert_eq!(hits[1].line, 5);
    for hit in &hits {
        let slice: String = text
            .chars()
            .skip(hit.start)
            .take(hit.end - hit.start)
            .collect();
        assert_eq!(slice, "add");
    }
}

#[test]
fn local_reads_and_write() {
    let (_text, hits) = usages_of(
        "\
fn main(): Int {
    var sum: Int = 1;
    sum = sum + 1;
    return su|m;
}
",
    );
    assert_eq!(hits.len(), 4);
    assert!(hits[0].write);
    assert!(hits[1].write);
    assert!(!hits[2].write);
    assert!(!hits[3].write);
}

#[test]
fn qualified_method_skips_other_speak() {
    let (_text, hits) = usages_of(
        "\
class Dog {
    fn speak(): String {
        return \"woof\";
    }
}
class Cat {
    fn speak(): String {
        return \"meow\";
    }
}
fn main(): Int {
    var d = Dog {};
    print(d.spe|ak());
    print(Cat {}.speak());
    return 0;
}
",
    );
    let lines: Vec<u32> = hits.iter().map(|h| h.line).collect();
    assert!(lines.contains(&2), "{lines:?}");
    assert!(lines.contains(&13), "{lines:?}");
    assert!(!lines.contains(&14), "{lines:?}");
}

#[test]
fn nothing_inside_string_usages() {
    let (_text, hits) = usages_of(
        "\
fn add(a: Int, b: Int): Int {
    return a + b;
}
fn main(): Int {
    print(\"ad|d\");
    return 0;
}
",
    );
    assert!(hits.is_empty());
}

#[test]
fn stdlib_member_includes_definition() {
    let (text, at) = caret(
        "\
fn main(): Int {
    return math.ab|s(-1);
}
",
    );
    let hits = including_crates(&text, &java_path(), at);
    assert!(
        hits.iter().any(|h| h.write
            && h.name == "abs"
            && h.path.replace('\\', "/").contains("builtin/std/math/")),
        "{hits:?}"
    );
    assert!(
        hits.iter().any(|h| !h.write && h.qualifier == "math"),
        "{hits:?}"
    );
}

#[test]
fn from_import_includes_crate_def() {
    let (text, at) = caret(
        "\
from ui import Col|or;
fn main(): Int {
    return 0;
}
",
    );
    let path = java_path();
    let hits = including_crates(&text, &path, at);
    assert!(
        hits.iter().any(|h| h.write
            && h.name == "Color"
            && h.path
                .replace('\\', "/")
                .contains("builtin/std/ui/color.rg")),
        "{hits:?}"
    );
    assert!(
        hits.iter().any(|h| h.path == path && h.name == "Color"),
        "{hits:?}"
    );
}

#[test]
fn project_module_includes_lib() {
    let dir = project("usages-mod");
    let (text, at) = caret(
        "\
import util;
fn main(): Int {
    return util.hel|lo();
}
",
    );
    let hits = including_crates(&text, &dir.join("main.rg").display().to_string(), at);
    assert!(
        hits.iter().any(|h| h.write
            && h.name == "hello"
            && h.path.replace('\\', "/").contains("util/lib.rg")),
        "{hits:?}"
    );
    assert!(hits.iter().any(|h| h.qualifier == "util"), "{hits:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn project_module_alias_includes_lib() {
    let dir = project("usages-alias");
    let (text, at) = caret(
        "\
import util as u;
fn main(): Int {
    return u.hel|lo();
}
",
    );
    let hits = including_crates(&text, &dir.join("main.rg").display().to_string(), at);
    assert!(
        hits.iter().any(|h| h.write
            && h.name == "hello"
            && h.path.replace('\\', "/").contains("util/lib.rg")),
        "{hits:?}"
    );
    assert!(hits.iter().any(|h| h.qualifier == "u"), "{hits:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn from_import_alias_includes_lib() {
    let dir = project("usages-from");
    let (text, at) = caret(
        "\
from util import hello as hi;
fn main(): Int {
    return h|i();
}
",
    );
    let hits = including_crates(&text, &dir.join("main.rg").display().to_string(), at);
    assert!(
        hits.iter().any(|h| h.write
            && h.name == "hello"
            && h.path.replace('\\', "/").contains("util/lib.rg")),
        "{hits:?}"
    );
    assert!(hits.iter().any(|h| h.name == "hi"), "{hits:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn valid_names() {
    assert!(is_valid_name("add"));
    assert!(is_valid_name("Coin"));
    assert!(is_valid_name("_x"));
    assert!(!is_valid_name(""));
    assert!(!is_valid_name("1x"));
    assert!(!is_valid_name("a-b"));
    assert!(!is_valid_name("fn"));
    assert!(!is_valid_name("self"));
    assert!(!is_valid_name("async"));
}

#[test]
fn skips_builtin_unless_origin_is_builtin() {
    assert!(skip_file(
        "src/app.rg",
        "C:/CODE/RoseGoldJava/builtin/std/io/lib.rg"
    ));
    assert!(skip_file("app.rg", "builtin/std/io/lib.rg"));
    assert!(!skip_file("builtin/std/io/lib.rg", "builtin/std/io/lib.rg"));
    assert!(!skip_file("src/app.rg", "src/other.rg"));
}

#[test]
fn applies_hits_back_to_front() {
    let src = "\
fn add(a: Int, b: Int): Int {
    return a + b;
}
fn main(): Int {
    return add(1, 2);
}
";
    let hits = of_name(src, "t.rg", "add", "");
    assert_eq!(hits.len(), 2);
    let out = rename_apply(src, &hits, "sum");
    assert!(out.contains("fn sum("), "{out}");
    assert!(out.contains("return sum(1, 2);"), "{out}");
    assert!(!out.contains("add"), "{out}");
}

#[test]
fn qualified_rename_keeps_other_speak() {
    let src = "\
class Dog {
    fn speak(): String {
        return \"woof\";
    }
}
class Cat {
    fn speak(): String {
        return \"meow\";
    }
}
fn main(): Int {
    var d = Dog {};
    print(d.speak());
    print(Cat {}.speak());
    return 0;
}
";
    let caret = src.find("d.speak").unwrap() + 2;
    let hits = usages_in_file(src, "t.rg", caret);
    let out = rename_apply(src, &hits, "bark");
    assert!(out.contains("d.bark()"), "{out}");
    assert!(out.contains("Cat {}.speak()"), "{out}");
    assert!(!out.contains("d.speak()"), "{out}");
}

#[test]
fn skips_stdlib_crate_on_rewrite() {
    let (text, at) = caret(
        "\
fn main(): Int {
    return math.ab|s(-1);
}
",
    );
    let path = java_path();
    let all = including_crates(&text, &path, at);
    assert!(all.iter().any(|h| skip_file(&path, &h.path)), "{all:?}");
    let rewrite = rewrite_hits(&text, &path, at);
    assert!(rewrite.iter().any(|h| h.qualifier == "math"), "{rewrite:?}");
    assert!(
        rewrite.iter().all(|h| !is_stdlib_file(&h.path)),
        "{rewrite:?}"
    );
}

#[test]
fn rewrites_project_module_crate() {
    let dir = project("rename-mod");
    let lib = dir.join("util").join("lib.rg");
    let (text, at) = caret(
        "\
import util;
fn main(): Int {
    return util.hel|lo();
}
",
    );
    let main_path = dir.join("main.rg").display().to_string();
    assert!(!skip_file(&main_path, &lib.display().to_string()));
    let hits = rewrite_hits(&text, &main_path, at);
    assert!(
        hits.iter()
            .any(|h| h.write && h.path.replace('\\', "/").contains("util/lib.rg")),
        "{hits:?}"
    );
    let lib_text = fs::read_to_string(&lib).unwrap();
    let new_main = rename_apply(
        &text,
        &hits
            .iter()
            .filter(|h| h.path == main_path)
            .cloned()
            .collect::<Vec<_>>(),
        "greet",
    );
    let new_lib = rename_apply(
        &lib_text,
        &hits
            .iter()
            .filter(|h| same_file(&h.path, &lib.display().to_string()))
            .cloned()
            .collect::<Vec<_>>(),
        "greet",
    );
    assert!(new_main.contains("util.greet()"), "{new_main}");
    assert!(new_lib.contains("fn greet("), "{new_lib}");
    assert!(!new_lib.contains("fn hello("), "{new_lib}");
    let _ = fs::remove_dir_all(&dir);
}

fn same_file(a: &str, b: &str) -> bool {
    rosegold_check::same_rg_file(a, b)
}

#[test]
fn local_fn_second_arg() {
    let info = sig("\
fn add(a: Int, b: Int): Int {
    return a + b;
}
fn main(): Int {
    return add(1, |2);
}
")
    .unwrap();
    assert_eq!(info.name, "add");
    assert_eq!(info.label, "add(a: Int, b: Int): Int");
    assert_eq!(info.active_param, 1);
    assert_eq!(info.params[0], "a: Int");
    assert_eq!(info.params[1], "b: Int");
}

#[test]
fn incomplete_call() {
    let info = sig("\
fn add(a: Int, b: Int): Int {
    return a + b;
}
fn main(): Int {
    return add(|
}
")
    .unwrap();
    assert_eq!(info.name, "add");
    assert_eq!(info.active_param, 0);
    assert_eq!(info.label, "add(a: Int, b: Int): Int");
}

#[test]
fn after_comma() {
    let info = sig("\
fn add(a: Int, b: Int): Int {
    return a + b;
}
fn main(): Int {
    return add(1,|
}
")
    .unwrap();
    assert_eq!(info.active_param, 1);
}

#[test]
fn builtin_len() {
    let info = sig("\
fn main(): Int {
    return len(|);
}
")
    .unwrap();
    assert_eq!(info.name, "len");
    assert_eq!(info.label, "len(xs)");
    assert_eq!(info.active_param, 0);
}

#[test]
fn builtin_print_signature() {
    let info = sig("\
fn main(): Int {
    print(|
    return 0;
}
")
    .unwrap();
    assert_eq!(info.label, "print(...)");
}

#[test]
fn method_skips_self() {
    let info = sig("\
class Cat {
    fn speak(self, times: Int): Int {
        return times;
    }
}
fn main(): Int {
    return Cat {}.speak(|);
}
")
    .unwrap();
    assert_eq!(info.name, "speak");
    assert_eq!(info.label, "speak(times: Int): Int");
    assert_eq!(info.active_param, 0);
    assert_eq!(info.params.len(), 1);
}

#[test]
fn nested_inner_call() {
    let info = sig("\
fn add(a: Int, b: Int): Int {
    return a + b;
}
fn main(): Int {
    return add(add(1, |2), 3);
}
")
    .unwrap();
    assert_eq!(info.name, "add");
    assert_eq!(info.active_param, 1);
}

#[test]
fn if_is_not_a_call() {
    assert!(sig("\
fn main(): Int {
    if (|true) {
        return 1;
    }
    return 0;
}
",)
    .is_none());
}

#[test]
fn throwing_fn() {
    let info = sig("\
fn boom(msg: String) throws {
    throw msg;
}
fn main(): Int {
    try boom(|);
    return 0;
}
")
    .unwrap();
    assert!(info.label.contains("throws"), "{}", info.label);
    assert_eq!(info.params[0], "msg: String");
}

#[test]
fn stdlib_math_abs_signature() {
    let (text, at) = caret(
        "\
fn main(): Int {
    return math.abs(|-1);
}
",
    );
    let info = signature_at(&text, &java_path(), at).unwrap();
    assert_eq!(info.name, "abs");
    assert_eq!(info.label, "abs(n: Int): Int");
    assert_eq!(info.active_param, 0);
}

#[test]
fn project_module_call() {
    let dir = project("sig-mod");
    let (text, at) = caret(
        "\
import util;
fn main(): Int {
    return util.hello(|);
}
",
    );
    let info = signature_at(&text, &dir.join("main.rg").display().to_string(), at).unwrap();
    assert_eq!(info.name, "hello");
    assert_eq!(info.label, "hello(): Int");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn project_module_alias_call() {
    let dir = project("sig-alias");
    let (text, at) = caret(
        "\
import util as u;
fn main(): Int {
    return u.hello(|);
}
",
    );
    let info = signature_at(&text, &dir.join("main.rg").display().to_string(), at).unwrap();
    assert_eq!(info.name, "hello");
    assert_eq!(info.label, "hello(): Int");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn from_import_call() {
    let dir = project("sig-from");
    let (text, at) = caret(
        "\
from util import hello;
fn main(): Int {
    return hello(|);
}
",
    );
    let info = signature_at(&text, &dir.join("main.rg").display().to_string(), at).unwrap();
    assert_eq!(info.name, "hello");
    assert_eq!(info.label, "hello(): Int");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn from_import_alias_call() {
    let dir = project("sig-from-alias");
    let (text, at) = caret(
        "\
from util import hello as hi;
fn main(): Int {
    return hi(|);
}
",
    );
    let info = signature_at(&text, &dir.join("main.rg").display().to_string(), at).unwrap();
    assert_eq!(info.name, "hi");
    assert_eq!(info.label, "hi(): Int");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cpp_style_offset() {
    let info = signature_at("add(1, 2)", "t.rg", 7).unwrap();
    assert_eq!(info.name, "add");
    assert_eq!(info.active_param, 1);
}

fn sig(src: &str) -> Option<SignatureInfo> {
    let (text, at) = caret(src);
    signature_at(&text, "sig.rg", at)
}
