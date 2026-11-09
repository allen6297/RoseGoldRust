use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;
use std::time::SystemTime;

use rosegold_check::{
    crate_from_imports, crate_of, files_for_crate, from_bind_crate, from_bind_exported,
    godot_project, godot_stub_dir, java_abs, lookup, type_head,
};
use rosegold_syntax::*;

use crate::idents::{self, Ident};
use crate::pos;
use crate::walk::{
    self, enclosing_class, enclosing_fn, find_class, guess_type, has_class_or_struct, parse_quiet,
    Visit,
};

const HOST: &[&str] = &["checks", "process"];

#[derive(Clone, Debug)]
pub struct Loc {
    pub path: String,
    pub name: String,
    pub line: u32,
    pub col: u32,
    pub start: usize,
    pub end: usize,
}

struct Engine {
    path: String,
    source: String,
    program: Program,
    stamp: SystemTime,
}

static ENGINE: Mutex<Option<Engine>> = Mutex::new(None);

pub fn at(source: &str, path: &str, offset: usize) -> Option<Loc> {
    let offset = pos::clamp_offset(source, offset);
    let ident = idents::at(source, offset)?;
    if let Some(loc) = external(source, path, &ident, offset) {
        return Some(loc);
    }
    if !ident.qualifier.is_empty() && HOST.contains(&ident.qualifier.as_str()) {
        return None;
    }
    let program = parse_quiet(source, path);
    if let Some(hit) = from_program(&program, &ident, source, path, offset) {
        return Some(hit);
    }
    if let Some(engine_loc) = godot(source, path, &program, &ident, offset) {
        return Some(engine_loc);
    }
    if ident.qualifier.is_empty() {
        let exported = from_bind_exported(&program, &ident.name);
        let crate_name = from_bind_crate(&program, &ident.name);
        if !exported.is_empty() && !crate_name.is_empty() {
            if let Some(loc) = name_in_crate(&crate_name, &exported, path) {
                return Some(loc);
            }
        }
        let aliased = crate_from_imports(&program, &ident.name);
        if !aliased.is_empty() {
            return crate_root(&aliased, path);
        }
        return exported_name(&ident.name, path);
    }
    None
}

fn external(source: &str, path: &str, ident: &Ident, offset: usize) -> Option<Loc> {
    let prefix = idents::line_prefix(source, offset);
    let from_crate = idents::from_import_crate(&prefix);
    if !from_crate.is_empty() {
        let mut look = ident.name.clone();
        let exported = from_bind_exported(&parse_quiet(source, path), &ident.name);
        if !exported.is_empty() {
            look = exported;
        }
        return name_in_crate(&from_crate, &look, path);
    }
    if idents::after_import_or_from(&prefix)
        && (ident.qualifier == "std" || ident.qualifier.is_empty())
    {
        let mut name = ident.name.clone();
        let aliased = crate_from_imports(&parse_quiet(source, path), &ident.name);
        if !aliased.is_empty() {
            name = aliased;
        }
        return crate_root(&name, path);
    }
    if !ident.qualifier.is_empty() && !HOST.contains(&ident.qualifier.as_str()) {
        let crate_name = crate_of(&ident.qualifier, &parse_quiet(source, path), path);
        let key = if crate_name.is_empty() {
            ident.qualifier.as_str()
        } else {
            crate_name.as_str()
        };
        if let Some(member) = name_in_crate(key, &ident.name, path) {
            return Some(member);
        }
        if ident.qualifier == "std" || crate_name == "std" {
            return crate_root(&ident.name, path);
        }
    }
    None
}

fn exported_name(name: &str, path: &str) -> Option<Loc> {
    let ex = lookup(name, path)?;
    if ex.crate_name.is_empty() {
        return None;
    }
    name_in_crate(&ex.crate_name, name, path)
}

fn crate_root(name: &str, from_file: &str) -> Option<Loc> {
    let files = files_for_crate(name, from_file);
    if files.is_empty() {
        return None;
    }
    let lib = files
        .iter()
        .find(|file| file.file_name().and_then(|n| n.to_str()) == Some("lib.rg"))
        .cloned()
        .unwrap_or_else(|| files[0].clone());
    file_loc(&lib, name)
}

fn name_in_crate(crate_name: &str, name: &str, from_file: &str) -> Option<Loc> {
    let ident = Ident {
        name: name.to_string(),
        qualifier: String::new(),
        start: 0,
        end: 0,
    };
    for file in files_for_crate(crate_name, from_file) {
        let Ok(src) = std::fs::read_to_string(&file) else {
            continue;
        };
        let path = file.display().to_string();
        if let Some(hit) = from_program(&parse_quiet(&src, &path), &ident, &src, &path, 0) {
            return Some(hit);
        }
    }
    crate_root(crate_name, from_file)
}

