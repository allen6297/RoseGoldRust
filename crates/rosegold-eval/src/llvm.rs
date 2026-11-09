//! LLVM IR and clang link, matching `Llvm.java` / `LlvmLink.java` for programs
//! that lower. Godot library linking is not part of this path.

use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use rosegold_check::{type_head, Checker, FnRc};
use rosegold_syntax::FnDecl;

use crate::bytecode::{Const, Fn as BcFn, Op};

use super::compile;

const HEADER: &str = "\
; RoseGold LLVM snapshot - RGValue is { i8 kind, i64 bits, ptr }
; kind: 0 void, 1 bool, 2 int, 3 float, 4 string, 5 array, 6 range, 7 map, 8 struct, 9 enum type, 10 enum, 11 fn, 12 future, 13 signal, 14 object
; link with native/runtime.c

%RGValue = type { i8, i64, ptr }

declare void @rg_set_void(ptr)
declare void @rg_set_bool(ptr, i32)
declare void @rg_set_int(ptr, i64)
declare void @rg_set_float(ptr, double)
declare void @rg_set_string(ptr, ptr)
declare void @rg_copy(ptr, ptr)
declare void @rg_print(ptr, i32)
declare i64 @rg_to_exit(ptr)
declare i32 @rg_truthy(ptr)
declare void @rg_bin(ptr, ptr, ptr, i32)
declare void @rg_unary(ptr, ptr, i32)
declare void @rg_array(ptr, ptr, i32)
declare void @rg_range(ptr, ptr, ptr, i32)
declare void @rg_index_get(ptr, ptr, ptr)
declare void @rg_index_set(ptr, ptr, ptr)
declare void @rg_iter_items(ptr, ptr)
declare void @rg_len(ptr, ptr)
declare void @rg_assert(ptr, ptr, i32)
declare void @rg_test_ok(ptr)
declare void @rg_test_fail(ptr, ptr)
declare void @rg_test_summary(i32, i32)
declare void @rg_map(ptr, ptr, i32)
declare void @rg_method(ptr, ptr, i32, i32)
declare void @rg_struct(ptr, ptr, ptr, i32, ptr, i32)
declare void @rg_member(ptr, ptr, ptr)
declare void @rg_field_set(ptr, ptr, ptr)
declare void @rg_register_method(ptr, ptr, ptr)
declare void @rg_register_parent(ptr, ptr)
declare void @rg_call_method(ptr, ptr, i32, ptr)
declare void @rg_enum_type(ptr, ptr)
declare void @rg_register_enum(ptr, ptr, i32, ptr)
declare void @rg_match_lit(ptr, ptr, ptr)
declare void @rg_match_var(ptr, ptr, ptr)
declare void @rg_payload(ptr, ptr, i32, ptr)
declare void @rg_fn_ref(ptr, ptr, i32)
declare void @rg_closure(ptr, ptr, i32, ptr, i32)
declare void @rg_call_val(ptr, ptr, i32)
declare void @rg_throw(ptr)
declare i32 @rg_has_throw()
declare void @rg_catch(ptr)
declare void @rg_spawn(ptr, ptr, ptr, i32)
declare void @rg_await(ptr, ptr)
declare void @rg_signal(ptr, ptr, i32)
declare void @rg_godot_singleton(ptr, ptr)
declare void @rg_godot_static(ptr, ptr, ptr, ptr, i32)
declare void @rg_register_type_signal(ptr, ptr, i32)
declare void @rg_flush_deferred()
declare void @rg_host_call(ptr, ptr, ptr, ptr, i32)
declare void @rg_set_argv(i32, ptr)
declare void @rg_godot_bind(ptr, i64)
declare void @rg_godot_call(ptr, ptr, ptr, i32)
declare void @rg_register_local(ptr, ptr)

";

struct Selection {
    fns: Vec<String>,
    methods: Vec<String>,
    mod_fns: Vec<String>,
}

struct Lam {
    proto: usize,
    func: FnRc,
    code: Rc<BcFn>,
    name: String,
    ncopy: i32,
}

pub fn emit(checker: &Checker, test: bool) -> String {
    let compiled = compile::attach(checker);
    let sel = select(checker, &compiled);
    let lams = collect_lambdas(checker, &compiled, &sel);
    let mut strings = Vec::new();
    let entry = compile::entry_fns(checker);
    for func in &entry {
        if let Some(code) = code_of(&compiled, func) {
            if sel.fns.iter().any(|n| n == &func.name) {
                collect_strings(code, &mut strings);
            }
        }
    }
    for key in &sel.methods {
        if let Some(func) = type_method(checker, key) {
            if let Some(code) = code_of(&compiled, &func) {
                collect_strings(code, &mut strings);
            }
        }
        intern(&mut strings, &method_type(key));
        intern(&mut strings, &method_name(key));
    }
    for key in &sel.mod_fns {
        if let Some(func) = module_fn_key(checker, key) {
            if let Some(code) = code_of(&compiled, &func) {
                collect_strings(code, &mut strings);
            }
        }
    }
    let layouts = layouts(checker);
    for (ty, fields) in &layouts {
        intern(&mut strings, ty);
        for field in fields {
            intern(&mut strings, field);
        }
    }
    for (child, parent) in ordered_parents(checker) {
        intern(&mut strings, &child);
        intern(&mut strings, &type_head(&parent));
    }
    intern_type_signals(checker, &mut strings);
    intern_enums(checker, &mut strings);
    if test {
        for name in test_fns(checker, &compiled, &sel) {
            intern(&mut strings, &name);
        }
    }
    for lam in &lams {
        collect_strings(&lam.code, &mut strings);
    }
    for key in &sel.methods {
        if let Some(func) = type_method(checker, key) {
            if godot_stub(&func) {
                intern(&mut strings, &method_name(key));
            }
        }
    }
    for (ty, name, func) in ordered_static(checker) {
        let _ = (ty, name);
        if let Some(code) = code_of(&compiled, &func) {
            if !godot_stub(&func) {
                collect_strings(code, &mut strings);
            }
        }
    }
    let mut ir = String::from(HEADER);
    for (i, s) in strings.iter().enumerate() {
        emit_string_global(&mut ir, i, s);
    }
    if !strings.is_empty() {
        ir.push('\n');
    }
    let mut used = Vec::new();
    let mut used_set = HashSet::new();
    let note_structs = |code: &BcFn, used: &mut Vec<String>, seen: &mut HashSet<String>| {
        for inst in &code.insts {
            if inst.op == Op::Struct && !inst.s.is_empty() && seen.insert(inst.s.clone()) {
                used.push(inst.s.clone());
            }
        }
    };
    for func in &entry {
        if sel.fns.iter().any(|n| n == &func.name) {
            if let Some(code) = code_of(&compiled, func) {
                note_structs(code, &mut used, &mut used_set);
            }
        }
    }
    for key in &sel.methods {
        if let Some(func) = type_method(checker, key) {
            if let Some(code) = code_of(&compiled, &func) {
                note_structs(code, &mut used, &mut used_set);
            }
        }
    }
    for (_, _, func) in ordered_static(checker) {
        if let Some(code) = code_of(&compiled, &func) {
            if !godot_stub(&func) {
                note_structs(code, &mut used, &mut used_set);
            }
        }
    }
    for key in &sel.mod_fns {
        if let Some(func) = module_fn_key(checker, key) {
            if let Some(code) = code_of(&compiled, &func) {
                note_structs(code, &mut used, &mut used_set);
            }
        }
    }
    for lam in &lams {
        note_structs(&lam.code, &mut used, &mut used_set);
    }
    for ty in &used {
        emit_field_table(
            &mut ir,
            ty,
            layouts.get(ty).map(Vec::as_slice).unwrap_or(&[]),
            &mut strings,
        );
    }
    if !used.is_empty() {
        ir.push('\n');
    }
    for en in ordered_enums(checker) {
        emit_enum_tables(&mut ir, &en, &mut strings);
    }
    let mut dbg = Dbg::new(&checker.file);
    let lam_by_ptr: HashMap<usize, &Lam> = lams.iter().map(|l| (l.proto, l)).collect();
    for func in &entry {
        if !func.module.is_empty() {
            continue;
        }
        match code_of(&compiled, func) {
            Some(code) if sel.fns.iter().any(|n| n == &func.name) => {
                emit_fn(
                    &mut ir,
                    code,
                    &mut strings,
                    &mangle(&func.name),
                    checker,
                    &layouts,
                    &lam_by_ptr,
                    code.arity as i32,
                    &mut dbg,
                    Some(func),
                );
                ir.push('\n');
            }
            Some(_) => {
                ir.push_str(&format!("; fn {} {{ not lowered to llvm }}\n\n", func.name));
            }
            None => {
                ir.push_str(&format!("; fn {} {{ tree-walk }}\n\n", func.name));
            }
        }
    }
    for key in &sel.mod_fns {
        let Some(func) = module_fn_key(checker, key) else {
            continue;
        };
        let Some(code) = code_of(&compiled, &func) else {
            continue;
        };
        let (module, name) = split_key(key);
        emit_fn(
            &mut ir,
            code,
            &mut strings,
            &mangle_mod(&module, &name),
            checker,
            &layouts,
            &lam_by_ptr,
            code.arity as i32,
            &mut dbg,
            Some(&func),
        );
        ir.push('\n');
    }
    for key in &sel.methods {
        let Some(func) = type_method(checker, key) else {
            continue;
        };
        if godot_stub(&func) {
            emit_godot_forward(&mut ir, &method_type(key), &method_name(key), &mut strings);
        } else if let Some(code) = code_of(&compiled, &func) {
            emit_fn(
                &mut ir,
                code,
                &mut strings,
                &mangle_method(&method_type(key), &method_name(key)),
                checker,
                &layouts,
                &lam_by_ptr,
                code.arity as i32,
                &mut dbg,
                Some(&func),
            );
        } else {
            continue;
        }
        ir.push('\n');
    }
    for (ty, name, func) in ordered_static(checker) {
        let Some(code) = code_of(&compiled, &func) else {
            continue;
        };
        if godot_stub(&func) || !ops_ok(code, checker, &compiled) {
            continue;
        }
        emit_fn(
            &mut ir,
            code,
            &mut strings,
            &mangle_method(&ty, &name),
            checker,
            &layouts,
            &lam_by_ptr,
            code.arity as i32,
            &mut dbg,
            Some(&func),
        );
        ir.push('\n');
    }
    for lam in &lams {
        emit_fn(
            &mut ir,
            &lam.code,
            &mut strings,
            &lam.name,
            checker,
            &layouts,
            &lam_by_ptr,
            lam.ncopy,
            &mut dbg,
            Some(&lam.func),
        );
        ir.push('\n');
    }
    let need_init = !sel.methods.is_empty()
        || !ordered_enums(checker).is_empty()
        || !ordered_parents(checker).is_empty()
        || has_type_signals(checker);
    if need_init {
        ir.push_str("define void @rg_init_methods() {\nentry:\n");
        for key in &sel.methods {
            let t = intern(&mut strings, &method_type(key));
            let m = intern(&mut strings, &method_name(key));
            ir.push_str(&format!(
                "  call void @rg_register_method(ptr @str{t}, ptr @str{m}, ptr @{})\n",
                mangle_method(&method_type(key), &method_name(key))
            ));
        }
        for (child, parent) in ordered_parents(checker) {
            let parent = type_head(&parent);
            if parent.is_empty() {
                continue;
            }
            let c = intern(&mut strings, &child);
            let p = intern(&mut strings, &parent);
            ir.push_str(&format!(
                "  call void @rg_register_parent(ptr @str{c}, ptr @str{p})\n"
            ));
        }
        emit_type_signals(&mut ir, checker, &mut strings);
        for en in ordered_enums(checker) {
            emit_enum_regs(&mut ir, &en, &mut strings);
        }
        ir.push_str("  ret void\n}\n\n");
    }
    if test {
        emit_test_main(&mut ir, checker, &compiled, &sel, &mut strings, need_init);
    } else if sel.fns.iter().any(|n| n == "main") {
        ir.push_str("define i32 @main(i32 %argc, ptr %argv) {\nentry:\n");
        ir.push_str("  call void @rg_set_argv(i32 %argc, ptr %argv)\n");
        ir.push_str("  %ret = alloca %RGValue\n");
        ir.push_str("  call void @rg_set_void(ptr %ret)\n");
        if need_init {
            ir.push_str("  call void @rg_init_methods()\n");
        }
        if is_async(checker, "main") {
            ir.push_str("  %fut = alloca %RGValue\n");
            ir.push_str("  call void @rg_set_void(ptr %fut)\n");
            ir.push_str("  call void @rg_spawn(ptr %fut, ptr @rg_fn_main, ptr null, i32 0)\n");
            ir.push_str("  call void @rg_await(ptr %ret, ptr %fut)\n");
        } else {
            ir.push_str("  call void @rg_fn_main(ptr %ret, ptr null, i32 0)\n");
        }
        ir.push_str("  call void @rg_flush_deferred()\n");
        ir.push_str("  %code = call i64 @rg_to_exit(ptr %ret)\n");
        ir.push_str("  %e = trunc i64 %code to i32\n");
        ir.push_str("  ret i32 %e\n}\n");
    }
    dbg.emit(&mut ir);
    ir
}

