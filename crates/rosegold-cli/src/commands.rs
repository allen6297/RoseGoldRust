use std::fs;
use std::path::{Path, PathBuf};

use rosegold_check::{check_owned, check_source};
use rosegold_eval::{dump_ir, emit_llvm, link_native, run_file, Outcome};
use rosegold_syntax::Diagnostic;

use crate::format::{self, list_rg_files, Options};
use crate::project::{self, Project};
use crate::scaffold;
use crate::suite;

pub const VERSION: &str = "1.0.0-SNAPSHOT";

pub fn execute(args: &[String], cwd: &Path, stdin: &str) -> i32 {
    if args.is_empty() {
        print_help();
        return 0;
    }
    let cmd = args[0].as_str();
    let rest = &args[1..];
    if is_command(cmd, "help") {
        print_help();
        return 0;
    }
    if is_command(cmd, "run") {
        return run_program(rest, cwd);
    }
    if is_command(cmd, "check") {
        return check_program(rest, cwd, stdin);
    }
    if is_command(cmd, "fmt") {
        return fmt_program(rest, cwd);
    }
    if is_command(cmd, "test") {
        return test_program(rest, cwd);
    }
    if is_command(cmd, "new") {
        return new_program(rest, cwd);
    }
    if cmd == "ide" {
        let mut input = std::io::Cursor::new(stdin.as_bytes());
        let mut output = std::io::stdout();
        rosegold_ide::serve(&mut input, &mut output);
        return 0;
    }
    if is_command(cmd, "ir") {
        return ir_program(rest, cwd);
    }
    if is_command(cmd, "llvm") {
        return llvm_program(rest, cwd);
    }
    if is_command(cmd, "version") {
        println!("version : {VERSION}");
        return 0;
    }
    eprintln!("Unknown command: {cmd}");
    eprintln!("Commands: check, run, test, fmt, new, ide, ir, llvm, version, help");
    2
}

fn ir_program(args: &[String], cwd: &Path) -> i32 {
    if args.len() > 1 {
        eprintln!("Usage: ir [file]");
        return 2;
    }
    let path = args.first().map(String::as_str).unwrap_or("");
    let file = match resolve_entry(path, cwd) {
        Err(err) => {
            eprintln!("{}", err.to_human());
            return 1;
        }
        Ok(file) => file,
    };
    let Some(file) = file else {
        eprintln!("Usage: ir [file]");
        return 2;
    };
    let source = match fs::read_to_string(&file) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("{err}");
            return 1;
        }
    };
    match dump_ir(&source, &file.display().to_string()) {
        Ok(text) => {
            print!("{text}");
            0
        }
        Err(message) => {
            eprintln!("{message}");
            1
        }
    }
}

fn llvm_program(args: &[String], cwd: &Path) -> i32 {
    let usage = "Usage: llvm [file] | llvm --link [file] [-o out] | llvm --run [file] [args...] | llvm --test [file] | llvm --test --run [file]";
    let mut link = false;
    let mut run = false;
    let mut test = false;
    let mut out_arg = String::new();
    let mut path = String::new();
    let mut extra = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--link" {
            link = true;
        } else if a == "--godot" {
            eprintln!("llvm --godot is not part of this toolchain");
            return 2;
        } else if a == "--run" {
            run = true;
            link = true;
        } else if a == "--test" {
            test = true;
        } else if a == "-o" {
            if i + 1 >= args.len() {
                eprintln!("{usage}");
                return 2;
            }
            i += 1;
            out_arg = args[i].clone();
        } else if path.is_empty() {
            path = a.to_string();
        } else if run {
            extra.push(a.to_string());
        } else {
            eprintln!("{usage}");
            return 2;
        }
        i += 1;
    }
    let file = match resolve_entry(&path, cwd) {
        Err(err) => {
            eprintln!("{}", err.to_human());
            return 1;
        }
        Ok(file) => file,
    };
    let Some(file) = file else {
        eprintln!("{usage}");
        return 2;
    };
    let source = match fs::read_to_string(&file) {
        Ok(source) => source,
        Err(err) => {
            eprintln!("{err}");
            return 1;
        }
    };
    let file_name = file.display().to_string();
    let (program, diags) = rosegold_syntax::parse_source(&source, &file_name);
    if !diags.is_empty() {
        for d in &diags {
            eprintln!("{}", d.to_human());
        }
        return if diags.iter().any(is_error) { 1 } else { 0 };
    }
    let (checker, diags) = check_owned(program, &source, &file_name, false);
    if diags.iter().any(is_error) {
        for d in &diags {
            eprintln!("{}", d.to_human());
        }
        return 1;
    }
    if test && !checker.program.fns.iter().any(|f| f.is_test) {
        eprintln!("no @test functions");
        return 0;
    }
    if !link {
        print!("{}", emit_llvm(&checker, test));
        return 0;
    }
    let output = if !out_arg.is_empty() {
        Some(cwd.join(&out_arg))
    } else if run {
        None
    } else {
        let stem = file.file_name().and_then(|s| s.to_str()).unwrap_or("a");
        let stem = match stem.rfind('.') {
            Some(dot) if dot > 0 => &stem[..dot],
            _ => stem,
        };
        Some(cwd.join("build").join(stem))
    };
    let mut argv = Vec::new();
    argv.push(file_name);
    argv.extend(extra);
    let result = link_native(&checker, test, output.as_deref(), run, &argv, cwd, false);
    if !run {
        if !result.ok {
            let message = result.message.trim();
            eprintln!(
                "{}",
                if message.is_empty() {
                    "link failed"
                } else {
                    message
                }
            );
            return if result.exit_code == 0 {
                1
            } else {
                result.exit_code
            };
        }
        print!("{}", result.stdout);
        return 0;
    }
    print!("{}", result.stdout);
    if !result.ok
        && !result.message.is_empty()
        && (result.stdout.is_empty() || !result.stdout.contains(&result.message))
    {
        eprintln!("{}", result.message.trim());
    }
    if !result.stdout.is_empty() && !result.stdout.ends_with('\n') && result.exit_code != 0 {
        println!();
    }
    result.exit_code
}

