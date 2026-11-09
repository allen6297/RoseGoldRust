use std::collections::HashSet;
use std::path::Path;

use rosegold_check::{
    crate_of, export_index, files_for_crate, list_rg_files, lookup, stdlib_children, type_head,
    Project,
};
use rosegold_syntax::*;

use crate::idents;
use crate::pos::{self, char_len};
use crate::walk::{
    self, enclosing_class, enclosing_fn, find_enum, guess_type, has_named_type, has_signal,
    parse_quiet, Visit,
};

#[derive(Clone, Debug)]
pub struct Item {
    pub label: String,
    pub kind: String,
    pub detail: String,
}

const KEYWORDS: &[&str] = &[
    "fn",
    "var",
    "const",
    "export",
    "static",
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
    "null",
    "try",
    "do",
    "throws",
    "throw",
    "catch",
    "async",
    "await",
    "spawn",
];

const TYPES: &[&str] = &[
    "Int", "Float", "String", "Bool", "Void", "Array", "Map", "Range", "UUID", "Vec2", "Vec3",
    "Window", "Color",
];

const COLOR_VARIANTS: &[(&str, &str)] = &[
    ("Black", "Color.Black"),
    ("Red", "Color.Red"),
    ("Green", "Color.Green"),
    ("Yellow", "Color.Yellow"),
    ("Blue", "Color.Blue"),
    ("Magenta", "Color.Magenta"),
    ("Cyan", "Color.Cyan"),
    ("White", "Color.White"),
    ("Rgb", "Color.Rgb(r, g, b)"),
    ("Argb", "Color.Argb(a, r, g, b)"),
];

const BUILTINS: &[(&str, &str)] = &[
    ("print", "print(...)"),
    ("assert", "assert(cond)"),
    ("len", "len(xs) — Array, String, or Map"),
    ("checks.eq", "checks.eq(a, b)"),
    ("checks.neq", "checks.neq(a, b)"),
    ("checks.eq_string", "checks.eq_string(a, b)"),
    ("checks.that", "checks.that(cond)"),
    ("argv", "argv(i) — script path at 0"),
    ("argv_len", "argv_len() — argc"),
];

pub fn suggest(source: &str, path: &str, offset: usize) -> Vec<Item> {
    let offset = pos::clamp_offset(source, offset);
    let mut out = Vec::new();
    if in_comment_or_string(source, path, offset) {
        return Vec::new();
    }
    let prefix = idents::line_prefix(source, offset);
    if after_at(&prefix) {
        add(&mut out, "test", "keyword", "@test");
        add(&mut out, "deprecated", "keyword", "@deprecated");
        add(&mut out, "constexpr", "keyword", "@constexpr");
        add(&mut out, "ufcs", "keyword", "@ufcs");
        add(&mut out, "optional", "keyword", "@optional");
        return unique(out);
    }
    let from_crate = idents::from_import_crate(&prefix);
    if !from_crate.is_empty() {
        add_from_import_names(&mut out, &from_crate, path);
        return unique(out);
    }
    if idents::after_import_or_from(&prefix) {
        let recv = member_recv(&prefix);
        if recv == "std" {
            add_stdlib_children(&mut out);
        } else {
            add_import_crates(&mut out, path);
        }
        return unique(out);
    }
    let recv = member_recv(&prefix);
    let program = parse_quiet(source, path);
    if !recv.is_empty() {
        add_member(&mut out, &recv, &program, source, path, offset);
        return unique(out);
    }
    if after_colon_type(&prefix) {
        add_types(&mut out, path);
        add_file_types(&mut out, &program);
        return unique(out);
    }
    for kw in KEYWORDS {
        add(&mut out, kw, "keyword", "keyword");
    }
    for ty in TYPES {
        add(&mut out, ty, "type", "type");
    }
    for (name, detail) in BUILTINS {
        add(&mut out, name, "function", detail);
    }
    add(&mut out, "std", "module", "stdlib");
    add_file_symbols(&mut out, &program);
    add_locals(&mut out, &program, pos::line_col(source, offset).0);
    unique(out)
}