fn file_loc(file: &Path, name: &str) -> Option<Loc> {
    let src = std::fs::read_to_string(file).ok()?;
    let path = java_abs(file).display().to_string();
    if let Some(t) = idents::name_token(&src, 1, name) {
        return Some(loc_at(&path, name, &src, 1, t.col()));
    }
    Some(loc_at(&path, name, &src, 1, 1))
}

fn from_program(
    program: &Program,
    ident: &Ident,
    source: &str,
    path: &str,
    offset: usize,
) -> Option<Loc> {
    let line = pos::line_col(source, offset).0;
    if !ident.qualifier.is_empty() {
        if let Some(member) = type_member(program, ident, source, path, line) {
            return Some(member);
        }
    }
    if ident.qualifier.is_empty() {
        if let Some(local) = local_loc(program, ident, source, path, line) {
            return Some(local);
        }
    }
    struct Find<'a> {
        ident: &'a Ident,
        source: &'a str,
        path: &'a str,
        hit: Option<Loc>,
    }
    impl Visit for Find<'_> {
        fn on_fn(&mut self, func: &FnDecl) {
            if func.name == self.ident.name && self.hit.is_none() {
                self.hit = Some(loc(
                    self.path,
                    &self.ident.name,
                    self.source,
                    func.span.line,
                ));
            }
        }
        fn on_class(&mut self, class: &ClassDecl) {
            if class.name == self.ident.name && self.hit.is_none() {
                self.hit = Some(loc(
                    self.path,
                    &self.ident.name,
                    self.source,
                    class.span.line,
                ));
            }
            for method in &class.methods {
                if method.name == self.ident.name && self.hit.is_none() {
                    self.hit = Some(loc(
                        self.path,
                        &self.ident.name,
                        self.source,
                        method.span.line,
                    ));
                }
            }
            for field in &class.fields {
                if field.name == self.ident.name && self.hit.is_none() {
                    self.hit = Some(loc_at(
                        self.path,
                        &self.ident.name,
                        self.source,
                        field.span.line,
                        field.span.col,
                    ));
                }
            }
        }
        fn on_struct(&mut self, st: &StructDecl) {
            if st.name == self.ident.name && self.hit.is_none() {
                self.hit = Some(loc(self.path, &self.ident.name, self.source, st.span.line));
            }
            for method in &st.methods {
                if method.name == self.ident.name && self.hit.is_none() {
                    self.hit = Some(loc(
                        self.path,
                        &self.ident.name,
                        self.source,
                        method.span.line,
                    ));
                }
            }
        }
        fn on_trait(&mut self, t: &TraitDecl) {
            if t.name == self.ident.name && self.hit.is_none() {
                self.hit = Some(loc(self.path, &self.ident.name, self.source, t.span.line));
            }
        }
        fn on_enum(&mut self, e: &EnumDecl) {
            if e.name == self.ident.name && self.hit.is_none() {
                self.hit = Some(loc(self.path, &self.ident.name, self.source, e.span.line));
            }
        }
        fn on_signal(&mut self, s: &SignalDecl) {
            if s.name == self.ident.name && self.hit.is_none() {
                self.hit = Some(loc(self.path, &self.ident.name, self.source, s.span.line));
            }
        }
        fn on_mod(&mut self, m: &ModDecl) {
            if m.name == self.ident.name && self.hit.is_none() {
                self.hit = Some(loc(self.path, &self.ident.name, self.source, m.span.line));
            }
        }
    }
    let mut find = Find {
        ident,
        source,
        path,
        hit: None,
    };
    walk::walk_program(program, &mut find);
    find.hit
}