fn emit_fn(
    ir: &mut String,
    func: &BcFn,
    strings: &mut Vec<String>,
    llvm_name: &str,
    checker: &Checker,
    layouts: &IndexMap<String, Vec<String>>,
    lams: &HashMap<usize, &Lam>,
    ncopy: i32,
    dbg: &mut Dbg,
    src: Option<&FnDecl>,
) {
    let nregs = func.nregs.max(1);
    let n = func.insts.len();
    let start = block_starts(func);
    let sp = dbg.subprogram(
        src.map(|f| f.name.as_str()).unwrap_or(llvm_name),
        src.map(|f| f.file.as_str()).unwrap_or(&checker.file),
        src.map(|f| f.span.line).unwrap_or(1),
    );
    let src_line = src.map(|f| f.span.line).filter(|l| *l >= 1).unwrap_or(1);
    ir.push_str(&format!(
        "define void @{llvm_name}(ptr %out, ptr %args, i32 %argc) !dbg !{sp} {{\n"
    ));
    ir.push_str("entry:\n");
    ir.push_str(&format!("  %regs = alloca [{nregs} x %RGValue]\n"));
    attach_dbg(ir, dbg, sp, src_line, 1);
    let copy = ncopy.max(0).min(nregs as i32);
    for i in 0..copy {
        ir.push_str(&format!(
            "  %arg{i} = getelementptr %RGValue, ptr %args, i32 {i}\n"
        ));
        attach_dbg(ir, dbg, sp, src_line, 1);
        ir.push_str(&format!(
            "  %dst{i} = getelementptr inbounds [{nregs} x %RGValue], ptr %regs, i32 0, i32 {i}\n"
        ));
        attach_dbg(ir, dbg, sp, src_line, 1);
        ir.push_str(&format!("  call void @rg_copy(ptr %dst{i}, ptr %arg{i})\n"));
        attach_dbg(ir, dbg, sp, src_line, 1);
    }
    ir.push_str("  br label %b0\n");
    attach_dbg(ir, dbg, sp, src_line, 1);
    let mut tmp = 0i32;
    let mut propagate = false;
    let mut terminated = true;
    for i in 0..n {
        if start[i] {
            if !terminated {
                ir.push_str(&format!("  br label %b{i}\n"));
                attach_dbg(ir, dbg, sp, src_line, 1);
            }
            ir.push_str(&format!("b{i}:\n"));
            terminated = false;
        }
        if terminated {
            continue;
        }
        let inst = &func.insts[i];
        if inst.op == Op::Debug {
            continue;
        }
        let from = ir.len();
        terminated = emit_inst(
            ir,
            func,
            i,
            nregs,
            &mut tmp,
            strings,
            checker,
            layouts,
            lams,
            &mut propagate,
        );
        attach_new_lines(ir, from, dbg, sp, inst.line, inst.col);
        if !terminated && may_throw(inst.op) {
            let from = ir.len();
            terminated = emit_throw_edge(ir, func, i, &mut tmp, &mut propagate);
            attach_new_lines(ir, from, dbg, sp, inst.line, inst.col);
        }
    }
    if start[n] {
        if !terminated {
            ir.push_str(&format!("  br label %b{n}\n"));
            attach_dbg(ir, dbg, sp, src_line, 1);
        }
        ir.push_str(&format!("b{n}:\n"));
        terminated = false;
    }
    if !terminated {
        ir.push_str("  call void @rg_set_void(ptr %out)\n");
        attach_dbg(ir, dbg, sp, src_line, 1);
        ir.push_str("  ret void\n");
        attach_dbg(ir, dbg, sp, src_line, 1);
    }
    emit_catch_pads(ir, func, nregs, &mut tmp, propagate, dbg, sp, src_line);
    ir.push_str("}\n");
}