fn add_member(
    out: &mut Vec<Item>,
    recv: &str,
    program: &Program,
    source: &str,
    path: &str,
    offset: usize,
) {
    let crate_name = crate_of(recv, program, path);
    let key = if crate_name.is_empty() {
        recv.to_string()
    } else {
        crate_name
    };
    match key.as_str() {
        "std" => {
            for (name, detail) in [
                ("math", "std.math"),
                ("str", "std.str"),
                ("io", "std.io"),
                ("vec", "std.vec"),
                ("time", "std.time"),
                ("path", "std.path"),
                ("json", "std.json"),
                ("regex", "std.regex"),
                ("ui", "std.ui"),
            ] {
                add(out, name, "module", detail);
            }
            add(out, "v4", "function", "std.v4()");
            add(out, "nil", "function", "std.nil()");
            add(out, "parse", "function", "std.parse(s) throws");
            add(out, "valid", "function", "std.valid(s)");
            add(out, "UUID", "struct", "data UUID");
        }
        "math" => add_rows(
            out,
            "method",
            &[
                ("abs", "math.abs(n)"),
                ("sign", "math.sign(n)"),
                ("min", "math.min(a, b)"),
                ("max", "math.max(a, b)"),
                ("clamp", "math.clamp(v, lo, hi)"),
                ("gcd", "math.gcd(a, b)"),
                ("pow", "math.pow(a, b) — Int"),
                ("rand_int", "math.rand_int(n)"),
                ("sin", "math.sin(n)"),
                ("cos", "math.cos(n)"),
                ("atan2", "math.atan2(y, x)"),
                ("sqrt", "math.sqrt(n)"),
                ("powf", "math.powf(a, b) — Float"),
                ("to_int", "math.to_int(n)"),
                ("to_float", "math.to_float(n)"),
                ("floor", "math.floor(n)"),
                ("ceil", "math.ceil(n)"),
                ("random", "math.random() — 0..1"),
                ("lerp", "math.lerp(a, b, t)"),
                ("move_toward", "math.move_toward(current, target, delta)"),
            ],
        ),
        "str" => add_rows(
            out,
            "method",
            &[
                ("contains", "str.contains(s, sub)"),
                ("starts_with", "str.starts_with(s, prefix)"),
                ("ends_with", "str.ends_with(s, suffix)"),
                ("length", "str.length(s)"),
                ("is_empty", "str.is_empty(s)"),
                ("repeat", "str.repeat(s, n)"),
                ("upper", "str.upper(s)"),
                ("lower", "str.lower(s)"),
                ("trim", "str.trim(s)"),
                ("slice", "str.slice(s, start, end)"),
                ("split", "str.split(s, sep)"),
                ("replace", "str.replace(s, old, with)"),
                ("find", "str.find(s, sub) — index or -1"),
            ],
        ),
        "io" => add_rows(
            out,
            "method",
            &[
                ("exists", "io.exists(path)"),
                ("remove", "io.remove(path)"),
                ("read_text", "io.read_text(path) throws"),
                ("read_lines", "io.read_lines(path) throws"),
                ("write_text", "io.write_text(path, content) throws"),
            ],
        ),
        "time" => add_rows(
            out,
            "method",
            &[("now", "time.now()"), ("sleep", "time.sleep(ms)")],
        ),
        "path" => add_rows(
            out,
            "method",
            &[
                ("join", "path.join(a, b)"),
                ("parent", "path.parent(p)"),
                ("stem", "path.stem(p)"),
            ],
        ),
        "json" => add_rows(
            out,
            "method",
            &[
                ("parse", "json.parse(s) throws"),
                ("stringify", "json.stringify(v)"),
                ("valid", "json.valid(s)"),
            ],
        ),
        "regex" => add_rows(
            out,
            "method",
            &[
                ("valid", "regex.valid(pattern)"),
                (
                    "is_match",
                    "regex.is_match(pattern, text) — search anywhere",
                ),
                ("find", "regex.find(pattern, text) — start index or -1"),
                (
                    "find_match",
                    "regex.find_match(pattern, text) — matched text or \"\"",
                ),
                (
                    "captures",
                    "regex.captures(pattern, text) — [full, g1, …] or []",
                ),
                (
                    "findall",
                    "regex.findall(pattern, text) — non-overlapping matches",
                ),
                (
                    "replace",
                    "regex.replace(pattern, text, with) — $1 backrefs",
                ),
                ("split", "regex.split(pattern, text)"),
            ],
        ),
        "vec" => {
            add(out, "Vec2", "class", "class Vec2");
            add(out, "Vec3", "class", "class Vec3");
        }
        "ui" => {
            add_rows(
                out,
                "function",
                &[
                    ("open", "ui.open(title, width, height) throws"),
                    ("open_hidden", "ui.open_hidden(title, width, height) throws"),
                    ("run", "ui.run()"),
                    ("count", "ui.count()"),
                    ("backend", "ui.backend()"),
                    ("platform", "ui.platform()"),
                    ("kind", "ui.kind()"),
                    ("font_height", "ui.font_height()"),
                    ("text_width", "ui.text_width(s)"),
                    ("rgb", "ui.rgb(r, g, b)"),
                    ("padding", "w.padding(n)"),
                    ("background", "w.background(color)"),
                    ("style", "w.style(Style { fill, pad })"),
                ],
            );
            add(out, "Color", "enum", "enum Color");
            add(out, "Window", "class", "class Window");
            add(out, "Label", "class", "class Label");
            add(out, "Button", "class", "class Button");
            add(out, "VStack", "class", "class VStack");
            add(out, "HStack", "class", "class HStack");
            add(out, "Style", "class", "class Style");
            add(out, "Widget", "trait", "trait Widget");
        }
        "checks" => add_rows(
            out,
            "method",
            &[
                ("eq", "checks.eq(a, b)"),
                ("neq", "checks.neq(a, b)"),
                ("eq_string", "checks.eq_string(a, b)"),
                ("that", "checks.that(cond)"),
            ],
        ),
        "process" => add_rows(
            out,
            "method",
            &[("argv", "process.argv(i)"), ("argc", "process.argc()")],
        ),
        _ => add_unknown_recv(out, recv, program, source, offset),
    }
    if key != "self" && key != "super" {
        add_crate_members(out, &key, path);
    }
}