pub fn fmt_program(args: &[String], cwd: &Path) -> i32 {
    let mut write = false;
    let mut check_only = false;
    let mut opts = Options::default();
    let mut path = String::new();
    for a in args {
        if a == "--write" || a == "-w" {
            write = true;
        } else if a == "--check" {
            check_only = true;
        } else if a == "--compact" {
            opts.blank_between_items = false;
        } else if a == "--no-comments" {
            opts.keep_comments = false;
        } else if path.is_empty() {
            path = a.clone();
        }
    }
    let file = match resolve_entry(&path, cwd) {
        Err(err) => {
            eprintln!("{}", err.to_human());
            return 1;
        }
        Ok(file) => file,
    };
    if file.is_none() && !path.is_empty() {
        let target = project::resolve(cwd, Path::new(&path));
        if target.is_dir() {
            return fmt_directory(&target, &opts, write, check_only);
        }
    }
    let Some(file) = file else {
        if path.is_empty() {
            eprintln!("Usage: fmt [--write|-w] [--check] [--compact] [--no-comments] [file|dir]");
        } else {
            eprintln!("cannot open {path}");
        }
        return 2;
    };
    fmt_file(&file, &opts, write, check_only)
}

fn fmt_directory(dir: &Path, opts: &Options, write: bool, check_only: bool) -> i32 {
    let files = list_rg_files(dir);
    let mut code = 0;
    let mutate = !check_only;
    for file in files {
        let next = fmt_file(&file, opts, mutate || write, check_only);
        if next > code {
            code = next;
        }
    }
    code
}

fn fmt_file(file: &Path, opts: &Options, write: bool, check_only: bool) -> i32 {
    let original = match fs::read_to_string(file) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("cannot open {}", file.display());
            return 2;
        }
    };
    let fmt = format::format_source(&original, &file.display().to_string(), opts);
    if !fmt.ok {
        eprintln!("{}: {}", file.display(), fmt.message);
        return if fmt.exit_code != 0 { fmt.exit_code } else { 1 };
    }
    if check_only {
        if fmt.out != original {
            eprintln!("{} needs formatting", file.display());
            return 1;
        }
        return 0;
    }
    if write {
        if fmt.out != original && fs::write(file, &fmt.out).is_err() {
            eprintln!("cannot write {}", file.display());
            return 2;
        }
        return 0;
    }
    print!("{}", fmt.out);
    0
}

fn run_program(args: &[String], cwd: &Path) -> i32 {
    let path = if args.is_empty() {
        ""
    } else {
        args[0].as_str()
    };
    let file = match resolve_entry(path, cwd) {
        Err(err) => {
            eprintln!("{}", err.to_human());
            return 1;
        }
        Ok(Some(file)) => file,
        Ok(None) => {
            eprintln!("Usage: run [file] [args...]");
            return 2;
        }
    };
    let argv = if !args.is_empty() && is_project_name(&args[0]) {
        let mut argv = vec![file.display().to_string()];
        argv.extend(args.iter().skip(1).cloned());
        argv
    } else if args.is_empty() {
        vec![file.display().to_string()]
    } else {
        args.to_vec()
    };
    let result = run_file(&file, &argv);
    print_run(&result);
    exit_of(&result)
}

