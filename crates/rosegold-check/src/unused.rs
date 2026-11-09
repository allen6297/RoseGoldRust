use rosegold_syntax::{tokenize, Diagnostic, ImportDecl, ModDecl, Program, Tok};

pub fn check(source: &str, path: &str, program: &Program, diags: &mut Vec<Diagnostic>) {
    let mut imports = Vec::new();
    let mut lines = std::collections::HashSet::new();
    collect_program(program, &mut imports, &mut lines);
    if imports.is_empty() {
        return;
    }
    let used = used_idents(source, path, &lines);
    for im in imports {
        let bind = bind_name(im);
        if bind.is_empty() || used.contains(&bind) || has_error_on_line(diags, im.span.line) {
            continue;
        }
        diags.push(Diagnostic::new(
            path,
            im.span.line,
            im.span.col,
            "warning",
            format!("unused import '{bind}'"),
            "unused",
        ));
    }
}

fn bind_name(im: &ImportDecl) -> String {
    if im.path.is_empty() {
        return String::new();
    }
    if im.is_from {
        if im.path.len() < 2 {
            return String::new();
        }
        return if im.alias.is_empty() {
            im.path[1].clone()
        } else {
            im.alias.clone()
        };
    }
    if im.alias.is_empty() {
        im.path.last().cloned().unwrap_or_default()
    } else {
        im.alias.clone()
    }
}

fn collect_program<'a>(
    program: &'a Program,
    out: &mut Vec<&'a ImportDecl>,
    lines: &mut std::collections::HashSet<u32>,
) {
    add(&program.imports, out, lines);
    for m in &program.mods {
        collect_mod(m, out, lines);
    }
}

fn collect_mod<'a>(
    m: &'a ModDecl,
    out: &mut Vec<&'a ImportDecl>,
    lines: &mut std::collections::HashSet<u32>,
) {
    add(&m.imports, out, lines);
    for nested in &m.mods {
        collect_mod(nested, out, lines);
    }
}

fn add<'a>(
    imports: &'a [ImportDecl],
    out: &mut Vec<&'a ImportDecl>,
    lines: &mut std::collections::HashSet<u32>,
) {
    for im in imports {
        out.push(im);
        lines.insert(im.span.line);
    }
}

fn used_idents(
    source: &str,
    path: &str,
    import_lines: &std::collections::HashSet<u32>,
) -> std::collections::HashSet<String> {
    let mut used = std::collections::HashSet::new();
    let mut junk = Vec::new();
    let tokens = tokenize(source, path, &mut junk);
    for t in tokens {
        if t.kind != Tok::Identifier || import_lines.contains(&t.span.line) {
            continue;
        }
        used.insert(t.text);
    }
    used
}

fn has_error_on_line(diags: &[Diagnostic], line: u32) -> bool {
    diags.iter().any(|d| {
        d.line == line && d.severity != "warning" && d.severity != "warn" && d.severity != "info"
    })
}
