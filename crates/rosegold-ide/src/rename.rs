use rosegold_check::{crate_name_of_file, from_bind_exported, same_rg_file};

use crate::idents;
use crate::usages::{self, Hit};
use crate::walk::parse_quiet;

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

pub fn is_ident(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

pub fn is_keyword(name: &str) -> bool {
    KEYWORDS.contains(&name)
}

pub fn is_valid_name(name: &str) -> bool {
    is_ident(name) && !is_keyword(name)
}

pub fn path_has_builtin(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    let n = path.replace('\\', "/");
    n.contains("/builtin/") || n.starts_with("builtin/")
}

pub fn is_stdlib_file(path: &str) -> bool {
    if path_has_builtin(path) {
        return true;
    }
    !path.is_empty() && !crate_name_of_file(path).is_empty()
}

pub fn skip_file(origin_path: &str, file_path: &str) -> bool {
    is_stdlib_file(file_path) && !is_stdlib_file(origin_path)
}

pub fn rewrite_hits(source: &str, path: &str, offset: usize) -> Vec<Hit> {
    let mut out = Vec::new();
    let alias = from_import_alias(source, path, offset);
    for hit in usages::including_crates(source, path, offset) {
        if skip_file(path, &hit.path) {
            continue;
        }
        if alias && !same_rg_file(&hit.path, path) {
            continue;
        }
        out.push(hit);
    }
    out
}

fn from_import_alias(source: &str, path: &str, offset: usize) -> bool {
    let Some(ident) = idents::at(source, offset) else {
        return false;
    };
    if ident.name.is_empty() {
        return false;
    }
    let program = parse_quiet(source, path);
    let exported = from_bind_exported(&program, &ident.name);
    !exported.is_empty() && exported != ident.name
}

pub fn apply(source: &str, hits: &[Hit], new_name: &str) -> String {
    if new_name.is_empty() || hits.is_empty() {
        return source.to_string();
    }
    let mut ordered: Vec<&Hit> = hits.iter().collect();
    ordered.sort_by(|a, b| b.start.cmp(&a.start));
    let mut chars: Vec<char> = source.chars().collect();
    for hit in ordered {
        if hit.start > hit.end || hit.end > chars.len() {
            continue;
        }
        let replacement: Vec<char> = new_name.chars().collect();
        chars.splice(hit.start..hit.end, replacement);
    }
    chars.into_iter().collect()
}
