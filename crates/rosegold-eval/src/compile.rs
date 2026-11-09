//! Lowers functions the way `Compile.java` does. A function that cannot be
//! lowered stays on the tree-walk interpreter.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use rosegold_check::{is_host_module, type_head, Checker, FnRc};
use rosegold_syntax::{Expr, ExprKind, FnDecl, ItemKind, Pat, Span, Stmt, StmtKind};

use crate::bytecode::{Const, Fn as BcFn, Handler, Inst, Op};
use crate::value::zero_of_type;

struct Skip;

type CResult<T> = Result<T, Skip>;

pub fn fn_key(func: &FnDecl) -> String {
    format!(
        "{}|{}|{}|{}",
        func.file, func.module, func.name, func.span.offset
    )
}

/// Compile every named function the checker knows. `None` means tree-walk.
pub fn attach(checker: &Checker) -> HashMap<String, Option<Rc<BcFn>>> {
    let mut work = Vec::new();
    work.extend(checker.fns.borrow().values().cloned());
    for methods in checker.type_methods.borrow().values() {
        work.extend(methods.values().cloned());
    }
    for methods in checker.static_methods.borrow().values() {
        work.extend(methods.values().cloned());
    }
    let loaded = checker.loaded.borrow();
    for module in loaded.values() {
        let module = module.borrow();
        work.extend(
            module
                .fns
                .values()
                .chain(module.exports.values())
                .chain(module.from_fns.values())
                .cloned(),
        );
        for group in module.ufcs_fns.values() {
            work.extend(group.iter().cloned());
        }
    }
    drop(loaded);
    let mut out = HashMap::new();
    for func in work {
        compile_one(checker, &func, &mut out);
    }
    out
}

fn compile_one(checker: &Checker, func: &FnDecl, out: &mut HashMap<String, Option<Rc<BcFn>>>) {
    let key = fn_key(func);
    if out.contains_key(&key) {
        return;
    }
    if func.is_abstract {
        out.insert(key, None);
        return;
    }
    let prev = checker.current_module();
    *checker.current_module.borrow_mut() = func.module.clone();
    let code = Compiler::compile(checker, func, Vec::new(), None).map(Rc::new);
    *checker.current_module.borrow_mut() = prev;
    out.insert(key, code);
}

pub fn dump_entry(checker: &Checker) -> String {
    let compiled = attach(checker);
    let mut ss = String::new();
    for func in entry_fns(checker) {
        match compiled.get(&fn_key(&func)).and_then(|c| c.as_ref()) {
            Some(code) => ss.push_str(&code.dump()),
            None => {
                ss.push_str("fn ");
                ss.push_str(&func.name);
                ss.push_str(" { tree-walk }\n");
            }
        }
        ss.push('\n');
    }
    ss
}

pub(super) fn entry_fns(checker: &Checker) -> Vec<FnRc> {
    let fns = checker.fns.borrow();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in &checker.program.items {
        if item.kind != ItemKind::Fn {
            continue;
        }
        let Some(decl) = checker.program.fns.get(item.index) else {
            continue;
        };
        let Some(rc) = fns.get(&decl.name) else {
            continue;
        };
        if seen.insert(fn_key(rc)) {
            out.push(rc.clone());
        }
    }
    for im in &checker.program.imports {
        if im.path.len() != 2 {
            continue;
        }
        let alias = if im.alias.is_empty() {
            &im.path[1]
        } else {
            &im.alias
        };
        let Some(rc) = fns.get(alias) else {
            continue;
        };
        if seen.insert(fn_key(rc)) {
            out.push(rc.clone());
        }
    }
    out
}

struct Compiler<'a> {
    checker: &'a Checker,
    func: &'a FnDecl,
    out: BcFn,
    slots: HashMap<String, i32>,
    loop_continue: Vec<i32>,
    loop_continue_fix: Vec<Vec<usize>>,
    loop_breaks: Vec<Vec<usize>>,
    await_break: Vec<Vec<Stmt>>,
    await_continue: Vec<Vec<Stmt>>,
    stop_block: bool,
    loop_seq: i32,
    ntemps: i32,
    nregs: i32,
    hidden: i32,
    extra_captures: Vec<String>,
    forced_self: Option<Option<String>>,
}

impl<'a> Compiler<'a> {
    fn compile(
        checker: &'a Checker,
        func: &'a FnDecl,
        extra: Vec<String>,
        forced_self: Option<Option<String>>,
    ) -> Option<BcFn> {
        let mut c = Compiler {
            checker,
            func,
            out: BcFn::new(&func.name),
            slots: HashMap::new(),
            loop_continue: Vec::new(),
            loop_continue_fix: Vec::new(),
            loop_breaks: Vec::new(),
            await_break: Vec::new(),
            await_continue: Vec::new(),
            stop_block: false,
            loop_seq: 0,
            ntemps: 0,
            nregs: 0,
            hidden: 0,
            extra_captures: extra,
            forced_self,
        };
        c.run().ok()
    }

    fn run(&mut self) -> CResult<BcFn> {
        self.out.arity = self.func.params.len();
        for param in &self.func.params {
            self.alloc_named(param, false);
        }
        let extra = self.extra_captures.clone();
        for cap in extra {
            self.alloc_named(&cap, false);
        }
        if !self.func.loop_break.is_empty() || !self.func.loop_continue.is_empty() {
            self.await_break.push(self.func.loop_break.clone());
            self.await_continue.push(self.func.loop_continue.clone());
        }
        self.compile_block(&self.func.body)?;
        let line = self.func.span.line;
        self.emit(Op::RetVoid, 0, 0, 0, "", line, 1);
        self.out.nregs = self.nregs.max(self.out.locals.len() as i32) as usize;
        Ok(std::mem::replace(&mut self.out, BcFn::new("")))
    }

    fn compile_block(&mut self, stmts: &[Stmt]) -> CResult<()> {
        self.compile_seq(stmts)?;
        Ok(())
    }