fn emit_inst(
    ir: &mut String,
    func: &BcFn,
    pc: usize,
    nregs: usize,
    tmp: &mut i32,
    strings: &mut Vec<String>,
    checker: &Checker,
    layouts: &IndexMap<String, Vec<String>>,
    lams: &HashMap<usize, &Lam>,
    propagate: &mut bool,
) -> bool {
    let inst = &func.insts[pc];
    match inst.op {
        Op::LoadConst => {
            let dest = slot(ir, nregs, inst.a, tmp);
            let k = &func.constants[inst.b as usize];
            match k {
                Const::Bool(b) => ir.push_str(&format!(
                    "  call void @rg_set_bool(ptr {dest}, i32 {})\n",
                    if *b { 1 } else { 0 }
                )),
                Const::Int(n) => {
                    ir.push_str(&format!("  call void @rg_set_int(ptr {dest}, i64 {n})\n"))
                }
                Const::Float(n) => ir.push_str(&format!(
                    "  call void @rg_set_float(ptr {dest}, double {})\n",
                    llvm_double(*n)
                )),
                Const::Str(s) => {
                    let id = intern(strings, s);
                    ir.push_str(&format!(
                        "  call void @rg_set_string(ptr {dest}, ptr @str{id})\n"
                    ));
                }
                Const::Void => ir.push_str(&format!("  call void @rg_set_void(ptr {dest})\n")),
            }
            false
        }
        Op::Move => {
            let dst = slot(ir, nregs, inst.a, tmp);
            let src = slot(ir, nregs, inst.b, tmp);
            ir.push_str(&format!("  call void @rg_copy(ptr {dst}, ptr {src})\n"));
            false
        }
        Op::Bin => {
            let d = slot(ir, nregs, inst.a, tmp);
            let l = slot(ir, nregs, inst.b, tmp);
            let r = slot(ir, nregs, inst.c, tmp);
            ir.push_str(&format!(
                "  call void @rg_bin(ptr {d}, ptr {l}, ptr {r}, i32 {})\n",
                bin_op_id(&inst.s)
            ));
            false
        }
        Op::Unary => {
            let d = slot(ir, nregs, inst.a, tmp);
            let s = slot(ir, nregs, inst.b, tmp);
            ir.push_str(&format!(
                "  call void @rg_unary(ptr {d}, ptr {s}, i32 {})\n",
                unary_op_id(&inst.s)
            ));
            false
        }
        Op::Truthy => {
            let src = slot(ir, nregs, inst.b, tmp);
            let t = bump(tmp);
            ir.push_str(&format!("  %t{t} = call i32 @rg_truthy(ptr {src})\n"));
            let dest = slot(ir, nregs, inst.a, tmp);
            ir.push_str(&format!(
                "  call void @rg_set_bool(ptr {dest}, i32 %t{t})\n"
            ));
            false
        }
        Op::Call => {
            let dp = slot(ir, nregs, inst.a, tmp);
            let bp = slot(ir, nregs, inst.b, tmp);
            if inst.s == "print" {
                ir.push_str(&format!(
                    "  call void @rg_print(ptr {bp}, i32 {})\n",
                    inst.c
                ));
                ir.push_str(&format!("  call void @rg_set_void(ptr {dp})\n"));
            } else if inst.s == "len" {
                ir.push_str(&format!("  call void @rg_len(ptr {dp}, ptr {bp})\n"));
            } else if inst.s == "assert" {
                ir.push_str(&format!(
                    "  call void @rg_assert(ptr {dp}, ptr {bp}, i32 {})\n",
                    inst.c
                ));
            } else if is_host_call(&inst.module, &inst.s) {
                let mid = intern(strings, &inst.module);
                let nid = intern(strings, &inst.s);
                ir.push_str(&format!(
                    "  call void @rg_host_call(ptr {dp}, ptr @str{mid}, ptr @str{nid}, ptr {bp}, i32 {})\n",
                    inst.c
                ));
            } else if !inst.module.is_empty() {
                let name = mangle_mod(&inst.module, &inst.s);
                if callee_async(checker, &inst.module, &inst.s) {
                    ir.push_str(&format!(
                        "  call void @rg_spawn(ptr {dp}, ptr @{name}, ptr {bp}, i32 {})\n",
                        inst.c
                    ));
                } else {
                    ir.push_str(&format!(
                        "  call void @{name}(ptr {dp}, ptr {bp}, i32 {})\n",
                        inst.c
                    ));
                }
            } else if callee_async(checker, "", &inst.s) {
                ir.push_str(&format!(
                    "  call void @rg_spawn(ptr {dp}, ptr @{}, ptr {bp}, i32 {})\n",
                    mangle(&inst.s),
                    inst.c
                ));
            } else {
                ir.push_str(&format!(
                    "  call void @{}(ptr {dp}, ptr {bp}, i32 {})\n",
                    mangle(&inst.s),
                    inst.c
                ));
            }
            false
        }
        Op::Array => {
            let dest = slot(ir, nregs, inst.a, tmp);
            if inst.c <= 0 {
                ir.push_str(&format!(
                    "  call void @rg_array(ptr {dest}, ptr null, i32 0)\n"
                ));
            } else {
                let base = slot(ir, nregs, inst.b, tmp);
                ir.push_str(&format!(
                    "  call void @rg_array(ptr {dest}, ptr {base}, i32 {})\n",
                    inst.c
                ));
            }
            false
        }
        Op::Range => {
            let d = slot(ir, nregs, inst.a, tmp);
            let a = slot(ir, nregs, inst.b, tmp);
            let b = slot(ir, nregs, inst.c, tmp);
            let flag = if inst.s == "..=" { 1 } else { 0 };
            ir.push_str(&format!(
                "  call void @rg_range(ptr {d}, ptr {a}, ptr {b}, i32 {flag})\n"
            ));
            false
        }
        Op::IndexGet => {
            let d = slot(ir, nregs, inst.a, tmp);
            let a = slot(ir, nregs, inst.b, tmp);
            let b = slot(ir, nregs, inst.c, tmp);
            ir.push_str(&format!(
                "  call void @rg_index_get(ptr {d}, ptr {a}, ptr {b})\n"
            ));
            false
        }
        Op::IndexSet => {
            let d = slot(ir, nregs, inst.a, tmp);
            let a = slot(ir, nregs, inst.b, tmp);
            let b = slot(ir, nregs, inst.c, tmp);
            ir.push_str(&format!(
                "  call void @rg_index_set(ptr {d}, ptr {a}, ptr {b})\n"
            ));
            false
        }
        Op::IterItems => {
            let d = slot(ir, nregs, inst.a, tmp);
            let a = slot(ir, nregs, inst.b, tmp);
            ir.push_str(&format!("  call void @rg_iter_items(ptr {d}, ptr {a})\n"));
            false
        }
        Op::Len => {
            let d = slot(ir, nregs, inst.a, tmp);
            let a = slot(ir, nregs, inst.b, tmp);
            ir.push_str(&format!("  call void @rg_len(ptr {d}, ptr {a})\n"));
            false
        }
        Op::Map => {
            let dest = slot(ir, nregs, inst.a, tmp);
            if inst.c <= 0 {
                ir.push_str(&format!(
                    "  call void @rg_map(ptr {dest}, ptr null, i32 0)\n"
                ));
            } else {
                let base = slot(ir, nregs, inst.b, tmp);
                ir.push_str(&format!(
                    "  call void @rg_map(ptr {dest}, ptr {base}, i32 {})\n",
                    inst.c
                ));
            }
            false
        }
        Op::Method => {
            let d = slot(ir, nregs, inst.a, tmp);
            let b = slot(ir, nregs, inst.b, tmp);
            let op = method_op_id(&inst.s);
            if op >= 0 {
                ir.push_str(&format!(
                    "  call void @rg_method(ptr {d}, ptr {b}, i32 {}, i32 {op})\n",
                    inst.c
                ));
            } else {
                let id = intern(strings, &inst.s);
                ir.push_str(&format!(
                    "  call void @rg_call_method(ptr {d}, ptr {b}, i32 {}, ptr @str{id})\n",
                    inst.c
                ));
            }
            false
        }
        Op::Super => {
            let d = slot(ir, nregs, inst.a, tmp);
            let b = slot(ir, nregs, inst.b, tmp);
            let (defined, async_fn) = super_hit(checker, &inst.module, &inst.s);
            let defined = if defined.is_empty() {
                inst.module.clone()
            } else {
                defined
            };
            let name = mangle_method(&defined, &inst.s);
            if async_fn {
                ir.push_str(&format!(
                    "  call void @rg_spawn(ptr {d}, ptr @{name}, ptr {b}, i32 {})\n",
                    inst.c
                ));
            } else {
                ir.push_str(&format!(
                    "  call void @{name}(ptr {d}, ptr {b}, i32 {})\n",
                    inst.c
                ));
            }
            false
        }
        Op::Struct => {
            let dest = slot(ir, nregs, inst.a, tmp);
            let type_id = intern(strings, &inst.s);
            let is_data = if checker.is_data_type(&inst.s) { 1 } else { 0 };
            let fields = layouts.get(&inst.s).map(Vec::as_slice).unwrap_or(&[]);
            let field_ptr = if fields.is_empty() {
                "null".to_string()
            } else {
                format!("@fields_{}", ident(&inst.s))
            };
            if inst.c <= 0 {
                ir.push_str(&format!(
                    "  call void @rg_struct(ptr {dest}, ptr @str{type_id}, ptr null, i32 0, ptr {field_ptr}, i32 {is_data})\n"
                ));
            } else {
                let base = slot(ir, nregs, inst.b, tmp);
                ir.push_str(&format!(
                    "  call void @rg_struct(ptr {dest}, ptr @str{type_id}, ptr {base}, i32 {}, ptr {field_ptr}, i32 {is_data})\n",
                    inst.c
                ));
            }
            false
        }
        Op::Member => {
            let d = slot(ir, nregs, inst.a, tmp);
            let b = slot(ir, nregs, inst.b, tmp);
            let id = intern(strings, &inst.s);
            ir.push_str(&format!(
                "  call void @rg_member(ptr {d}, ptr {b}, ptr @str{id})\n"
            ));
            false
        }
        Op::FieldSet => {
            let d = slot(ir, nregs, inst.a, tmp);
            let b = slot(ir, nregs, inst.b, tmp);
            let id = intern(strings, &inst.s);
            ir.push_str(&format!(
                "  call void @rg_field_set(ptr {d}, ptr @str{id}, ptr {b})\n"
            ));
            false
        }
        Op::LoadEnum => {
            let dest = slot(ir, nregs, inst.a, tmp);
            let id = intern(strings, &inst.s);
            ir.push_str(&format!(
                "  call void @rg_enum_type(ptr {dest}, ptr @str{id})\n"
            ));
            false
        }
        Op::LoadSignal => {
            let dest = slot(ir, nregs, inst.a, tmp);
            let id = intern(strings, &inst.s);
            let arity = checker
                .signal_arity
                .borrow()
                .get(&inst.s)
                .copied()
                .unwrap_or(0);
            ir.push_str(&format!(
                "  call void @rg_signal(ptr {dest}, ptr @str{id}, i32 {arity})\n"
            ));
            false
        }
        Op::LoadSingleton => {
            let dest = slot(ir, nregs, inst.a, tmp);
            let id = intern(strings, &inst.s);
            ir.push_str(&format!(
                "  call void @rg_godot_singleton(ptr {dest}, ptr @str{id})\n"
            ));
            false
        }
        Op::Static => {
            let d = slot(ir, nregs, inst.a, tmp);
            let b = slot(ir, nregs, inst.b, tmp);
            let cls = intern(strings, &inst.module);
            let name = intern(strings, &inst.s);
            ir.push_str(&format!(
                "  call void @rg_godot_static(ptr {d}, ptr @str{cls}, ptr @str{name}, ptr {b}, i32 {})\n",
                inst.c
            ));
            false
        }
        Op::MatchLit => {
            let d = slot(ir, nregs, inst.a, tmp);
            let a = slot(ir, nregs, inst.b, tmp);
            let b = slot(ir, nregs, inst.c, tmp);
            ir.push_str(&format!(
                "  call void @rg_match_lit(ptr {d}, ptr {a}, ptr {b})\n"
            ));
            false
        }
        Op::MatchVar => {
            let d = slot(ir, nregs, inst.a, tmp);
            let b = slot(ir, nregs, inst.b, tmp);
            let id = intern(strings, &inst.s);
            ir.push_str(&format!(
                "  call void @rg_match_var(ptr {d}, ptr {b}, ptr @str{id})\n"
            ));
            false
        }
        Op::Payload => {
            let d = slot(ir, nregs, inst.a, tmp);
            let b = slot(ir, nregs, inst.b, tmp);
            let field = if inst.s.is_empty() {
                "null".to_string()
            } else {
                format!("@str{}", intern(strings, &inst.s))
            };
            ir.push_str(&format!(
                "  call void @rg_payload(ptr {d}, ptr {b}, i32 {}, ptr {field})\n",
                inst.c
            ));
            false
        }
        Op::LoadFn => {
            let dest = slot(ir, nregs, inst.a, tmp);
            let arity = checker
                .fns
                .borrow()
                .get(&inst.s)
                .map(|f| f.params.len())
                .unwrap_or(0);
            ir.push_str(&format!(
                "  call void @rg_fn_ref(ptr {dest}, ptr @{}, i32 {arity})\n",
                mangle(&inst.s)
            ));
            false
        }
        Op::Closure => {
            let dest = slot(ir, nregs, inst.a, tmp);
            let ptr = inst
                .proto
                .as_ref()
                .map(|p| Rc::as_ptr(p) as usize)
                .unwrap_or(0);
            let (name, arity) = lams
                .get(&ptr)
                .map(|l| (l.name.clone(), l.code.arity as i32))
                .unwrap_or_else(|| ("rg_fn_lam0".to_string(), 0));
            if inst.c <= 0 {
                ir.push_str(&format!(
                    "  call void @rg_closure(ptr {dest}, ptr @{name}, i32 {arity}, ptr null, i32 0)\n"
                ));
            } else {
                let base = slot(ir, nregs, inst.b, tmp);
                ir.push_str(&format!(
                    "  call void @rg_closure(ptr {dest}, ptr @{name}, i32 {arity}, ptr {base}, i32 {})\n",
                    inst.c
                ));
            }
            false
        }
        Op::CallVal => {
            let d = slot(ir, nregs, inst.a, tmp);
            let b = slot(ir, nregs, inst.b, tmp);
            ir.push_str(&format!(
                "  call void @rg_call_val(ptr {d}, ptr {b}, i32 {})\n",
                inst.c
            ));
            false
        }
        Op::Await => {
            let d = slot(ir, nregs, inst.a, tmp);
            let b = slot(ir, nregs, inst.b, tmp);
            ir.push_str(&format!("  call void @rg_await(ptr {d}, ptr {b})\n"));
            false
        }
        Op::Throw => {
            let dest = slot(ir, nregs, inst.a, tmp);
            ir.push_str(&format!("  call void @rg_throw(ptr {dest})\n"));
            ir.push_str(&format!(
                "  br label {}\n",
                throw_target(func, pc, propagate)
            ));
            true
        }
        Op::Jump => {
            ir.push_str(&format!("  br label %b{}\n", inst.a));
            true
        }
        Op::JumpF | Op::JumpT => {
            let src = slot(ir, nregs, inst.a, tmp);
            let t = bump(tmp);
            let c = bump(tmp);
            ir.push_str(&format!("  %t{t} = call i32 @rg_truthy(ptr {src})\n"));
            ir.push_str(&format!("  %t{c} = icmp ne i32 %t{t}, 0\n"));
            let fall = pc + 1;
            if inst.op == Op::JumpF {
                ir.push_str(&format!(
                    "  br i1 %t{c}, label %b{fall}, label %b{}\n",
                    inst.b
                ));
            } else {
                ir.push_str(&format!(
                    "  br i1 %t{c}, label %b{}, label %b{fall}\n",
                    inst.b
                ));
            }
            true
        }
        Op::Ret => {
            let src = slot(ir, nregs, inst.a, tmp);
            ir.push_str(&format!("  call void @rg_copy(ptr %out, ptr {src})\n"));
            ir.push_str("  ret void\n");
            true
        }
        Op::RetVoid => {
            ir.push_str("  call void @rg_set_void(ptr %out)\n");
            ir.push_str("  ret void\n");
            true
        }
        Op::Debug => false,
    }
}

