use std::fs;
use std::path::{Path, PathBuf};

use rosegold_syntax::*;

#[derive(Clone, Debug)]
pub struct Options {
    pub blank_between_items: bool,
    pub keep_comments: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            blank_between_items: true,
            keep_comments: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FormatResult {
    pub ok: bool,
    pub out: String,
    pub message: String,
    pub exit_code: i32,
}

pub fn format_source(source: &str, path: &str, opts: &Options) -> FormatResult {
    let (program, diags) = parse_source(source, path);
    if let Some(d) = diags.first() {
        return FormatResult {
            ok: false,
            out: String::new(),
            message: d.message.clone(),
            exit_code: 1,
        };
    }
    let mut printer = Printer {
        opts: opts.clone(),
        out: String::new(),
        indent: 0,
    };
    printer.program(&program);
    let mut s = printer.out;
    if !s.is_empty() && !s.ends_with('\n') {
        s.push('\n');
    }
    FormatResult {
        ok: true,
        out: s,
        message: String::new(),
        exit_code: 0,
    }
}

pub fn list_rg_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if dir.is_dir() {
        walk_rg(dir, dir, &mut files);
    }
    files.sort_by(|a, b| a.to_string_lossy().cmp(&b.to_string_lossy()));
    files
}

fn walk_rg(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for ent in entries {
        let path = ent.path();
        let name = ent.file_name().to_string_lossy().to_string();
        if skipped_part(root, &path, &name) {
            continue;
        }
        if path.is_dir() {
            walk_rg(root, &path, out);
        } else if path.is_file() && name.to_ascii_lowercase().ends_with(".rg") {
            out.push(path);
        }
    }
}

fn skipped_part(root: &Path, file: &Path, name: &str) -> bool {
    if name.starts_with('.') || name == "build" {
        let Ok(rel) = file.strip_prefix(root) else {
            return name.starts_with('.') || name == "build";
        };
        if rel.components().count() == 0 {
            return false;
        }
        return true;
    }
    false
}

struct Printer {
    opts: Options,
    out: String,
    indent: i32,
}

const OR: i32 = 1;
const AND: i32 = 2;
const EQ: i32 = 3;
const CMP: i32 = 4;
const BITOR: i32 = 5;
const XOR: i32 = 6;
const BITAND: i32 = 7;
const SHIFT: i32 = 8;
const ADD: i32 = 9;
const MUL: i32 = 10;
const UNARY: i32 = 11;
const POSTFIX: i32 = 13;
const ATOM: i32 = 20;

impl Printer {
    fn line(&mut self) {
        self.out.push('\n');
    }

    fn pad(&mut self) {
        for _ in 0..self.indent.max(0) {
            self.out.push_str("    ");
        }
    }

    fn write(&mut self, s: &str) {
        self.out.push_str(s);
    }

    fn writeln(&mut self, s: &str) {
        self.pad();
        self.out.push_str(s);
        self.out.push('\n');
    }

    fn program(&mut self, p: &Program) {
        self.emit_items(
            &p.items,
            &p.imports,
            &p.fns,
            &p.structs,
            &p.classes,
            &p.traits,
            &p.enums,
            &p.impls,
            &p.signals,
            &p.mods,
            &p.singletons,
            &p.consts,
            &p.trailing_comments,
            false,
        );
    }

