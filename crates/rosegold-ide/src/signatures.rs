use rosegold_check::{crate_of, files_for_crate};
use rosegold_syntax::*;

use crate::idents::tokens_of;
use crate::pos::{self, char_len};
use crate::walk::parse_quiet;

#[derive(Clone, Debug)]
pub struct Info {
    pub name: String,
    pub qualifier: String,
    pub label: String,
    pub params: Vec<String>,
    pub active_param: i32,
    pub paren_offset: usize,
    pub param_starts: Vec<i32>,
    pub param_ends: Vec<i32>,
}

struct Site {
    name: String,
    qualifier: String,
    active_param: i32,
    paren_offset: usize,
}

#[derive(Clone, Debug, Default)]
struct Sig {
    params: Vec<String>,
    throws_ex: bool,
    return_type: String,
}

const HOST: &[&str] = &["checks", "process"];

pub fn at(source: &str, path: &str, offset: usize) -> Option<Info> {
    let offset = pos::clamp_offset(source, offset);
    let site = call_site_at(source, offset)?;
    let sig = resolve(source, path, &site);
    let params = sig.params;
    let mut active = site.active_param;
    if active < 0 {
        active = 0;
    }
    if !params.is_empty() && active as usize >= params.len() {
        active = params.len() as i32 - 1;
    }
    let mut label = String::new();
    label.push_str(&site.name);
    label.push('(');
    let mut starts = Vec::new();
    let mut ends = Vec::new();
    for (i, param) in params.iter().enumerate() {
        if i > 0 {
            label.push_str(", ");
        }
        starts.push(char_len(&label) as i32);
        label.push_str(param);
        ends.push(char_len(&label) as i32);
    }
    label.push(')');
    if sig.throws_ex {
        label.push_str(" throws");
    }
    if !sig.return_type.is_empty() {
        label.push_str(": ");
        label.push_str(&sig.return_type);
    }
    Some(Info {
        name: site.name,
        qualifier: site.qualifier,
        label,
        params,
        active_param: active,
        paren_offset: site.paren_offset,
        param_starts: starts,
        param_ends: ends,
    })
}

fn call_site_at(source: &str, offset: usize) -> Option<Site> {
    let tokens = tokens_of(source);
    let (line1, col1) = pos::line_col(source, offset);
    let mut stack: Vec<Site> = Vec::new();
    let mut pending = String::new();
    let mut pending_qual = String::new();
    for i in 0..tokens.len() {
        let t = &tokens[i];
        if t.kind == Tok::Eof {
            break;
        }
        if t.line() > line1 || (t.line() == line1 && t.col() > col1) {
            break;
        }
        if t.kind == Tok::Identifier {
            pending = t.text.clone();
            pending_qual.clear();
            if i >= 2 && tokens[i - 1].kind == Tok::Dot && tokens[i - 2].kind == Tok::Identifier {
                pending_qual = tokens[i - 2].text.clone();
            }
        } else if t.kind == Tok::LParen {
            stack.push(Site {
                name: pending.clone(),
                qualifier: pending_qual.clone(),
                active_param: 0,
                paren_offset: pos::offset(source, t.line(), t.col()),
            });
            pending.clear();
            pending_qual.clear();
        } else if t.kind == Tok::RParen {
            let closed = t.line() < line1 || (t.line() == line1 && t.col() < col1);
            if closed && !stack.is_empty() {
                stack.pop();
            }
            pending.clear();
            pending_qual.clear();
        } else if t.kind == Tok::Comma && !stack.is_empty() {
            stack.last_mut().unwrap().active_param += 1;
        } else if t.kind != Tok::LineComment && t.kind != Tok::BlockComment {
            pending.clear();
            pending_qual.clear();
        }
    }
    let site = stack.pop()?;
    if site.name.is_empty() {
        return None;
    }
    Some(site)
}

fn resolve(source: &str, path: &str, site: &Site) -> Sig {
    if let Some(builtin) = builtin(site) {
        return builtin;
    }
    let program = parse_quiet(source, path);
    if !site.qualifier.is_empty() {
        if !HOST.contains(&site.qualifier.as_str()) {
            let crate_name = crate_of(&site.qualifier, &program, path);
            let key = if crate_name.is_empty() {
                site.qualifier.as_str()
            } else {
                crate_name.as_str()
            };
            if let Some(hit) = from_crate(key, &site.name, path) {
                return hit;
            }
        }
        if let Some(method) = find_fn(&program, &site.name, true) {
            return method;
        }
    }
    if let Some(func) = find_fn(&program, &site.name, false) {
        return func;
    }
    if site.qualifier.is_empty() {
        if let Some(imported) = from_import(&program, &site.name, path) {
            return imported;
        }
    }
    Sig::default()
}

fn builtin(site: &Site) -> Option<Sig> {
    if !site.qualifier.is_empty() {
        if site.qualifier == "checks" {
            return match site.name.as_str() {
                "eq" | "neq" | "eq_string" => Some(params(&["a", "b"])),
                "that" | "truthy" => Some(params(&["cond"])),
                _ => None,
            };
        }
        if site.qualifier == "process" {
            return match site.name.as_str() {
                "argv" => Some(params(&["i"])),
                "argc" => Some(params(&[])),
                _ => None,
            };
        }
        return None;
    }
    match site.name.as_str() {
        "print" => Some(params(&["..."])),
        "len" => Some(params(&["xs"])),
        "assert" => Some(params(&["cond"])),
        "argv" => Some(params(&["i"])),
        "argv_len" => Some(params(&[])),
        _ => None,
    }
}