fn slot(ir: &mut String, nregs: usize, index: i32, tmp: &mut i32) -> String {
    let id = bump(tmp);
    ir.push_str(&format!(
        "  %slot{id} = getelementptr inbounds [{nregs} x %RGValue], ptr %regs, i32 0, i32 {index}\n"
    ));
    format!("%slot{id}")
}

fn bump(tmp: &mut i32) -> i32 {
    let id = *tmp;
    *tmp += 1;
    id
}

fn block_starts(func: &BcFn) -> Vec<bool> {
    let n = func.insts.len();
    let mut start = vec![false; n + 1];
    start[0] = true;
    for (i, inst) in func.insts.iter().enumerate() {
        match inst.op {
            Op::Jump => mark(&mut start, inst.a),
            Op::JumpF | Op::JumpT => {
                mark(&mut start, inst.b);
                mark(&mut start, (i + 1) as i32);
            }
            op if may_throw(op) => mark(&mut start, (i + 1) as i32),
            _ => {}
        }
    }
    for h in &func.handlers {
        mark(&mut start, h.handler as i32);
    }
    start
}

fn mark(start: &mut [bool], pc: i32) {
    if pc >= 0 && (pc as usize) < start.len() {
        start[pc as usize] = true;
    }
}

fn may_throw(op: Op) -> bool {
    matches!(
        op,
        Op::Call | Op::CallVal | Op::Method | Op::Await | Op::Super
    )
}

fn throw_target(func: &BcFn, pc: usize, propagate: &mut bool) -> String {
    for (i, h) in func.handlers.iter().enumerate() {
        if pc >= h.start && pc < h.end {
            return format!("%catch{i}");
        }
    }
    *propagate = true;
    "%propagate".to_string()
}

fn emit_throw_edge(
    ir: &mut String,
    func: &BcFn,
    pc: usize,
    tmp: &mut i32,
    propagate: &mut bool,
) -> bool {
    let t = bump(tmp);
    let c = bump(tmp);
    ir.push_str(&format!("  %t{t} = call i32 @rg_has_throw()\n"));
    ir.push_str(&format!("  %t{c} = icmp ne i32 %t{t}, 0\n"));
    ir.push_str(&format!(
        "  br i1 %t{c}, label {}, label %b{}\n",
        throw_target(func, pc, propagate),
        pc + 1
    ));
    true
}

fn emit_catch_pads(
    ir: &mut String,
    func: &BcFn,
    nregs: usize,
    tmp: &mut i32,
    propagate: bool,
    dbg: &mut Dbg,
    sp: i32,
    src_line: u32,
) {
    for (i, h) in func.handlers.iter().enumerate() {
        ir.push_str(&format!("catch{i}:\n"));
        if h.dest >= 0 {
            let from = ir.len();
            let dest = slot(ir, nregs, h.dest, tmp);
            ir.push_str(&format!("  call void @rg_catch(ptr {dest})\n"));
            attach_new_lines(ir, from, dbg, sp, src_line, 1);
        }
        ir.push_str(&format!("  br label %b{}\n", h.handler));
        attach_dbg(ir, dbg, sp, src_line, 1);
    }
    if propagate {
        ir.push_str("propagate:\n");
        ir.push_str("  ret void\n");
        attach_dbg(ir, dbg, sp, src_line, 1);
    }
}

fn select(checker: &Checker, compiled: &HashMap<String, Option<Rc<BcFn>>>) -> Selection {
    let mut sel = Selection {
        fns: Vec::new(),
        methods: Vec::new(),
        mod_fns: Vec::new(),
    };
    for func in compile::entry_fns(checker) {
        if let Some(code) = code_of(compiled, &func) {
            if ops_ok(code, checker, compiled) {
                sel.fns.push(func.name.clone());
            }
        }
    }
    for (key, func) in ordered_method_pairs(checker) {
        if let Some(code) = code_of(compiled, &func) {
            if ops_ok(code, checker, compiled) {
                sel.methods.push(key);
            }
        }
    }
    loop {
        let before = sel.mod_fns.len();
        let names = sel.fns.clone();
        for name in &names {
            if let Some(func) = checker.fns.borrow().get(name).cloned() {
                if let Some(code) = code_of(compiled, &func) {
                    add_module_refs(code, &mut sel, checker, compiled);
                }
            }
        }
        let methods = sel.methods.clone();
        for key in &methods {
            if let Some(func) = type_method(checker, key) {
                if let Some(code) = code_of(compiled, &func) {
                    add_module_refs(code, &mut sel, checker, compiled);
                }
            }
        }
        let mods = sel.mod_fns.clone();
        for key in &mods {
            if let Some(func) = module_fn_key(checker, key) {
                if let Some(code) = code_of(compiled, &func) {
                    add_module_refs(code, &mut sel, checker, compiled);
                }
            }
        }
        let mut changed = sel.mod_fns.len() != before;
        let mut next_fns = Vec::new();
        for name in &sel.fns {
            let ok = checker.fns.borrow().get(name).cloned().is_some_and(|func| {
                code_of(compiled, &func).is_some_and(|code| calls_ok(code, &sel, checker, compiled))
            });
            if ok {
                next_fns.push(name.clone());
            } else {
                changed = true;
            }
        }
        let mut next_methods = Vec::new();
        for key in &sel.methods {
            let ok = type_method(checker, key).is_some_and(|func| {
                code_of(compiled, &func).is_some_and(|code| calls_ok(code, &sel, checker, compiled))
            });
            if ok {
                next_methods.push(key.clone());
            } else {
                changed = true;
            }
        }
        let mut next_mods = Vec::new();
        for key in &sel.mod_fns {
            let ok = module_fn_key(checker, key).is_some_and(|func| {
                code_of(compiled, &func).is_some_and(|code| calls_ok(code, &sel, checker, compiled))
            });
            if ok {
                next_mods.push(key.clone());
            } else {
                changed = true;
            }
        }
        sel.fns = next_fns;
        sel.methods = next_methods;
        sel.mod_fns = next_mods;
        if !changed {
            break;
        }
    }
    sel
}

fn add_module_refs(
    func: &BcFn,
    sel: &mut Selection,
    checker: &Checker,
    compiled: &HashMap<String, Option<Rc<BcFn>>>,
) {
    for inst in &func.insts {
        if inst.op == Op::Call && !inst.module.is_empty() && !is_host_call(&inst.module, &inst.s) {
            if let Some(target) = module_fn(checker, &inst.module, &inst.s) {
                if let Some(code) = code_of(compiled, &target) {
                    if ops_ok(code, checker, compiled) {
                        let key = format!("{}.{}", inst.module, inst.s);
                        if !sel.mod_fns.iter().any(|k| k == &key) {
                            sel.mod_fns.push(key);
                            add_module_refs(code, sel, checker, compiled);
                        }
                    }
                }
            }
        }
        if inst.op == Op::Closure {
            if let Some(code) = inst.code.as_deref() {
                add_module_refs(code, sel, checker, compiled);
            }
        }
    }
}