fn add_crate_members(out: &mut Vec<Item>, crate_name: &str, path: &str) {
    for file in files_for_crate(crate_name, path) {
        let Ok(src) = std::fs::read_to_string(&file) else {
            continue;
        };
        add_file_symbols(out, &parse_quiet(&src, &file.display().to_string()));
    }
}

fn add_unknown_recv(
    out: &mut Vec<Item>,
    recv: &str,
    program: &Program,
    source: &str,
    offset: usize,
) {
    if let Some(en) = find_enum(program, recv) {
        add_enum_variants(out, en);
        add_type_members(out, program, &en.name, true);
        return;
    }
    if recv == "Color" {
        add_color_members(out);
        return;
    }
    if has_signal(program, recv) {
        add(out, "connect", "method", &format!("{recv}.connect(fn)"));
        add(out, "emit", "method", &format!("{recv}.emit()"));
        add(
            out,
            "emit_deferred",
            "method",
            &format!("{recv}.emit_deferred()"),
        );
        add(
            out,
            "disconnect",
            "method",
            &format!("{recv}.disconnect(fn)"),
        );
        return;
    }
    let line = pos::line_col(source, offset).0;
    let mut type_name = recv.to_string();
    if !has_named_type(program, recv) {
        if recv == "self" || recv == "super" {
            let class = enclosing_class(program, line);
            type_name = class.map(|c| c.name.clone()).unwrap_or_default();
            if recv == "super" {
                type_name = class.map(|c| c.parent.clone()).unwrap_or_default();
            }
        } else {
            type_name = guess_type(program, recv, line);
        }
    }
    add_type_members(out, program, &type_name, recv == "super");
}