    fn emit_items(
        &mut self,
        order: &[OrderedItem],
        imports: &[ImportDecl],
        fns: &[FnDecl],
        structs: &[StructDecl],
        classes: &[ClassDecl],
        traits: &[TraitDecl],
        enums: &[EnumDecl],
        impls: &[ImplDecl],
        signals: &[SignalDecl],
        mods: &[ModDecl],
        singletons: &[String],
        consts: &[ConstDecl],
        trailing: &[String],
        in_mod: bool,
    ) {
        let mut prev = ItemKind::Import;
        let mut first = true;
        for it in order {
            let blank = self.opts.blank_between_items
                && !first
                && !(prev == ItemKind::Import && it.kind == ItemKind::Import);
            if blank {
                self.line();
            }
            first = false;
            prev = it.kind;
            self.emit_comments(&it.leading_comments);
            match it.kind {
                ItemKind::Import => {
                    if let Some(im) = imports.get(it.index) {
                        self.import_decl(im);
                    }
                }
                ItemKind::Fn => {
                    if let Some(f) = fns.get(it.index) {
                        self.fn_item(f, in_mod);
                    }
                }
                ItemKind::Struct => {
                    if let Some(s) = structs.get(it.index) {
                        self.struct_decl(s, in_mod);
                    }
                }
                ItemKind::Class => {
                    if let Some(c) = classes.get(it.index) {
                        self.class_decl(c, in_mod);
                    }
                }
                ItemKind::Trait => {
                    if let Some(t) = traits.get(it.index) {
                        self.trait_decl(t, in_mod);
                    }
                }
                ItemKind::Enum => {
                    if let Some(e) = enums.get(it.index) {
                        self.enum_decl(e, in_mod);
                    }
                }
                ItemKind::Impl => {
                    if let Some(im) = impls.get(it.index) {
                        self.impl_decl(im);
                    }
                }
                ItemKind::Signal => {
                    if let Some(sig) = signals.get(it.index) {
                        self.signal(sig, in_mod);
                    }
                }
                ItemKind::Mod => {
                    if let Some(m) = mods.get(it.index) {
                        self.mod_decl(m, in_mod);
                    }
                }
                ItemKind::Singleton => {
                    if let Some(name) = singletons.get(it.index) {
                        self.pad();
                        self.write("singleton ");
                        self.write(name);
                        self.write(";\n");
                    }
                }
                ItemKind::Const => {
                    if let Some(c) = consts.get(it.index) {
                        self.const_decl(c);
                    }
                }
            }
        }
        if self.opts.keep_comments && !trailing.is_empty() {
            if !first {
                self.line();
            }
            self.emit_comments(trailing);
        }
    }

    fn emit_comments(&mut self, comments: &[String]) {
        if !self.opts.keep_comments {
            return;
        }
        for c in comments {
            self.pad();
            self.write(c);
            self.write("\n");
        }
    }

    fn item_vis(&mut self, is_pub: bool, in_mod: bool) {
        if !is_pub {
            self.write("private ");
        } else if in_mod {
            self.write("pub ");
        }
    }

    fn fn_vis(&mut self, vis: Vis, is_pub: bool, in_mod: bool) {
        if vis == Vis::Private || !is_pub {
            self.write("private ");
        } else if vis == Vis::Protected {
            self.write("protected ");
        } else if in_mod && is_pub {
            self.write("pub ");
        }
    }

    fn mod_decl(&mut self, m: &ModDecl, in_mod: bool) {
        self.pad();
        self.item_vis(m.is_pub, in_mod);
        self.write("mod ");
        self.write(&m.name);
        self.write(" {\n");
        self.indent += 1;
        self.emit_items(
            &m.items,
            &m.imports,
            &m.fns,
            &m.structs,
            &m.classes,
            &m.traits,
            &m.enums,
            &m.impls,
            &m.signals,
            &m.mods,
            &m.singletons,
            &m.consts,
            &m.trailing_comments,
            true,
        );
        self.indent -= 1;
        self.writeln("}");
    }

    fn import_decl(&mut self, im: &ImportDecl) {
        self.pad();
        if im.is_from {
            self.write("from ");
            if let Some(head) = im.path.first() {
                self.write(head);
            }
            self.write(" import ");
            for (i, part) in im.path.iter().enumerate().skip(1) {
                if i > 1 {
                    self.write(".");
                }
                self.write(part);
            }
        } else {
            self.write("import ");
            for (i, part) in im.path.iter().enumerate() {
                if i > 0 {
                    self.write(".");
                }
                self.write(part);
            }
        }
        if !im.alias.is_empty() {
            self.write(" as ");
            self.write(&im.alias);
        }
        self.write(";\n");
    }

    fn fn_item(&mut self, func: &FnDecl, in_mod: bool) {
        if func.is_test {
            self.writeln("@test");
        }
        if func.is_deprecated {
            self.writeln("@deprecated");
        }
        if func.is_constexpr {
            self.writeln("@constexpr");
        }
        if func.is_ufcs {
            self.writeln("@ufcs");
        }
        self.pad();
        if func.is_abstract {
            self.write("abstract ");
        }
        if func.is_final {
            self.write("final ");
        }
        if func.is_static {
            self.write("static ");
        }
        self.fn_vis(func.vis, func.is_pub, in_mod);
        if func.is_async {
            self.write("async ");
        }
        self.write("fn ");
        self.write(&func.name);
        self.type_params(&func.type_params);
        self.write_sig(func);
        if func.is_abstract {
            self.write(";\n");
            return;
        }
        self.write(" ");
        self.block_open();
        self.stmts(&func.body);
        self.block_close(true);
    }

