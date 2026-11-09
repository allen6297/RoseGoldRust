use std::fs;
use std::path::{Path, PathBuf};

use rosegold_eval::{exec_native_file, run_file, test_file, Outcome};
use rosegold_syntax::parse_source;

use crate::project::{self, Project};

pub fn test_path(path: &Path, native: bool, cwd: &Path) -> Outcome {
    if path.is_file() && project::is_project_file(path) {
        return match project::load(path) {
            Ok(project) => test_project(&project, native),
            Err(err) => fail(&err.to_human()),
        };
    }
    if path.is_dir() {
        let suite = path.join("pass").is_dir() || path.join("fail").is_dir();
        let toml = path.join("project.toml");
        if !suite && toml.is_file() {
            return match project::load(&toml) {
                Ok(project) => test_project(&project, native),
                Err(err) => fail(&err.to_human()),
            };
        }
        return test_suite(path, native);
    }
    test_maybe_native(path, native, cwd)
}

pub fn test_project(project: &Project, native: bool) -> Outcome {
    let target = project.test_target();
    if !target.exists() {
        if !project.test_entry.is_empty() {
            return fail(&format!(
                "profile.test.entry '{}' not found",
                project.test_entry
            ));
        }
        return fail(&format!(
            "no tests found (tests.rg, tests/, or @test in {})",
            project.entry
        ));
    }
    if target.is_dir() {
        test_suite(&target, native)
    } else {
        test_maybe_native(&target, native, &project.dir)
    }
}

pub fn test_language(root: &Path, native: bool) -> Outcome {
    let unit = root.join("examples").join("tests.rg");
    let suite = root.join("tests");
    let has_unit = unit.is_file();
    let has_suite = suite.is_dir();
    if !has_unit && !has_suite {
        return fail("no tests found (examples/tests.rg or tests/)");
    }
    let mut result = Outcome {
        ok: true,
        out: String::new(),
        message: String::new(),
        exit_code: 0,
    };
    if has_unit {
        let unit_result = test_maybe_native(&unit, native, root);
        result.out.push_str(&unit_result.out);
        if !unit_result.ok {
            result.ok = false;
        }
    }
    if has_suite {
        let files = test_suite(&suite, native);
        result.out.push_str(&files.out);
        if !files.ok {
            result.ok = false;
        }
    }
    result.exit_code = if result.ok { 0 } else { 1 };
    if !result.ok {
        result.message = "tests failed".to_string();
    }
    result
}

pub fn test_suite(root: &Path, native: bool) -> Outcome {
    if !root.is_dir() {
        return fail(&format!("no test directory {}", root.display()));
    }
    let mut failed = 0;
    let mut total = 0;
    let mut out = String::new();
    for path in list_top_rg(&root.join("pass")) {
        let check = check_pass(&path, native);
        if check.skipped {
            continue;
        }
        total += 1;
        out.push_str(&check.line);
        if !check.ok {
            failed += 1;
        }
    }
    for path in list_top_rg(&root.join("fail")) {
        let check = check_fail(&path, native);
        if check.skipped {
            continue;
        }
        total += 1;
        out.push_str(&check.line);
        if !check.ok {
            failed += 1;
        }
    }
    if total == 0 {
        return fail(&format!(
            "no .rg files in {}/pass or {}/fail",
            root.display(),
            root.display()
        ));
    }
    let passed = total - failed;
    let summary = format!("{passed}/{total} file tests passed");
    out.push_str(&summary);
    out.push('\n');
    Outcome {
        ok: failed == 0,
        out,
        message: summary,
        exit_code: if failed == 0 { 0 } else { 1 },
    }
}

struct PassCheck {
    ok: bool,
    skipped: bool,
    line: String,
}