fn add_types(out: &mut Vec<Item>, path: &str) {
    for ty in TYPES {
        let mut detail = "type".to_string();
        if let Some(ex) = lookup(ty, path) {
            if !ex.crate_name.is_empty() {
                detail = format!("crate {}", ex.crate_name);
            }
        }
        add(out, ty, "type", &detail);
    }
    for (name, hits) in export_index(path).iter() {
        for ex in hits {
            if ex.kind == "fn" {
                continue;
            }
            add(out, name, "type", &format!("crate {}", ex.crate_name));
            break;
        }
    }
}

fn add_file_types(out: &mut Vec<Item>, program: &Program) {
    struct V<'a> {
        out: &'a mut Vec<Item>,
    }
    impl Visit for V<'_> {
        fn on_class(&mut self, c: &ClassDecl) {
            add(self.out, &c.name, "class", "class");
        }
        fn on_struct(&mut self, s: &StructDecl) {
            add(
                self.out,
                &s.name,
                "struct",
                if s.is_data { "data" } else { "struct" },
            );
        }
        fn on_trait(&mut self, t: &TraitDecl) {
            add(self.out, &t.name, "trait", "trait");
        }
        fn on_enum(&mut self, e: &EnumDecl) {
            add(self.out, &e.name, "enum", "enum");
        }
    }
    walk::walk_program(program, &mut V { out });
}

fn add_file_symbols(out: &mut Vec<Item>, program: &Program) {
    struct V<'a> {
        out: &'a mut Vec<Item>,
    }
    impl Visit for V<'_> {
        fn on_fn(&mut self, func: &FnDecl) {
            add_fn(self.out, func);
        }
        fn on_class(&mut self, c: &ClassDecl) {
            add(self.out, &c.name, "class", "class");
            for method in &c.methods {
                add_fn(self.out, method);
            }
        }
        fn on_struct(&mut self, s: &StructDecl) {
            add(
                self.out,
                &s.name,
                "struct",
                if s.is_data { "data" } else { "struct" },
            );
            for method in &s.methods {
                add_fn(self.out, method);
            }
        }
        fn on_trait(&mut self, t: &TraitDecl) {
            add(self.out, &t.name, "trait", "trait");
        }
        fn on_enum(&mut self, e: &EnumDecl) {
            add(self.out, &e.name, "enum", "enum");
        }
        fn on_signal(&mut self, s: &SignalDecl) {
            add(self.out, &s.name, "function", "signal");
        }
        fn on_mod(&mut self, m: &ModDecl) {
            add(self.out, &m.name, "module", "mod");
        }
    }
    walk::walk_program(program, &mut V { out });
}

fn add_fn(out: &mut Vec<Item>, func: &FnDecl) {
    let mut detail = format!("fn {}(", func.name);
    for (i, name) in func.params.iter().enumerate() {
        if i > 0 {
            detail.push_str(", ");
        }
        detail.push_str(name);
        if let Some(ty) = func.param_types.get(i) {
            if !ty.is_empty() {
                detail.push_str(": ");
                detail.push_str(ty);
            }
        }
    }
    detail.push(')');
    if !func.return_type.is_empty() {
        detail.push_str(": ");
        detail.push_str(&func.return_type);
    }
    add(out, &func.name, "function", &detail);
}

fn add_locals(out: &mut Vec<Item>, program: &Program, line: u32) {
    let Some(func) = enclosing_fn(program, line) else {
        return;
    };
    for (i, name) in func.params.iter().enumerate() {
        if name == "self" {
            continue;
        }
        let ty = func.param_types.get(i).map(|s| s.as_str()).unwrap_or("");
        add(
            out,
            name,
            "variable",
            if ty.is_empty() { "param" } else { ty },
        );
    }
    collect_locals(&func.body, line, out);
}