    fn fn_inline(&mut self, func: &FnDecl) {
        if func.is_async {
            self.write("async ");
        }
        self.write("fn (");
        self.write_params(func);
        self.write(")");
        if func.throws_ex {
            self.write(" throws");
        }
        if !func.return_type.is_empty() {
            self.write(": ");
            self.write(&func.return_type);
        }
        self.write(" ");
        self.block_open();
        self.stmts(&func.body);
        self.block_close(false);
    }

    fn write_sig(&mut self, func: &FnDecl) {
        self.write("(");
        self.write_params(func);
        self.write(")");
        if func.throws_ex {
            self.write(" throws");
        }
        if !func.return_type.is_empty() {
            self.write(": ");
            self.write(&func.return_type);
        }
    }

    fn write_params(&mut self, func: &FnDecl) {
        for i in 0..func.params.len() {
            if i > 0 {
                self.write(", ");
            }
            self.write(&func.params[i]);
            if let Some(ty) = func.param_types.get(i) {
                if !ty.is_empty() {
                    self.write(": ");
                    self.write(ty);
                }
            }
            if let Some(Some(def)) = func.param_defaults.get(i) {
                self.write(" = ");
                self.expr(Some(def), 0);
            }
        }
        if func.is_vararg {
            if !func.params.is_empty() {
                self.write(", ");
            }
            self.write("...");
        }
    }

