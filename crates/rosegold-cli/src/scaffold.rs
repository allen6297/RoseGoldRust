use std::fs;
use std::path::{Path, PathBuf};

use crate::project::{self, Project};

pub struct ScaffoldResult {
    pub ok: bool,
    pub exit_code: i32,
    pub message: String,
}

const MODULE_LIB: &str = "\
fn hello(): Int {
    return 1;
}
";

const MAIN_RG: &str = "\
fn main(): Int {
    print(\"hello from RoseGold\");
    return 0;
}
";

const TESTS_RG: &str = "\
@test
fn ok() {
    assert(true);
}

fn main(): Int {
    return 0;
}
";

const KEYWORDS: &[&str] = &[
    "fn",
    "var",
    "const",
    "export",
    "struct",
    "data",
    "class",
    "trait",
    "extends",
    "enum",
    "mod",
    "impl",
    "pub",
    "private",
    "protected",
    "abstract",
    "final",
    "match",
    "switch",
    "for",
    "in",
    "super",
    "signal",
    "import",
    "from",
    "as",
    "is",
    "return",
    "pass",
    "break",
    "continue",
    "if",
    "elif",
    "else",
    "while",
    "self",
    "true",
    "false",
    "none",
    "try",
    "do",
    "throws",
    "throw",
    "catch",
    "async",
    "await",
    "spawn",
];

pub fn create(dir: &Path) -> ScaffoldResult {
    if dir.as_os_str().is_empty() {
        return bad(2, "missing directory");
    }
    let target = project::normalize(dir);
    if target.exists() && !target.is_dir() {
        return bad(2, format!("not a directory: {}", target.display()));
    }
    if let Err(err) = fs::create_dir_all(&target) {
        return bad(2, err.to_string());
    }
    let toml = target.join("project.toml");
    if toml.exists() {
        return bad(1, "project.toml already exists");
    }
    let name = project_name(&target);
    if let Err(err) = fs::write(&toml, project_toml(&name)) {
        return bad(2, err.to_string());
    }
    let entry = target.join("main.rg");
    if !entry.exists() {
        if let Err(err) = fs::write(&entry, MAIN_RG) {
            return bad(2, err.to_string());
        }
    }
    let tests = target.join("tests.rg");
    if !tests.exists() {
        if let Err(err) = fs::write(&tests, TESTS_RG) {
            return bad(2, err.to_string());
        }
    }
    ScaffoldResult {
        ok: true,
        exit_code: 0,
        message: "created project.toml, main.rg, tests.rg".to_string(),
    }
}

pub fn add_module(start: &Path, name: &str) -> ScaffoldResult {
    let ident = name.trim();
    if !is_valid_name(ident) {
        return bad(2, "invalid module name");
    }
    if is_crate_stdlib(ident) {
        return bad(2, format!("module name '{ident}' is reserved"));
    }
    let from = if start.as_os_str().is_empty() {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        project::normalize(start)
    };
    let Some(toml_file) = project::locate(&from) else {
        return bad(1, "no project.toml here");
    };
    let project = match Project::load_at(&toml_file) {
        Ok(p) => p,
        Err(err) => return bad(1, err.to_human()),
    };
    if project.modules.contains_key(ident) {
        return bad(1, format!("module '{ident}' already exists"));
    }
    let mod_dir = project.dir.join(ident);
    if mod_dir.exists() && !mod_dir.is_dir() {
        return bad(1, format!("not a directory: {}", mod_dir.display()));
    }
    if let Err(err) = fs::create_dir_all(&mod_dir) {
        return bad(1, err.to_string());
    }
    let lib = mod_dir.join("lib.rg");
    if !lib.exists() {
        if let Err(err) = fs::write(&lib, MODULE_LIB) {
            return bad(1, err.to_string());
        }
    }
    let source = match fs::read_to_string(&toml_file) {
        Ok(s) => s,
        Err(err) => return bad(1, err.to_string()),
    };
    if let Err(err) = fs::write(&toml_file, insert_module_line(&source, ident, ident)) {
        return bad(1, err.to_string());
    }
    ScaffoldResult {
        ok: true,
        exit_code: 0,
        message: format!("created module {ident}"),
    }
}

impl Project {
    fn load_at(path: &Path) -> Result<Project, rosegold_syntax::Diagnostic> {
        project::load(path)
    }
}

pub fn insert_module_line(source: &str, name: &str, rel: &str) -> String {
    let crlf = source.contains("\r\n");
    let norm = source.replace("\r\n", "\n");
    let mapping = format!("{name} = {}", toml_string(rel));
    let mut lines: Vec<String> = norm.split('\n').map(|s| s.to_string()).collect();
    let mut modules: i32 = -1;
    let mut next_header: i32 = -1;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t == "[modules]" {
            modules = i as i32;
        } else if modules >= 0 && next_header < 0 && t.starts_with('[') && t.ends_with(']') {
            next_header = i as i32;
            break;
        }
    }
    if modules < 0 {
        if !lines.is_empty() && !lines.last().unwrap().is_empty() {
            lines.push(String::new());
        }
        lines.push("[modules]".to_string());
        lines.push(mapping);
    } else {
        let mut insert = if next_header < 0 {
            lines.len() as i32
        } else {
            next_header
        };
        while insert > modules + 1 && lines[(insert - 1) as usize].trim().is_empty() {
            insert -= 1;
        }
        lines.insert(insert as usize, mapping);
    }
    let mut joined = lines.join("\n");
    if crlf {
        joined = joined.replace('\n', "\r\n");
    }
    joined
}

fn project_name(dir: &Path) -> String {
    let Some(name) = dir.file_name() else {
        return "app".to_string();
    };
    let s = name.to_string_lossy().trim().to_string();
    if s.is_empty() || s == "." || s == ".." {
        "app".to_string()
    } else {
        s
    }
}

fn project_toml(name: &str) -> String {
    format!(
        "[project]\nname = {}\nentry = \"main.rg\"\n\n[modules]\n\n[profile.debug]\n\n[profile.release]\n\n[profile.test]\nentry = \"tests.rg\"\n",
        toml_string(name)
    )
}

fn toml_string(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn is_valid_name(name: &str) -> bool {
    if name.is_empty() || KEYWORDS.contains(&name) {
        return false;
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

fn is_crate_stdlib(name: &str) -> bool {
    matches!(
        name,
        "std" | "math" | "str" | "io" | "vec" | "time" | "path" | "json" | "regex" | "ui"
    ) || (name.len() > 4 && name.starts_with("std."))
}

fn bad(code: i32, message: impl Into<String>) -> ScaffoldResult {
    ScaffoldResult {
        ok: false,
        exit_code: code,
        message: message.into(),
    }
}