fn ops_ok(func: &BcFn, checker: &Checker, compiled: &HashMap<String, Option<Rc<BcFn>>>) -> bool {
    let n = func.insts.len() as i32;
    for inst in &func.insts {
        if matches!(inst.op, Op::LoadConst) {
            let Some(k) = func.constants.get(inst.b as usize) else {
                return false;
            };
            let _ = k;
        }
        if inst.op == Op::Call && !inst.module.is_empty() {
            if !is_host_call(&inst.module, &inst.s)
                && module_fn(checker, &inst.module, &inst.s).is_none()
                && checker.lookup_static(&inst.module, &inst.s).1.is_none()
            {
                return false;
            }
        }
        if inst.op == Op::Static
            && (inst.s.is_empty()
                || inst.module.is_empty()
                || checker.lookup_static(&inst.module, &inst.s).1.is_none())
        {
            return false;
        }
        if inst.op == Op::Bin && bin_op_id(&inst.s) < 0 {
            return false;
        }
        if inst.op == Op::Unary && unary_op_id(&inst.s) < 0 {
            return false;
        }
        if inst.op == Op::Struct
            && (inst.s.is_empty() || checker.all_types.borrow().get(&inst.s).is_none())
        {
            return false;
        }
        if inst.op == Op::LoadEnum && inst.s.is_empty() {
            return false;
        }
        if inst.op == Op::LoadFn && inst.s.is_empty() {
            return false;
        }
        if inst.op == Op::LoadSignal
            && (inst.s.is_empty() || !checker.signal_arity.borrow().contains_key(&inst.s))
        {
            return false;
        }
        if inst.op == Op::LoadSingleton
            && (inst.s.is_empty() || !checker.singletons.borrow().contains(&inst.s))
        {
            return false;
        }
        if inst.op == Op::Closure {
            let Some(code) = inst.code.as_deref() else {
                return false;
            };
            if !ops_ok(code, checker, compiled) {
                return false;
            }
        }
        if inst.op == Op::Jump && (inst.a < 0 || inst.a > n) {
            return false;
        }
        if matches!(inst.op, Op::JumpF | Op::JumpT) && (inst.b < 0 || inst.b > n) {
            return false;
        }
        if inst.op == Op::Super && super_hit(checker, &inst.module, &inst.s).0.is_empty() {
            return false;
        }
    }
    true
}

fn calls_ok(
    func: &BcFn,
    sel: &Selection,
    checker: &Checker,
    compiled: &HashMap<String, Option<Rc<BcFn>>>,
) -> bool {
    for inst in &func.insts {
        match inst.op {
            Op::Call => {
                if is_builtin_call(&inst.module, &inst.s) || is_host_call(&inst.module, &inst.s) {
                    continue;
                }
                if !inst.module.is_empty() {
                    let key = format!("{}.{}", inst.module, inst.s);
                    if sel.mod_fns.iter().any(|k| k == &key)
                        || checker.lookup_static(&inst.module, &inst.s).1.is_some()
                    {
                        continue;
                    }
                    return false;
                }
                if !sel.fns.iter().any(|n| n == &inst.s) {
                    return false;
                }
            }
            Op::LoadFn => {
                if !sel.fns.iter().any(|n| n == &inst.s) {
                    return false;
                }
            }
            Op::Method => {
                if method_op_id(&inst.s) >= 0
                    || sel.methods.iter().any(|k| method_name(k) == inst.s)
                    || is_enum_variant(checker, &inst.s)
                    || is_signal_method(&inst.s)
                    || inst.s == "cancel"
                {
                    continue;
                }
                return false;
            }
            Op::Super => {
                let (defined, _) = super_hit(checker, &inst.module, &inst.s);
                let key = format!("{defined}.{}", inst.s);
                if defined.is_empty() || !sel.methods.iter().any(|k| k == &key) {
                    return false;
                }
            }
            Op::Closure => {
                if let Some(code) = inst.code.as_deref() {
                    if !calls_ok(code, sel, checker, compiled) {
                        return false;
                    }
                }
            }
            _ => {}
        }
    }
    true
}

fn collect_lambdas(
    checker: &Checker,
    compiled: &HashMap<String, Option<Rc<BcFn>>>,
    sel: &Selection,
) -> Vec<Lam> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut next = 0i32;
    for func in compile::entry_fns(checker) {
        if sel.fns.iter().any(|n| n == &func.name) {
            if let Some(code) = code_of(compiled, &func) {
                collect_lams_from(code, &mut out, &mut seen, &mut next);
            }
        }
    }
    for key in &sel.methods {
        if let Some(func) = type_method(checker, key) {
            if let Some(code) = code_of(compiled, &func) {
                collect_lams_from(code, &mut out, &mut seen, &mut next);
            }
        }
    }
    for key in &sel.mod_fns {
        if let Some(func) = module_fn_key(checker, key) {
            if let Some(code) = code_of(compiled, &func) {
                collect_lams_from(code, &mut out, &mut seen, &mut next);
            }
        }
    }
    out
}

fn collect_lams_from(func: &BcFn, out: &mut Vec<Lam>, seen: &mut HashSet<usize>, next: &mut i32) {
    for inst in &func.insts {
        if inst.op != Op::Closure {
            continue;
        }
        let (Some(proto), Some(code)) = (inst.proto.clone(), inst.code.clone()) else {
            continue;
        };
        let ptr = Rc::as_ptr(&proto) as usize;
        if !seen.insert(ptr) {
            continue;
        }
        let name = format!("rg_fn_lam{next}");
        *next += 1;
        let ncopy = code.arity as i32 + inst.c.max(0);
        out.push(Lam {
            proto: ptr,
            func: proto,
            code: code.clone(),
            name,
            ncopy,
        });
        collect_lams_from(&code, out, seen, next);
    }
}

fn code_of<'a>(compiled: &'a HashMap<String, Option<Rc<BcFn>>>, func: &FnDecl) -> Option<&'a BcFn> {
    compiled
        .get(&compile::fn_key(func))
        .and_then(|c| c.as_deref())
}

fn ordered_method_pairs(checker: &Checker) -> Vec<(String, FnRc)> {
    let methods = checker.type_methods.borrow();
    let mut out = Vec::new();
    for (ty, group) in methods.iter() {
        for (name, func) in group {
            out.push((format!("{ty}.{name}"), func.clone()));
        }
    }
    out
}

fn ordered_static(checker: &Checker) -> Vec<(String, String, FnRc)> {
    let methods = checker.static_methods.borrow();
    let mut out = Vec::new();
    for (ty, group) in methods.iter() {
        for (name, func) in group {
            out.push((ty.clone(), name.clone(), func.clone()));
        }
    }
    out
}

