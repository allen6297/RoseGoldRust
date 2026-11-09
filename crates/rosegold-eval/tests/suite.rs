use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use rosegold_eval::{run_file, run_source, Outcome, Session};
use rosegold_syntax::parse_source;

fn java_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../RoseGoldJava")
}

fn cwd_mutex() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
}

fn cwd_lock() -> MutexGuard<'static, ()> {
    let guard = cwd_mutex();
    let root = java_root();
    std::env::set_current_dir(&root).unwrap_or_else(|err| {
        panic!("set cwd to {}: {err}", root.display());
    });
    guard
}

fn walk_rg(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk_rg(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rg") {
            out.push(path);
        }
    }
}

fn skip_no_main(path: &Path, text: &str) -> bool {
    let (program, diags) = parse_source(text, &path.display().to_string());
    diags.iter().all(|d| d.severity != "error") && program.fns.iter().all(|f| f.name != "main")
}

fn show(path: &Path, result: &Outcome) -> String {
    format!(
        "{}\nmessage: {}\nstdout:\n{}",
        path.display(),
        result.message,
        result.out
    )
}

#[test]
fn pass_files_exit_zero() {
    let _cwd = cwd_lock();
    let root = java_root().join("tests/pass");
    let mut files = Vec::new();
    walk_rg(&root, &mut files);
    assert!(!files.is_empty(), "no pass files under {}", root.display());
    let mut failures = Vec::new();
    let mut ran = 0usize;
    for path in files {
        let text = fs::read_to_string(&path).unwrap_or_default();
        if skip_no_main(&path, &text) {
            continue;
        }
        ran += 1;
        let arg = path.display().to_string();
        let result = run_file(&path, &[arg]);
        if !result.ok || result.exit_code != 0 {
            failures.push(show(&path, &result));
        }
    }
    assert!(ran > 0, "every pass file was skipped");
    assert!(
        failures.is_empty(),
        "{} pass file(s) failed:\n{}",
        failures.len(),
        failures.join("\n---\n")
    );
}

#[test]
fn math_import_works_outside_the_java_tree() {
    let path = r"C:\__rg_no_stdlib__\mods\probe.rg";
    let source = r#"
import math;

fn main(): Int {
    checks.eq(math.abs(-7), 7);
    checks.eq(math.sign(0), 0);
    checks.eq(math.min(3, 8), 3);
    checks.eq(math.max(3, 8), 8);
    checks.eq(math.clamp(5, 0, 3), 3);
    checks.eq(math.clamp(-1, 0, 15), 0);
    checks.eq(math.gcd(-48, 18), 6);
    checks.eq(math.pow(2, 10), 1024);
    checks.eq(math.lerp(0.0, 10.0, 0.5), 5.0);
    checks.eq(math.move_toward(0.0, 10.0, 3.0), 3.0);
    checks.eq(math.move_toward(8.0, 10.0, 3.0), 10.0);
    checks.eq(math.floor(3.9), 3.0);
    checks.eq(math.abs(-3.0), 3);
    return 0;
}
"#;
    let result = run_source(source, path, &[path.to_string()]);
    assert!(
        result.ok && result.exit_code == 0,
        "{}\n{}",
        result.message,
        result.out
    );
}

#[test]
fn str_and_collections_import_outside_the_java_tree() {
    let _cwd = cwd_mutex();
    let scratch = std::env::temp_dir();
    std::env::set_current_dir(&scratch).unwrap_or_else(|err| {
        panic!("set cwd to {}: {err}", scratch.display());
    });
    let path = r"C:\__rg_no_stdlib__\mods\probe.rg";
    let source = r#"
import str;
import collections;

fn main(): Int {
    checks.that(str.contains("cobblestone", "stone"));
    checks.that(str.starts_with("rosegold:rose", "rosegold:"));
    checks.that(str.ends_with("oak_log", "log"));
    checks.eq(str.length("hi"), 2);
    checks.that(str.is_empty(""));
    checks.eq_string(str.repeat("ab", 2), "abab");
    checks.eq_string(str.upper("Hi"), "HI");
    checks.eq_string(str.lower("Hi"), "hi");
    checks.eq_string(str.trim("  x  "), "x");
    checks.eq_string(str.slice("hello", 1, 4), "ell");
    var parts = str.split("a,b", ",");
    checks.eq(len(parts), 2);
    checks.eq_string(parts[0], "a");
    checks.eq_string(str.replace("oak_log", "oak", "birch"), "birch_log");
    checks.eq(str.find("hello", "ll"), 2);
    checks.eq(str.find("hello", "z"), -1);

    checks.that(collections.contains(["stone", "grass"], "grass"));
    checks.that(collections.contains([1, 2], 9) == false);
    checks.eq(collections.index_of(["a", "b"], "b"), 1);
    checks.eq(collections.index_of([1, 2], 9), -1);
    checks.eq(collections.count([1, 2, 1], 1), 2);
    checks.eq(collections.sum([1, 2, 3]), 6);
    checks.eq(collections.sum([]), 0);
    checks.eq(collections.min([3, 1, 2], 0), 1);
    checks.eq(collections.min([], 7), 7);
    checks.eq(collections.max([3, 1, 2], 0), 3);
    checks.eq(collections.max([], -1), -1);
    checks.eq_string(collections.join(["a", "b"], "-"), "a-b");
    checks.eq_string(collections.join([], ","), "");
    var back = collections.reversed([1, 2, 3]);
    checks.eq(len(back), 3);
    checks.eq(back[0], 3);
    checks.eq(back[2], 1);
    var names = collections.reversed(["stone", "grass"]);
    checks.eq_string(names[0], "grass");
    return 0;
}
"#;
    let result = run_source(source, path, &[path.to_string()]);
    assert!(
        result.ok && result.exit_code == 0,
        "{}\n{}",
        result.message,
        result.out
    );
}