fn check_program(args: &[String], cwd: &Path, stdin: &str) -> i32 {
    let mut json = false;
    let mut from_stdin = false;
    let mut path = String::new();
    for a in args {
        if a == "--json" || a == "-j" {
            json = true;
        } else if a == "--stdin" {
            from_stdin = true;
        } else if path.is_empty() {
            path = a.clone();
        }
    }
    let file = if from_stdin {
        None
    } else {
        match resolve_entry(&path, cwd) {
            Err(err) => {
                if json {
                    print!("{}", diagnostics_json(&[err]));
                } else {
                    eprintln!("{}", err.to_human());
                }
                return 1;
            }
            Ok(file) => file,
        }
    };
    if !from_stdin && file.is_none() && path.is_empty() {
        eprintln!("Usage: check [--json] [--stdin] [file|dir]");
        return 2;
    }
    let mut dir = None;
    if !from_stdin && file.is_none() && !path.is_empty() {
        let target = project::resolve(cwd, Path::new(&path));
        if target.is_dir() {
            dir = Some(target);
        }
    }
    if !from_stdin && file.is_none() && dir.is_none() {
        eprintln!("cannot open {path}");
        return 2;
    }
    let diags = if from_stdin {
        if path.is_empty() {
            eprintln!("Usage: check [--json] [--stdin] <file>");
            return 2;
        }
        check_source(stdin, &path)
    } else if let Some(dir) = dir {
        check_directory(&dir)
    } else {
        let file = file.unwrap();
        match fs::read_to_string(&file) {
            Ok(source) => check_source(&source, &file.display().to_string()),
            Err(_) => {
                eprintln!("cannot open {}", file.display());
                return 2;
            }
        }
    };
    if json {
        print!("{}", diagnostics_json(&diags));
    } else {
        for d in &diags {
            eprintln!("{}", d.to_human());
        }
    }
    if diags.iter().any(is_error) {
        1
    } else {
        0
    }
}

fn check_directory(dir: &Path) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for file in list_rg_files(dir) {
        match fs::read_to_string(&file) {
            Ok(source) => diags.extend(check_source(&source, &file.display().to_string())),
            Err(err) => diags.push(Diagnostic::new(
                file.display().to_string(),
                1,
                1,
                "error",
                err.to_string(),
                "error",
            )),
        }
    }
    diags
}

fn test_program(args: &[String], cwd: &Path) -> i32 {
    if args.len() > 2 {
        eprintln!("Usage: test [--native] [file|dir]");
        return 2;
    }
    let mut native = false;
    let mut path_arg = String::new();
    for a in args {
        if a == "--native" {
            native = true;
        } else if path_arg.is_empty() {
            path_arg = a.clone();
        } else {
            eprintln!("Usage: test [--native] [file|dir]");
            return 2;
        }
    }
    let result = if path_arg.is_empty() {
        let toml = cwd.join("project.toml");
        if toml.is_file() {
            match project::load(&toml) {
                Ok(project) => suite::test_project(&project, native),
                Err(err) => fail_outcome(&err.to_human()),
            }
        } else {
            suite::test_language(cwd, native)
        }
    } else {
        let target = project::resolve(cwd, Path::new(&path_arg));
        if !target.exists() {
            eprintln!("cannot open {path_arg}");
            return 2;
        }
        suite::test_path(&target, native, cwd)
    };
    print_run(&result);
    exit_of(&result)
}

fn new_program(args: &[String], cwd: &Path) -> i32 {
    if args.first().map(String::as_str) == Some("module") {
        if args.len() != 2 {
            eprintln!("Usage: new module <name>");
            return 2;
        }
        return print_scaffold(scaffold::add_module(cwd, &args[1]));
    }
    if args.len() > 1 {
        eprintln!("Usage: new [dir]");
        return 2;
    }
    let dir = if args.is_empty() {
        cwd.to_path_buf()
    } else {
        project::resolve(cwd, Path::new(&args[0]))
    };
    print_scaffold(scaffold::create(&dir))
}

fn print_scaffold(result: scaffold::ScaffoldResult) -> i32 {
    if !result.message.is_empty() {
        if result.ok {
            println!("{}", result.message);
        } else {
            eprintln!("{}", result.message);
        }
    }
    result.exit_code
}

