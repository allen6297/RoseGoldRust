//! Static fail suite. Java sources live beside this workspace:
//! `CARGO_MANIFEST_DIR/../../../RoseGoldJava/tests/fail`.
//!
//! Empty expects are fixtures and are not asserted.
//! Runtime expects are produced by the interpreter, not the checker, so a
//! matching diagnostic is a failure (the checker must not invent them).
//! Every other expect must appear in some error diagnostic.

use std::fs;
use std::path::{Path, PathBuf};

fn java_fail_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../RoseGoldJava/tests/fail")
}

fn read_expect(source: &str) -> String {
    for raw in source.split('\n') {
        let line = raw.trim();
        for prefix in ["# expect:", "// expect:", "/// expect:"] {
            if let Some(rest) = line.strip_prefix(prefix) {
                return rest.trim().to_string();
            }
        }
    }
    String::new()
}

/// Interpreter / JSON runtime messages. Checker must not emit these.
fn runtime_only(expect: &str) -> bool {
    const PHRASES: &[&str] = &[
        "path outside sandbox",
        "str.repeat result too large",
        "time.sleep duration too large",
        "integer overflow",
        "range too large",
        "invalid regex",
        "call stack overflow",
        "signal emit nested too deeply",
        "uncaught throw",
        "invalid JSON",
        "json null",
        "division by zero",
        "cannot iterate",
        "key 'missing' not found",
        "pop from empty array",
        "index 3 out of bounds",
        "modulo by zero",
        "argv index out of range",
        "expected 0 parameter(s)",
    ];
    PHRASES.iter().any(|p| expect.contains(p))
}