fn collect_locals(stmts: &[Stmt], line: u32, out: &mut Vec<Item>) {
    for s in stmts {
        if s.span.line > line {
            continue;
        }
        if matches!(s.kind, StmtKind::Var | StmtKind::Const | StmtKind::For) && !s.name.is_empty() {
            let mut detail = match s.kind {
                StmtKind::Const => "const",
                StmtKind::For => "for",
                _ => "var",
            }
            .to_string();
            if !s.type_name.is_empty() {
                detail = s.type_name.clone();
            }
            add(out, &s.name, "variable", &detail);
        }
        collect_locals(&s.body, line, out);
        collect_locals(&s.else_body, line, out);
        for arm in &s.arms {
            for bind in &arm.binds {
                add(out, bind, "variable", "match");
            }
            collect_locals(&arm.body, line, out);
        }
    }
}

fn add_type_members(out: &mut Vec<Item>, program: &Program, type_name: &str, skip_fields: bool) {
    if type_name.is_empty() {
        return;
    }
    let head = type_head(type_name);
    struct V<'a> {
        out: &'a mut Vec<Item>,
        program: &'a Program,
        head: String,
        skip_fields: bool,
    }
    impl Visit for V<'_> {
        fn on_class(&mut self, c: &ClassDecl) {
            if c.name != self.head {
                return;
            }
            if !self.skip_fields {
                for field in &c.fields {
                    add(self.out, &field.name, "field", &field.ty);
                }
            }
            for method in &c.methods {
                add_fn(self.out, method);
            }
            for signal in &c.signals {
                add(self.out, &signal.name, "function", "signal");
            }
            let parent = c.parent.clone();
            add_type_members(self.out, self.program, &parent, true);
        }
        fn on_struct(&mut self, s: &StructDecl) {
            if s.name != self.head {
                return;
            }
            if !self.skip_fields {
                for (i, name) in s.fields.iter().enumerate() {
                    let ty = s.field_types.get(i).map(|t| t.as_str()).unwrap_or("");
                    add(self.out, name, "field", ty);
                }
            }
            for method in &s.methods {
                add_fn(self.out, method);
            }
        }
    }
    walk::walk_program(
        program,
        &mut V {
            out,
            program,
            head: head.clone(),
            skip_fields,
        },
    );
    for im in &program.impls {
        if head == type_head(&im.type_name) {
            for method in &im.methods {
                add_fn(out, method);
            }
        }
    }
}

fn add_enum_variants(out: &mut Vec<Item>, en: &EnumDecl) {
    for v in &en.variants {
        add(out, &v.name, "enum", &variant_detail(en, v));
    }
}

fn add_color_members(out: &mut Vec<Item>) {
    for (name, detail) in COLOR_VARIANTS {
        add(out, name, "enum", detail);
    }
}

fn variant_detail(en: &EnumDecl, v: &EnumVariant) -> String {
    let mut detail = format!("{}.{}", en.name, v.name);
    if v.arity <= 0 {
        return detail;
    }
    detail.push('(');
    for i in 0..v.arity {
        if i > 0 {
            detail.push_str(", ");
        }
        let field = v
            .field_names
            .get(i as usize)
            .map(|s| s.as_str())
            .unwrap_or("");
        detail.push_str(if field.is_empty() { "_" } else { field });
    }
    detail.push(')');
    detail
}

fn in_comment_or_string(source: &str, path: &str, offset: usize) -> bool {
    let mut junk = Vec::new();
    let tokens = tokenize(source, path, &mut junk);
    let probe = offset.saturating_sub(1);
    for t in tokens {
        if t.kind != Tok::LineComment && t.kind != Tok::BlockComment && t.kind != Tok::String {
            continue;
        }
        let start = pos::offset(source, t.line(), t.col());
        let end = token_end(source, &t, start);
        if probe >= start && probe < end {
            return true;
        }
    }
    false
}

fn token_end(source: &str, t: &Token, start: usize) -> usize {
    if t.kind == Tok::String {
        let chars: Vec<char> = source.chars().collect();
        let mut i = start.min(chars.len());
        if i < chars.len() && chars[i] == '"' {
            i += 1;
        }
        while i < chars.len() {
            let c = chars[i];
            i += 1;
            if c == '\\' && i < chars.len() {
                i += 1;
            } else if c == '"' {
                break;
            }
        }
        return i;
    }
    let len = char_len(&t.text).max(1);
    start.saturating_add(len).min(char_len(source))
}