fn type_member(
    program: &Program,
    ident: &Ident,
    source: &str,
    path: &str,
    line: u32,
) -> Option<Loc> {
    let mut type_name = ident.qualifier.clone();
    if !has_class_or_struct(program, &type_name) {
        if ident.qualifier == "self" || ident.qualifier == "super" {
            let class = enclosing_class(program, line);
            type_name = class.map(|c| c.name.clone()).unwrap_or_default();
            if ident.qualifier == "super" {
                type_name = class.map(|c| c.parent.clone()).unwrap_or_default();
            }
        } else {
            type_name = guess_type(program, &ident.qualifier, line);
        }
    }
    if type_name.is_empty() {
        return named_method(program, ident, source, path);
    }
    let head = type_head(&type_name);
    struct Find<'a> {
        head: String,
        ident: &'a Ident,
        source: &'a str,
        path: &'a str,
        hit: Option<Loc>,
    }
    impl Visit for Find<'_> {
        fn on_class(&mut self, class: &ClassDecl) {
            if class.name != self.head {
                return;
            }
            for method in &class.methods {
                if method.name == self.ident.name {
                    self.hit = Some(loc(
                        self.path,
                        &self.ident.name,
                        self.source,
                        method.span.line,
                    ));
                }
            }
            for field in &class.fields {
                if field.name == self.ident.name && self.hit.is_none() {
                    self.hit = Some(loc_at(
                        self.path,
                        &self.ident.name,
                        self.source,
                        field.span.line,
                        field.span.col,
                    ));
                }
            }
        }
        fn on_struct(&mut self, st: &StructDecl) {
            if st.name != self.head {
                return;
            }
            for method in &st.methods {
                if method.name == self.ident.name {
                    self.hit = Some(loc(
                        self.path,
                        &self.ident.name,
                        self.source,
                        method.span.line,
                    ));
                }
            }
        }
    }
    let mut find = Find {
        head,
        ident,
        source,
        path,
        hit: None,
    };
    walk::walk_program(program, &mut find);
    if find.hit.is_some() {
        find.hit
    } else {
        named_method(program, ident, source, path)
    }
}

fn named_method(program: &Program, ident: &Ident, source: &str, path: &str) -> Option<Loc> {
    struct Find<'a> {
        ident: &'a Ident,
        source: &'a str,
        path: &'a str,
        hit: Option<Loc>,
    }
    impl Visit for Find<'_> {
        fn on_fn(&mut self, func: &FnDecl) {
            if func.name == self.ident.name && self.hit.is_none() {
                self.hit = Some(loc(
                    self.path,
                    &self.ident.name,
                    self.source,
                    func.span.line,
                ));
            }
        }
        fn on_class(&mut self, class: &ClassDecl) {
            for method in &class.methods {
                if method.name == self.ident.name && self.hit.is_none() {
                    self.hit = Some(loc(
                        self.path,
                        &self.ident.name,
                        self.source,
                        method.span.line,
                    ));
                }
            }
        }
        fn on_struct(&mut self, st: &StructDecl) {
            for method in &st.methods {
                if method.name == self.ident.name && self.hit.is_none() {
                    self.hit = Some(loc(
                        self.path,
                        &self.ident.name,
                        self.source,
                        method.span.line,
                    ));
                }
            }
        }
    }
    let mut find = Find {
        ident,
        source,
        path,
        hit: None,
    };
    walk::walk_program(program, &mut find);
    find.hit
}

fn local_loc(program: &Program, ident: &Ident, source: &str, path: &str, line: u32) -> Option<Loc> {
    let func = enclosing_fn(program, line)?;
    for param in &func.params {
        if param == &ident.name && param != "self" {
            return Some(loc(path, &ident.name, source, func.span.line));
        }
    }
    local_stmts(&func.body, ident, source, path, line)
}

fn local_stmts(stmts: &[Stmt], ident: &Ident, source: &str, path: &str, line: u32) -> Option<Loc> {
    let mut found = None;
    for s in stmts {
        if s.span.line > line {
            continue;
        }
        if matches!(s.kind, StmtKind::Var | StmtKind::Const | StmtKind::For) && s.name == ident.name
        {
            found = Some(loc_at(path, &ident.name, source, s.span.line, s.span.col));
        }
        if let Some(inner) = local_stmts(&s.body, ident, source, path, line) {
            found = Some(inner);
        }
        if let Some(inner) = local_stmts(&s.else_body, ident, source, path, line) {
            found = Some(inner);
        }
        for arm in &s.arms {
            if let Some(inner) = local_stmts(&arm.body, ident, source, path, line) {
                found = Some(inner);
            }
        }
    }
    found
}

fn loc(path: &str, name: &str, source: &str, line: u32) -> Loc {
    let col = idents::name_token(source, line, name)
        .map(|t| t.col())
        .unwrap_or(1);
    loc_at(path, name, source, line, col)
}

fn loc_at(path: &str, name: &str, source: &str, line: u32, col: u32) -> Loc {
    let start = pos::offset(source, line, col);
    Loc {
        path: path.to_string(),
        name: name.to_string(),
        line,
        col,
        start,
        end: start + name.chars().count(),
    }
}