fn resolve_entry(path: &str, cwd: &Path) -> Result<Option<PathBuf>, Diagnostic> {
    if !path.is_empty() {
        let file = project::resolve(cwd, Path::new(path));
        if file.is_file() && project::is_project_file(&file) {
            let project = project::load(&file)?;
            if !project.entry_file.is_file() {
                return Err(missing_entry(&project));
            }
            return Ok(Some(project.entry_file));
        }
        if file.is_file() {
            return Ok(Some(file));
        }
        return Ok(None);
    }
    let Some(toml) = project::locate(cwd) else {
        return Ok(None);
    };
    let project = project::load(&toml)?;
    if !project.entry_file.is_file() {
        return Err(missing_entry(&project));
    }
    Ok(Some(project.entry_file))
}

fn missing_entry(project: &Project) -> Diagnostic {
    Diagnostic::new(
        project.file.display().to_string(),
        1,
        1,
        "error",
        format!("entry '{}' not found", project.entry),
        "error",
    )
}

fn is_project_name(name: &str) -> bool {
    Path::new(name).file_name().and_then(|n| n.to_str()) == Some("project.toml")
}

fn print_run(result: &Outcome) {
    print!("{}", result.out);
    if result.message.is_empty() {
        return;
    }
    if !result.ok && !result.out.contains(&result.message) {
        eprintln!("{}", result.message);
    } else if result.ok && result.out.is_empty() {
        eprintln!("{}", result.message);
    }
}

fn exit_of(result: &Outcome) -> i32 {
    if !result.ok && result.exit_code == 0 {
        1
    } else {
        result.exit_code
    }
}

fn fail_outcome(message: &str) -> Outcome {
    Outcome {
        ok: false,
        out: String::new(),
        message: message.to_string(),
        exit_code: 1,
    }
}

fn is_error(d: &Diagnostic) -> bool {
    let s = d.severity.to_ascii_lowercase();
    !matches!(
        s.as_str(),
        "warning" | "warn" | "info" | "information" | "hint"
    )
}

fn diagnostics_json(diags: &[Diagnostic]) -> String {
    if diags.is_empty() {
        return "[]\n".to_string();
    }
    let mut ss = String::from("[\n");
    for (i, d) in diags.iter().enumerate() {
        if i > 0 {
            ss.push_str(",\n");
        }
        ss.push_str("  {\n");
        ss.push_str("    \"file\": \"");
        ss.push_str(&json_escape(&d.file));
        ss.push_str("\",\n");
        ss.push_str("    \"line\": ");
        ss.push_str(&d.line.to_string());
        ss.push_str(",\n");
        ss.push_str("    \"col\": ");
        ss.push_str(&d.col.to_string());
        ss.push_str(",\n");
        ss.push_str("    \"severity\": \"");
        ss.push_str(&json_escape(&d.severity));
        ss.push_str("\",\n");
        ss.push_str("    \"message\": \"");
        ss.push_str(&json_escape(&d.message));
        ss.push_str("\"\n");
        ss.push_str("  }");
    }
    ss.push_str("\n]\n");
    ss
}

fn json_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = std::fmt::Write::write_fmt(&mut out, format_args!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

fn print_help() {
    println!("Commands:");
    print_aliases("help");
    print_aliases("run");
    println!("      run [file] [args...]");
    print_aliases("check");
    println!("      check [--json] [--stdin] [file|dir]");
    print_aliases("fmt");
    println!("      fmt [--write|-w] [--check] [--compact] [--no-comments] [file|dir]");
    print_aliases("test");
    println!("      test [file|dir]");
    print_aliases("new");
    println!("      new [dir]");
    println!("      new module <name>");
    println!("  ide");
    println!("      ide    JSON language service on stdin (one request per line)");
    print_aliases("ir");
    println!("      ir [file]");
    print_aliases("llvm");
    println!("      llvm [file]");
    println!("      llvm --link [file] [-o out]");
    println!("      llvm --run [file] [args...]");
    println!("      llvm --test [file]");
    println!("      llvm --test --run [file]");
    print_aliases("version");
    println!("      version");
}

fn print_aliases(name: &str) {
    let letter = &name[..1];
    println!("  {name} : {letter}, -{letter}, --{letter}");
}

fn is_command(actual: &str, name: &str) -> bool {
    let letter = &name[..1];
    actual == name
        || actual == letter
        || actual == format!("-{letter}")
        || actual == format!("--{letter}")
}