#[test]
fn helpers_import_outside_the_java_tree() {
    let _cwd = cwd_mutex();
    let scratch = std::env::temp_dir();
    std::env::set_current_dir(&scratch).unwrap_or_else(|err| {
        panic!("set cwd to {}: {err}", scratch.display());
    });
    let path = r"C:\__rg_no_stdlib__\mods\probe.rg";
    let source = r#"
import helpers;

fn main(): Int {
    checks.eq_string(helpers.id("rosegold", "rose"), "rosegold:rose");
    checks.eq_string(helpers.id("", "stone"), "stone");
    checks.eq_string(helpers.namespace_of("rosegold:rose_block"), "rosegold");
    checks.eq_string(helpers.name_of("rosegold:rose_block"), "rose_block");
    checks.eq_string(helpers.name_of("stone"), "stone");
    checks.that(helpers.same_id("rosegold:rose", "rose"));
    checks.that(helpers.same_id("stone", "stone"));
    checks.that(helpers.same_id("minecraft:rose", "rosegold:rose") == false);
    checks.eq(helpers.chunk_of(16.0), 1);
    checks.eq(helpers.chunk_of(15.0), 0);
    checks.eq(helpers.chunk_of(-1.0), -1);
    checks.eq(helpers.floor_block(-0.2), -1);
    return 0;
}
"#;
    let result = run_source(source, path, &[path.to_string()]);
    assert!(
        result.ok && result.exit_code == 0,
        "{}\n{}",
        result.message,
        result.out
    );
}

#[test]
fn register_returns_the_block_id() {
    let source = r#"
fn main(): Int {
    var block_id = block.register("rosegold", "rose_block");
    var item_id = item.register("rosegold", "rose");
    var mob_id = entity.register("rosegold", "wisp");
    checks.eq_string(block_id, "rosegold:rose_block");
    checks.eq_string(item_id, "rosegold:rose");
    checks.eq_string(mob_id, "rosegold:wisp");
    return 0;
}
"#;
    let result = run_source(source, "register.rg", &["register.rg".into()]);
    assert!(
        result.ok && result.exit_code == 0,
        "{}\n{}",
        result.message,
        result.out
    );
}