fn godot(source: &str, path: &str, user: &Program, ident: &Ident, offset: usize) -> Option<Loc> {
    with_engine(path, |eng| {
        let line = pos::line_col(source, offset).0;
        if !ident.qualifier.is_empty() {
            let type_name = receiver_type(user, eng, ident, line);
            if type_name.is_empty() {
                return None;
            }
            return member(user, source, path, eng, &type_name, &ident.name);
        }
        if let Some(here) = enclosing_class(user, line) {
            if !here.parent.is_empty() {
                if let Some(inherited) = member(user, source, path, eng, &here.parent, &ident.name)
                {
                    return Some(inherited);
                }
            }
        }
        type_decl(eng, &ident.name)
    })?
}

fn with_engine<T>(from_file: &str, f: impl FnOnce(&Engine) -> T) -> Option<T> {
    if !godot_project(from_file) {
        return None;
    }
    let dir = godot_stub_dir(from_file)?;
    let lib = java_abs(&dir.join("lib.rg"));
    if !lib.is_file() {
        return None;
    }
    let stamp = std::fs::metadata(&lib).ok()?.modified().ok()?;
    let path = lib.display().to_string();
    let mut guard = ENGINE.lock().unwrap_or_else(|e| e.into_inner());
    let fresh = match guard.as_ref() {
        Some(cached) if cached.path == path && cached.stamp == stamp => false,
        _ => true,
    };
    if fresh {
        let source = std::fs::read_to_string(&lib).ok()?;
        let program = parse_quiet(&source, &path);
        *guard = Some(Engine {
            path: path.clone(),
            source,
            program,
            stamp,
        });
    }
    Some(f(guard.as_ref().unwrap()))
}

fn receiver_type(user: &Program, eng: &Engine, ident: &Ident, line: u32) -> String {
    let qualifier = ident.qualifier.as_str();
    if qualifier == "self" || qualifier == "super" {
        let Some(class) = enclosing_class(user, line) else {
            return String::new();
        };
        if qualifier == "super" {
            return type_head(&class.parent);
        }
        return class.name.clone();
    }
    let guessed = guess_type(user, qualifier, line);
    if !guessed.is_empty() {
        return guessed;
    }
    if let Some(here) = enclosing_class(user, line) {
        let field = field_type(user, eng, &here.name, qualifier);
        if !field.is_empty() {
            return type_head(&field);
        }
    }
    if find_class(user, qualifier).is_some() || find_class(&eng.program, qualifier).is_some() {
        return qualifier.to_string();
    }
    String::new()
}

fn field_type(user: &Program, eng: &Engine, type_name: &str, field: &str) -> String {
    let mut seen = HashSet::new();
    let mut cur = type_head(type_name);
    while !cur.is_empty() && seen.insert(cur.clone()) {
        if let Some(class) = find_class(user, &cur) {
            for f in &class.fields {
                if f.name == field {
                    return f.ty.clone();
                }
            }
            cur = type_head(&class.parent);
            continue;
        }
        if let Some(class) = find_class(&eng.program, &cur) {
            for f in &class.fields {
                if f.name == field {
                    return f.ty.clone();
                }
            }
            cur = type_head(&class.parent);
            continue;
        }
        break;
    }
    String::new()
}

fn member(
    user: &Program,
    user_source: &str,
    user_path: &str,
    eng: &Engine,
    type_name: &str,
    name: &str,
) -> Option<Loc> {
    let mut seen = HashSet::new();
    let mut cur = type_head(type_name);
    while !cur.is_empty() && seen.insert(cur.clone()) {
        if let Some(class) = find_class(user, &cur) {
            if let Some(hit) = class_member(user_path, user_source, class, name) {
                return Some(hit);
            }
            cur = type_head(&class.parent);
            continue;
        }
        if let Some(class) = find_class(&eng.program, &cur) {
            if let Some(hit) = class_member(&eng.path, &eng.source, class, name) {
                return Some(hit);
            }
            cur = type_head(&class.parent);
            continue;
        }
        break;
    }
    None
}

fn class_member(path: &str, source: &str, class: &ClassDecl, name: &str) -> Option<Loc> {
    for method in &class.methods {
        if method.name == name {
            return Some(loc(path, name, source, method.span.line));
        }
    }
    for field in &class.fields {
        if field.name == name {
            return Some(loc(path, name, source, field.span.line));
        }
    }
    for signal in &class.signals {
        if signal.name == name {
            return Some(loc(path, name, source, signal.span.line));
        }
    }
    None
}

fn type_decl(eng: &Engine, name: &str) -> Option<Loc> {
    let class = find_class(&eng.program, name)?;
    Some(loc(&eng.path, name, &eng.source, class.span.line))
}