    fn type_params(&mut self, params: &[TypeParam]) {
        if params.is_empty() {
            return;
        }
        self.write("[");
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.write(&p.name);
            if !p.bounds.is_empty() {
                self.write(": ");
                for (b, bound) in p.bounds.iter().enumerate() {
                    if b > 0 {
                        self.write(" + ");
                    }
                    self.write(bound);
                }
            }
        }
        self.write("]");
    }

    fn type_args(&mut self, args: &[String]) {
        if args.is_empty() {
            return;
        }
        self.write("[");
        for (i, a) in args.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.write(a);
        }
        self.write("]");
    }

    fn signal(&mut self, sig: &SignalDecl, in_mod: bool) {
        self.pad();
        self.item_vis(sig.is_pub, in_mod);
        self.write("signal ");
        self.write(&sig.name);
        self.write("(");
        for i in 0..sig.params.len() {
            if i > 0 {
                self.write(", ");
            }
            self.write(&sig.params[i]);
            if let Some(ty) = sig.param_types.get(i) {
                if !ty.is_empty() {
                    self.write(": ");
                    self.write(ty);
                }
            }
        }
        self.write(");\n");
    }

    fn const_decl(&mut self, c: &ConstDecl) {
        self.pad();
        self.write("const ");
        self.write(&c.name);
        if !c.type_name.is_empty() {
            self.write(": ");
            self.write(&c.type_name);
        }
        self.write(" = ");
        self.expr(c.value.as_ref(), 0);
        self.write(";\n");
    }

    fn struct_decl(&mut self, s: &StructDecl, in_mod: bool) {
        self.pad();
        self.item_vis(s.is_pub, in_mod);
        self.write(if s.is_data { "data " } else { "struct " });
        self.write(&s.name);
        self.type_params(&s.type_params);
        if !s.impl_traits.is_empty() {
            self.write(" impl ");
            for (i, t) in s.impl_traits.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.write(t);
            }
        }
        self.write(" {\n");
        self.indent += 1;
        for i in 0..s.fields.len() {
            self.pad();
            if let Some(v) = s.field_vis.get(i) {
                if *v == Vis::Private {
                    self.write("private ");
                } else if *v == Vis::Protected {
                    self.write("protected ");
                }
            }
            if s.field_optional.get(i).copied().unwrap_or(false) {
                self.write("@optional ");
            }
            self.write(&s.fields[i]);
            if let Some(ty) = s.field_types.get(i) {
                if !ty.is_empty() {
                    self.write(": ");
                    self.write(ty);
                }
            }
            self.write(";\n");
        }
        for sig in &s.signals {
            self.signal(sig, false);
        }
        for m in &s.methods {
            self.line();
            self.fn_item(m, false);
        }
        self.indent -= 1;
        self.writeln("}");
    }

    fn class_decl(&mut self, c: &ClassDecl, in_mod: bool) {
        self.pad();
        if c.is_abstract {
            self.write("abstract ");
        }
        if c.is_final {
            self.write("final ");
        }
        self.item_vis(c.is_pub, in_mod);
        self.write("class ");
        self.write(&c.name);
        self.type_params(&c.type_params);
        if !c.parent.is_empty() {
            self.write(" extends ");
            self.write(&c.parent);
        }
        if !c.impl_traits.is_empty() {
            self.write(" impl ");
            for (i, t) in c.impl_traits.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.write(t);
            }
        }
        self.write(" {\n");
        self.indent += 1;
        for f in &c.fields {
            self.class_field(f);
        }
        for cd in &c.consts {
            self.const_decl(cd);
        }
        for sig in &c.signals {
            self.signal(sig, false);
        }
        for m in &c.methods {
            if m.generated {
                continue;
            }
            self.line();
            self.fn_item(m, false);
        }
        for ti in &c.trait_impls {
            self.line();
            self.pad();
            self.write("impl ");
            self.write(&ti.trait_name);
            self.type_params(&ti.type_params);
            self.write(" {\n");
            self.indent += 1;
            for m in &ti.methods {
                self.fn_item(m, false);
            }
            self.indent -= 1;
            self.writeln("}");
        }
        self.indent -= 1;
        self.writeln("}");
    }

    fn class_field(&mut self, f: &ClassField) {
        self.pad();
        if f.vis == Vis::Private {
            self.write("private ");
        } else if f.vis == Vis::Protected {
            self.write("protected ");
        }
        if f.optional {
            self.write("@optional ");
        }
        if f.exported {
            self.write("export ");
        }
        if f.has_range {
            self.write("range(");
            self.write(&f.range_min);
            self.write(", ");
            self.write(&f.range_max);
            if !f.range_step.is_empty() {
                self.write(", ");
                self.write(&f.range_step);
            }
            for s in &f.range_suffixes {
                self.write(", ");
                self.write(&escape_string(s));
            }
            self.write(") ");
        }
        if f.has_enum {
            self.write("enum(");
            for (n, name) in f.enum_names.iter().enumerate() {
                if n > 0 {
                    self.write(", ");
                }
                self.write(&escape_string(name));
            }
            self.write(") ");
        }
        if f.has_flags {
            self.write("flags(");
            for (n, name) in f.flag_names.iter().enumerate() {
                if n > 0 {
                    self.write(", ");
                }
                self.write(&escape_string(name));
            }
            self.write(") ");
        }
        if f.has_file {
            self.write("file(");
            for (n, name) in f.file_filters.iter().enumerate() {
                if n > 0 {
                    self.write(", ");
                }
                self.write(&escape_string(name));
            }
            self.write(") ");
        }
        if f.has_dir {
            self.write("dir ");
        }
        if f.has_multiline {
            self.write("multiline ");
        }
        if f.onready {
            self.write("@onready ");
        }
        self.write("var ");
        self.write(&f.name);
        if !f.ty.is_empty() {
            self.write(": ");
            self.write(&f.ty);
        }
        if f.has_default {
            self.write(" = ");
            self.expr(f.default_value.as_ref(), 0);
        }
        self.write(";\n");
    }

    fn trait_decl(&mut self, t: &TraitDecl, in_mod: bool) {
        self.pad();
        self.item_vis(t.is_pub, in_mod);
        self.write("trait ");
        self.write(&t.name);
        self.type_params(&t.type_params);
        self.write(" {\n");
        self.indent += 1;
        for m in &t.methods {
            self.pad();
            self.write("fn ");
            self.write(&m.name);
            self.write("(");
            for i in 0..m.params.len() {
                if i > 0 {
                    self.write(", ");
                }
                self.write(&m.params[i]);
                if let Some(ty) = m.param_types.get(i) {
                    if !ty.is_empty() {
                        self.write(": ");
                        self.write(ty);
                    }
                }
            }
            self.write(")");
            if m.throws_ex {
                self.write(" throws");
            }
            if !m.return_type.is_empty() {
                self.write(": ");
                self.write(&m.return_type);
            }
            self.write(";\n");
        }
        for sig in &t.signals {
            self.signal(sig, false);
        }
        self.indent -= 1;
        self.writeln("}");
    }

    fn enum_decl(&mut self, e: &EnumDecl, in_mod: bool) {
        self.pad();
        self.item_vis(e.is_pub, in_mod);
        self.write("enum ");
        self.write(&e.name);
        self.write(" {\n");
        self.indent += 1;
        for (i, v) in e.variants.iter().enumerate() {
            self.pad();
            self.write(&v.name);
            self.enum_payload(v);
            if i + 1 < e.variants.len() {
                self.write(",");
            }
            self.line();
        }
        self.indent -= 1;
        self.writeln("}");
    }

    fn enum_payload(&mut self, v: &EnumVariant) {
        let n = v.field_names.len().max(v.field_types.len());
        if n > 0 {
            self.write("(");
            for i in 0..n {
                if i > 0 {
                    self.write(", ");
                }
                let name = v.field_names.get(i).map(String::as_str).unwrap_or("");
                let ty = v.field_types.get(i).map(String::as_str).unwrap_or("");
                if !name.is_empty() {
                    self.write(name);
                    if !ty.is_empty() {
                        self.write(": ");
                        self.write(ty);
                    }
                } else if !ty.is_empty() {
                    self.write(ty);
                } else {
                    self.write("_");
                }
            }
            self.write(")");
        } else if v.arity > 0 {
            self.write("(");
            for a in 0..v.arity {
                if a > 0 {
                    self.write(", ");
                }
                self.write("_");
            }
            self.write(")");
        }
    }

    fn impl_decl(&mut self, im: &ImplDecl) {
        self.pad();
        self.write("impl");
        self.type_params(&im.type_params);
        self.write(" ");
        if !im.trait_name.is_empty() {
            self.write(&im.trait_name);
            self.write(" for ");
            self.write(&im.type_name);
        } else {
            self.write(&im.type_name);
        }
        self.write(" {\n");
        self.indent += 1;
        for m in &im.methods {
            self.fn_item(m, false);
        }
        self.indent -= 1;
        self.writeln("}");
    }

    fn block_open(&mut self) {
        self.write("{\n");
        self.indent += 1;
    }

    fn block_close(&mut self, newline_after: bool) {
        self.indent -= 1;
        self.pad();
        self.write("}");
        if newline_after {
            self.line();
        }
    }

    fn stmts(&mut self, body: &[Stmt]) {
        for s in body {
            self.stmt(s);
        }
    }

    fn end_stmt(&mut self, s: &Stmt) {
        self.write(";");
        if self.opts.keep_comments && !s.trailing_comment.is_empty() {
            self.write(" ");
            self.write(&s.trailing_comment);
        }
        self.write("\n");
    }

    fn stmt(&mut self, s: &Stmt) {
        if s.generated {
            return;
        }
        if self.opts.keep_comments {
            self.emit_comments(&s.leading_comments);
        }
        match s.kind {
            StmtKind::Comment => {}
            StmtKind::Expr => {
                self.pad();
                self.expr(s.expr.as_ref(), 0);
                self.end_stmt(s);
            }
            StmtKind::Var | StmtKind::Const => {
                self.pad();
                self.write(if s.kind == StmtKind::Const {
                    "const "
                } else {
                    "var "
                });
                self.write(&s.name);
                if !s.type_name.is_empty() {
                    self.write(": ");
                    self.write(&s.type_name);
                }
                self.write(" = ");
                self.expr(s.expr.as_ref(), 0);
                self.end_stmt(s);
            }
            StmtKind::Assign => {
                self.pad();
                self.write(&s.name);
                self.write(" ");
                self.write(&s.op);
                self.write(" ");
                self.expr(s.expr.as_ref(), 0);
                self.end_stmt(s);
            }
            StmtKind::FieldAssign => {
                self.pad();
                self.expr(s.target.as_ref(), 0);
                self.write(".");
                self.write(&s.name);
                self.write(" ");
                self.write(&s.op);
                self.write(" ");
                self.expr(s.expr.as_ref(), 0);
                self.end_stmt(s);
            }
            StmtKind::IndexAssign => {
                self.pad();
                if let Some(target) = s.target.as_ref() {
                    if target.kind == ExprKind::Index && target.kids.len() >= 2 {
                        self.expr(target.kids.first(), POSTFIX);
                        self.write("[");
                        self.expr(target.kids.get(1), 0);
                        self.write("]");
                    } else {
                        self.expr(Some(target), 0);
                    }
                }
                self.write(" ");
                self.write(&s.op);
                self.write(" ");
                self.expr(s.expr.as_ref(), 0);
                self.end_stmt(s);
            }
            StmtKind::Return => {
                self.pad();
                self.write("return");
                if s.has_expr {
                    self.write(" ");
                    self.expr(s.expr.as_ref(), 0);
                }
                self.end_stmt(s);
            }
            StmtKind::If => self.stmt_if(s),
            StmtKind::While => {
                self.pad();
                self.write("while (");
                self.expr(s.expr.as_ref(), 0);
                self.write(") ");
                self.block_open();
                self.stmts(&s.body);
                self.block_close(true);
            }
            StmtKind::For => {
                self.pad();
                self.write("for ");
                self.write(&s.name);
                self.write(" in ");
                self.expr(s.expr.as_ref(), 0);
                self.write(" ");
                self.block_open();
                self.stmts(&s.body);
                self.block_close(true);
            }
            StmtKind::Match => self.stmt_match(s),
            StmtKind::Pass => {
                self.pad();
                self.write("pass");
                self.end_stmt(s);
            }
            StmtKind::Break => {
                self.pad();
                self.write("break");
                self.end_stmt(s);
            }
            StmtKind::Continue => {
                self.pad();
                self.write("continue");
                self.end_stmt(s);
            }
            StmtKind::Throw => {
                self.pad();
                self.write("throw ");
                self.expr(s.expr.as_ref(), 0);
                self.end_stmt(s);
            }
            StmtKind::Do => {
                self.pad();
                self.write("do ");
                self.block_open();
                self.stmts(&s.body);
                self.block_close(false);
                if !s.name.is_empty() {
                    self.write(" catch ");
                    self.write(&s.name);
                    self.write(" ");
                    self.block_open();
                    self.stmts(&s.else_body);
                    self.block_close(false);
                }
                self.line();
            }
        }
    }

    fn stmt_if(&mut self, s: &Stmt) {
        self.pad();
        self.write("if (");
        self.expr(s.expr.as_ref(), 0);
        self.write(") ");
        self.block_open();
        self.stmts(&s.body);
        self.block_close(false);
        if !s.else_body.is_empty() {
            if s.else_body.len() == 1 && s.else_body[0].kind == StmtKind::If {
                let mut cur = &s.else_body[0];
                loop {
                    self.write(" elif (");
                    self.expr(cur.expr.as_ref(), 0);
                    self.write(") ");
                    self.block_open();
                    self.stmts(&cur.body);
                    self.block_close(false);
                    if cur.else_body.len() == 1 && cur.else_body[0].kind == StmtKind::If {
                        cur = &cur.else_body[0];
                        continue;
                    }
                    if !cur.else_body.is_empty() {
                        self.write(" else ");
                        self.block_open();
                        self.stmts(&cur.else_body);
                        self.block_close(false);
                    }
                    break;
                }
            } else {
                self.write(" else ");
                self.block_open();
                self.stmts(&s.else_body);
                self.block_close(false);
            }
        }
        self.line();
    }

    fn stmt_match(&mut self, s: &Stmt) {
        self.pad();
        self.write("match ");
        self.expr(s.expr.as_ref(), 0);
        self.write(" {\n");
        self.indent += 1;
        for arm in &s.arms {
            self.pad();
            match arm.pat {
                Pat::Wildcard => self.write("_"),
                Pat::Int => self.write(&arm.number.to_string()),
                Pat::Float => {
                    if !arm.text.is_empty() {
                        self.write(&arm.text);
                    } else {
                        self.write(&float_fallback(arm.real));
                    }
                }
                Pat::String => self.write(&escape_string(&arm.text)),
                Pat::Bool => self.write(if arm.boolean_value { "true" } else { "false" }),
                Pat::Variant => self.match_variant(arm),
            }
            self.write(" {\n");
            self.indent += 1;
            self.stmts(&arm.body);
            self.indent -= 1;
            self.writeln("}");
        }
        self.indent -= 1;
        self.writeln("}");
    }

    fn match_variant(&mut self, arm: &MatchArm) {
        self.write(&arm.name);
        let named = arm.field_names.iter().any(|n| !n.is_empty());
        if named {
            let n = arm.field_names.len().max(arm.binds.len());
            self.write("(");
            for i in 0..n {
                if i > 0 {
                    self.write(", ");
                }
                let fname = arm.field_names.get(i).map(String::as_str).unwrap_or("");
                let bind = arm.binds.get(i).map(String::as_str).unwrap_or("");
                if !fname.is_empty() {
                    self.write(fname);
                    if !bind.is_empty() {
                        self.write(": ");
                        self.write(bind);
                    }
                } else if !bind.is_empty() {
                    self.write(bind);
                }
            }
            self.write(")");
        } else if !arm.binds.is_empty() {
            self.write("(");
            for (i, b) in arm.binds.iter().enumerate() {
                if i > 0 {
                    self.write(", ");
                }
                self.write(b);
            }
            self.write(")");
        } else if !arm.field_names.is_empty() {
            self.write("(");
            self.write(")");
        }
    }

    fn expr(&mut self, e: Option<&Expr>, parent: i32) {
        let Some(e) = e else {
            return;
        };
        let prec = expr_prec(e);
        let wrap = prec < parent;
        if wrap {
            self.write("(");
        }
        self.expr_body(e);
        if wrap {
            self.write(")");
        }
    }

    fn expr_body(&mut self, e: &Expr) {
        match e.kind {
            ExprKind::Int => self.write(&e.number.to_string()),
            ExprKind::Float => {
                if !e.text.is_empty() {
                    self.write(&e.text);
                } else {
                    self.write(&float_fallback(e.real));
                }
            }
            ExprKind::String => self.write(&escape_string(&e.text)),
            ExprKind::Format => {
                self.write("f\"");
                for (n, kid) in e.kids.iter().enumerate() {
                    let lit = e.names.get(n).map(String::as_str) == Some("lit");
                    if lit && kid.kind == ExprKind::String {
                        self.write(&escape_format_body(&kid.text));
                    } else {
                        self.write("{");
                        self.expr(Some(kid), 0);
                        self.write("}");
                    }
                }
                self.write("\"");
            }
            ExprKind::Bool => self.write(if e.boolean_value { "true" } else { "false" }),
            ExprKind::Var => {
                if !e.module.is_empty() {
                    self.write(&e.module);
                    self.write(".");
                }
                self.write(&e.text);
                self.type_args(&e.type_args);
            }
            ExprKind::Unary => {
                self.write(&e.text);
                self.expr(e.kids.first(), UNARY);
            }
            ExprKind::Binary => {
                let prec = bin_prec(&e.text);
                self.expr(e.kids.first(), prec);
                self.write(" ");
                self.write(&e.text);
                self.write(" ");
                self.expr(e.kids.get(1), prec + 1);
            }
            ExprKind::Call => {
                let mut start = 0;
                if !e.text.is_empty() {
                    self.write(&e.text);
                } else if let Some(callee) = e.kids.first() {
                    self.expr(Some(callee), POSTFIX);
                    start = 1;
                }
                self.type_args(&e.type_args);
                self.write("(");
                for (i, kid) in e.kids.iter().enumerate().skip(start) {
                    if i > start {
                        self.write(", ");
                    }
                    self.expr(Some(kid), 0);
                }
                self.write(")");
            }
            ExprKind::MethodCall => {
                self.expr(e.kids.first(), POSTFIX);
                self.write(".");
                self.write(&e.text);
                self.type_args(&e.type_args);
                self.write("(");
                for (i, kid) in e.kids.iter().enumerate().skip(1) {
                    if i > 1 {
                        self.write(", ");
                    }
                    self.expr(Some(kid), 0);
                }
                self.write(")");
            }
            ExprKind::Member => {
                self.expr(e.kids.first(), POSTFIX);
                self.write(".");
                self.write(&e.text);
            }
            ExprKind::Index => {
                self.expr(e.kids.first(), POSTFIX);
                self.write("[");
                self.expr(e.kids.get(1), 0);
                self.write("]");
            }
            ExprKind::Array => {
                self.write("[");
                for (i, kid) in e.kids.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.expr(Some(kid), 0);
                }
                self.write("]");
            }
            ExprKind::Map => {
                self.write("{");
                let mut i = 0;
                while i + 1 < e.kids.len() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.expr(e.kids.get(i), 0);
                    self.write(": ");
                    self.expr(e.kids.get(i + 1), 0);
                    i += 2;
                }
                self.write("}");
            }
            ExprKind::StructLit => {
                self.write(&e.text);
                self.type_args(&e.type_args);
                self.write(" {");
                if !e.names.is_empty() {
                    self.write(" ");
                    for (i, name) in e.names.iter().enumerate() {
                        if i > 0 {
                            self.write(", ");
                        }
                        self.write(name);
                        self.write(": ");
                        if let Some(kid) = e.kids.get(i) {
                            self.expr(Some(kid), 0);
                        }
                    }
                    self.write(" ");
                }
                self.write("}");
            }
            ExprKind::Range => {
                self.expr(e.kids.first(), 1);
                if e.text.is_empty() {
                    self.write(if e.boolean_value { "..=" } else { ".." });
                } else {
                    self.write(&e.text);
                }
                self.expr(e.kids.get(1), 1);
            }
            ExprKind::Try => {
                self.write("try ");
                self.expr(e.kids.first(), UNARY);
            }
            ExprKind::Await => {
                self.write("await ");
                self.expr(e.kids.first(), UNARY);
            }
            ExprKind::Cast => {
                self.expr(e.kids.first(), POSTFIX);
                self.write(" as ");
                self.write(&e.text);
            }
            ExprKind::Is => {
                self.expr(e.kids.first(), POSTFIX);
                self.write(" is ");
                self.write(&e.text);
            }
            ExprKind::If => self.write_if_expr(e, true),
            ExprKind::New => {
                self.write("new ");
                self.write(&e.text);
            }
            ExprKind::NodePath => {
                self.write(if e.boolean_value { "%" } else { "$" });
                if plain_node_path(&e.text) {
                    self.write(&e.text);
                } else {
                    self.write(&escape_string(&e.text));
                }
            }
            ExprKind::Null => self.write("null"),
            ExprKind::Lambda => {
                if let Some(func) = e.lambda.as_ref() {
                    self.fn_inline(func);
                } else {
                    self.write("fn () {}");
                }
            }
        }
    }

    fn write_if_expr(&mut self, e: &Expr, first: bool) {
        if e.kind != ExprKind::If || e.kids.len() < 3 {
            return;
        }
        self.write(if first { "if " } else { "elif " });
        self.expr(e.kids.first(), 0);
        self.write(" { ");
        self.expr(e.kids.get(1), 0);
        self.write(" } ");
        if let Some(alt) = e.kids.get(2) {
            if alt.kind == ExprKind::If {
                self.write_if_expr(alt, false);
            } else {
                self.write("else { ");
                self.expr(Some(alt), 0);
                self.write(" }");
            }
        }
    }
}

