use rosegold_check::{
    crate_dir, crate_from_imports, crate_of, files_for_crate, from_bind_crate, from_bind_exported,
    lookup, type_head,
};
use rosegold_syntax::*;

use crate::completions;
use crate::idents::{self, Ident};
use crate::pos;
use crate::walk::{
    self, enclosing_class, enclosing_fn, guess_type, has_class_or_struct, parse_quiet, Visit,
};

const HOST: &[&str] = &["checks", "process"];
const TYPES: &[&str] = &[
    "Int", "Float", "String", "Bool", "Void", "Array", "Map", "Range", "UUID", "Vec2", "Vec3",
    "Window",
];
const BUILTINS: &[(&str, &str)] = &[
    ("print", "print(...)"),
    ("assert", "assert(cond)"),
    ("len", "len(xs) — Array, String, or Map"),
    ("argv", "argv(i) — script path at 0"),
    ("argv_len", "argv_len() — argc"),
];

#[derive(Clone, Debug)]
pub struct Info {
    pub name: String,
    pub kind: String,
    pub signature: String,
    pub doc: String,
    pub deprecated: bool,
    pub crate_name: String,
    pub start: usize,
    pub end: usize,
}

pub fn at(source: &str, path: &str, offset: usize) -> Option<Info> {
    let offset = pos::clamp_offset(source, offset);
    let ident = idents::at(source, offset)?;
    if !ident.qualifier.is_empty() && HOST.contains(&ident.qualifier.as_str()) {
        return Some(module_member(source, path, &ident));
    }
    if let Some(info) = crate_info(source, path, &ident, offset) {
        return Some(info);
    }
    if ident.qualifier.is_empty() {
        for (name, sig) in BUILTINS {
            if *name == ident.name {
                return Some(info(&ident, "function", sig, "", false, ""));
            }
        }
    }
    let program = parse_quiet(source, path);
    if let Some(info) = from_program(&program, &ident, source, offset) {
        return Some(info);
    }
    if ident.qualifier.is_empty() {
        let exported = from_bind_exported(&program, &ident.name);
        let bind_crate = from_bind_crate(&program, &ident.name);
        if !exported.is_empty() && !bind_crate.is_empty() {
            if let Some(info) = name_in_crate(&bind_crate, &exported, path, &ident) {
                return Some(info);
            }
        }
        let aliased = crate_from_imports(&program, &ident.name);
        if !aliased.is_empty() {
            if let Some(info) = crate_module(&ident, &aliased, path) {
                return Some(info);
            }
        }
        for ty in TYPES {
            if *ty == ident.name {
                return Some(info(&ident, "type", &format!("type {ty}"), "", false, ""));
            }
        }
        if ident.name == "std" {
            return Some(info(&ident, "module", "module std", "", false, "std"));
        }
        if HOST.contains(&ident.name.as_str()) {
            return Some(info(
                &ident,
                "module",
                &format!("module {}", ident.name),
                "host module",
                false,
                &ident.name,
            ));
        }
        if let Some(ex) = lookup(&ident.name, path) {
            if !ex.crate_name.is_empty() {
                if let Some(hit) = name_in_crate(&ex.crate_name, &ident.name, path, &ident) {
                    return Some(hit);
                }
            }
        }
    }
    None
}

fn module_member(source: &str, path: &str, ident: &Ident) -> Info {
    let mut signature = format!("{}.{}", ident.qualifier, ident.name);
    let mut kind = "function".to_string();
    for item in completions::suggest(source, path, ident.start) {
        if item.label == ident.name {
            if !item.detail.is_empty() {
                signature = item.detail;
            }
            kind = item.kind;
            break;
        }
    }
    info(ident, &kind, &signature, "", false, &ident.qualifier)
}

fn crate_info(source: &str, path: &str, ident: &Ident, offset: usize) -> Option<Info> {
    let prefix = idents::line_prefix(source, offset);
    let from_crate = idents::from_import_crate(&prefix);
    if !from_crate.is_empty() {
        let mut look = ident.name.clone();
        let program = parse_quiet(source, path);
        let exported = from_bind_exported(&program, &ident.name);
        if !exported.is_empty() {
            look = exported;
        }
        return name_in_crate(&from_crate, &look, path, ident);
    }
    if idents::after_import_or_from(&prefix)
        && (ident.qualifier.is_empty() || ident.qualifier == "std")
    {
        let mut name = ident.name.clone();
        let program = parse_quiet(source, path);
        let aliased = crate_from_imports(&program, &ident.name);
        if !aliased.is_empty() {
            name = aliased;
        }
        return crate_module(ident, &name, path);
    }
    if !ident.qualifier.is_empty() && !HOST.contains(&ident.qualifier.as_str()) {
        let program = parse_quiet(source, path);
        let crate_name = crate_of(&ident.qualifier, &program, path);
        let key = if crate_name.is_empty() {
            ident.qualifier.as_str()
        } else {
            crate_name.as_str()
        };
        if let Some(member) = name_in_crate(key, &ident.name, path, ident) {
            return Some(member);
        }
        if ident.qualifier == "std" || crate_name == "std" {
            return crate_module(ident, &ident.name, path);
        }
    }
    None
}

