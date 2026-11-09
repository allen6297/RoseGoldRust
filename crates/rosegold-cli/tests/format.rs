use std::fs;
use std::path::Path;

use rosegold_cli::{fmt_program, format_source, list_rg_files, Options};

fn fmt(source: &str, path: &str) -> rosegold_cli::FormatResult {
    format_source(source, path, &Options::default())
}

fn round_trip(source: &str, path: &str) -> String {
    let f = fmt(source, path);
    assert!(f.ok, "{}", f.message);
    let again = format_source(&f.out, path, &Options::default());
    assert!(again.ok, "{}", again.message);
    assert_eq!(f.out, again.out);
    f.out
}

#[test]
fn spaces_after_colon() {
    let out = round_trip("fn main():Int{\nreturn 0;\n}\n", "fmt_check.rg");
    assert!(out.contains("fn main(): Int"), "{out}");
}

#[test]
fn keeps_formatted_string() {
    let out = round_trip(
        "fn main(): Int {\n    print(f\"mods {n}\");\n    return 0;\n}\n",
        "fmt_fstr.rg",
    );
    assert!(out.contains("f\"mods {n}\""), "{out}");
}

#[test]
fn keeps_export_range() {
    let out = round_trip(
        "class Probe extends Node {\n    export range(0.0, 800.0, 1) var speed: Float = 200.0;\n    fn _ready() {}\n}\n",
        "fmt_range.rg",
    );
    assert!(
        out.contains("export range(0.0, 800.0, 1) var speed: Float = 200.0;"),
        "{out}"
    );
}

#[test]
fn keeps_export_range_or_greater() {
    let out = round_trip(
        "class Probe extends Node {\n    export range(0, 100, 1, \"or_greater\") var health: Int = 0;\n    fn _ready() {}\n}\n",
        "fmt_or_greater.rg",
    );
    assert!(
        out.contains("export range(0, 100, 1, \"or_greater\") var health: Int = 0;"),
        "{out}"
    );
}

#[test]
fn keeps_export_range_or_less() {
    let out = round_trip(
        "class Probe extends Node {\n    export range(0, 100, 1, \"or_greater\", \"or_less\") var health: Int = 0;\n    fn _ready() {}\n}\n",
        "fmt_or_less.rg",
    );
    assert!(
        out.contains("export range(0, 100, 1, \"or_greater\", \"or_less\") var health: Int = 0;"),
        "{out}"
    );
}

#[test]
fn keeps_export_range_suffix() {
    let out = round_trip(
        "class Probe extends Node {\n    export range(0, 800, 1, \"or_greater\", \"suffix:px\") var width: Int = 0;\n    fn _ready() {}\n}\n",
        "fmt_suffix.rg",
    );
    assert!(
        out.contains("export range(0, 800, 1, \"or_greater\", \"suffix:px\") var width: Int = 0;"),
        "{out}"
    );
}

#[test]
fn keeps_export_enum() {
    let out = round_trip(
        "class Probe extends Node {\n    export enum(\"Idle\", \"Run\", \"Jump\") var state: Int = 0;\n    fn _ready() {}\n}\n",
        "fmt_enum.rg",
    );
    assert!(
        out.contains("export enum(\"Idle\", \"Run\", \"Jump\") var state: Int = 0;"),
        "{out}"
    );
}

#[test]
fn keeps_export_flags() {
    let out = round_trip(
        "class Probe extends Node {\n    export flags(\"Fire\", \"Water\", \"Poison\") var resist: Int = 0;\n    fn _ready() {}\n}\n",
        "fmt_flags.rg",
    );
    assert!(
        out.contains("export flags(\"Fire\", \"Water\", \"Poison\") var resist: Int = 0;"),
        "{out}"
    );
}

#[test]
fn keeps_export_file() {
    let out = round_trip(
        "class Probe extends Node {\n    export file(\"*.png\", \"*.webp\") var portrait: String = \"\";\n    fn _ready() {}\n}\n",
        "fmt_file.rg",
    );
    assert!(
        out.contains("export file(\"*.png\", \"*.webp\") var portrait: String = \"\";"),
        "{out}"
    );
}

#[test]
fn keeps_export_dir() {
    let out = round_trip(
        "class Probe extends Node {\n    export dir var folder: String = \"\";\n    fn _ready() {}\n}\n",
        "fmt_dir.rg",
    );
    assert!(
        out.contains("export dir var folder: String = \"\";"),
        "{out}"
    );
}