    fn compile_seq(&mut self, stmts: &[Stmt]) -> CResult<bool> {
        self.stop_block = false;
        for (i, stmt) in stmts.iter().enumerate() {
            let follow = stmts[i + 1..].to_vec();
            if is_signal_await(stmt) {
                self.compile_signal_await(stmt, &follow)?;
                return Ok(true);
            }
            if stmt.kind == StmtKind::If && contains_signal_await(stmt) {
                self.compile_suspending_if(stmt, &follow)?;
                return Ok(true);
            }
            if stmt.kind == StmtKind::Match && contains_signal_await(stmt) {
                self.compile_suspending_match(stmt, &follow)?;
                return Ok(true);
            }
            if stmt.kind == StmtKind::Do && contains_signal_await(stmt) {
                self.compile_suspending_do(stmt, &follow)?;
                return Ok(true);
            }
            if stmt.kind == StmtKind::While && contains_signal_await(stmt) {
                self.compile_suspending_while(stmt, &follow, &[])?;
                return Ok(true);
            }
            if stmt.kind == StmtKind::For && contains_signal_await(stmt) {
                self.compile_suspending_for(stmt, &follow)?;
                return Ok(true);
            }
            self.compile_stmt(stmt)?;
            if self.stop_block {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn prime_await_locals(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            if stmt.kind == StmtKind::While || stmt.kind == StmtKind::For {
                continue;
            }
            if is_signal_await(stmt) {
                if let Some(expr) = &stmt.expr {
                    for name in &expr.names {
                        self.prime_local(name, stmt.span.line, stmt.span.col);
                    }
                }
            }
            if stmt.kind == StmtKind::Var || stmt.kind == StmtKind::Const {
                self.prime_local(&stmt.name, stmt.span.line, stmt.span.col);
            }
            self.prime_await_locals(&stmt.body);
            self.prime_await_locals(&stmt.else_body);
            for arm in &stmt.arms {
                self.prime_await_locals(&arm.body);
            }
        }
    }

    fn prime_local(&mut self, name: &str, line: u32, col: u32) {
        if name.is_empty() || self.slots.contains_key(name) {
            return;
        }
        let slot = self.alloc_named(name, false);
        let blank = self.load_const(Const::Void, line, col);
        self.emit(Op::Move, slot, blank, 0, "", line, col);
    }

    fn compile_suspending_if(&mut self, stmt: &Stmt, follow: &[Stmt]) -> CResult<()> {
        self.ntemps = 0;
        self.prime_await_locals(&stmt.body);
        self.prime_await_locals(&stmt.else_body);
        self.ntemps = 0;
        let line = stmt.span.line;
        let col = stmt.span.col;
        self.emit(Op::Debug, 0, 0, 0, "", line, col);
        let cond = self.compile_expr(need(stmt.expr.as_ref())?)?;
        let jump_else = self.emit(Op::JumpF, cond, 0, 0, "", line, col);
        let then = concat(&stmt.body, follow);
        self.compile_seq(&then)?;
        let jump_end = self.emit(Op::Jump, 0, 0, 0, "", line, col);
        self.patch(jump_else, self.out.insts.len());
        if stmt.else_body.is_empty() {
            self.compile_seq(follow)?;
        } else {
            let else_body = concat(&stmt.else_body, follow);
            self.compile_seq(&else_body)?;
        }
        self.patch(jump_end, self.out.insts.len());
        Ok(())
    }

    fn compile_suspending_match(&mut self, stmt: &Stmt, follow: &[Stmt]) -> CResult<()> {
        for arm in &stmt.arms {
            self.prime_await_locals(&arm.body);
        }
        self.ntemps = 0;
        let line = stmt.span.line;
        let col = stmt.span.col;
        self.emit(Op::Debug, 0, 0, 0, "", line, col);
        let scrut = self.pin();
        let src = self.compile_expr(need(stmt.expr.as_ref())?)?;
        self.emit(Op::Move, scrut, src, 0, "", line, col);
        let mut ends = Vec::new();
        for arm in &stmt.arms {
            if has_match_binds(arm) && stmts_escape(&arm.body, 0) {
                return Err(Skip);
            }
            self.ntemps = 0;
            let next = match arm.pat {
                Pat::Wildcard => None,
                Pat::Int => Some(self.emit_match_lit(
                    scrut,
                    Const::Int(arm.number),
                    arm.span.line,
                    arm.span.col,
                )),
                Pat::Float => Some(self.emit_match_lit(
                    scrut,
                    Const::Float(arm.real),
                    arm.span.line,
                    arm.span.col,
                )),
                Pat::String => Some(self.emit_match_lit(
                    scrut,
                    Const::Str(arm.text.clone()),
                    arm.span.line,
                    arm.span.col,
                )),
                Pat::Bool => Some(self.emit_match_lit(
                    scrut,
                    Const::Bool(arm.boolean_value),
                    arm.span.line,
                    arm.span.col,
                )),
                Pat::Variant => {
                    let hit = self.alloc_temp();
                    self.emit(
                        Op::MatchVar,
                        hit,
                        scrut,
                        0,
                        &arm.name,
                        arm.span.line,
                        arm.span.col,
                    );
                    Some(self.emit(Op::JumpF, hit, 0, 0, "", arm.span.line, arm.span.col))
                }
            };
            let restores = self.bind_match_arm(arm, scrut, line, col);
            let body_await = contains_signal_stmts(&arm.body);
            if body_await {
                let seq = concat(&arm.body, follow);
                self.compile_seq(&seq)?;
            } else {
                self.compile_block(&arm.body)?;
            }
            for (slot, saved) in restores {
                self.emit(Op::Move, slot, saved, 0, "", line, col);
            }
            if !body_await {
                self.compile_seq(follow)?;
            }
            ends.push(self.emit(Op::Jump, 0, 0, 0, "", line, col));
            if let Some(next) = next {
                self.patch(next, self.out.insts.len());
            }
        }
        let end = self.out.insts.len();
        for jump in ends {
            self.patch(jump, end);
        }
        Ok(())
    }

    fn compile_suspending_do(&mut self, stmt: &Stmt, follow: &[Stmt]) -> CResult<()> {
        self.prime_await_locals(&stmt.body);
        self.prime_await_locals(&stmt.else_body);
        self.ntemps = 0;
        let line = stmt.span.line;
        let col = stmt.span.col;
        self.emit(Op::Debug, 0, 0, 0, "", line, col);
        let dest = if stmt.else_body.is_empty() {
            -1
        } else {
            self.alloc_named(&stmt.name, false)
        };
        let start = self.out.insts.len();
        let body_await = contains_signal_stmts(&stmt.body);
        if body_await {
            let seq = concat(&stmt.body, follow);
            self.compile_seq(&seq)?;
        } else {
            self.compile_block(&stmt.body)?;
        }
        let end = self.out.insts.len();
        if stmt.else_body.is_empty() {
            if !body_await {
                self.compile_seq(follow)?;
            }
            return Ok(());
        }
        let jump_end = self.emit(Op::Jump, 0, 0, 0, "", line, col);
        let handler = self.out.insts.len();
        self.out.handlers.push(Handler {
            start,
            end,
            handler,
            dest,
        });
        let else_await = contains_signal_stmts(&stmt.else_body);
        if else_await {
            let seq = concat(&stmt.else_body, follow);
            self.compile_seq(&seq)?;
            let skip_follow = self.emit(Op::Jump, 0, 0, 0, "", line, col);
            let follow_at = self.out.insts.len();
            if !body_await {
                self.compile_seq(follow)?;
            }
            let end_at = self.out.insts.len();
            self.patch(jump_end, if body_await { end_at } else { follow_at });
            self.patch(skip_follow, end_at);
            return Ok(());
        }
        self.compile_block(&stmt.else_body)?;
        if body_await {
            self.compile_seq(follow)?;
        }
        let follow_at = self.out.insts.len();
        if !body_await {
            self.compile_seq(follow)?;
        }
        let end_at = self.out.insts.len();
        self.patch(jump_end, if body_await { end_at } else { follow_at });
        Ok(())
    }

    fn compile_signal_await(&mut self, stmt: &Stmt, rest: &[Stmt]) -> CResult<()> {
        let await_expr = need(stmt.expr.as_ref())?;
        if await_expr.kids.is_empty() {
            return Err(Skip);
        }
        let mut lambda = FnDecl::new("", span_of(await_expr.span.line, await_expr.span.col));
        lambda.params = await_expr.names.clone();
        lambda.param_types = await_expr.type_args.clone();
        lambda.body = rest.to_vec();
        let mut lam = Expr::new(ExprKind::Lambda, await_expr.span);
        lam.lambda = Some(Box::new(lambda));
        let mut call = Expr::new(ExprKind::MethodCall, await_expr.span);
        call.text = "connect_once".to_string();
        call.kids.push(await_expr.kids[0].clone());
        call.kids.push(lam);
        self.compile_expr(&call)?;
        Ok(())
    }

    fn compile_suspending_while(
        &mut self,
        stmt: &Stmt,
        follow: &[Stmt],
        on_continue: &[Stmt],
    ) -> CResult<()> {
        let line = stmt.span.line;
        let col = stmt.span.col;
        let box_name = self.fresh_loop("$loop");
        let box_slot = self.alloc_named(&box_name, false);
        let mut blank = Expr::new(ExprKind::Null, span_of(line, col));
        let _ = &mut blank;
        let mut arr = Expr::new(ExprKind::Array, span_of(line, col));
        arr.kids.push(Expr::new(ExprKind::Null, span_of(line, col)));
        let arr_reg = self.compile_array(&arr)?;
        self.emit(Op::Move, box_slot, arr_reg, 0, "", line, col);
        let mut declared = HashSet::new();
        let mut carried = Vec::new();
        let mut seen = HashSet::new();
        self.collect_assigned(stmt.body.as_slice(), &mut declared, &mut carried, &mut seen);
        declared.clear();
        self.collect_assigned(on_continue, &mut declared, &mut carried, &mut seen);
        let params = carried;
        let resume = call_resume(&box_name, &params, line, col);
        let mut again = on_continue.to_vec();
        again.push(resume);
        let mut then_body = stmt.body.clone();
        then_body.extend(again.iter().cloned());
        let mut branch = Stmt::new(StmtKind::If, span_of(line, col));
        branch.expr = stmt.expr.clone();
        branch.body = then_body;
        branch.else_body = follow.to_vec();
        let mut loop_fn = FnDecl::new("", span_of(line, col));
        loop_fn.params = params.clone();
        loop_fn.param_types = vec![String::new(); params.len()];
        loop_fn.body = vec![branch];
        loop_fn.loop_break = follow.to_vec();
        loop_fn.loop_continue = again;
        let mut lam = Expr::new(ExprKind::Lambda, span_of(line, col));
        lam.lambda = Some(Box::new(loop_fn));
        let fn_reg = self.compile_lambda(&lam)?;
        let zero = self.load_const(Const::Int(0), line, col);
        self.emit(Op::IndexSet, box_slot, zero, fn_reg, "", line, col);
        let mut args = Vec::new();
        args.push(self.compile_index(&index_of_int(&box_name, 0, line, col))?);
        for name in &params {
            let Some(&slot) = self.slots.get(name) else {
                return Err(Skip);
            };
            args.push(slot);
        }
        let base = self.pack(&args, line, col);
        let dest = self.alloc_temp();
        self.emit(Op::CallVal, dest, base, args.len() as i32, "", line, col);
        Ok(())
    }

    fn compile_suspending_for(&mut self, stmt: &Stmt, follow: &[Stmt]) -> CResult<()> {
        let line = stmt.span.line;
        let col = stmt.span.col;
        let items = self.fresh_loop("$items");
        let idx = self.fresh_loop("$idx");
        let items_slot = self.alloc_named(&items, false);
        let idx_slot = self.alloc_named(&idx, false);
        let iter = self.compile_expr(need(stmt.expr.as_ref())?)?;
        self.emit(Op::IterItems, items_slot, iter, 0, "", line, col);
        let zero = self.load_const(Const::Int(0), line, col);
        self.emit(Op::Move, idx_slot, zero, 0, "", line, col);
        let existed = self.slots.contains_key(&stmt.name);
        let saved = if existed {
            let saved = self.fresh_loop("$save");
            let save_slot = self.alloc_named(&saved, false);
            let cur = self.slots[&stmt.name];
            self.emit(Op::Move, save_slot, cur, 0, "", line, col);
            saved
        } else {
            String::new()
        };
        let mut set_item = Stmt::new(
            if existed {
                StmtKind::Assign
            } else {
                StmtKind::Var
            },
            span_of(line, col),
        );
        set_item.name = stmt.name.clone();
        set_item.expr = Some(index_of_name(&items, &idx, line, col));
        let mut len = Expr::new(ExprKind::Call, span_of(line, col));
        len.text = "len".to_string();
        len.kids.push(var_expr(&items, line, col));
        let mut cond = Expr::new(ExprKind::Binary, span_of(line, col));
        cond.text = "<".to_string();
        cond.kids.push(var_expr(&idx, line, col));
        cond.kids.push(len);
        let mut one = Expr::new(ExprKind::Int, span_of(line, col));
        one.number = 1;
        let mut add = Expr::new(ExprKind::Binary, span_of(line, col));
        add.text = "+".to_string();
        add.kids.push(var_expr(&idx, line, col));
        add.kids.push(one);
        let mut inc = Stmt::new(StmtKind::Assign, span_of(line, col));
        inc.name = idx.clone();
        inc.expr = Some(add);
        let mut loop_stmt = Stmt::new(StmtKind::While, span_of(line, col));
        loop_stmt.expr = Some(cond);
        loop_stmt.body.push(set_item);
        loop_stmt.body.extend(stmt.body.iter().cloned());
        let mut after = follow.to_vec();
        if existed {
            let mut restore = Stmt::new(StmtKind::Assign, span_of(line, col));
            restore.name = stmt.name.clone();
            restore.expr = Some(var_expr(&saved, line, col));
            after.insert(0, restore);
        }
        self.compile_suspending_while(&loop_stmt, &after, &[inc])
    }

    fn collect_assigned(
        &self,
        stmts: &[Stmt],
        declared: &mut HashSet<String>,
        assigned: &mut Vec<String>,
        seen: &mut HashSet<String>,
    ) {
        for stmt in stmts {
            if stmt.kind == StmtKind::Var || stmt.kind == StmtKind::Const {
                declared.insert(stmt.name.clone());
            }
            if stmt.kind == StmtKind::Assign
                && self.slots.contains_key(&stmt.name)
                && !declared.contains(&stmt.name)
                && seen.insert(stmt.name.clone())
            {
                assigned.push(stmt.name.clone());
            }
            let mut inner = declared.clone();
            self.collect_assigned(&stmt.body, &mut inner, assigned, seen);
            let mut inner = declared.clone();
            self.collect_assigned(&stmt.else_body, &mut inner, assigned, seen);
            for arm in &stmt.arms {
                let mut inner = declared.clone();
                inner.extend(arm.binds.iter().cloned());
                self.collect_assigned(&arm.body, &mut inner, assigned, seen);
            }
        }
    }

    fn compile_stmt(&mut self, stmt: &Stmt) -> CResult<()> {
        self.ntemps = 0;
        let line = stmt.span.line;
        let col = stmt.span.col;
        match stmt.kind {
            StmtKind::Comment => Ok(()),
            StmtKind::Pass => {
                self.emit(Op::Debug, 0, 0, 0, "", line, col);
                Ok(())
            }
            StmtKind::Expr => {
                self.emit(Op::Debug, 0, 0, 0, "", line, col);
                if let Some(expr) = &stmt.expr {
                    self.compile_expr(expr)?;
                }
                Ok(())
            }
            StmtKind::Var | StmtKind::Const => {
                self.emit(Op::Debug, 0, 0, 0, "", line, col);
                let rhs = self.compile_expr(need(stmt.expr.as_ref())?)?;
                let slot = self.alloc_named(&stmt.name, stmt.kind == StmtKind::Const);
                self.emit(Op::Move, slot, rhs, 0, "", line, col);
                Ok(())
            }
            StmtKind::Assign => self.compile_assign(stmt),
            StmtKind::Return => {
                self.emit(Op::Debug, 0, 0, 0, "", line, col);
                if stmt.has_expr {
                    let src = self.compile_expr(need(stmt.expr.as_ref())?)?;
                    self.emit(Op::Ret, src, 0, 0, "", line, col);
                } else {
                    self.emit(Op::RetVoid, 0, 0, 0, "", line, col);
                }
                Ok(())
            }
            StmtKind::If => self.compile_if(stmt),
            StmtKind::While => self.compile_while(stmt),
            StmtKind::For => self.compile_for(stmt),
            StmtKind::Match => self.compile_match(stmt),
            StmtKind::IndexAssign => self.compile_index_assign(stmt),
            StmtKind::FieldAssign => self.compile_field_assign(stmt),
            StmtKind::Break => {
                self.emit(Op::Debug, 0, 0, 0, "", line, col);
                if self.loop_breaks.last().is_some() {
                    let at = self.emit(Op::Jump, 0, 0, 0, "", line, col);
                    self.loop_breaks.last_mut().unwrap().push(at);
                    Ok(())
                } else if self.await_break.is_empty() {
                    Err(Skip)
                } else {
                    let seq = self.await_break.last().cloned().unwrap_or_default();
                    self.compile_seq(&seq)?;
                    self.emit(Op::RetVoid, 0, 0, 0, "", line, col);
                    self.stop_block = true;
                    Ok(())
                }
            }
            StmtKind::Continue => {
                self.emit(Op::Debug, 0, 0, 0, "", line, col);
                if let Some(&target) = self.loop_continue.last() {
                    if target >= 0 {
                        self.emit(Op::Jump, target, 0, 0, "", line, col);
                    } else if self.loop_continue_fix.last().is_some() {
                        let at = self.emit(Op::Jump, 0, 0, 0, "", line, col);
                        self.loop_continue_fix.last_mut().unwrap().push(at);
                    }
                    Ok(())
                } else if self.await_continue.is_empty() {
                    Err(Skip)
                } else {
                    let seq = self.await_continue.last().cloned().unwrap_or_default();
                    self.compile_seq(&seq)?;
                    self.emit(Op::RetVoid, 0, 0, 0, "", line, col);
                    self.stop_block = true;
                    Ok(())
                }
            }
            StmtKind::Throw => self.compile_throw(stmt),
            StmtKind::Do => self.compile_do(stmt),
        }
    }

    fn compile_assign(&mut self, stmt: &Stmt) -> CResult<()> {
        let line = stmt.span.line;
        let col = stmt.span.col;
        self.emit(Op::Debug, 0, 0, 0, "", line, col);
        let op = if stmt.op.is_empty() {
            "="
        } else {
            stmt.op.as_str()
        };
        if let Some(&slot) = self.slots.get(&stmt.name) {
            if self.out.local_const.get(slot as usize).copied() == Some(true) {
                return Err(Skip);
            }
            let mut rhs = self.compile_expr(need(stmt.expr.as_ref())?)?;
            if op != "=" {
                let dest = self.alloc_temp();
                let bin = &op[..op.len() - 1];
                self.emit(Op::Bin, dest, slot, rhs, bin, line, col);
                rhs = dest;
            }
            self.emit(Op::Move, slot, rhs, 0, "", line, col);
            return Ok(());
        }
        let Some(&self_slot) = self.slots.get("self") else {
            return Err(Skip);
        };
        if !self.is_self_field(&stmt.name) {
            return Err(Skip);
        }
        let mut rhs = self.compile_expr(need(stmt.expr.as_ref())?)?;
        if op != "=" {
            let cur = self.alloc_temp();
            self.emit(Op::Member, cur, self_slot, 0, &stmt.name, line, col);
            let dest = self.alloc_temp();
            let bin = &op[..op.len() - 1];
            self.emit(Op::Bin, dest, cur, rhs, bin, line, col);
            rhs = dest;
        }
        self.emit(Op::FieldSet, self_slot, rhs, 0, &stmt.name, line, col);
        Ok(())
    }

    fn compile_if(&mut self, stmt: &Stmt) -> CResult<()> {
        let line = stmt.span.line;
        let col = stmt.span.col;
        self.emit(Op::Debug, 0, 0, 0, "", line, col);
        let cond = self.compile_expr(need(stmt.expr.as_ref())?)?;
        let jump_else = self.emit(Op::JumpF, cond, 0, 0, "", line, col);
        self.compile_block(&stmt.body)?;
        if stmt.else_body.is_empty() {
            self.patch(jump_else, self.out.insts.len());
            return Ok(());
        }
        let jump_end = self.emit(Op::Jump, 0, 0, 0, "", line, col);
        self.patch(jump_else, self.out.insts.len());
        self.compile_block(&stmt.else_body)?;
        self.patch(jump_end, self.out.insts.len());
        Ok(())
    }

    fn compile_while(&mut self, stmt: &Stmt) -> CResult<()> {
        let line = stmt.span.line;
        let col = stmt.span.col;
        self.emit(Op::Debug, 0, 0, 0, "", line, col);
        let loop_at = self.out.insts.len() as i32;
        self.loop_continue.push(loop_at);
        self.loop_continue_fix.push(Vec::new());
        self.loop_breaks.push(Vec::new());
        let cond = self.compile_expr(need(stmt.expr.as_ref())?)?;
        let jump_end = self.emit(Op::JumpF, cond, 0, 0, "", line, col);
        self.compile_block(&stmt.body)?;
        self.emit(Op::Jump, loop_at, 0, 0, "", line, col);
        let end = self.out.insts.len();
        self.patch(jump_end, end);
        let breaks = self.loop_breaks.pop().unwrap_or_default();
        for br in breaks {
            self.patch(br, end);
        }
        self.loop_continue_fix.pop();
        self.loop_continue.pop();
        Ok(())
    }

    fn compile_for(&mut self, stmt: &Stmt) -> CResult<()> {
        let line = stmt.span.line;
        let col = stmt.span.col;
        self.emit(Op::Debug, 0, 0, 0, "", line, col);
        let existed = self.slots.contains_key(&stmt.name);
        let saved = if existed { self.pin() } else { -1 };
        let items = self.pin();
        let idx = self.pin();
        let len = self.pin();
        let item_slot = self.alloc_named(&stmt.name, false);
        let iter = self.compile_expr(need(stmt.expr.as_ref())?)?;
        self.emit(Op::IterItems, items, iter, 0, "", line, col);
        if existed {
            self.emit(Op::Move, saved, item_slot, 0, "", line, col);
        }
        let zero = self.load_const(Const::Int(0), line, col);
        self.emit(Op::Move, idx, zero, 0, "", line, col);
        self.emit(Op::Len, len, items, 0, "", line, col);
        let loop_at = self.out.insts.len() as i32;
        self.loop_continue.push(-1);
        self.loop_continue_fix.push(Vec::new());
        self.loop_breaks.push(Vec::new());
        let cond = self.alloc_temp();
        self.emit(Op::Bin, cond, idx, len, "<", line, col);
        let jump_end = self.emit(Op::JumpF, cond, 0, 0, "", line, col);
        self.emit(Op::IndexGet, item_slot, items, idx, "", line, col);
        self.compile_block(&stmt.body)?;
        let cont = self.out.insts.len();
        let fixes = self.loop_continue_fix.pop().unwrap_or_default();
        for j in fixes {
            self.patch(j, cont);
        }
        self.loop_continue.pop();
        self.ntemps = 0;
        let one = self.load_const(Const::Int(1), line, col);
        let next = self.alloc_temp();
        self.emit(Op::Bin, next, idx, one, "+", line, col);
        self.emit(Op::Move, idx, next, 0, "", line, col);
        self.emit(Op::Jump, loop_at, 0, 0, "", line, col);
        let end = self.out.insts.len();
        self.patch(jump_end, end);
        if existed {
            self.emit(Op::Move, item_slot, saved, 0, "", line, col);
        }
        let breaks = self.loop_breaks.pop().unwrap_or_default();
        for br in breaks {
            self.patch(br, end);
        }
        Ok(())
    }

    fn compile_throw(&mut self, stmt: &Stmt) -> CResult<()> {
        let line = stmt.span.line;
        let col = stmt.span.col;
        self.emit(Op::Debug, 0, 0, 0, "", line, col);
        let src = self.compile_expr(need(stmt.expr.as_ref())?)?;
        self.emit(Op::Throw, src, 0, 0, "", line, col);
        Ok(())
    }

    fn compile_do(&mut self, stmt: &Stmt) -> CResult<()> {
        let line = stmt.span.line;
        let col = stmt.span.col;
        self.emit(Op::Debug, 0, 0, 0, "", line, col);
        let dest = if stmt.else_body.is_empty() {
            -1
        } else {
            self.alloc_named(&stmt.name, false)
        };
        let start = self.out.insts.len();
        self.compile_block(&stmt.body)?;
        let end = self.out.insts.len();
        if stmt.else_body.is_empty() {
            return Ok(());
        }
        let jump_end = self.emit(Op::Jump, 0, 0, 0, "", line, col);
        let handler = self.out.insts.len();
        self.out.handlers.push(Handler {
            start,
            end,
            handler,
            dest,
        });
        self.compile_block(&stmt.else_body)?;
        self.patch(jump_end, self.out.insts.len());
        Ok(())
    }

    fn compile_match(&mut self, stmt: &Stmt) -> CResult<()> {
        let line = stmt.span.line;
        let col = stmt.span.col;
        self.emit(Op::Debug, 0, 0, 0, "", line, col);
        let scrut = self.pin();
        let src = self.compile_expr(need(stmt.expr.as_ref())?)?;
        self.emit(Op::Move, scrut, src, 0, "", line, col);
        let mut ends = Vec::new();
        for arm in &stmt.arms {
            if has_match_binds(arm) && stmts_escape(&arm.body, 0) {
                return Err(Skip);
            }
            self.ntemps = 0;
            let next = match arm.pat {
                Pat::Wildcard => None,
                Pat::Int => Some(self.emit_match_lit(
                    scrut,
                    Const::Int(arm.number),
                    arm.span.line,
                    arm.span.col,
                )),
                Pat::Float => Some(self.emit_match_lit(
                    scrut,
                    Const::Float(arm.real),
                    arm.span.line,
                    arm.span.col,
                )),
                Pat::String => Some(self.emit_match_lit(
                    scrut,
                    Const::Str(arm.text.clone()),
                    arm.span.line,
                    arm.span.col,
                )),
                Pat::Bool => Some(self.emit_match_lit(
                    scrut,
                    Const::Bool(arm.boolean_value),
                    arm.span.line,
                    arm.span.col,
                )),
                Pat::Variant => {
                    let hit = self.alloc_temp();
                    self.emit(
                        Op::MatchVar,
                        hit,
                        scrut,
                        0,
                        &arm.name,
                        arm.span.line,
                        arm.span.col,
                    );
                    Some(self.emit(Op::JumpF, hit, 0, 0, "", arm.span.line, arm.span.col))
                }
            };
            let restores = self.bind_match_arm(arm, scrut, line, col);
            self.compile_block(&arm.body)?;
            for (slot, saved) in restores {
                self.emit(Op::Move, slot, saved, 0, "", line, col);
            }
            ends.push(self.emit(Op::Jump, 0, 0, 0, "", line, col));
            if let Some(next) = next {
                self.patch(next, self.out.insts.len());
            }
        }
        let end = self.out.insts.len();
        for jump in ends {
            self.patch(jump, end);
        }
        Ok(())
    }

    fn emit_match_lit(&mut self, scrut: i32, pat: Const, line: u32, col: u32) -> usize {
        let k = self.load_const(pat, line, col);
        let hit = self.alloc_temp();
        self.emit(Op::MatchLit, hit, scrut, k, "", line, col);
        self.emit(Op::JumpF, hit, 0, 0, "", line, col)
    }

    fn bind_match_arm(
        &mut self,
        arm: &rosegold_syntax::MatchArm,
        scrut: i32,
        line: u32,
        col: u32,
    ) -> Vec<(i32, i32)> {
        let mut restores = Vec::new();
        if arm.pat != Pat::Variant {
            return restores;
        }
        let named = arm.field_names.iter().any(|f| !f.is_empty());
        if named {
            for (i, name) in arm.binds.iter().enumerate() {
                let field = arm.field_names.get(i).map(String::as_str).unwrap_or("");
                if field.is_empty() {
                    continue;
                }
                self.bind_payload(name, scrut, 0, field, line, col, &mut restores);
            }
            return restores;
        }
        if arm.binds.len() == 1 {
            self.bind_payload(&arm.binds[0], scrut, -1, "", line, col, &mut restores);
            return restores;
        }
        for (i, name) in arm.binds.iter().enumerate() {
            self.bind_payload(name, scrut, i as i32, "", line, col, &mut restores);
        }
        restores
    }

    fn bind_payload(
        &mut self,
        name: &str,
        scrut: i32,
        index: i32,
        field: &str,
        line: u32,
        col: u32,
        restores: &mut Vec<(i32, i32)>,
    ) {
        if name.is_empty() || name == "_" {
            return;
        }
        let existed = self.slots.contains_key(name);
        let saved = if existed { self.pin() } else { -1 };
        if existed {
            let cur = self.slots[name];
            self.emit(Op::Move, saved, cur, 0, "", line, col);
        }
        let slot = self.alloc_named(name, false);
        self.emit(Op::Payload, slot, scrut, index, field, line, col);
        if existed {
            restores.push((slot, saved));
        }
    }

    fn compile_index_assign(&mut self, stmt: &Stmt) -> CResult<()> {
        let target = need(stmt.target.as_ref())?;
        if target.kids.len() < 2 {
            return Err(Skip);
        }
        let line = stmt.span.line;
        let col = stmt.span.col;
        self.emit(Op::Debug, 0, 0, 0, "", line, col);
        let obj = self.compile_expr(&target.kids[0])?;
        let index = self.compile_expr(&target.kids[1])?;
        let mut rhs = self.compile_expr(need(stmt.expr.as_ref())?)?;
        let op = if stmt.op.is_empty() {
            "="
        } else {
            stmt.op.as_str()
        };
        if op != "=" {
            let cur = self.alloc_temp();
            self.emit(Op::IndexGet, cur, obj, index, "", line, col);
            let dest = self.alloc_temp();
            self.emit(Op::Bin, dest, cur, rhs, &op[..op.len() - 1], line, col);
            rhs = dest;
        }
        self.emit(Op::IndexSet, obj, index, rhs, "", line, col);
        Ok(())
    }

    fn compile_field_assign(&mut self, stmt: &Stmt) -> CResult<()> {
        let target = need(stmt.target.as_ref())?;
        let line = stmt.span.line;
        let col = stmt.span.col;
        self.emit(Op::Debug, 0, 0, 0, "", line, col);
        let obj = self.compile_recv(target)?;
        let mut rhs = self.compile_expr(need(stmt.expr.as_ref())?)?;
        let op = if stmt.op.is_empty() {
            "="
        } else {
            stmt.op.as_str()
        };
        if op != "=" {
            let cur = self.alloc_temp();
            self.emit(Op::Member, cur, obj, 0, &stmt.name, line, col);
            let dest = self.alloc_temp();
            self.emit(Op::Bin, dest, cur, rhs, &op[..op.len() - 1], line, col);
            rhs = dest;
        }
        self.emit(Op::FieldSet, obj, rhs, 0, &stmt.name, line, col);
        Ok(())
    }

    fn compile_expr(&mut self, e: &Expr) -> CResult<i32> {
        let line = e.span.line;
        let col = e.span.col;
        match e.kind {
            ExprKind::Int => Ok(self.load_const(Const::Int(e.number), line, col)),
            ExprKind::Float => Ok(self.load_const(Const::Float(e.real), line, col)),
            ExprKind::String => Ok(self.load_const(Const::Str(e.text.clone()), line, col)),
            ExprKind::Bool => Ok(self.load_const(Const::Bool(e.boolean_value), line, col)),
            ExprKind::Null => Ok(self.load_const(Const::Void, line, col)),
            ExprKind::Var => self.compile_var(e),
            ExprKind::Unary => {
                let src = self.compile_expr(need(e.kids.first())?)?;
                let dest = self.alloc_temp();
                self.emit(Op::Unary, dest, src, 0, &e.text, line, col);
                Ok(dest)
            }
            ExprKind::Binary => self.compile_binary(e),
            ExprKind::Call => self.compile_call(e),
            ExprKind::Lambda => self.compile_lambda(e),
            ExprKind::MethodCall => self.compile_method_call(e),
            ExprKind::StructLit => self.compile_struct_lit(e),
            ExprKind::Member => self.compile_member(e),
            ExprKind::Array => self.compile_array(e),
            ExprKind::Map => self.compile_map(e),
            ExprKind::Range => self.compile_range(e),
            ExprKind::Index => self.compile_index(e),
            ExprKind::Try | ExprKind::Cast => self.compile_expr(need(e.kids.first())?),
            ExprKind::Is => self.compile_is(e),
            ExprKind::If => self.compile_if_expr(e),
            ExprKind::Format => self.compile_format(e),
            ExprKind::New => self.compile_new(e),
            ExprKind::NodePath => self.compile_node_path(e),
            ExprKind::Await => self.compile_await(e),
        }
    }

    fn compile_var(&mut self, e: &Expr) -> CResult<i32> {
        let line = e.span.line;
        let col = e.span.col;
        if let Some(&slot) = self.slots.get(&e.text) {
            return Ok(slot);
        }
        if let Some(&self_slot) = self.slots.get("self") {
            if self.is_self_field(&e.text) || self.is_self_signal(&e.text) {
                let dest = self.alloc_temp();
                self.emit(Op::Member, dest, self_slot, 0, &e.text, line, col);
                return Ok(dest);
            }
        }
        if self.is_fn_name(&e.text) {
            let dest = self.alloc_temp();
            self.emit(Op::LoadFn, dest, 0, 0, &e.text, line, col);
            return Ok(dest);
        }
        if self.checker.find_enum(&e.text).is_some() {
            let dest = self.alloc_temp();
            self.emit(Op::LoadEnum, dest, 0, 0, &e.text, line, col);
            return Ok(dest);
        }
        if self.checker.signal_arity.borrow().contains_key(&e.text) {
            let dest = self.alloc_temp();
            self.emit(Op::LoadSignal, dest, 0, 0, &e.text, line, col);
            return Ok(dest);
        }
        if self.checker.singletons.borrow().contains(&e.text) {
            let dest = self.alloc_temp();
            self.emit(Op::LoadSingleton, dest, 0, 0, &e.text, line, col);
            return Ok(dest);
        }
        if let Some(named) = self.checker.lookup_const("", &e.text) {
            return Ok(self.load_const(Const::Int(named), line, col));
        }
        Err(Skip)
    }

    fn compile_binary(&mut self, e: &Expr) -> CResult<i32> {
        let line = e.span.line;
        let col = e.span.col;
        if e.text == "&&" || e.text == "||" {
            let and = e.text == "&&";
            let dest = self.alloc_temp();
            let left = self.compile_expr(need(e.kids.first())?)?;
            self.emit(Op::Truthy, dest, left, 0, "", line, col);
            let skip = self.emit(
                if and { Op::JumpF } else { Op::JumpT },
                dest,
                0,
                0,
                "",
                line,
                col,
            );
            let right = self.compile_expr(need(e.kids.get(1))?)?;
            self.emit(Op::Truthy, dest, right, 0, "", line, col);
            self.patch(skip, self.out.insts.len());
            return Ok(dest);
        }
        let lhs = self.compile_expr(need(e.kids.first())?)?;
        let rhs = self.compile_expr(need(e.kids.get(1))?)?;
        let dest = self.alloc_temp();
        self.emit(Op::Bin, dest, lhs, rhs, &e.text, line, col);
        Ok(dest)
    }

    fn compile_call(&mut self, e: &Expr) -> CResult<i32> {
        let line = e.span.line;
        let col = e.span.col;
        if !e.module.is_empty() {
            return Err(Skip);
        }
        if e.text.is_empty() {
            if e.kids.is_empty() {
                return Err(Skip);
            }
            let mut packed = Vec::new();
            for kid in &e.kids {
                packed.push(self.compile_expr(kid)?);
            }
            let base = self.pack(&packed, line, col);
            let dest = self.alloc_temp();
            self.emit(Op::CallVal, dest, base, packed.len() as i32, "", line, col);
            return Ok(dest);
        }
        if self.slots.contains_key(&e.text) && !self.is_fn_name(&e.text) {
            let mut packed = vec![self.slots[&e.text]];
            for kid in &e.kids {
                packed.push(self.compile_expr(kid)?);
            }
            let base = self.pack(&packed, line, col);
            let dest = self.alloc_temp();
            self.emit(Op::CallVal, dest, base, packed.len() as i32, "", line, col);
            return Ok(dest);
        }
        let mut callee = self
            .checker
            .find_local_fn(&e.text)
            .or_else(|| self.checker.fns.borrow().get(&e.text).cloned());
        let self_slot = self.slots.get("self").copied();
        let ty = self.self_type();
        if callee.is_none() {
            if let (Some(self_slot), Some(ty)) = (self_slot, ty.clone()) {
                if let Some(method) = self.checker.lookup_method(&ty, &e.text).1 {
                    let full = call_args(Some(&method), &e.kids, true);
                    let mut packed = vec![self_slot];
                    for arg in &full {
                        packed.push(self.compile_expr(arg)?);
                    }
                    let base = self.pack(&packed, line, col);
                    let dest = self.alloc_temp();
                    self.emit(
                        Op::Method,
                        dest,
                        base,
                        packed.len() as i32,
                        &e.text,
                        line,
                        col,
                    );
                    return Ok(dest);
                }
            }
        }
        let _ = &mut callee;
        let full = call_args(callee.as_deref(), &e.kids, false);
        let mut args = Vec::new();
        for arg in &full {
            args.push(self.compile_expr(arg)?);
        }
        let base = self.pack(&args, line, col);
        let dest = self.alloc_temp();
        let idx = self.emit(Op::Call, dest, base, args.len() as i32, &e.text, line, col);
        if let Some(callee) = &callee {
            self.out.insts[idx].s = callee.name.clone();
            self.out.insts[idx].module = callee.module.clone();
        }
        Ok(dest)
    }

    fn compile_lambda(&mut self, e: &Expr) -> CResult<i32> {
        let lambda = e.lambda.as_deref().ok_or(Skip)?;
        let cap_names = self.lambda_captures(lambda);
        let code = self.lower_lambda(lambda, &cap_names);
        let mut caps = Vec::new();
        let mut names = Vec::new();
        let mut flags = Vec::new();
        for name in &cap_names {
            let Some(&slot) = self.slots.get(name) else {
                return Err(Skip);
            };
            caps.push(slot);
            names.push(name.clone());
            let is_const = self.out.local_const.get(slot as usize).copied() == Some(true);
            flags.push(is_const);
        }
        let line = e.span.line;
        let col = e.span.col;
        let base = self.pack(&caps, line, col);
        let dest = self.alloc_temp();
        let idx = self.emit(Op::Closure, dest, base, caps.len() as i32, "", line, col);
        self.out.insts[idx].proto = Some(Rc::new(lambda.clone()));
        self.out.insts[idx].code = code;
        self.out.insts[idx].names = names;
        self.out.insts[idx].flags = flags;
        Ok(dest)
    }

    fn lower_lambda(&self, lambda: &FnDecl, captures: &[String]) -> Option<Rc<BcFn>> {
        if lambda.is_abstract {
            return None;
        }
        Compiler::compile(
            self.checker,
            lambda,
            captures.to_vec(),
            Some(self.self_type()),
        )
        .map(Rc::new)
    }

    fn lambda_captures(&self, lambda: &FnDecl) -> Vec<String> {
        let mut bound: HashSet<String> = lambda.params.iter().cloned().collect();
        let mut free = HashSet::new();
        for st in &lambda.body {
            super::collect_free_stmt(st, &mut bound, &mut free);
        }
        let mut cap_names = Vec::new();
        let mut captured_self = false;
        let mut names: Vec<String> = free.into_iter().collect();
        names.sort();
        for name in names {
            if self.skip_lambda_capture(&name) {
                continue;
            }
            if self.slots.contains_key(&name) {
                if name == "self" {
                    captured_self = true;
                }
                cap_names.push(name);
                continue;
            }
            if let Some(ty) = self.self_type() {
                if self.slots.contains_key("self")
                    && self.checker.lookup_method(&ty, &name).1.is_some()
                {
                    if !captured_self {
                        cap_names.push("self".to_string());
                        captured_self = true;
                    }
                    continue;
                }
            }
            if (self.is_self_field(&name) || self.is_self_signal(&name))
                && self.slots.contains_key("self")
                && !captured_self
            {
                cap_names.push("self".to_string());
                captured_self = true;
            }
        }
        cap_names
    }

    fn skip_lambda_capture(&self, name: &str) -> bool {
        super::skip_capture(name)
            || self.checker.find_local_fn(name).is_some()
            || self.checker.fns.borrow().contains_key(name)
            || self.checker.structs.borrow().contains_key(name)
            || self.checker.all_types.borrow().contains_key(name)
            || self.checker.enums.borrow().contains_key(name)
            || self.checker.traits.borrow().contains_key(name)
            || self.checker.signal_arity.borrow().contains_key(name)
            || self.checker.module_binds.borrow().contains_key(name)
            || is_host_module(name)
    }

    fn compile_method_call(&mut self, e: &Expr) -> CResult<i32> {
        if e.kids.is_empty() {
            return Err(Skip);
        }
        let recv = &e.kids[0];
        let line = e.span.line;
        let col = e.span.col;
        if recv.kind == ExprKind::Var {
            if recv.text == "super" {
                return self.compile_super_call(e);
            }
            if !self.slots.contains_key(&recv.text) {
                let (defined_on, st) = self.checker.lookup_static(&recv.text, &e.text);
                if st.is_some() && self.checker.find_struct(&recv.text).is_some() {
                    return self.compile_static_call(e, &defined_on, st.as_ref().unwrap());
                }
                if let Some(bound) = self.checker.find_module_bind(&recv.text) {
                    return self.compile_qualified(&bound, &e.text, &e.kids, 1, line, col);
                }
                if is_host_module(&recv.text) || recv.text == "Future" {
                    return self.compile_qualified(&recv.text, &e.text, &e.kids, 1, line, col);
                }
            }
        }
        if let Some(nested) = self.module_path(recv)? {
            return self.compile_qualified(&nested, &e.text, &e.kids, 1, line, col);
        }
        let recv_ty = self.recv_type_hint(recv);
        let ufcs = self
            .checker
            .find_ufcs(&e.text, recv_ty.as_deref().unwrap_or(""));
        let type_method = recv_ty
            .as_ref()
            .is_some_and(|ty| self.checker.lookup_method(ty, &e.text).1.is_some());
        let mut use_ufcs = ufcs.is_some() && method_op_id(&e.text) < 0 && !type_method;
        if use_ufcs && recv.kind == ExprKind::Var {
            use_ufcs = !self.is_type_method_name(&e.text);
        }
        if use_ufcs {
            let ufcs = ufcs.unwrap();
            let mut packed = Vec::new();
            for kid in &e.kids {
                packed.push(self.compile_expr(kid)?);
            }
            let base = self.pack(&packed, line, col);
            let dest = self.alloc_temp();
            let idx = self.emit(
                Op::Call,
                dest,
                base,
                packed.len() as i32,
                &ufcs.name,
                line,
                col,
            );
            self.out.insts[idx].module = ufcs.module.clone();
            return Ok(dest);
        }
        let provided: Vec<Expr> = e.kids.iter().skip(1).cloned().collect();
        let method = recv_ty
            .as_ref()
            .and_then(|ty| self.checker.lookup_method(ty, &e.text).1);
        let full = call_args(method.as_deref(), &provided, true);
        let mut packed = vec![self.compile_expr(recv)?];
        for arg in &full {
            packed.push(self.compile_expr(arg)?);
        }
        let base = self.pack(&packed, line, col);
        let dest = self.alloc_temp();
        self.emit(
            Op::Method,
            dest,
            base,
            packed.len() as i32,
            &e.text,
            line,
            col,
        );
        Ok(dest)
    }

    fn compile_node_path(&mut self, e: &Expr) -> CResult<i32> {
        let Some(&self_slot) = self.slots.get("self") else {
            return Err(Skip);
        };
        let mut text = e.text.clone();
        if e.boolean_value {
            text = format!("%{text}");
        }
        let line = e.span.line;
        let col = e.span.col;
        let path = self.load_const(Const::Str(text), line, col);
        let base = self.pack(&[self_slot, path], line, col);
        let dest = self.alloc_temp();
        self.emit(Op::Method, dest, base, 2, "get_node", line, col);
        Ok(dest)
    }

    fn compile_is(&mut self, e: &Expr) -> CResult<i32> {
        if e.kids.is_empty() {
            return Err(Skip);
        }
        let head = type_head(&e.text);
        let line = e.span.line;
        let col = e.span.col;
        let obj = self.compile_expr(&e.kids[0])?;
        if self.checker.is_engine_class(&head) {
            let name = self.load_const(Const::Str(head), line, col);
            let base = self.pack(&[obj, name], line, col);
            let dest = self.alloc_temp();
            self.emit(Op::Method, dest, base, 2, "is_class", line, col);
            return Ok(dest);
        }
        if self.checker.native_base(&head).is_empty() {
            return Err(Skip);
        }
        Err(Skip)
    }

    fn compile_new(&mut self, e: &Expr) -> CResult<i32> {
        let head = type_head(&e.text);
        if self.checker.is_engine_class(&head) {
            return self.compile_engine_new(&head, e);
        }
        if !self.checker.native_base(&head).is_empty() {
            return Err(Skip);
        }
        let mut lit = Expr::new(ExprKind::StructLit, e.span);
        lit.text = head;
        self.compile_struct_lit(&lit)
    }

    fn compile_engine_new(&mut self, type_name: &str, e: &Expr) -> CResult<i32> {
        let line = e.span.line;
        let col = e.span.col;
        let self_reg = self.alloc_temp();
        self.emit(Op::LoadSingleton, self_reg, 0, 0, "ClassDB", line, col);
        let name = self.load_const(Const::Str(type_name.to_string()), line, col);
        let base = self.pack(&[self_reg, name], line, col);
        let dest = self.alloc_temp();
        self.emit(Op::Method, dest, base, 2, "instantiate", line, col);
        Ok(dest)
    }

    fn compile_static_call(&mut self, e: &Expr, defined_on: &str, func: &FnDecl) -> CResult<i32> {
        let line = e.span.line;
        let col = e.span.col;
        let provided: Vec<Expr> = e.kids.iter().skip(1).cloned().collect();
        let full = call_args(Some(func), &provided, false);
        let mut packed = Vec::new();
        for arg in &full {
            packed.push(self.compile_expr(arg)?);
        }
        let base = self.pack(&packed, line, col);
        let dest = self.alloc_temp();
        let file = func.file.replace('\\', "/");
        if file.ends_with("/godot/lib.rg") {
            let idx = self.emit(
                Op::Static,
                dest,
                base,
                packed.len() as i32,
                &e.text,
                line,
                col,
            );
            self.out.insts[idx].module = defined_on.to_string();
            return Ok(dest);
        }
        let idx = self.emit(
            Op::Call,
            dest,
            base,
            packed.len() as i32,
            &func.name,
            line,
            col,
        );
        self.out.insts[idx].module = defined_on.to_string();
        Ok(dest)
    }

    fn compile_super_call(&mut self, e: &Expr) -> CResult<i32> {
        let Some(&self_slot) = self.slots.get("self") else {
            return Err(Skip);
        };
        let line = e.span.line;
        let col = e.span.col;
        let provided: Vec<Expr> = e.kids.iter().skip(1).cloned().collect();
        let parent = self.super_start_type();
        let method = if parent.is_empty() {
            None
        } else {
            self.checker.lookup_method(&parent, &e.text).1
        };
        let full = call_args(method.as_deref(), &provided, true);
        let mut packed = vec![self_slot];
        for arg in &full {
            packed.push(self.compile_expr(arg)?);
        }
        let base = self.pack(&packed, line, col);
        let dest = self.alloc_temp();
        let idx = self.emit(
            Op::Super,
            dest,
            base,
            packed.len() as i32,
            &e.text,
            line,
            col,
        );
        self.out.insts[idx].module = parent;
        Ok(dest)
    }

    fn compile_qualified(
        &mut self,
        module: &str,
        name: &str,
        kids: &[Expr],
        arg_start: usize,
        line: u32,
        col: u32,
    ) -> CResult<i32> {
        let mut args = Vec::new();
        for kid in kids.iter().skip(arg_start) {
            args.push(self.compile_expr(kid)?);
        }
        let base = self.pack(&args, line, col);
        let dest = self.alloc_temp();
        let idx = self.emit(Op::Call, dest, base, args.len() as i32, name, line, col);
        self.out.insts[idx].module = module.to_string();
        Ok(dest)
    }

    fn compile_await(&mut self, e: &Expr) -> CResult<i32> {
        if e.kids.is_empty() {
            return Err(Skip);
        }
        let src = self.compile_expr(&e.kids[0])?;
        let dest = self.alloc_temp();
        self.emit(Op::Await, dest, src, 0, "", e.span.line, e.span.col);
        Ok(dest)
    }

    fn compile_array(&mut self, e: &Expr) -> CResult<i32> {
        let mut elems = Vec::new();
        for kid in &e.kids {
            elems.push(self.compile_expr(kid)?);
        }
        let base = self.pack(&elems, e.span.line, e.span.col);
        let dest = self.alloc_temp();
        self.emit(
            Op::Array,
            dest,
            base,
            elems.len() as i32,
            "",
            e.span.line,
            e.span.col,
        );
        Ok(dest)
    }

    fn compile_map(&mut self, e: &Expr) -> CResult<i32> {
        if e.kids.len() % 2 != 0 {
            return Err(Skip);
        }
        let mut kids = Vec::new();
        for kid in &e.kids {
            kids.push(self.compile_expr(kid)?);
        }
        let base = self.pack(&kids, e.span.line, e.span.col);
        let dest = self.alloc_temp();
        self.emit(
            Op::Map,
            dest,
            base,
            kids.len() as i32,
            "",
            e.span.line,
            e.span.col,
        );
        Ok(dest)
    }

    fn compile_struct_lit(&mut self, e: &Expr) -> CResult<i32> {
        let Some(decl) = self.checker.find_struct(&e.text) else {
            return Err(Skip);
        };
        let (name, fields, field_types, field_optional) = {
            let decl = decl.borrow();
            (
                decl.name.clone(),
                decl.fields.clone(),
                decl.field_types.clone(),
                decl.field_optional.clone(),
            )
        };
        let mut provided: HashMap<String, i32> = HashMap::new();
        let mut order = Vec::new();
        for (i, field) in e.names.iter().enumerate() {
            if !fields.iter().any(|f| f == field) || order.iter().any(|f| f == field) {
                return Err(Skip);
            }
            let reg = self.compile_expr(need(e.kids.get(i))?)?;
            provided.insert(field.clone(), reg);
            order.push(field.clone());
        }
        let defaults = self
            .checker
            .field_defaults
            .borrow()
            .get(&name)
            .cloned()
            .unwrap_or_default();
        let mut vals = Vec::new();
        for (i, field) in fields.iter().enumerate() {
            if let Some(reg) = provided.get(field) {
                vals.push(*reg);
                continue;
            }
            if let Some(default) = defaults.get(field) {
                vals.push(self.compile_expr(default)?);
                continue;
            }
            if field_optional.get(i).copied().unwrap_or(false) {
                let ty = field_types.get(i).map(String::as_str).unwrap_or("");
                vals.push(self.load_zero(ty, e.span.line, e.span.col)?);
                continue;
            }
            return Err(Skip);
        }
        let base = self.pack(&vals, e.span.line, e.span.col);
        let dest = self.alloc_temp();
        self.emit(
            Op::Struct,
            dest,
            base,
            vals.len() as i32,
            &e.text,
            e.span.line,
            e.span.col,
        );
        Ok(dest)
    }

    fn compile_member(&mut self, e: &Expr) -> CResult<i32> {
        if e.kids.is_empty() {
            return Err(Skip);
        }
        let recv = &e.kids[0];
        if recv.kind == ExprKind::Var
            && !self.slots.contains_key(&recv.text)
            && self.checker.find_struct(&recv.text).is_some()
        {
            if let Some(named) = self.checker.lookup_const(&recv.text, &e.text) {
                return Ok(self.load_const(Const::Int(named), e.span.line, e.span.col));
            }
        }
        let obj = self.compile_recv(recv)?;
        let dest = self.alloc_temp();
        self.emit(Op::Member, dest, obj, 0, &e.text, e.span.line, e.span.col);
        Ok(dest)
    }

    fn compile_range(&mut self, e: &Expr) -> CResult<i32> {
        if e.kids.len() < 2 {
            return Err(Skip);
        }
        let start = self.compile_expr(&e.kids[0])?;
        let end = self.compile_expr(&e.kids[1])?;
        let dest = self.alloc_temp();
        let op = if e.boolean_value { "..=" } else { ".." };
        self.emit(Op::Range, dest, start, end, op, e.span.line, e.span.col);
        Ok(dest)
    }

    fn compile_index(&mut self, e: &Expr) -> CResult<i32> {
        if e.kids.len() < 2 {
            return Err(Skip);
        }
        let obj = self.compile_expr(&e.kids[0])?;
        let idx = self.compile_expr(&e.kids[1])?;
        let dest = self.alloc_temp();
        self.emit(Op::IndexGet, dest, obj, idx, "", e.span.line, e.span.col);
        Ok(dest)
    }

    fn compile_format(&mut self, e: &Expr) -> CResult<i32> {
        let mut acc = self.load_const(Const::Str(String::new()), e.span.line, e.span.col);
        for kid in &e.kids {
            let rhs = self.compile_expr(kid)?;
            let dest = self.alloc_temp();
            self.emit(Op::Bin, dest, acc, rhs, "+", e.span.line, e.span.col);
            acc = dest;
        }
        Ok(acc)
    }

    fn compile_if_expr(&mut self, e: &Expr) -> CResult<i32> {
        if e.kids.len() < 3 {
            return Err(Skip);
        }
        let dest = self.alloc_temp();
        let cond = self.compile_expr(&e.kids[0])?;
        let jump_else = self.emit(Op::JumpF, cond, 0, 0, "", e.span.line, e.span.col);
        let then = self.compile_expr(&e.kids[1])?;
        self.emit(Op::Move, dest, then, 0, "", e.span.line, e.span.col);
        let jump_end = self.emit(Op::Jump, 0, 0, 0, "", e.span.line, e.span.col);
        self.patch(jump_else, self.out.insts.len());
        let else_reg = self.compile_expr(&e.kids[2])?;
        self.emit(Op::Move, dest, else_reg, 0, "", e.span.line, e.span.col);
        self.patch(jump_end, self.out.insts.len());
        Ok(dest)
    }

    fn compile_recv(&mut self, e: &Expr) -> CResult<i32> {
        if e.kind == ExprKind::Var && e.text == "super" {
            return self.slots.get("self").copied().ok_or(Skip);
        }
        self.compile_expr(e)
    }

    fn module_path(&self, e: &Expr) -> CResult<Option<String>> {
        if e.kind == ExprKind::Var {
            if self.slots.contains_key(&e.text) {
                return Ok(None);
            }
            return Ok(self.checker.find_module_bind(&e.text));
        }
        if e.kind == ExprKind::Member && !e.kids.is_empty() {
            let Some(parent) = self.module_path(&e.kids[0])? else {
                return Ok(None);
            };
            let loaded = self.checker.loaded.borrow();
            let Some(lit) = loaded.get(&parent) else {
                return Err(Skip);
            };
            let lit = lit.borrow();
            let internal = self.checker.current_module() == parent;
            let hit = if internal {
                lit.modules.get(&e.text).cloned()
            } else {
                lit.export_mods.get(&e.text).cloned()
            };
            return hit.map(Some).ok_or(Skip);
        }
        Ok(None)
    }

    fn recv_type_hint(&self, recv: &Expr) -> Option<String> {
        if recv.kind == ExprKind::StructLit {
            return Some(recv.text.clone());
        }
        if recv.kind == ExprKind::Var {
            if recv.text == "self" {
                return self.self_type();
            }
            if !self.slots.contains_key(&recv.text) && self.is_self_field(&recv.text) {
                let ty = self.self_type()?;
                let st = self.checker.find_struct(&ty)?;
                let st = st.borrow();
                let field_ty = type_of_field(&st, &recv.text);
                if field_ty.is_empty() {
                    return None;
                }
                return Some(type_head(&field_ty));
            }
        }
        None
    }

    fn self_type(&self) -> Option<String> {
        if let Some(forced) = &self.forced_self {
            return forced.clone().filter(|s| !s.is_empty());
        }
        if !self.func.params.iter().any(|p| p == "self") && !self.slots.contains_key("self") {
            return None;
        }
        let methods = self.checker.type_methods.borrow();
        for (ty, map) in methods.iter() {
            for func in map.values() {
                if func.name == self.func.name
                    && func.file == self.func.file
                    && func.span.offset == self.func.span.offset
                    && func.module == self.func.module
                {
                    return Some(ty.clone());
                }
            }
        }
        None
    }

    fn super_start_type(&self) -> String {
        let Some(ty) = self.self_type() else {
            return String::new();
        };
        self.checker
            .class_parents
            .borrow()
            .get(&ty)
            .map(|p| type_head(p))
            .unwrap_or_default()
    }

    fn is_fn_name(&self, name: &str) -> bool {
        self.checker.find_local_fn(name).is_some() || self.checker.fns.borrow().contains_key(name)
    }

    fn is_self_field(&self, name: &str) -> bool {
        let Some(ty) = self.self_type() else {
            return false;
        };
        let Some(st) = self.checker.find_struct(&ty) else {
            return false;
        };
        let fields = st.borrow().fields.clone();
        fields.iter().any(|f| f == name)
    }

    fn is_self_signal(&self, name: &str) -> bool {
        let Some(ty) = self.self_type() else {
            return false;
        };
        self.checker.lookup_type_signal(&ty, name).is_some()
    }

    fn is_type_method_name(&self, name: &str) -> bool {
        self.checker
            .type_methods
            .borrow()
            .values()
            .any(|methods| methods.contains_key(name))
    }

    fn load_zero(&mut self, ty: &str, line: u32, col: u32) -> CResult<i32> {
        let head = type_head(ty);
        if head == "Array" {
            let dest = self.alloc_temp();
            self.emit(Op::Array, dest, 0, 0, "", line, col);
            return Ok(dest);
        }
        if head == "Map" {
            let dest = self.alloc_temp();
            self.emit(Op::Map, dest, 0, 0, "", line, col);
            return Ok(dest);
        }
        let c = match zero_of_type(ty) {
            crate::value::Value::Int(n) => Const::Int(n),
            crate::value::Value::Float(n) => Const::Float(n),
            crate::value::Value::Bool(b) => Const::Bool(b),
            crate::value::Value::String(s) => Const::Str(s),
            _ => Const::Void,
        };
        Ok(self.load_const(c, line, col))
    }

    fn load_const(&mut self, value: Const, line: u32, col: u32) -> i32 {
        let k = self.intern(value);
        let dest = self.alloc_temp();
        self.emit(Op::LoadConst, dest, k, 0, "", line, col);
        dest
    }

    fn intern(&mut self, value: Const) -> i32 {
        if internable(&value) {
            for (i, c) in self.out.constants.iter().enumerate() {
                if const_eq(c, &value) {
                    return i as i32;
                }
            }
        }
        self.out.constants.push(value);
        self.out.constants.len() as i32 - 1
    }

    fn alloc_named(&mut self, name: &str, is_const: bool) -> i32 {
        if let Some(&existing) = self.slots.get(name) {
            if let Some(slot) = self.out.local_const.get_mut(existing as usize) {
                *slot = is_const;
            }
            self.touch(existing);
            return existing;
        }
        let r = self.out.locals.len() as i32;
        self.slots.insert(name.to_string(), r);
        self.out.locals.push(name.to_string());
        self.out.local_const.push(is_const);
        self.touch(r);
        r
    }

    fn alloc_temp(&mut self) -> i32 {
        let r = self.out.locals.len() as i32 + self.ntemps;
        self.ntemps += 1;
        self.touch(r);
        r
    }

    fn touch(&mut self, r: i32) {
        self.nregs = self.nregs.max(r + 1);
    }

    fn pin(&mut self) -> i32 {
        self.hidden += 1;
        let name = format!("#{}", self.hidden);
        self.alloc_named(&name, false)
    }

    fn pack(&mut self, srcs: &[i32], line: u32, col: u32) -> i32 {
        if srcs.is_empty() {
            return 0;
        }
        let base = self.alloc_temp();
        self.emit(Op::Move, base, srcs[0], 0, "", line, col);
        for src in srcs.iter().skip(1) {
            let dest = self.alloc_temp();
            self.emit(Op::Move, dest, *src, 0, "", line, col);
        }
        base
    }

    fn emit(&mut self, op: Op, a: i32, b: i32, c: i32, s: &str, line: u32, col: u32) -> usize {
        self.out.insts.push(Inst::new(op, a, b, c, s, line, col));
        self.out.insts.len() - 1
    }

    fn patch(&mut self, index: usize, target: usize) {
        let inst = &mut self.out.insts[index];
        if inst.op == Op::Jump {
            inst.a = target as i32;
        } else {
            inst.b = target as i32;
        }
    }

    fn fresh_loop(&mut self, prefix: &str) -> String {
        self.loop_seq += 1;
        format!("{prefix}{}", self.loop_seq)
    }
}

fn need<T>(value: Option<T>) -> CResult<T> {
    value.ok_or(Skip)
}

fn span_of(line: u32, col: u32) -> Span {
    Span::new(line, col, 0)
}

fn var_expr(name: &str, line: u32, col: u32) -> Expr {
    let mut e = Expr::new(ExprKind::Var, span_of(line, col));
    e.text = name.to_string();
    e
}

fn index_of_int(name: &str, at: i64, line: u32, col: u32) -> Expr {
    let mut ix = Expr::new(ExprKind::Index, span_of(line, col));
    ix.kids.push(var_expr(name, line, col));
    let mut key = Expr::new(ExprKind::Int, span_of(line, col));
    key.number = at;
    ix.kids.push(key);
    ix
}

fn index_of_name(name: &str, key_name: &str, line: u32, col: u32) -> Expr {
    let mut ix = Expr::new(ExprKind::Index, span_of(line, col));
    ix.kids.push(var_expr(name, line, col));
    ix.kids.push(var_expr(key_name, line, col));
    ix
}

fn call_resume(box_name: &str, params: &[String], line: u32, col: u32) -> Stmt {
    let mut call = Expr::new(ExprKind::Call, span_of(line, col));
    call.kids.push(index_of_int(box_name, 0, line, col));
    for name in params {
        call.kids.push(var_expr(name, line, col));
    }
    let mut stmt = Stmt::new(StmtKind::Expr, span_of(line, col));
    stmt.expr = Some(call);
    stmt
}

fn concat(first: &[Stmt], second: &[Stmt]) -> Vec<Stmt> {
    let mut out = first.to_vec();
    out.extend(second.iter().cloned());
    out
}

fn call_args(func: Option<&FnDecl>, provided: &[Expr], method: bool) -> Vec<Expr> {
    let mut all = provided.to_vec();
    let Some(func) = func else {
        return all;
    };
    let off = if method { 1 } else { 0 };
    let max = func.params.len().saturating_sub(off);
    while all.len() < max {
        match func
            .param_defaults
            .get(off + all.len())
            .and_then(|d| d.clone())
        {
            Some(default) => all.push(default),
            None => break,
        }
    }
    all
}

fn is_signal_await(stmt: &Stmt) -> bool {
    stmt.kind == StmtKind::Expr
        && stmt
            .expr
            .as_ref()
            .is_some_and(|e| e.kind == ExprKind::Await && e.boolean_value)
}

fn contains_signal_await(stmt: &Stmt) -> bool {
    if is_signal_await(stmt) {
        return true;
    }
    contains_signal_stmts(&stmt.body)
        || contains_signal_stmts(&stmt.else_body)
        || stmt.arms.iter().any(|arm| contains_signal_stmts(&arm.body))
}

fn contains_signal_stmts(stmts: &[Stmt]) -> bool {
    stmts.iter().any(contains_signal_await)
}

fn has_match_binds(arm: &rosegold_syntax::MatchArm) -> bool {
    arm.pat == Pat::Variant && arm.binds.iter().any(|name| !name.is_empty() && name != "_")
}

fn stmts_escape(stmts: &[Stmt], loops: i32) -> bool {
    for stmt in stmts {
        match stmt.kind {
            StmtKind::Break | StmtKind::Continue if loops == 0 => return true,
            StmtKind::While | StmtKind::For => {
                if stmts_escape(&stmt.body, loops + 1) || stmts_escape(&stmt.else_body, loops + 1) {
                    return true;
                }
            }
            StmtKind::If | StmtKind::Do => {
                if stmts_escape(&stmt.body, loops) || stmts_escape(&stmt.else_body, loops) {
                    return true;
                }
            }
            StmtKind::Match => {
                if stmt.arms.iter().any(|arm| stmts_escape(&arm.body, loops)) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
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

fn type_of_field(st: &rosegold_syntax::StructDecl, name: &str) -> String {
    st.fields
        .iter()
        .position(|f| f == name)
        .and_then(|i| st.field_types.get(i).cloned())
        .unwrap_or_default()
}

fn internable(value: &Const) -> bool {
    !matches!(value, Const::Void) || true
}

fn const_eq(a: &Const, b: &Const) -> bool {
    match (a, b) {
        (Const::Void, Const::Void) => true,
        (Const::Bool(a), Const::Bool(b)) => a == b,
        (Const::Int(a), Const::Int(b)) => a == b,
        (Const::Float(a), Const::Float(b)) => a.to_bits() == b.to_bits(),
        (Const::Str(a), Const::Str(b)) => a == b,
        _ => false,
    }
}