fn check_pass(path: &Path, native: bool) -> PassCheck {
    let name = slash(path);
    if let Ok(source) = fs::read_to_string(path) {
        let (program, diags) = parse_source(&source, &name);
        if diags.is_empty() && !program.fns.iter().any(|f| f.name == "main") {
            return PassCheck {
                ok: false,
                skipped: true,
                line: String::new(),
            };
        }
    }
    match run_maybe_native(path, native) {
        Ok(r) => {
            if r.ok && r.exit_code == 0 {
                PassCheck {
                    ok: true,
                    skipped: false,
                    line: format!("ok   {name}\n"),
                }
            } else {
                let mut got = if r.message.is_empty() {
                    r.out
                } else {
                    r.message
                };
                if got.is_empty() {
                    got = format!("exit {}", r.exit_code);
                }
                PassCheck {
                    ok: false,
                    skipped: false,
                    line: format!("FAIL {name}: expected pass, got {got}\n"),
                }
            }
        }
        Err(err) => PassCheck {
            ok: false,
            skipped: false,
            line: format!("FAIL {name}: {err}\n"),
        },
    }
}

fn check_fail(path: &Path, native: bool) -> PassCheck {
    let name = slash(path);
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(err) => {
            return PassCheck {
                ok: false,
                skipped: false,
                line: format!("FAIL {name}: {err}\n"),
            }
        }
    };
    let expect = read_expect(&source);
    if expect.is_empty() {
        return PassCheck {
            ok: false,
            skipped: true,
            line: String::new(),
        };
    }
    match run_maybe_native(path, native) {
        Ok(r) => {
            let did_fail = !r.ok || r.exit_code != 0;
            let got = if r.message.is_empty() {
                r.out
            } else {
                r.message
            };
            if !did_fail {
                return PassCheck {
                    ok: false,
                    skipped: false,
                    line: format!("FAIL {name}: expected error, program succeeded\n"),
                };
            }
            if !got.contains(&expect) {
                return PassCheck {
                    ok: false,
                    skipped: false,
                    line: format!("FAIL {name}: expected '{expect}', got {got}\n"),
                };
            }
            PassCheck {
                ok: true,
                skipped: false,
                line: format!("ok   {name}\n"),
            }
        }
        Err(err) => PassCheck {
            ok: false,
            skipped: false,
            line: format!("FAIL {name}: {err}\n"),
        },
    }
}

fn run_maybe_native(path: &Path, native: bool) -> Result<Outcome, String> {
    if native {
        let cwd = std::env::current_dir().map_err(|err| err.to_string())?;
        let shown = path.display().to_string();
        match exec_native_file(path, &[shown], &cwd, false) {
            Ok(hit) if hit.lowered => return Ok(hit.outcome),
            Ok(_) => {}
            Err(err) => return Err(err),
        }
    }
    let shown = path.display().to_string();
    Ok(run_file(path, &[shown]))
}

fn test_maybe_native(path: &Path, native: bool, work: &Path) -> Outcome {
    if native {
        let shown = path.display().to_string();
        match exec_native_file(path, &[shown], work, true) {
            Ok(hit) if hit.lowered => return hit.outcome,
            Ok(_) => {}
            Err(err) => return fail(&err),
        }
    }
    test_file(path)
}

fn list_top_rg(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return out;
    }
    let Ok(rd) = fs::read_dir(dir) else {
        return out;
    };
    for ent in rd.flatten() {
        let path = ent.path();
        if !path.is_file() {
            continue;
        }
        let name = ent.file_name().to_string_lossy().to_string();
        if name.to_ascii_lowercase().ends_with(".rg") {
            out.push(path);
        }
    }
    out.sort_by(|a, b| slash(a).cmp(&slash(b)));
    out
}

fn read_expect(source: &str) -> String {
    for raw in source.split('\n') {
        let line = raw.trim().trim_end_matches('\r');
        for prefix in ["# expect:", "// expect:", "/// expect:"] {
            if let Some(rest) = line.strip_prefix(prefix) {
                return rest.trim().to_string();
            }
        }
    }
    String::new()
}

fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn fail(message: &str) -> Outcome {
    Outcome {
        ok: false,
        out: String::new(),
        message: message.to_string(),
        exit_code: 1,
    }
}
