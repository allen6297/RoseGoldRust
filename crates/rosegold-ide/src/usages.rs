use std::collections::HashSet;
use std::path::Path;

use rosegold_check::{
    crate_of, files_for_crate, from_bind_crate, from_bind_exported, generic, lookup,
    weakly_canonical,
};
use rosegold_syntax::{tokenize, Tok};

use crate::idents::{self, Ident};
use crate::pos;
use crate::walk::parse_quiet;

#[derive(Clone, Debug)]
pub struct Hit {
    pub path: String,
    pub name: String,
    pub qualifier: String,
    pub write: bool,
    pub line: u32,
    pub col: u32,
    pub start: usize,
    pub end: usize,
}

pub fn in_file(source: &str, path: &str, offset: usize) -> Vec<Hit> {
    let source = source;
    let path = path;
    let offset = pos::clamp_offset(source, offset);
    let Some(ident) = idents::at(source, offset) else {
        return Vec::new();
    };
    of_name(source, path, &ident.name, &ident.qualifier)
}

pub fn including_crates(source: &str, path: &str, offset: usize) -> Vec<Hit> {
    let offset = pos::clamp_offset(source, offset);
    let Some(ident) = ident_at(source, offset) else {
        return Vec::new();
    };
    let mut out = of_name(source, path, &ident.name, &ident.qualifier);
    let mut seen = HashSet::new();
    seen.insert(file_id(path));
    let mut crate_name = ident.name.clone();
    let program = parse_quiet(source, path);
    let exported = from_bind_exported(&program, &ident.name);
    if !exported.is_empty() {
        crate_name = exported;
    }
    for file in crate_files(source, path, &ident) {
        let id = file_id(&file.display().to_string());
        if !seen.insert(id) {
            continue;
        }
        let Ok(src) = std::fs::read_to_string(&file) else {
            continue;
        };
        let file_path = file.display().to_string();
        out.extend(of_name(&src, &file_path, &crate_name, ""));
    }
    out
}

fn crate_files(source: &str, path: &str, ident: &Ident) -> Vec<std::path::PathBuf> {
    let crate_name = crate_of_ident(source, path, ident);
    if crate_name.is_empty() {
        return Vec::new();
    }
    files_for_crate(&crate_name, path)
}

fn crate_of_ident(source: &str, path: &str, ident: &Ident) -> String {
    if !ident.qualifier.is_empty() {
        let program = parse_quiet(source, path);
        return crate_of(&ident.qualifier, &program, path);
    }
    let program = parse_quiet(source, path);
    let crate_name = from_bind_crate(&program, &ident.name);
    if !crate_name.is_empty() {
        return crate_name;
    }
    if let Some(ex) = lookup(&ident.name, path) {
        if !ex.crate_name.is_empty() {
            return ex.crate_name;
        }
    }
    String::new()
}

fn file_id(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    generic(&weakly_canonical(Path::new(path)))
}

fn ident_at(source: &str, offset: usize) -> Option<Ident> {
    let offset = pos::clamp_offset(source, offset);
    idents::at(source, offset)
}

pub fn of_name(source: &str, path: &str, name: &str, qualifier: &str) -> Vec<Hit> {
    if name.is_empty() {
        return Vec::new();
    }
    let mut junk = Vec::new();
    let tokens = tokenize(source, path, &mut junk);
    let mut out = Vec::new();
    for i in 0..tokens.len() {
        let t = &tokens[i];
        if t.kind != Tok::Identifier || t.text != name {
            continue;
        }
        let mut hit_qual = String::new();
        if i >= 2 && tokens[i - 1].kind == Tok::Dot && tokens[i - 2].kind == Tok::Identifier {
            hit_qual = tokens[i - 2].text.clone();
        }
        let mut write = i > 0 && is_decl_tok(tokens[i - 1].kind);
        if i + 1 < tokens.len() && is_assign_tok(tokens[i + 1].kind) {
            write = true;
        }
        if !qualifier.is_empty() && hit_qual != qualifier && !write {
            continue;
        }
        let start = pos::offset(source, t.line(), t.col());
        out.push(Hit {
            path: path.to_string(),
            name: name.to_string(),
            qualifier: hit_qual,
            write,
            line: t.line(),
            col: t.col(),
            start,
            end: start + t.text.chars().count(),
        });
    }
    out
}

fn is_decl_tok(k: Tok) -> bool {
    matches!(
        k,
        Tok::Function
            | Tok::Struct
            | Tok::Data
            | Tok::Class
            | Tok::Trait
            | Tok::Enum
            | Tok::Module
            | Tok::Variable
            | Tok::Constant
            | Tok::Signal
    )
}

fn is_assign_tok(k: Tok) -> bool {
    matches!(
        k,
        Tok::Eq
            | Tok::PlusEq
            | Tok::MinusEq
            | Tok::StarEq
            | Tok::SlashEq
            | Tok::PercentEq
            | Tok::AmpEq
            | Tok::PipeEq
            | Tok::CaretEq
            | Tok::ShlEq
            | Tok::ShrEq
    )
}