fn crate_module(ident: &Ident, name: &str, from_file: &str) -> Option<Info> {
    if HOST.contains(&name) {
        return Some(info(
            ident,
            "module",
            &format!("module {name}"),
            "host module",
            false,
            name,
        ));
    }
    let files = files_for_crate(name, from_file);
    if files.is_empty() {
        return None;
    }
    let doc = if crate_dir(name, from_file).is_some() {
        "stdlib"
    } else {
        "project module"
    };
    Some(info(
        ident,
        "module",
        &format!("module {name}"),
        doc,
        false,
        name,
    ))
}

fn name_in_crate(crate_name: &str, name: &str, from_file: &str, at: &Ident) -> Option<Info> {
    let look = Ident {
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
        if let Some(hit) = from_program(&parse_quiet(&src, &path), &look, &src, 0) {
            return Some(Info {
                name: at.name.clone(),
                kind: hit.kind,
                signature: hit.signature,
                doc: hit.doc,
                deprecated: hit.deprecated,
                crate_name: crate_name.to_string(),
                start: at.start,
                end: at.end,
            });
        }
    }
    None
}

fn from_program(program: &Program, ident: &Ident, source: &str, offset: usize) -> Option<Info> {
    let line = pos::line_col(source, offset).0;
    if !ident.qualifier.is_empty() {
        if let Some(member) = type_member(program, ident, source, line) {
            return Some(member);
        }
    }
    if ident.qualifier.is_empty() {
        if let Some(local) = local_info(program, ident, line) {
            return Some(local);
        }
    }
    struct Find<'a> {
        ident: &'a Ident,
        source: &'a str,
        hit: Option<Info>,
    }
    impl Visit for Find<'_> {
        fn on_fn(&mut self, func: &FnDecl) {
            if func.name == self.ident.name && self.hit.is_none() {
                self.hit = Some(fn_info(self.ident, func, self.source));
            }
        }
        fn on_class(&mut self, class: &ClassDecl) {
            if class.name == self.ident.name && self.hit.is_none() {
                let mut sig = String::new();
                if class.is_abstract {
                    sig.push_str("abstract ");
                }
                if class.is_final {
                    sig.push_str("final ");
                }
                sig.push_str("class ");
                sig.push_str(&class.name);
                if !class.parent.is_empty() {
                    sig.push_str(" extends ");
                    sig.push_str(&class.parent);
                }
                self.hit = Some(info(
                    self.ident,
                    "class",
                    &sig,
                    &docs_above(self.source, class.span.line),
                    false,
                    "",
                ));
            }
            for method in &class.methods {
                if method.name == self.ident.name && self.hit.is_none() {
                    self.hit = Some(fn_info(self.ident, method, self.source));
                }
            }
            for field in &class.fields {
                if field.name == self.ident.name && self.hit.is_none() {
                    let ty = if field.ty.is_empty() {
                        "field"
                    } else {
                        field.ty.as_str()
                    };
                    self.hit = Some(info(self.ident, "field", ty, "", false, ""));
                }
            }
        }
        fn on_struct(&mut self, st: &StructDecl) {
            if st.name == self.ident.name && self.hit.is_none() {
                let kind = if st.is_data { "data" } else { "struct" };
                self.hit = Some(info(
                    self.ident,
                    "struct",
                    &format!("{kind} {}", st.name),
                    &docs_above(self.source, st.span.line),
                    false,
                    "",
                ));
            }
            for method in &st.methods {
                if method.name == self.ident.name && self.hit.is_none() {
                    self.hit = Some(fn_info(self.ident, method, self.source));
                }
            }
        }
        fn on_trait(&mut self, t: &TraitDecl) {
            if t.name == self.ident.name && self.hit.is_none() {
                self.hit = Some(info(
                    self.ident,
                    "trait",
                    &format!("trait {}", t.name),
                    &docs_above(self.source, t.span.line),
                    false,
                    "",
                ));
            }
        }
        fn on_enum(&mut self, e: &EnumDecl) {
            if e.name == self.ident.name && self.hit.is_none() {
                self.hit = Some(info(
                    self.ident,
                    "enum",
                    &format!("enum {}", e.name),
                    &docs_above(self.source, e.span.line),
                    false,
                    "",
                ));
            }
        }
        fn on_signal(&mut self, s: &SignalDecl) {
            if s.name == self.ident.name && self.hit.is_none() {
                self.hit = Some(info(
                    self.ident,
                    "function",
                    &format!("signal {}", s.name),
                    &docs_above(self.source, s.span.line),
                    false,
                    "",
                ));
            }
        }
        fn on_mod(&mut self, m: &ModDecl) {
            if m.name == self.ident.name && self.hit.is_none() {
                self.hit = Some(info(
                    self.ident,
                    "module",
                    &format!("mod {}", m.name),
                    &docs_above(self.source, m.span.line),
                    false,
                    "",
                ));
            }
        }
    }
    let mut find = Find {
        ident,
        source,
        hit: None,
    };
    walk::walk_program(program, &mut find);
    find.hit
}

