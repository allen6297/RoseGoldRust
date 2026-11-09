use rosegold_check::type_head;
use rosegold_syntax::*;

pub fn parse_quiet(source: &str, path: &str) -> Program {
    parse_source(source, path).0
}

pub trait Visit {
    fn on_fn(&mut self, _f: &FnDecl) {}
    fn on_struct(&mut self, _s: &StructDecl) {}
    fn on_class(&mut self, _c: &ClassDecl) {}
    fn on_trait(&mut self, _t: &TraitDecl) {}
    fn on_enum(&mut self, _e: &EnumDecl) {}
    fn on_signal(&mut self, _s: &SignalDecl) {}
    fn on_mod(&mut self, _m: &ModDecl) {}
}

pub fn walk_program(program: &Program, v: &mut impl Visit) {
    walk_mod(
        &program.fns,
        &program.structs,
        &program.classes,
        &program.traits,
        &program.enums,
        &program.signals,
        &program.mods,
        v,
    );
}

fn walk_mod(
    fns: &[FnDecl],
    structs: &[StructDecl],
    classes: &[ClassDecl],
    traits: &[TraitDecl],
    enums: &[EnumDecl],
    signals: &[SignalDecl],
    mods: &[ModDecl],
    v: &mut impl Visit,
) {
    for f in fns {
        v.on_fn(f);
    }
    for s in structs {
        v.on_struct(s);
    }
    for c in classes {
        v.on_class(c);
    }
    for t in traits {
        v.on_trait(t);
    }
    for e in enums {
        v.on_enum(e);
    }
    for s in signals {
        v.on_signal(s);
    }
    for m in mods {
        v.on_mod(m);
        walk_mod(
            &m.fns, &m.structs, &m.classes, &m.traits, &m.enums, &m.signals, &m.mods, v,
        );
    }
}

fn prefer<'a>(best: Option<&'a FnDecl>, cand: &'a FnDecl, line: u32) -> Option<&'a FnDecl> {
    if cand.span.line <= line {
        match best {
            None => Some(cand),
            Some(cur) if cand.span.line >= cur.span.line => Some(cand),
            Some(cur) => Some(cur),
        }
    } else {
        best
    }
}

pub fn enclosing_fn(program: &Program, line: u32) -> Option<&FnDecl> {
    fn mods<'a>(
        list: &'a [ModDecl],
        line: u32,
        mut best: Option<&'a FnDecl>,
    ) -> Option<&'a FnDecl> {
        for m in list {
            for f in &m.fns {
                best = prefer(best, f, line);
            }
            for c in &m.classes {
                for method in &c.methods {
                    best = prefer(best, method, line);
                }
            }
            best = mods(&m.mods, line, best);
        }
        best
    }
    let mut best = None;
    for f in &program.fns {
        best = prefer(best, f, line);
    }
    for c in &program.classes {
        for method in &c.methods {
            best = prefer(best, method, line);
        }
    }
    mods(&program.mods, line, best)
}

pub fn enclosing_class(program: &Program, line: u32) -> Option<&ClassDecl> {
    let func = enclosing_fn(program, line)?;
    find_owner(program, func)
}

fn find_owner<'a>(program: &'a Program, func: &FnDecl) -> Option<&'a ClassDecl> {
    fn mods<'a>(list: &'a [ModDecl], func: &FnDecl) -> Option<&'a ClassDecl> {
        for m in list {
            for c in &m.classes {
                if c.methods.iter().any(|method| std::ptr::eq(method, func)) {
                    return Some(c);
                }
            }
            if let Some(hit) = mods(&m.mods, func) {
                return Some(hit);
            }
        }
        None
    }
    for c in &program.classes {
        if c.methods.iter().any(|method| std::ptr::eq(method, func)) {
            return Some(c);
        }
    }
    mods(&program.mods, func)
}