/// Line and column of the diagnostic the Java checker records.
/// `import_cycle` is the re-entrant import in cycle_b/lib.rg.
/// `import_dup` is the second `add` in nums_b.rg.
/// `type_add` is omitted: Int+String is String (`joinsNumber`), so there is no diagnostic.
/// `type_multi`'s comparison is pinned in the test body (line 4, column 16, the `<`).
fn java_span(file_name: &str) -> Option<(u32, u32, Option<&'static str>)> {
    let row = match file_name {
        "abstract_construct.rg" => (7, 13, None),
        "abstract_data.rg" => (2, 10, None),
        "abstract_final_class.rg" => (2, 16, None),
        "abstract_fn.rg" => (2, 10, None),
        "abstract_missing.rg" => (6, 1, None),
        "abstract_need_class.rg" => (3, 1, None),
        "abstract_struct.rg" => (2, 10, None),
        "abstract_super.rg" => (8, 22, None),
        "array_bare_arg.rg" => (8, 12, None),
        "array_bare_smuggle.rg" => (5, 9, None),
        "array_elem.rg" => (4, 8, None),
        "array_index.rg" => (4, 11, None),
        "await_not_future.rg" => (3, 5, None),
        "await_outside_async.rg" => (5, 5, None),
        "break_outside.rg" => (3, 5, None),
        "class_cycle.rg" => (2, 1, None),
        "class_missing_field.rg" => (7, 13, None),
        "class_unknown_parent.rg" => (2, 1, None),
        "compare_type.rg" => (3, 13, None),
        "compound_type.rg" => (4, 5, None),
        "connect_not_fn.rg" => (5, 10, None),
        "constexpr_call.rg" => (8, 12, None),
        "constexpr_lambda.rg" => (4, 15, None),
        "constexpr_multi.rg" => (4, 9, None),
        "constexpr_print.rg" => (4, 5, None),
        "constexpr_var.rg" => (4, 9, None),
        "continue_outside.rg" => (3, 5, None),
        "crate_math.rg" => (3, 12, None),
        "crate_std.rg" => (2, 1, None),
        "crate_uuid.rg" => (2, 1, None),
        "data_assign.rg" => (9, 7, None),
        "data_extends.rg" => (6, 1, None),
        "data_method_mut.rg" => (6, 9, None),
        "data_signal.rg" => (3, 5, None),
        "duplicate_field.rg" => (5, 1, None),
        "emit_arity.rg" => (10, 10, None),
        "emit_deferred_arity.rg" => (10, 10, None),
        "enum_arity.rg" => (7, 19, None),
        "enum_unknown_variant.rg" => (7, 19, None),
        "final_extends.rg" => (6, 1, None),
        "final_override.rg" => (9, 1, None),
        "fn_type_arity.rg" => (3, 12, None),
        "from_dup.rg" => (3, 1, None),
        "from_private.rg" => (2, 1, None),
        "future_all_not_future.rg" => (3, 18, None),
        "future_bad_method.rg" => (6, 7, None),
        "future_unused.rg" => (5, 10, None),
        "generic_arg.rg" => (7, 5, None),
        "generic_arity.rg" => (7, 5, None),
        "generic_bound.rg" => (11, 11, None),
        "generic_extends_field.rg" => (10, 5, None),
        "generic_field.rg" => (7, 13, None),
        "generic_lambda_arg.rg" => (3, 12, None),
        "generic_method_arg.rg" => (11, 18, None),
        "generic_trait_arity.rg" => (6, 1, None),
        "generic_trait_bound.rg" => (11, 11, None),
        "generic_type_arity.rg" => (7, 9, None),
        "import_cycle.rg" => (1, 1, Some("cycle_b/lib.rg")),
        "import_dotted_private.rg" => (2, 1, None),
        "import_dup.rg" => (2, 1, Some("nums_b.rg")),
        "import_in_mod_missing.rg" => (3, 5, None),
        "import_private.rg" => (4, 18, None),
        "instance_emit_arity.rg" => (13, 15, None),
        "int_literal_large.rg" => (3, 11, None),
        "io_try.rg" => (5, 8, None),
        "json_try.rg" => (5, 10, None),
        "lambda_not_fn.rg" => (4, 5, None),
        "load_multi.rg" => (5, 1, None),
        "map_key.rg" => (4, 11, None),
        "match_missing.rg" => (10, 5, None),
        "match_multi.rg" => (14, 5, None),
        "match_payload_missing.rg" => (9, 5, None),
        "match_unknown_arm.rg" => (9, 9, None),
        "method_field.rg" => (8, 1, None),
        "missing_field.rg" => (8, 13, None),
        "missing_mod.rg" => (2, 1, None),
        "no_method.rg" => (8, 7, None),
        "optional_fn.rg" => (7, 1, None),
        "optional_method.rg" => (5, 5, None),
        "parse_expr.rg" => (3, 13, None),
        "parse_multi.rg" => (3, 13, None),
        "private_nested_fn.rg" => (14, 24, None),
        "private_struct.rg" => (4, 13, None),
        "range_end.rg" => (3, 17, None),
        "range_start.rg" => (3, 14, None),
        "std_missing.rg" => (2, 1, None),
        "super_try.rg" => (11, 15, None),
        "throw_outside.rg" => (3, 5, None),
        "throws_return.rg" => (7, 5, None),
        "trait_missing.rg" => (3, 1, None),
        "trait_object_array.rg" => (15, 9, None),
        "trait_object_bound.rg" => (11, 11, None),
        "trait_object_construct.rg" => (7, 13, None),
        "trait_object_field.rg" => (16, 13, None),
        "trait_object_method.rg" => (20, 13, None),
        "try_need.rg" => (7, 5, None),
        "type_arity.rg" => (7, 12, None),
        "type_assign.rg" => (4, 5, None),
        "type_return.rg" => (3, 5, None),
        "ufcs_arity.rg" => (8, 14, None),
        "ufcs_method.rg" => (8, 5, None),
        "ufcs_type.rg" => (8, 16, None),
        "ui_try.rg" => (5, 8, None),
        "undefined_var.rg" => (3, 11, None),
        "unexpected_char.rg" => (3, 11, None),
        "unknown_field.rg" => (7, 13, None),
        "unknown_type_name.rg" => (3, 9, None),
        "uuid_try.rg" => (5, 9, None),
        "vis_child_private.rg" => (10, 16, None),
        "vis_private_field.rg" => (8, 14, None),
        "vis_private_method.rg" => (10, 14, None),
        "vis_protected_field.rg" => (8, 14, None),
        "vis_protected_fn.rg" => (2, 11, None),
        "vis_protected_method.rg" => (10, 14, None),
        "vis_struct_protected.rg" => (3, 15, None),
        "vis_super_private.rg" => (10, 22, None),
        _ => return None,
    };
    Some(row)
}

/// Expect comments that the current Java checker does not emit.
/// The replacement is the diagnostic text TypeChecker or Interp actually produces.
fn java_instead(file_name: &str, expect: &str) -> Option<&'static str> {
    match (file_name, expect) {
        // joinsNumber: Int + String is String, then += checks assignment.
        ("compound_type.rg", "cannot add") => Some("cannot assign String to Int"),
        // method-form minArity skips the receiver, so clamp(n, lo, hi) expects 2 args.
        ("ufcs_arity.rg", "expected 3 args") => Some("expected 2 args"),
        _ => None,
    }
}

/// Files whose expect is produced by Interp, not Checker/TypeChecker.
fn interp_only_file(file_name: &str) -> bool {
    matches!(
        file_name,
        // local const assignment is Interp.execStmt
        "const_assign.rg" | "compound_const.rg"
        // private child modules fail in Interp.modulePath, and TypeChecker
        // returns early from member walks when the parent is a module bind
        | "import_private_nested.rg"
        | "private_nested.rg"
    )
}

fn walk_rg(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk_rg(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rg") {
            out.push(path);
        }
    }
}

fn diag_file_matches(got: &str, suffix: &str) -> bool {
    let norm = got.replace('\\', "/");
    norm.ends_with(suffix) || norm.ends_with(&suffix.replace('/', "\\"))
}