fn expr_prec(e: &Expr) -> i32 {
    match e.kind {
        ExprKind::Binary => bin_prec(&e.text),
        ExprKind::Range => 0,
        ExprKind::Unary | ExprKind::Try | ExprKind::Await | ExprKind::New => UNARY,
        ExprKind::Call
        | ExprKind::MethodCall
        | ExprKind::Member
        | ExprKind::Index
        | ExprKind::Cast
        | ExprKind::Is => POSTFIX,
        _ => ATOM,
    }
}

fn bin_prec(op: &str) -> i32 {
    match op {
        "||" => OR,
        "&&" => AND,
        "==" | "!=" => EQ,
        "<" | "<=" | ">" | ">=" => CMP,
        "|" => BITOR,
        "^" => XOR,
        "&" => BITAND,
        "<<" | ">>" => SHIFT,
        "+" | "-" => ADD,
        "*" | "/" | "%" => MUL,
        _ => 0,
    }
}

fn plain_node_path(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    let bytes = path.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !is_ident_start(bytes[i] as char) {
            return false;
        }
        i += 1;
        while i < bytes.len() && is_ident_part(bytes[i] as char) {
            i += 1;
        }
        if i == bytes.len() {
            return true;
        }
        if bytes[i] != b'/' {
            return false;
        }
        i += 1;
    }
    false
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_part(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}

fn escape_string(s: &str) -> String {
    let mut o = String::from("\"");
    push_escaped(&mut o, s, false);
    o.push('"');
    o
}

fn escape_format_body(s: &str) -> String {
    let mut o = String::new();
    push_escaped(&mut o, s, true);
    o
}

fn push_escaped(o: &mut String, s: &str, braces: bool) {
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            '{' if braces => o.push_str("{{"),
            '}' if braces => o.push_str("}}"),
            c if (c as u32) < 32 => {
                let _ = std::fmt::Write::write_fmt(o, format_args!("\\x{:02x}", c as u32));
            }
            c => o.push(c),
        }
    }
}

fn float_fallback(n: f64) -> String {
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n.is_infinite() {
        return if n.is_sign_negative() {
            "-Infinity".to_string()
        } else {
            "Infinity".to_string()
        };
    }
    let s = format!("{n}");
    if let Some(stripped) = s.strip_suffix(".0") {
        stripped.to_string()
    } else {
        s
    }
}