fn type_member(program: &Program, ident: &Ident, source: &str, line: u32) -> Option<Info> {
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
        return named_method(program, ident, source);
    }
    let head = type_head(&type_name);
    struct Find<'a> {
        head: String,
        ident: &'a Ident,
        source: &'a str,
        hit: Option<Info>,
    }
    impl Visit for Find<'_> {
        fn on_class(&mut self, class: &ClassDecl) {
            if class.name != self.head {
                return;
            }
            for method in &class.methods {
                if method.name == self.ident.name {
                    self.hit = Some(fn_info(self.ident, method, self.source));
                }
            }
            for field in &class.fields {
                if field.name == self.ident.name && self.hit.is_none() {
                    let ty = if field.ty.is_empty() {
                        "field"
                    } else {
                        field.ty.as_str()
                    };
                    self.hit = Some(info(self.ident, "field", ty, "", false, ""));
                }
            }
        }
        fn on_struct(&mut self, st: &StructDecl) {
            if st.name != self.head {
                return;
            }
            for method in &st.methods {
                if method.name == self.ident.name {
                    self.hit = Some(fn_info(self.ident, method, self.source));
                }
            }
        }
    }
    let mut find = Find {
        head,
        ident,
        source,
        hit: None,
    };
    walk::walk_program(program, &mut find);
    if find.hit.is_some() {
        find.hit
    } else {
        named_method(program, ident, source)
    }
}

fn named_method(program: &Program, ident: &Ident, source: &str) -> Option<Info> {
    struct Find<'a> {
        ident: &'a Ident,
        source: &'a str,
        hit: Option<Info>,
    }
    impl Visit for Find<'_> {
        fn on_fn(&mut self, func: &FnDecl) {
            if func.name == self.ident.name && self.hit.is_none() {
                self.hit = Some(fn_info(self.ident, func, self.source));
            }
        }
        fn on_class(&mut self, class: &ClassDecl) {
            for method in &class.methods {
                if method.name == self.ident.name && self.hit.is_none() {
                    self.hit = Some(fn_info(self.ident, method, self.source));
                }
            }
        }
        fn on_struct(&mut self, st: &StructDecl) {
            for method in &st.methods {
                if method.name == self.ident.name && self.hit.is_none() {
                    self.hit = Some(fn_info(self.ident, method, self.source));
                }
            }
        }
    }
    let mut find = Find {
        ident,
        source,
        hit: None,
    };
    walk::walk_program(program, &mut find);
    find.hit
}

fn local_info(program: &Program, ident: &Ident, line: u32) -> Option<Info> {
    let func = enclosing_fn(program, line)?;
    for (i, name) in func.params.iter().enumerate() {
        if name == &ident.name {
            let ty = func.param_types.get(i).map(|s| s.as_str()).unwrap_or("");
            let sig = if ty.is_empty() {
                format!("param {}", ident.name)
            } else {
                format!("{}: {ty}", ident.name)
            };
            return Some(info(ident, "variable", &sig, "", false, ""));
        }
    }
    local_stmts(&func.body, ident, line)
}