fn ordered_parents(checker: &Checker) -> Vec<(String, String)> {
    checker
        .class_parents
        .borrow()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

fn ordered_enums(checker: &Checker) -> Vec<rosegold_syntax::EnumDecl> {
    checker
        .enums
        .borrow()
        .values()
        .map(|en| (**en).clone())
        .collect()
}

fn type_method(checker: &Checker, key: &str) -> Option<FnRc> {
    let (ty, name) = split_key(key);
    checker.type_methods.borrow().get(&ty)?.get(&name).cloned()
}

fn module_fn(checker: &Checker, module: &str, name: &str) -> Option<FnRc> {
    if module.is_empty() || name.is_empty() {
        return None;
    }
    let loaded = checker.loaded.borrow();
    let module = loaded.get(module)?.clone();
    let module = module.borrow();
    module
        .exports
        .get(name)
        .cloned()
        .or_else(|| module.fns.get(name).cloned())
}

fn module_fn_key(checker: &Checker, key: &str) -> Option<FnRc> {
    let (module, name) = split_key(key);
    if module.is_empty() {
        return None;
    }
    module_fn(checker, &module, &name)
}

fn split_key(key: &str) -> (String, String) {
    match key.rfind('.') {
        Some(i) => (key[..i].to_string(), key[i + 1..].to_string()),
        None => (String::new(), key.to_string()),
    }
}

fn method_type(key: &str) -> String {
    split_key(key).0
}

fn method_name(key: &str) -> String {
    split_key(key).1
}

fn super_hit(checker: &Checker, module: &str, name: &str) -> (String, bool) {
    if module.is_empty() || name.is_empty() {
        return (String::new(), false);
    }
    let (defined, func) = checker.lookup_method(module, name);
    match func {
        Some(func) if !func.is_abstract && !defined.is_empty() => (defined, func.is_async),
        _ => (String::new(), false),
    }
}

fn is_async(checker: &Checker, name: &str) -> bool {
    checker.fns.borrow().get(name).is_some_and(|f| f.is_async)
}

fn callee_async(checker: &Checker, module: &str, name: &str) -> bool {
    if !module.is_empty() {
        return module_fn(checker, module, name).is_some_and(|f| f.is_async);
    }
    is_async(checker, name)
}

fn is_enum_variant(checker: &Checker, name: &str) -> bool {
    checker
        .enums
        .borrow()
        .values()
        .any(|en| en.variants.iter().any(|v| v.name == name))
}

fn is_signal_method(name: &str) -> bool {
    matches!(
        name,
        "connect" | "connect_once" | "disconnect" | "emit" | "emit_deferred"
    )
}

fn is_builtin_call(module: &str, name: &str) -> bool {
    module.is_empty() && matches!(name, "print" | "len" | "assert")
}

fn is_host_call(module: &str, name: &str) -> bool {
    if module.is_empty() {
        return matches!(name, "argv" | "argv_len");
    }
    match module {
        "Future" => matches!(name, "all" | "race"),
        "process" => matches!(name, "argv" | "argc"),
        "checks" => matches!(name, "eq" | "neq" | "eq_string" | "that" | "truthy"),
        "__time" => matches!(name, "now" | "sleep" | "delay"),
        "__math" => matches!(
            name,
            "pow"
                | "powf"
                | "atan2"
                | "random"
                | "rand_int"
                | "sin"
                | "cos"
                | "sqrt"
                | "to_int"
                | "to_float"
                | "floor"
                | "ceil"
        ),
        "__str" => matches!(
            name,
            "slice"
                | "replace"
                | "repeat"
                | "contains"
                | "starts_with"
                | "ends_with"
                | "split"
                | "find"
                | "length"
                | "is_empty"
                | "upper"
                | "lower"
                | "trim"
        ),
        "__uuid" => matches!(name, "v4" | "parse" | "valid"),
        "__path" => matches!(name, "join" | "parent" | "stem"),
        "__io" => matches!(
            name,
            "exists" | "remove" | "read_text" | "read_lines" | "write_text"
        ),
        "__json" => matches!(name, "parse" | "valid" | "stringify"),
        "__regex" => matches!(
            name,
            "valid"
                | "is_match"
                | "find"
                | "find_match"
                | "captures"
                | "findall"
                | "replace"
                | "split"
        ),
        "__ui" => matches!(
            name,
            "backend"
                | "platform"
                | "open"
                | "close"
                | "alive"
                | "show"
                | "hide"
                | "title"
                | "set_title"
                | "width"
                | "height"
                | "set_size"
                | "clear"
                | "fill"
                | "line"
                | "stroke_rect"
                | "fill_round"
                | "stroke_round"
                | "image_rgb"
                | "image"
                | "image_width"
                | "image_height"
                | "clip_push"
                | "clip_pop"
                | "text"
                | "text_width"
                | "font_height"
                | "present"
                | "wait"
                | "run"
                | "cursor"
                | "mouse_x"
                | "mouse_y"
                | "mouse_down"
                | "take_click"
                | "take_right_click"
                | "feed_click"
                | "feed_right_click"
                | "feed_mouse"
                | "feed_down"
                | "take_key"
                | "key_code"
                | "key_text"
                | "feed_key"
                | "take_scroll"
                | "scroll_dx"
                | "scroll_dy"
                | "feed_scroll"
                | "set_frame"
                | "clipboard_get"
                | "clipboard_set"
                | "poll"
                | "next_frame"
                | "count"
        ),
        _ => false,
    }
}

fn godot_stub(func: &FnDecl) -> bool {
    func.file.replace('\\', "/").ends_with("/godot/lib.rg")
}

fn has_type_signals(checker: &Checker) -> bool {
    checker
        .type_signals
        .borrow()
        .values()
        .any(|m| !m.is_empty())
}

fn intern_type_signals(checker: &Checker, strings: &mut Vec<String>) {
    for (ty, sigs) in signal_pairs(checker) {
        intern(strings, &ty);
        for (name, _) in sigs {
            intern(strings, &name);
        }
    }
}

fn emit_type_signals(ir: &mut String, checker: &Checker, strings: &mut Vec<String>) {
    for (ty, sigs) in signal_pairs(checker) {
        for (name, arity) in sigs {
            let t = intern(strings, &ty);
            let n = intern(strings, &name);
            ir.push_str(&format!(
                "  call void @rg_register_type_signal(ptr @str{t}, ptr @str{n}, i32 {arity})\n"
            ));
        }
    }
}

fn signal_pairs(checker: &Checker) -> Vec<(String, Vec<(String, i32)>)> {
    checker
        .type_signals
        .borrow()
        .iter()
        .map(|(ty, sigs)| {
            (
                ty.clone(),
                sigs.iter().map(|(n, a)| (n.clone(), *a)).collect(),
            )
        })
        .collect()
}

fn intern_enums(checker: &Checker, strings: &mut Vec<String>) {
    for en in ordered_enums(checker) {
        intern(strings, &en.name);
        for v in &en.variants {
            intern(strings, &v.name);
            if variant_names(v) {
                let n = v.arity.max(v.field_names.len() as i32);
                for i in 0..n {
                    let field = v
                        .field_names
                        .get(i as usize)
                        .map(String::as_str)
                        .unwrap_or("");
                    intern(strings, field);
                }
            }
        }
    }
}

fn variant_names(v: &rosegold_syntax::EnumVariant) -> bool {
    v.field_names.iter().any(|f| !f.is_empty())
}

fn emit_enum_tables(ir: &mut String, en: &rosegold_syntax::EnumDecl, strings: &mut Vec<String>) {
    let mut any = false;
    for v in &en.variants {
        if !variant_names(v) || v.arity <= 0 {
            continue;
        }
        any = true;
        ir.push_str(&format!(
            "@{} = private unnamed_addr constant [{} x ptr] [",
            enum_fields(&en.name, &v.name),
            v.arity
        ));
        for i in 0..v.arity {
            if i > 0 {
                ir.push_str(", ");
            }
            let field = v
                .field_names
                .get(i as usize)
                .map(String::as_str)
                .unwrap_or("");
            ir.push_str(&format!("ptr @str{}", intern(strings, field)));
        }
        ir.push_str("]\n");
    }
    if any {
        ir.push('\n');
    }
}

fn emit_enum_regs(ir: &mut String, en: &rosegold_syntax::EnumDecl, strings: &mut Vec<String>) {
    let type_id = intern(strings, &en.name);
    for v in &en.variants {
        let var_id = intern(strings, &v.name);
        let fields = if variant_names(v) && v.arity > 0 {
            format!("@{}", enum_fields(&en.name, &v.name))
        } else {
            "null".to_string()
        };
        ir.push_str(&format!(
            "  call void @rg_register_enum(ptr @str{type_id}, ptr @str{var_id}, i32 {}, ptr {fields})\n",
            v.arity
        ));
    }
}

fn enum_fields(ty: &str, variant: &str) -> String {
    format!("enumfields_{}_{}", ident(ty), ident(variant))
}

fn layouts(checker: &Checker) -> IndexMap<String, Vec<String>> {
    checker
        .all_types
        .borrow()
        .iter()
        .map(|(k, v)| (k.clone(), v.borrow().fields.clone()))
        .collect()
}

fn emit_field_table(ir: &mut String, ty: &str, fields: &[String], strings: &mut Vec<String>) {
    if fields.is_empty() {
        return;
    }
    ir.push_str(&format!(
        "@fields_{} = private unnamed_addr constant [{} x ptr] [",
        ident(ty),
        fields.len()
    ));
    for (i, field) in fields.iter().enumerate() {
        if i > 0 {
            ir.push_str(", ");
        }
        ir.push_str(&format!("ptr @str{}", intern(strings, field)));
    }
    ir.push_str("]\n");
}

fn emit_godot_forward(ir: &mut String, ty: &str, method: &str, strings: &mut Vec<String>) {
    let id = intern(strings, method);
    ir.push_str(&format!(
        "define void @{}(ptr %out, ptr %args, i32 %argc) {{\nentry:\n  call void @rg_godot_call(ptr %out, ptr @str{id}, ptr %args, i32 %argc)\n  ret void\n}}\n",
        mangle_method(ty, method)
    ));
}

fn test_fns(
    checker: &Checker,
    compiled: &HashMap<String, Option<Rc<BcFn>>>,
    sel: &Selection,
) -> Vec<String> {
    compile::entry_fns(checker)
        .into_iter()
        .filter(|f| {
            f.is_test
                && f.module.is_empty()
                && code_of(compiled, f).is_some()
                && sel.fns.iter().any(|n| n == &f.name)
        })
        .map(|f| f.name.clone())
        .collect()
}

fn emit_test_main(
    ir: &mut String,
    checker: &Checker,
    compiled: &HashMap<String, Option<Rc<BcFn>>>,
    sel: &Selection,
    strings: &mut Vec<String>,
    need_init: bool,
) {
    let tests = test_fns(checker, compiled, sel);
    if tests.is_empty() {
        return;
    }
    let any_async = tests.iter().any(|n| is_async(checker, n));
    ir.push_str("define i32 @main(i32 %argc, ptr %argv) {\nentry:\n");
    ir.push_str("  call void @rg_set_argv(i32 %argc, ptr %argv)\n");
    ir.push_str("  %ret = alloca %RGValue\n  %err = alloca %RGValue\n  %failed = alloca i32\n");
    ir.push_str("  call void @rg_set_void(ptr %ret)\n  store i32 0, ptr %failed\n");
    if need_init {
        ir.push_str("  call void @rg_init_methods()\n");
    }
    if any_async {
        ir.push_str("  %fut = alloca %RGValue\n  call void @rg_set_void(ptr %fut)\n");
    }
    for (i, name) in tests.iter().enumerate() {
        let sid = intern(strings, name);
        ir.push_str("  call void @rg_set_void(ptr %ret)\n");
        if is_async(checker, name) {
            ir.push_str(&format!(
                "  call void @rg_spawn(ptr %fut, ptr @{}, ptr null, i32 0)\n  call void @rg_await(ptr %ret, ptr %fut)\n",
                mangle(name)
            ));
        } else {
            ir.push_str(&format!(
                "  call void @{}(ptr %ret, ptr null, i32 0)\n",
                mangle(name)
            ));
        }
        ir.push_str(&format!("  %h{i} = call i32 @rg_has_throw()\n"));
        ir.push_str(&format!("  %c{i} = icmp ne i32 %h{i}, 0\n"));
        ir.push_str(&format!("  br i1 %c{i}, label %fail{i}, label %ok{i}\n"));
        ir.push_str(&format!(
            "ok{i}:\n  call void @rg_test_ok(ptr @str{sid})\n  br label %next{i}\n"
        ));
        ir.push_str(&format!(
            "fail{i}:\n  call void @rg_catch(ptr %err)\n  call void @rg_test_fail(ptr @str{sid}, ptr %err)\n  %f{i} = load i32, ptr %failed\n  %fn{i} = add i32 %f{i}, 1\n  store i32 %fn{i}, ptr %failed\n  br label %next{i}\nnext{i}:\n"
        ));
    }
    ir.push_str("  %fend = load i32, ptr %failed\n");
    ir.push_str(&format!("  %passed = sub i32 {}, %fend\n", tests.len()));
    ir.push_str(&format!(
        "  call void @rg_test_summary(i32 %passed, i32 {})\n",
        tests.len()
    ));
    ir.push_str(
        "  %any = icmp ne i32 %fend, 0\n  %code = zext i1 %any to i32\n  ret i32 %code\n}\n",
    );
}

fn collect_strings(func: &BcFn, strings: &mut Vec<String>) {
    for inst in &func.insts {
        if matches!(
            inst.op,
            Op::Struct
                | Op::Member
                | Op::FieldSet
                | Op::Method
                | Op::LoadEnum
                | Op::MatchVar
                | Op::Payload
                | Op::LoadSignal
                | Op::LoadSingleton
                | Op::Call
                | Op::Static
        ) {
            if !inst.s.is_empty() {
                intern(strings, &inst.s);
            }
            if matches!(inst.op, Op::Call | Op::Static) {
                intern(strings, &inst.module);
            }
        }
        if inst.op == Op::LoadConst {
            if let Some(Const::Str(s)) = func.constants.get(inst.b as usize) {
                intern(strings, s);
            }
        }
        if inst.op == Op::Closure {
            if let Some(code) = inst.code.as_deref() {
                collect_strings(code, strings);
            }
        }
    }
}

fn intern(strings: &mut Vec<String>, s: &str) -> usize {
    if let Some(i) = strings.iter().position(|c| c == s) {
        return i;
    }
    strings.push(s.to_string());
    strings.len() - 1
}

fn emit_string_global(ir: &mut String, id: usize, s: &str) {
    let marker = format!("@str{id} =");
    if ir.contains(&marker) {
        return;
    }
    let mut raw = s.as_bytes().to_vec();
    raw.push(0);
    ir.push_str(&format!(
        "@str{id} = private unnamed_addr constant [{} x i8] c\"{}\"\n",
        raw.len(),
        llvm_bytes(&raw)
    ));
}

fn llvm_bytes(raw: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789ABCDEF";
    let mut ss = String::new();
    for &b in raw {
        if (32..127).contains(&b) && b != b'"' && b != b'\\' {
            ss.push(b as char);
        } else {
            ss.push('\\');
            ss.push(HEX[(b >> 4) as usize] as char);
            ss.push(HEX[(b & 15) as usize] as char);
        }
    }
    ss
}

fn llvm_double(n: f64) -> String {
    if n.is_nan() {
        return "0x7FF8000000000000".to_string();
    }
    if n.is_infinite() {
        return if n.is_sign_positive() {
            "0x7FF0000000000000".to_string()
        } else {
            "0xFFF0000000000000".to_string()
        };
    }
    let s = java_double(n);
    if !s.contains('.') && !s.contains('E') && !s.contains('e') {
        format!("{s}.0")
    } else {
        s
    }
}

fn java_double(n: f64) -> String {
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n.is_infinite() {
        return if n.is_sign_positive() {
            "Infinity"
        } else {
            "-Infinity"
        }
        .to_string();
    }
    let mut s = format!("{}", n);
    if !s.contains('.') && !s.contains('e') && !s.contains('E') {
        s.push_str(".0");
    }
    s
}

fn ident(name: &str) -> String {
    if name.is_empty() {
        return "anon".to_string();
    }
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn mangle(name: &str) -> String {
    format!("rg_fn_{}", ident(name))
}

fn mangle_mod(module: &str, name: &str) -> String {
    format!("rg_fn_{}_{}", ident(module), ident(name))
}

fn mangle_method(ty: &str, method: &str) -> String {
    format!("rg_fn_{}_{}", ident(ty), ident(method))
}

fn bin_op_id(op: &str) -> i32 {
    match op {
        "+" => 1,
        "-" => 2,
        "*" => 3,
        "/" => 4,
        "%" => 5,
        "==" => 6,
        "!=" => 7,
        "<" => 8,
        ">" => 9,
        "<=" => 10,
        ">=" => 11,
        "<<" => 12,
        ">>" => 13,
        "&" => 14,
        "^" => 15,
        "|" => 16,
        _ => -1,
    }
}

fn unary_op_id(op: &str) -> i32 {
    match op {
        "-" => 1,
        "!" => 2,
        "~" => 3,
        _ => -1,
    }
}

fn method_op_id(name: &str) -> i32 {
    match name {
        "len" => 1,
        "push" => 2,
        "pop" => 3,
        "has" => 4,
        "keys" => 5,
        "remove" => 6,
        "insert" => 7,
        _ => -1,
    }
}

struct Dbg {
    nodes: Vec<String>,
    files: HashMap<String, i32>,
    locs: HashMap<String, i32>,
    sub_ty: i32,
    cu: i32,
    dwarf: i32,
    dbg_ver: i32,
}

impl Dbg {
    fn new(path: &str) -> Self {
        let mut dbg = Dbg {
            nodes: Vec::new(),
            files: HashMap::new(),
            locs: HashMap::new(),
            sub_ty: 0,
            cu: 0,
            dwarf: 0,
            dbg_ver: 0,
        };
        let empty = dbg.add("!{}");
        let file = dbg.file_id(path);
        dbg.cu = dbg.add(&format!(
            "distinct !DICompileUnit(language: DW_LANG_C99, file: !{file}, producer: \"RoseGold\", isOptimized: false, runtimeVersion: 0, emissionKind: FullDebug, enums: !{empty})"
        ));
        dbg.sub_ty = dbg.add(&format!("!DISubroutineType(types: !{empty})"));
        dbg.dwarf = dbg.add("!{i32 7, !\"Dwarf Version\", i32 4}");
        dbg.dbg_ver = dbg.add("!{i32 2, !\"Debug Info Version\", i32 3}");
        dbg
    }

    fn subprogram(&mut self, name: &str, path: &str, line: u32) -> i32 {
        let file = self.file_id(path);
        let name = if name.is_empty() { "fn" } else { name };
        let line = line.max(1);
        self.add(&format!(
            "distinct !DISubprogram(name: \"{}\", scope: !{file}, file: !{file}, line: {line}, type: !{}, spFlags: DISPFlagDefinition, unit: !{})",
            esc(name),
            self.sub_ty,
            self.cu
        ))
    }

    fn location(&mut self, sp: i32, line: u32, col: u32) -> i32 {
        let line = line.max(1);
        let col = col.max(1);
        let key = format!("{sp}:{line}:{col}");
        if let Some(hit) = self.locs.get(&key) {
            return *hit;
        }
        let id = self.add(&format!(
            "!DILocation(line: {line}, column: {col}, scope: !{sp})"
        ));
        self.locs.insert(key, id);
        id
    }

    fn emit(&self, ir: &mut String) {
        ir.push('\n');
        ir.push_str(&format!("!llvm.dbg.cu = !{{!{}}}\n", self.cu));
        ir.push_str(&format!(
            "!llvm.module.flags = !{{!{}, !{}}}\n",
            self.dwarf, self.dbg_ver
        ));
        for node in &self.nodes {
            ir.push_str(node);
            ir.push('\n');
        }
    }

    fn file_id(&mut self, path: &str) -> i32 {
        let (key, file, dir) = java_file(path);
        if let Some(hit) = self.files.get(&key) {
            return *hit;
        }
        let id = self.add(&format!(
            "!DIFile(filename: \"{}\", directory: \"{}\")",
            esc(&file),
            esc(&dir)
        ));
        self.files.insert(key, id);
        id
    }

    fn add(&mut self, body: &str) -> i32 {
        let id = self.nodes.len() as i32;
        self.nodes.push(format!("!{id} = {body}"));
        id
    }
}

fn attach_dbg(ir: &mut String, dbg: &mut Dbg, sp: i32, line: u32, col: u32) {
    let Some(nl) = ir.rfind('\n') else {
        return;
    };
    let last = &ir[..nl];
    let prev = last.rfind('\n').map(|i| i + 1).unwrap_or(0);
    let text = last[prev..].trim();
    if text.is_empty() || text.ends_with(':') || text.starts_with(';') {
        return;
    }
    let tag = format!(", !dbg !{}", dbg.location(sp, line, col));
    ir.insert_str(nl, &tag);
}

fn attach_new_lines(ir: &mut String, from: usize, dbg: &mut Dbg, sp: i32, line: u32, col: u32) {
    let tag = format!(", !dbg !{}", dbg.location(sp, line, col));
    let mut i = from;
    while i < ir.len() {
        let Some(rel) = ir[i..].find('\n') else {
            break;
        };
        let nl = i + rel;
        let text = ir[i..nl].trim();
        if !text.is_empty()
            && !text.ends_with(':')
            && !text.starts_with(';')
            && !text.contains("!dbg")
        {
            ir.insert_str(nl, &tag);
            i = nl + tag.len() + 1;
        } else {
            i = nl + 1;
        }
    }
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn java_file(path: &str) -> (String, String, String) {
    let raw = if path.is_empty() { "unknown.rg" } else { path };
    let path = PathBuf::from(raw);
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let path = normalize_path(&path);
    let key = path.display().to_string().replace('\\', "/");
    let file = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| key.clone());
    let dir = path
        .parent()
        .map(|p| p.display().to_string().replace('\\', "/"))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| ".".to_string());
    (key, file, dir)
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

pub fn fully_lowered(ir: &str) -> bool {
    !ir.contains("{ not lowered to llvm }") && ir.contains("define i32 @main(")
}

pub struct Native {
    pub ok: bool,
    pub exit_code: i32,
    pub stdout: String,
    pub message: String,
}

pub fn link_and_run(
    checker: &Checker,
    test: bool,
    output: Option<&Path>,
    run: bool,
    argv: &[String],
    cwd: &Path,
    headless: bool,
) -> Native {
    let ir = emit(checker, test);
    if !fully_lowered(&ir) {
        return Native {
            ok: false,
            exit_code: 1,
            stdout: String::new(),
            message: "cannot link: some functions were not lowered to llvm".to_string(),
        };
    }
    let Some(clang) = find_clang() else {
        return Native {
            ok: false,
            exit_code: 1,
            stdout: String::new(),
            message: "llvm --link needs clang on PATH (LLVM 15+)".to_string(),
        };
    };
    let native = resolve_native_dir(cwd);
    if !is_native_dir(&native) {
        return Native {
            ok: false,
            exit_code: 1,
            stdout: String::new(),
            message: "native/runtime.c not found".to_string(),
        };
    }
    let cache = match run_cache() {
        Ok(dir) => dir,
        Err(err) => {
            return Native {
                ok: false,
                exit_code: 1,
                stdout: String::new(),
                message: err,
            };
        }
    };
    let stamp = link_stamp(&ir, test, &native, &clang);
    let cached = with_exe(cache.join(format!("l{stamp}")));
    let out = output
        .map(|p| with_exe(cwd.join(p)))
        .unwrap_or_else(|| cached.clone());
    if let Some(parent) = out.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let cached_ok = cached.is_file() && fs::metadata(&cached).map(|m| m.len() > 0).unwrap_or(false);
    if !cached_ok {
        let tmp = match std::env::temp_dir().read_dir() {
            Ok(_) => std::env::temp_dir().join(format!("rg-llvm-{}", std::process::id())),
            Err(err) => {
                return Native {
                    ok: false,
                    exit_code: 1,
                    stdout: String::new(),
                    message: err.to_string(),
                };
            }
        };
        let _ = fs::create_dir_all(&tmp);
        let ll = tmp.join("program.ll");
        if let Err(err) = fs::write(&ll, &ir) {
            return Native {
                ok: false,
                exit_code: 1,
                stdout: String::new(),
                message: err.to_string(),
            };
        }
        let runtime = native.join("runtime.c");
        let mut cmd = Command::new(&clang);
        cmd.arg("-std=c11")
            .arg("-O0")
            .arg("-g")
            .arg("-Wno-override-module")
            .arg("-I")
            .arg(native.display().to_string())
            .arg("-o")
            .arg(cached.display().to_string())
            .arg(ll.display().to_string())
            .arg(runtime.display().to_string());
        if cfg!(windows) {
            cmd.args([
                "-luser32",
                "-lgdi32",
                "-lole32",
                "-luuid",
                "-lwindowscodecs",
            ]);
        } else {
            cmd.arg("-lm");
        }
        let link = cmd.output();
        let _ = fs::remove_dir_all(&tmp);
        let link = match link {
            Ok(link) => link,
            Err(err) => {
                return Native {
                    ok: false,
                    exit_code: 1,
                    stdout: String::new(),
                    message: err.to_string(),
                };
            }
        };
        if !link.status.success() {
            let log = String::from_utf8_lossy(&link.stdout);
            let err = String::from_utf8_lossy(&link.stderr);
            let message = if log.is_empty() && err.is_empty() {
                "clang failed".to_string()
            } else {
                format!("{log}{err}")
            };
            let _ = fs::remove_file(&cached);
            return Native {
                ok: false,
                exit_code: 1,
                stdout: String::new(),
                message,
            };
        }
    }
    if !cached.is_file() {
        return Native {
            ok: false,
            exit_code: 1,
            stdout: String::new(),
            message: "clang produced no output".to_string(),
        };
    }
    if cached != out {
        if let Err(err) = fs::copy(&cached, &out) {
            return Native {
                ok: false,
                exit_code: 1,
                stdout: String::new(),
                message: err.to_string(),
            };
        }
    }
    if !run {
        return Native {
            ok: true,
            exit_code: 0,
            stdout: format!("{}\n", out.display()),
            message: String::new(),
        };
    }
    let mut exec_cmd = Command::new(&out);
    exec_cmd.args(argv).current_dir(cwd);
    if headless {
        exec_cmd.env("RG_UI_HEADLESS", "1");
    }
    let exec = exec_cmd.output();
    match exec {
        Ok(exec) => Native {
            ok: exec.status.success(),
            exit_code: exec.status.code().unwrap_or(1),
            stdout: String::from_utf8_lossy(&exec.stdout).into_owned(),
            message: String::from_utf8_lossy(&exec.stderr).into_owned(),
        },
        Err(err) => Native {
            ok: false,
            exit_code: 1,
            stdout: String::new(),
            message: err.to_string(),
        },
    }
}

pub struct NativeFile {
    pub lowered: bool,
    pub outcome: super::Outcome,
}

/// Java `NativeRun.execFile`. `lowered` is false when the file should fall back to the interpreter.
pub fn exec_native_file(
    path: &Path,
    argv: &[String],
    work_dir: &Path,
    test: bool,
) -> Result<NativeFile, String> {
    if !path.is_file() {
        return Ok(NativeFile {
            lowered: true,
            outcome: super::Outcome {
                ok: false,
                out: String::new(),
                message: "RoseGold file not found".to_string(),
                exit_code: 1,
            },
        });
    }
    let file = absolute(path);
    let cwd = if work_dir.as_os_str().is_empty() {
        file.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        absolute(work_dir)
    };
    let source = fs::read_to_string(&file).map_err(|err| err.to_string())?;
    let file_name = file.display().to_string();
    let (program, diags) = rosegold_syntax::parse_source(&source, &file_name);
    if let Some(diag) = first_problem(&diags) {
        return Ok(native_diag(&diag.to_human()));
    }
    if test && !program.fns.iter().any(|func| func.is_test) {
        return Ok(NativeFile {
            lowered: true,
            outcome: super::Outcome {
                ok: true,
                out: String::new(),
                message: "no @test functions".to_string(),
                exit_code: 0,
            },
        });
    }
    let (checker, diags) = rosegold_check::check_owned(program, &source, &file_name, false);
    if let Some(diag) = first_problem(&diags) {
        return Ok(native_diag(&diag.to_human()));
    }
    let native = link_and_run(&checker, test, None, true, argv, &cwd, true);
    let message = if native.ok {
        String::new()
    } else if native.stdout.is_empty() {
        native.message
    } else {
        native.stdout.clone()
    };
    Ok(NativeFile {
        lowered: !message.contains("not lowered"),
        outcome: super::Outcome {
            ok: native.ok,
            out: native.stdout,
            message,
            exit_code: native.exit_code,
        },
    })
}

fn first_problem(diags: &[rosegold_syntax::Diagnostic]) -> Option<&rosegold_syntax::Diagnostic> {
    diags.iter().find(|diag| {
        let severity = diag.severity.to_ascii_lowercase();
        !matches!(
            severity.as_str(),
            "warning" | "warn" | "info" | "information" | "hint"
        )
    })
}

fn native_diag(message: &str) -> NativeFile {
    NativeFile {
        lowered: !message.contains("not lowered"),
        outcome: super::Outcome {
            ok: false,
            out: String::new(),
            message: message.to_string(),
            exit_code: 1,
        },
    }
}

fn absolute(path: &Path) -> PathBuf {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    normalize_path(&path)
}

fn link_stamp(ir: &str, test: bool, native: &Path, clang: &Path) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    let mut mix = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    mix(if test { b"t" } else { b"r" });
    mix(ir.as_bytes());
    for name in ["runtime.c", "host_ui.c", "host_img.c", "rg_value.h"] {
        let path = native.join(name);
        mix(path.display().to_string().as_bytes());
        if let Ok(meta) = fs::metadata(&path) {
            mix(&meta.len().to_le_bytes());
            if let Ok(modified) = meta.modified() {
                if let Ok(dur) = modified.duration_since(std::time::UNIX_EPOCH) {
                    mix(&dur.as_millis().to_le_bytes());
                }
            }
        }
    }
    mix(clang.display().to_string().as_bytes());
    format!("{hash:016x}")
}

fn with_exe(path: PathBuf) -> PathBuf {
    if cfg!(windows) {
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !name.to_ascii_lowercase().ends_with(".exe") {
            return path.with_file_name(format!("{name}.exe"));
        }
    }
    path
}

fn find_clang() -> Option<PathBuf> {
    let mut cands = Vec::new();
    if let Ok(env) = std::env::var("CLANG") {
        let p = PathBuf::from(env);
        if p.is_file() {
            cands.push(p);
        }
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            for name in ["clang.exe", "clang"] {
                let cand = dir.join(name);
                if cand.is_file() {
                    cands.push(cand);
                }
            }
        }
    }
    if cfg!(windows) {
        let extra = PathBuf::from(r"C:\Program Files\LLVM\bin\clang.exe");
        if extra.is_file() {
            cands.push(extra);
        }
    }
    cands.into_iter().find(|c| clang_usable(c))
}