#[test]
fn block_texture_and_model_are_recorded() {
    let source = r#"
fn main(): Int {
    block.register("rosegold", "rose_block");
    block.texture("rosegold:rose_block", "assets/terralite/textures/wool.png");
    block.model("rosegold:rose_block", "cube");
    block.hardness("rosegold:rose_block", 2.5);
    block.light("rosegold:rose_block", 12);
    item.register("rosegold", "rose");
    item.places("rosegold:rose", "rosegold:rose_block");
    item.stack("rosegold:rose", 16);
    recipe.register("rosegold:rose_block", "rosegold:rose");
    recipe.shapeless("rosegold:bouquet", "rosegold:rose+rosegold:rose");
    recipe.shaped("rosegold:rose_post", "rosegold:rose,,,rosegold:rose,,,rosegold:rose,,");
    return 0;
}
"#;
    let session = Session::load(source, "attach.rg", None).expect("script loads");
    assert!(session.attachments().iter().any(|(kind, id, value)| {
        kind == "texture" && id == "rosegold:rose_block" && value.ends_with("wool.png")
    }));
    assert!(session
        .attachments()
        .iter()
        .any(|(kind, id, value)| kind == "model"
            && id == "rosegold:rose_block"
            && value == "cube"));
    assert!(session.attachments().iter().any(|(kind, id, value)| {
        kind == "hardness" && id == "rosegold:rose_block" && value.starts_with("2.5")
    }));
    assert!(session.attachments().iter().any(|(kind, id, value)| {
        kind == "light" && id == "rosegold:rose_block" && value == "12"
    }));
    assert!(session.attachments().iter().any(|(kind, id, value)| {
        kind == "places" && id == "rosegold:rose" && value == "rosegold:rose_block"
    }));
    assert!(session.attachments().iter().any(|(kind, id, value)| {
        kind == "stack" && id == "rosegold:rose" && value == "16"
    }));
    assert!(session
        .attachments()
        .iter()
        .any(|(kind, id, value)| kind == "recipe"
            && id == "rosegold:rose_block"
            && value == "rosegold:rose"));
    assert!(session.attachments().iter().any(|(kind, id, value)| {
        kind == "shapeless" && id == "rosegold:bouquet" && value == "rosegold:rose+rosegold:rose"
    }));
    assert!(session.attachments().iter().any(|(kind, id, value)| {
        kind == "shaped"
            && id == "rosegold:rose_post"
            && value == "rosegold:rose,,,rosegold:rose,,,rosegold:rose,,"
    }));
}

#[test]
fn fail_files_contain_expect() {
    let _cwd = cwd_lock();
    let root = java_root();
    let cases = [
        ("tests/fail/int_overflow_add.rg", "integer overflow"),
        ("tests/fail/int_overflow_div.rg", "integer overflow"),
        ("tests/fail/div_zero.rg", "division by zero"),
        ("tests/fail/float_div_zero.rg", "division by zero"),
        ("tests/fail/array_pop_empty.rg", "pop from empty array"),
    ];
    for (rel, expect) in cases {
        let path = root.join(rel);
        let arg = path.display().to_string();
        let result = run_file(&path, &[arg]);
        let succeeded = result.ok && result.exit_code == 0;
        let blob = format!("{}\n{}", result.message, result.out);
        assert!(!succeeded, "{rel} succeeded:\n{blob}");
        assert!(
            blob.contains(expect),
            "{rel} missing {expect:?} in message or stdout:\n{blob}"
        );
    }
}

fn assert_out(rel: &str, argv: &[String], expected: &str) {
    let path = java_root().join(rel);
    let result = run_file(&path, argv);
    assert!(
        result.ok && result.exit_code == 0,
        "{rel}\nmessage: {}\nstdout:\n{}",
        result.message,
        result.out
    );
    assert_eq!(result.out, expected, "{rel} stdout mismatch");
}