fn after_at(prefix: &str) -> bool {
    let chars: Vec<char> = prefix.chars().collect();
    let i = skip_ident(&chars);
    i > 0 && chars[i - 1] == '@'
}

fn add_stdlib_children(out: &mut Vec<Item>) {
    for child in stdlib_children() {
        add(out, child, "module", &format!("std.{child}"));
    }
}

fn add_import_crates(out: &mut Vec<Item>, path: &str) {
    add(out, "std", "module", "stdlib");
    add_stdlib_children(out);
    let start = if path.is_empty() {
        Path::new(".").to_path_buf()
    } else {
        Path::new(path).to_path_buf()
    };
    let Ok(Some(project)) = Project::find(&start) else {
        return;
    };
    let mut names: Vec<_> = project.modules.keys().cloned().collect();
    names.sort();
    for name in names {
        add(out, &name, "module", "project module");
    }
}

fn add_from_import_names(out: &mut Vec<Item>, crate_name: &str, path: &str) {
    let want = if crate_name.len() > 4 && crate_name.starts_with("std.") {
        &crate_name[4..]
    } else {
        crate_name
    };
    for (name, hits) in export_index(path).iter() {
        for ex in hits {
            if crate_equals(&ex.crate_name, crate_name, want) {
                let kind = if ex.kind.is_empty() {
                    "value"
                } else {
                    ex.kind.as_str()
                };
                add(out, name, kind, &ex.crate_name);
                break;
            }
        }
    }
    add_project_module_names(out, crate_name, path);
}

fn crate_equals(got: &str, crate_name: &str, want: &str) -> bool {
    got == crate_name || got == want || got == format!("std.{want}")
}

fn add_project_module_names(out: &mut Vec<Item>, crate_name: &str, path: &str) {
    let start = if path.is_empty() {
        Path::new(".").to_path_buf()
    } else {
        Path::new(path).to_path_buf()
    };
    let Ok(Some(project)) = Project::find(&start) else {
        return;
    };
    let Some(mapped) = project.module_path(crate_name) else {
        return;
    };
    let files = if mapped.is_dir() {
        list_rg_files(&mapped)
    } else {
        vec![mapped]
    };
    for file in files {
        if !file.is_file() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        add_file_symbols(out, &parse_quiet(&text, &file.display().to_string()));
    }
}

fn after_colon_type(prefix: &str) -> bool {
    let chars: Vec<char> = prefix.chars().collect();
    let mut i = skip_ident(&chars);
    while i > 0 && chars[i - 1].is_whitespace() {
        i -= 1;
    }
    i > 0 && chars[i - 1] == ':'
}

fn member_recv(prefix: &str) -> String {
    let chars: Vec<char> = prefix.chars().collect();
    let i = skip_ident(&chars);
    if i == 0 || chars[i - 1] != '.' {
        return String::new();
    }
    let end = i - 1;
    let mut start = end;
    while start > 0 && idents::ident_char(chars[start - 1]) {
        start -= 1;
    }
    chars[start..end].iter().collect()
}

fn skip_ident(chars: &[char]) -> usize {
    let mut i = chars.len();
    while i > 0 && idents::ident_char(chars[i - 1]) {
        i -= 1;
    }
    i
}

fn add_rows(out: &mut Vec<Item>, kind: &str, rows: &[(&str, &str)]) {
    for (label, detail) in rows {
        add(out, label, kind, detail);
    }
}

fn add(out: &mut Vec<Item>, label: &str, kind: &str, detail: &str) {
    if label.is_empty() {
        return;
    }
    out.push(Item {
        label: label.to_string(),
        kind: kind.to_string(),
        detail: detail.to_string(),
    });
}

fn unique(items: Vec<Item>) -> Vec<Item> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in items {
        if seen.insert(format!("{}:{}", item.kind, item.label)) {
            out.push(item);
        }
    }
    out
}