#[test]
fn static_fail_suite() {
    let root = java_fail_root();
    assert!(
        root.is_dir(),
        "missing Java fail suite at {}",
        root.display()
    );
    let mut files = Vec::new();
    walk_rg(&root, &mut files);
    assert!(!files.is_empty(), "no .rg files under {}", root.display());

    let mut failures = Vec::new();
    let mut checked = 0u32;
    for path in &files {
        let source = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                failures.push(format!("{}: read error {err}", path.display()));
                continue;
            }
        };
        let expect = read_expect(&source);
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let path_str = path.to_string_lossy().to_string();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rosegold_check::check_source(&source, &path_str)
        }));
        let diags = match caught {
            Ok(d) => d,
            Err(_) => {
                failures.push(format!("{name}: check_source panicked"));
                continue;
            }
        };
        if expect.is_empty() {
            continue;
        }
        checked += 1;
        let errors: Vec<_> = diags.iter().filter(|d| d.severity == "error").collect();
        if name == "type_add.rg" || name == "type_multi.rg" {
            // Current TypeChecker joins a number and a string with +. type_add then
            // has no error. type_multi still reports the comparison on the next line.
            if errors.iter().any(|d| d.message.contains("cannot add")) {
                failures.push(format!(
                    "{name}: checker rejected number+string, but TypeChecker.joinsNumber accepts it\n{}",
                    fmt_diags(&diags)
                ));
            }
            if name == "type_multi.rg"
                && !errors
                    .iter()
                    .any(|d| d.message.contains("cannot compare") && d.line == 4 && d.col == 16)
            {
                failures.push(format!(
                    "{name}: missing comparison error at 4:16\n{}",
                    fmt_diags(&diags)
                ));
            }
            continue;
        }
        if interp_only_file(&name) || runtime_only(&expect) {
            if errors.iter().any(|d| d.message.contains(&expect)) {
                failures.push(format!(
                    "{name}: checker invented runtime diagnostic {expect:?}\n{}",
                    fmt_diags(&diags)
                ));
            }
            continue;
        }
        let needle = java_instead(&name, &expect)
            .unwrap_or(expect.as_str())
            .to_string();
        let hits: Vec<_> = errors
            .iter()
            .filter(|d| d.message.contains(&needle))
            .collect();
        if hits.is_empty() {
            failures.push(format!(
                "{name}: no error contains {needle:?} (file expect {expect:?})\n{}",
                fmt_diags(&diags)
            ));
            continue;
        }
        let Some((line, col, file_suffix)) = java_span(&name) else {
            failures.push(format!("{name}: static fail file has no pinned line/col"));
            continue;
        };
        let span_ok = hits.iter().any(|d| {
            d.line == line
                && d.col == col
                && file_suffix
                    .map(|s| diag_file_matches(&d.file, s))
                    .unwrap_or(true)
        });
        if !span_ok {
            failures.push(format!(
                "{name}: message matches {needle:?} but not line {line} col {col}{}\n{}",
                file_suffix
                    .map(|s| format!(" file *{s}"))
                    .unwrap_or_default(),
                fmt_diags(&diags)
            ));
        }
    }
    assert!(checked > 0, "no fail files with an expect");
    if !failures.is_empty() {
        panic!(
            "{} failure(s):\n\n{}",
            failures.len(),
            failures.join("\n\n")
        );
    }
}

#[test]
fn dump_static_spans() {
    if std::env::var("SPAN_DUMP").is_err() {
        return;
    }
    let root = java_fail_root();
    let mut files = Vec::new();
    walk_rg(&root, &mut files);
    for path in &files {
        let source = fs::read_to_string(path).unwrap();
        let expect = read_expect(&source);
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if expect.is_empty() || name == "type_add.rg" || name == "type_multi.rg" {
            continue;
        }
        if interp_only_file(name) || runtime_only(&expect) {
            continue;
        }
        let diags = rosegold_check::check_source(&source, &path.to_string_lossy());
        let needle = java_instead(name, &expect).unwrap_or(expect.as_str());
        let hit = diags
            .iter()
            .find(|d| d.severity == "error" && d.message.contains(needle));
        match hit {
            Some(d) => {
                let file = d.file.replace('\\', "/");
                let short = file.rsplit("tests/fail/").next().unwrap_or(&file);
                println!(
                    "SPAN\t{name}\t{}\t{}\t{short}\t{}",
                    d.line, d.col, d.message
                );
            }
            None => println!("SPAN\t{name}\tMISSING\t{needle}"),
        }
    }
}

fn fmt_diags(diags: &[rosegold_syntax::Diagnostic]) -> String {
    if diags.is_empty() {
        return "  (no diagnostics)".into();
    }
    diags
        .iter()
        .map(|d| {
            format!(
                "  {}:{}:{} [{}] {}",
                d.file, d.line, d.col, d.severity, d.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