fn params(names: &[&str]) -> Sig {
    Sig {
        params: names.iter().map(|s| (*s).to_string()).collect(),
        throws_ex: false,
        return_type: String::new(),
    }
}

fn from_import(program: &Program, name: &str, path: &str) -> Option<Sig> {
    for im in &program.imports {
        if !im.is_from || im.path.len() < 2 {
            continue;
        }
        let crate_name = &im.path[0];
        let imported = &im.path[1];
        let local = if im.alias.is_empty() {
            imported.as_str()
        } else {
            im.alias.as_str()
        };
        if local != name {
            continue;
        }
        if let Some(hit) = from_crate(crate_name, imported, path) {
            return Some(hit);
        }
    }
    None
}

fn from_crate(crate_name: &str, name: &str, path: &str) -> Option<Sig> {
    for file in files_for_crate(crate_name, path) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let program = parse_quiet(&text, &file.display().to_string());
        if let Some(hit) = find_fn(&program, name, false) {
            return Some(hit);
        }
    }
    None
}

fn find_fn(program: &Program, name: &str, methods_only: bool) -> Option<Sig> {
    let mut hit = None;
    walk_find(program, name, methods_only, &mut hit);
    hit
}

fn walk_find(program: &Program, name: &str, methods_only: bool, hit: &mut Option<Sig>) {
    walk_items(
        &program.fns,
        &program.signals,
        &program.structs,
        &program.classes,
        &program.traits,
        &program.impls,
        name,
        methods_only,
        hit,
    );
    for m in &program.mods {
        walk_mod(m, name, methods_only, hit);
    }
}

fn walk_mod(m: &ModDecl, name: &str, methods_only: bool, hit: &mut Option<Sig>) {
    walk_items(
        &m.fns,
        &m.signals,
        &m.structs,
        &m.classes,
        &m.traits,
        &m.impls,
        name,
        methods_only,
        hit,
    );
    for nested in &m.mods {
        walk_mod(nested, name, methods_only, hit);
    }
}

fn walk_items(
    fns: &[FnDecl],
    signals: &[SignalDecl],
    structs: &[StructDecl],
    classes: &[ClassDecl],
    traits: &[TraitDecl],
    impls: &[ImplDecl],
    name: &str,
    methods_only: bool,
    hit: &mut Option<Sig>,
) {
    if !methods_only {
        for func in fns {
            if hit.is_none() && func.name == name {
                *hit = Some(from_fn(func, false));
            }
        }
    }
    for signal in signals {
        if hit.is_none() && signal.name == name {
            *hit = Some(from_signal(signal));
        }
    }
    for st in structs {
        for method in &st.methods {
            if hit.is_none() && method.name == name {
                *hit = Some(from_fn(method, true));
            }
        }
        for signal in &st.signals {
            if hit.is_none() && signal.name == name {
                *hit = Some(from_signal(signal));
            }
        }
    }
    for class in classes {
        for method in &class.methods {
            if hit.is_none() && method.name == name {
                *hit = Some(from_fn(method, true));
            }
        }
        for block in &class.trait_impls {
            for method in &block.methods {
                if hit.is_none() && method.name == name {
                    *hit = Some(from_fn(method, true));
                }
            }
        }
        for signal in &class.signals {
            if hit.is_none() && signal.name == name {
                *hit = Some(from_signal(signal));
            }
        }
    }
    for trait_decl in traits {
        for method in &trait_decl.methods {
            if hit.is_none() && method.name == name {
                *hit = Some(from_trait_method(method));
            }
        }
        for signal in &trait_decl.signals {
            if hit.is_none() && signal.name == name {
                *hit = Some(from_signal(signal));
            }
        }
    }
    for im in impls {
        for method in &im.methods {
            if hit.is_none() && method.name == name {
                *hit = Some(from_fn(method, true));
            }
        }
    }
}

fn from_trait_method(method: &TraitMethod) -> Sig {
    let mut fake = FnDecl::new(&method.name, method.span);
    fake.params = method.params.clone();
    fake.param_types = method.param_types.clone();
    fake.return_type = method.return_type.clone();
    fake.throws_ex = method.throws_ex;
    from_fn(&fake, true)
}

fn from_fn(func: &FnDecl, method: bool) -> Sig {
    let mut sig = Sig {
        throws_ex: func.throws_ex,
        return_type: func.return_type.clone(),
        params: Vec::new(),
    };
    for (i, name) in func.params.iter().enumerate() {
        if name == "self" || name == "..." {
            continue;
        }
        if method && i == 0 && name == "self" {
            continue;
        }
        let mut p = name.clone();
        if let Some(ty) = func.param_types.get(i) {
            if !ty.is_empty() {
                p.push_str(": ");
                p.push_str(ty);
            }
        }
        sig.params.push(p);
    }
    sig
}

fn from_signal(signal: &SignalDecl) -> Sig {
    let mut sig = Sig::default();
    for name in &signal.params {
        if name == "self" || name == "..." {
            continue;
        }
        sig.params.push(name.clone());
    }
    sig
}