fn local_stmts(stmts: &[Stmt], ident: &Ident, line: u32) -> Option<Info> {
    let mut found = None;
    for s in stmts {
        if s.span.line > line {
            continue;
        }
        if matches!(s.kind, StmtKind::Var | StmtKind::Const | StmtKind::For) && s.name == ident.name
        {
            let kind = if s.kind == StmtKind::Const {
                "const"
            } else {
                "var"
            };
            let mut sig = format!("{kind} {}", s.name);
            if !s.type_name.is_empty() {
                sig.push_str(": ");
                sig.push_str(&s.type_name);
            }
            found = Some(info(ident, "variable", &sig, "", false, ""));
        }
        if let Some(inner) = local_stmts(&s.body, ident, line) {
            found = Some(inner);
        }
        if let Some(inner) = local_stmts(&s.else_body, ident, line) {
            found = Some(inner);
        }
        for arm in &s.arms {
            if let Some(inner) = local_stmts(&arm.body, ident, line) {
                found = Some(inner);
            }
        }
    }
    found
}

fn fn_info(ident: &Ident, func: &FnDecl, source: &str) -> Info {
    let mut sig = format!("fn {}(", func.name);
    let mut first = true;
    for (i, name) in func.params.iter().enumerate() {
        if name == "self" {
            continue;
        }
        if !first {
            sig.push_str(", ");
        }
        first = false;
        sig.push_str(name);
        if let Some(ty) = func.param_types.get(i) {
            if !ty.is_empty() {
                sig.push_str(": ");
                sig.push_str(ty);
            }
        }
    }
    sig.push(')');
    if !func.return_type.is_empty() {
        sig.push_str(": ");
        sig.push_str(&func.return_type);
    }
    if func.throws_ex && !sig.contains("throws") {
        sig.push_str(" throws");
    }
    info(
        ident,
        "function",
        &sig,
        &docs_above(source, func.span.line),
        func.is_deprecated,
        "",
    )
}

fn info(
    ident: &Ident,
    kind: &str,
    signature: &str,
    doc: &str,
    deprecated: bool,
    crate_name: &str,
) -> Info {
    Info {
        name: ident.name.clone(),
        kind: kind.to_string(),
        signature: signature.to_string(),
        doc: doc.to_string(),
        deprecated,
        crate_name: crate_name.to_string(),
        start: ident.start,
        end: ident.end,
    }
}

pub fn docs_above(source: &str, line1: u32) -> String {
    let tokens = idents::tokens_of(source);
    let mut i = 0usize;
    while i < tokens.len() && tokens[i].kind != Tok::Eof {
        let t = &tokens[i];
        if t.line() >= line1 && t.kind != Tok::LineComment && t.kind != Tok::BlockComment {
            break;
        }
        i += 1;
    }
    let mut docs = Vec::new();
    let mut j = i as isize - 1;
    while j >= 0 {
        let t = &tokens[j as usize];
        if t.kind == Tok::LineComment || t.kind == Tok::BlockComment {
            let text = strip_comment(t);
            if !text.is_empty() {
                docs.insert(0, text);
            }
            j -= 1;
            continue;
        }
        if t.kind == Tok::Identifier && j > 0 && tokens[j as usize - 1].kind == Tok::At {
            j -= 2;
            continue;
        }
        if t.kind == Tok::At {
            j -= 1;
            continue;
        }
        break;
    }
    docs.join("\n")
}

fn strip_comment(t: &Token) -> String {
    let mut s = t.text.clone();
    if t.kind == Tok::BlockComment {
        if let Some(rest) = s.strip_prefix("/#") {
            s = rest.to_string();
        }
        if let Some(rest) = s.strip_suffix("#/") {
            s = rest.to_string();
        }
        return trim_comment_block(&s);
    }
    if let Some(rest) = s.strip_prefix("///") {
        s = rest.to_string();
    } else if let Some(rest) = s.strip_prefix("//") {
        s = rest.to_string();
    } else if let Some(rest) = s.strip_prefix("##") {
        s = rest.to_string();
    } else if let Some(rest) = s.strip_prefix('#') {
        s = rest.to_string();
    }
    java_trim(&s).to_string()
}

fn trim_comment_block(s: &str) -> String {
    let mut kept = Vec::new();
    for line in s.split('\n') {
        let trimmed = java_trim(line);
        if trimmed.is_empty() && kept.is_empty() {
            continue;
        }
        kept.push(trimmed.to_string());
    }
    while kept.last().map(|s| s.is_empty()).unwrap_or(false) {
        kept.pop();
    }
    kept.join("\n")
}

fn java_trim(s: &str) -> &str {
    let bytes = s.as_bytes();
    let mut i = 0usize;
    let mut j = bytes.len();
    while i < j && bytes[i] <= b' ' {
        i += 1;
    }
    while j > i && bytes[j - 1] <= b' ' {
        j -= 1;
    }
    &s[i..j]
}