pub fn guess_type(program: &Program, recv: &str, line: u32) -> String {
    let Some(func) = enclosing_fn(program, line) else {
        return String::new();
    };
    for (i, name) in func.params.iter().enumerate() {
        if name == recv {
            if let Some(ty) = func.param_types.get(i) {
                return type_head(ty);
            }
        }
    }
    guess_stmts(&func.body, recv, line)
}

pub fn guess_stmts(stmts: &[Stmt], recv: &str, line: u32) -> String {
    let mut found = String::new();
    for s in stmts {
        if s.span.line > line {
            continue;
        }
        if (s.kind == StmtKind::Var || s.kind == StmtKind::Const) && s.name == recv {
            if !s.type_name.is_empty() {
                found = type_head(&s.type_name);
            } else if let Some(expr) = &s.expr {
                if expr.kind == ExprKind::StructLit {
                    found = type_head(&expr.text);
                }
            }
        }
        let inner = guess_stmts(&s.body, recv, line);
        if !inner.is_empty() {
            found = inner;
        }
        let inner = guess_stmts(&s.else_body, recv, line);
        if !inner.is_empty() {
            found = inner;
        }
        for arm in &s.arms {
            let inner = guess_stmts(&arm.body, recv, line);
            if !inner.is_empty() {
                found = inner;
            }
        }
    }
    found
}

pub fn has_class_or_struct(program: &Program, name: &str) -> bool {
    struct Hit<'a> {
        name: &'a str,
        found: bool,
    }
    impl Visit for Hit<'_> {
        fn on_class(&mut self, c: &ClassDecl) {
            if c.name == self.name {
                self.found = true;
            }
        }
        fn on_struct(&mut self, s: &StructDecl) {
            if s.name == self.name {
                self.found = true;
            }
        }
    }
    let mut hit = Hit { name, found: false };
    walk_program(program, &mut hit);
    hit.found
}

pub fn has_named_type(program: &Program, name: &str) -> bool {
    struct Hit<'a> {
        name: &'a str,
        found: bool,
    }
    impl Visit for Hit<'_> {
        fn on_class(&mut self, c: &ClassDecl) {
            if c.name == self.name {
                self.found = true;
            }
        }
        fn on_struct(&mut self, s: &StructDecl) {
            if s.name == self.name {
                self.found = true;
            }
        }
        fn on_enum(&mut self, e: &EnumDecl) {
            if e.name == self.name {
                self.found = true;
            }
        }
    }
    let mut hit = Hit { name, found: false };
    walk_program(program, &mut hit);
    hit.found
}

pub fn find_enum<'a>(program: &'a Program, name: &str) -> Option<&'a EnumDecl> {
    fn last<'a>(enums: &'a [EnumDecl], mods: &'a [ModDecl], name: &str) -> Option<&'a EnumDecl> {
        let mut hit = None;
        for e in enums {
            if e.name == name {
                hit = Some(e);
            }
        }
        for m in mods {
            if let Some(inner) = last(&m.enums, &m.mods, name) {
                hit = Some(inner);
            }
        }
        hit
    }
    last(&program.enums, &program.mods, name)
}

pub fn find_class<'a>(program: &'a Program, name: &str) -> Option<&'a ClassDecl> {
    fn mods<'a>(list: &'a [ModDecl], name: &str) -> Option<&'a ClassDecl> {
        for m in list {
            for c in &m.classes {
                if c.name == name {
                    return Some(c);
                }
            }
            if let Some(hit) = mods(&m.mods, name) {
                return Some(hit);
            }
        }
        None
    }
    for c in &program.classes {
        if c.name == name {
            return Some(c);
        }
    }
    mods(&program.mods, name)
}

pub fn has_signal(program: &Program, name: &str) -> bool {
    fn mods(list: &[ModDecl], name: &str) -> bool {
        for m in list {
            if m.signals.iter().any(|s| s.name == name) || mods(&m.mods, name) {
                return true;
            }
        }
        false
    }
    program.signals.iter().any(|s| s.name == name) || mods(&program.mods, name)
}