#[test]
fn keeps_export_multiline() {
    let out = round_trip(
        "class Probe extends Node {\n    export multiline var notes: String = \"\";\n    fn _ready() {}\n}\n",
        "fmt_multiline.rg",
    );
    assert!(
        out.contains("export multiline var notes: String = \"\";"),
        "{out}"
    );
}

#[test]
fn keeps_float_lexemes() {
    let out = round_trip(
        "fn main(): Float {\n  return 1.0 + 2.50;\n}\n",
        "fmt_float.rg",
    );
    assert!(out.contains("1.0") && out.contains("2.50"), "{out}");
}

#[test]
fn keeps_line_comments_between_items() {
    let out = round_trip(
        "fn a(): Int {\n  return 1;\n}\n\n// keep me\nfn b(): Int {\n  return 2;\n}\n",
        "fmt_comment.rg",
    );
    assert!(out.contains("// keep me"), "{out}");
}

#[test]
fn keeps_block_comments() {
    let out = round_trip(
        "/# block note #/\nfn main(): Int {\n  return 0;\n}\n",
        "fmt_block.rg",
    );
    assert!(out.contains("/# block note #/"), "{out}");
}

#[test]
fn trailing_comments_compact_and_strip() {
    let src = "fn a(): Int {\n  return 1; // trail\n}\n\nfn b(): Int {\n  return 2;\n}\n";
    let out = round_trip(src, "fmt_inline.rg");
    assert!(out.contains("return 1; // trail"), "{out}");

    let mut compact = Options::default();
    compact.blank_between_items = false;
    let c = format_source(src, "fmt_compact.rg", &compact);
    assert!(c.ok, "{}", c.message);
    assert!(c.out.contains("}\nfn b()"), "{c}", c = c.out);
    assert!(!c.out.contains("}\n\nfn b()"), "{c}", c = c.out);

    let mut noc = Options::default();
    noc.keep_comments = false;
    let n = format_source(src, "fmt_nocomment.rg", &noc);
    assert!(n.ok, "{}", n.message);
    assert!(!n.out.contains("// trail"), "{n}", n = n.out);
    assert!(n.out.contains("return 1;"), "{n}", n = n.out);
}

#[test]
fn body_comments() {
    let out = round_trip(
        "fn main(): Int {\n  // inside\n  return 0; // trail\n}\n",
        "fmt_body_comment.rg",
    );
    assert!(out.contains("// inside"), "{out}");
    assert!(out.contains("return 0; // trail"), "{out}");
}

#[test]
fn parse_error_fails() {
    let f = fmt("fn main( {\n", "bad.rg");
    assert!(!f.ok);
    assert_eq!(1, f.exit_code);
    assert!(!f.message.is_empty());
}

#[test]
fn list_rg_files_skips_hidden_and_build() {
    let dir = std::env::temp_dir().join(format!("rg-fmt-list-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join(".cache")).unwrap();
    fs::create_dir_all(dir.join("build")).unwrap();
    fs::write(dir.join("a.rg"), "fn a(): Int { return 0; }\n").unwrap();
    fs::write(dir.join("src").join("b.rg"), "fn b(): Int { return 0; }\n").unwrap();
    fs::write(
        dir.join(".cache").join("skip.rg"),
        "fn skip(): Int { return 0; }\n",
    )
    .unwrap();
    fs::write(
        dir.join("build").join("out.rg"),
        "fn out(): Int { return 0; }\n",
    )
    .unwrap();
    let files = list_rg_files(&dir);
    assert_eq!(2, files.len(), "{files:?}");
    assert!(files[0].ends_with("a.rg"), "{}", files[0].display());
    assert!(
        files[1].ends_with(Path::new("src").join("b.rg")),
        "{}",
        files[1].display()
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn fmt_directory_check_and_write() {
    let dir = std::env::temp_dir().join(format!("rg-fmt-dir-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let messy = dir.join("messy.rg");
    let ok = dir.join("ok.rg");
    let messy_src = "fn main():Int{\nreturn 0;\n}\n";
    let expected = format_source(messy_src, "messy.rg", &Options::default()).out;
    fs::write(&messy, messy_src).unwrap();
    fs::write(&ok, &expected).unwrap();
    let dir_s = dir.display().to_string();
    assert_eq!(
        1,
        fmt_program(&[String::from("--check"), dir_s.clone()], &dir)
    );
    assert_eq!(0, fmt_program(&[dir_s.clone()], &dir));
    assert_eq!(expected, fs::read_to_string(&messy).unwrap());
    assert_eq!(0, fmt_program(&[String::from("--check"), dir_s], &dir));
    let _ = fs::remove_dir_all(&dir);
}