#[test]
fn evaltest_stdout() {
    let _cwd = cwd_lock();
    let hello = run_source(
        "fn main(): Int {\n    print(\"hello from RoseGold\");\n    return 0;\n}\n",
        "hello.rg",
        &["hello.rg".into()],
    );
    assert!(hello.ok, "{}", hello.message);
    assert_eq!(hello.out, "hello from RoseGold\n");
    assert_eq!(hello.exit_code, 0);

    let exit = run_source(
        "fn main(): Int {\n    return 7;\n}\n",
        "exit.rg",
        &["exit.rg".into()],
    );
    assert!(exit.ok, "{}", exit.message);
    assert_eq!(exit.exit_code, 7);

    let lambda = run_source(
        "fn main(): Int {\n    print((fn (x: Int): Int { return x * 2; })(4));\n    var n = 1;\n    var add = fn (x: Int): Int { return n + x; };\n    print(add(2));\n    n = 9;\n    print(add(2));\n    return 0;\n}\n",
        "lambda.rg",
        &["lambda.rg".into()],
    );
    assert!(lambda.ok, "{}", lambda.message);
    assert_eq!(lambda.out, "8\n3\n3\n");

    let file = |rel: &str| {
        let path = java_root().join(rel);
        vec![path.display().to_string()]
    };
    assert_out(
        "examples/hello.rg",
        &file("examples/hello.rg"),
        "hello from RoseGold\n",
    );
    assert_out(
        "examples/control.rg",
        &file("examples/control.rg"),
        "1\n-1\n0\n6\n6\n0\n1\n2\n1\n2\n",
    );
    assert_out(
        "examples/class.rg",
        &file("examples/class.rg"),
        "woof\n1\nmeow\n",
    );
    assert_out("examples/type.rg", &file("examples/type.rg"), "two\n");
    assert_out(
        "examples/point.rg",
        &file("examples/point.rg"),
        "3\n0\n25\neq\n",
    );
    assert_out(
        "examples/collections.rg",
        &file("examples/collections.rg"),
        "1\n4\n4\n12\ntrue\n3\n",
    );
    assert_out(
        "examples/signals.rg",
        &file("examples/signals.rg"),
        "5\n3\n",
    );
    assert_out(
        "examples/modules.rg",
        &file("examples/modules.rg"),
        "5\n8\n",
    );
    assert_out(
        "examples/argv.rg",
        &["examples/argv.rg".into(), "hello".into()],
        "2\nexamples/argv.rg\nhello\n",
    );
    assert_out(
        "examples/enum.rg",
        &file("examples/enum.rg"),
        "green\ntwo\n2\n3\n",
    );
    assert_out("examples/try.rg", &file("examples/try.rg"), "nope\n7\n");
    assert_out("examples/ufcs.rg", &file("examples/ufcs.rg"), "6\n6\n");
    assert_out(
        "examples/array.rg",
        &file("examples/array.rg"),
        "4\n1.5\n3\n1\n4\n4\nh\n12\ntrue\n3\n",
    );
    assert_out(
        "examples/import.rg",
        &file("examples/import.rg"),
        "5\n8\n5\n",
    );
    assert_out(
        "examples/generics.rg",
        &file("examples/generics.rg"),
        "3\nhi\n9\nok\n1\nok\n4\n3\n",
    );
    assert_out(
        "examples/trait_objects.rg",
        &file("examples/trait_objects.rg"),
        "ada\nr2\na\nb\n",
    );
    assert_out(
        "examples/stdlib.rg",
        &file("examples/stdlib.rg"),
        "7\n3\nHI\na\ntrue\ntrue\nREADME\n{\"ok\":true}\ntrue\n00000000-0000-0000-0000-000000000000\n5\n",
    );
    assert_out(
        "examples/async.rg",
        &file("examples/async.rg"),
        "42\n42\n42\n42\ncancelled\nnope\n",
    );
}

#[test]
fn stdlib_pass_files() {
    let _cwd = cwd_lock();
    let root = java_root().join("tests/pass");
    let mut files = Vec::new();
    walk_rg(&root, &mut files);
    let mut ran = 0usize;
    let mut failures = Vec::new();
    for path in files {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.starts_with("stdlib_") {
            continue;
        }
        ran += 1;
        let arg = path.display().to_string();
        let result = run_file(&path, &[arg]);
        if !result.ok || result.exit_code != 0 {
            failures.push(show(&path, &result));
        }
    }
    assert!(ran >= 8, "expected the stdlib pass files, ran {ran}");
    assert!(
        failures.is_empty(),
        "{} stdlib file(s) failed:\n{}",
        failures.len(),
        failures.join("\n---\n")
    );
}

#[test]
fn examples_with_main_exit_zero() {
    let _cwd = cwd_lock();
    let root = java_root().join("examples");
    let mut files = Vec::new();
    walk_rg(&root, &mut files);
    assert!(!files.is_empty(), "no examples under {}", root.display());
    let mut failures = Vec::new();
    let mut ran = 0usize;
    for path in files {
        let text = fs::read_to_string(&path).unwrap_or_default();
        if skip_no_main(&path, &text) {
            continue;
        }
        ran += 1;
        // These call `Window.run` and block until the window closes, same as
        // the Java Swing host. The automated suite does not sit on that loop.
        if text.contains(".run()") {
            continue;
        }
        let arg = path.display().to_string();
        let result = run_file(&path, &[arg]);
        if !result.ok || result.exit_code != 0 {
            failures.push(show(&path, &result));
        }
    }
    assert!(ran > 0, "every example was skipped");
    assert!(
        failures.is_empty(),
        "{} example(s) failed:\n{}",
        failures.len(),
        failures.join("\n---\n")
    );
}

#[test]
fn visible_window_opens_and_closes() {
    let _cwd = cwd_lock();
    let result = run_source(
        r#"
import ui;
fn main(): Int {
    var w = try ui.open("rg-visible", 120, 80);
    checks.that(w.alive());
    w.close();
    checks.that(!w.alive());
    checks.eq(ui.count(), 0);
    return 0;
}
"#,
        "visible.rg",
        &["visible.rg".to_string()],
    );
    if result.message.contains("cannot create window") {
        return;
    }
    assert!(
        result.ok && result.exit_code == 0,
        "{}",
        show(Path::new("visible.rg"), &result)
    );
}