fn clang_usable(path: &Path) -> bool {
    if !cfg!(windows) {
        return true;
    }
    let Ok(buf) = fs::read(path) else {
        return false;
    };
    if buf.len() < 0x40 || buf[0] != b'M' || buf[1] != b'Z' {
        return true;
    }
    let pe = u32::from_le_bytes([buf[0x3C], buf[0x3D], buf[0x3E], buf[0x3F]]) as usize;
    if pe + 6 >= buf.len() {
        return true;
    }
    let machine = u16::from_le_bytes([buf[pe + 4], buf[pe + 5]]);
    let os = if cfg!(target_arch = "x86_64") {
        0x8664
    } else if cfg!(target_arch = "aarch64") {
        0xAA64
    } else if cfg!(target_arch = "x86") {
        0x14C
    } else {
        0
    };
    os == 0 || machine == os
}

fn resolve_native_dir(cwd: &Path) -> PathBuf {
    let mut p = cwd.to_path_buf();
    loop {
        let local = p.join("native");
        if is_native_dir(&local) {
            return local;
        }
        if !p.pop() {
            break;
        }
    }
    PathBuf::from(r"C:\CODE\RoseGoldJava\native")
}

fn is_native_dir(dir: &Path) -> bool {
    dir.join("runtime.c").is_file()
        && dir.join("host_ui.c").is_file()
        && dir.join("host_img.c").is_file()
        && dir.join("rg_value.h").is_file()
}

fn run_cache() -> Result<PathBuf, String> {
    let dir = if let Ok(local) = std::env::var("LOCALAPPDATA") {
        if !local.is_empty() {
            PathBuf::from(local).join("RoseGold").join("run")
        } else {
            PathBuf::from(".").join(".rosegold").join("run")
        }
    } else {
        PathBuf::from(".").join(".rosegold").join("run")
    };
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}
