//! Register machine. Operations that need the language runtime call back into
//! the tree-walk interpreter, the same way `Vm.java` calls `Interp`.

use std::collections::HashMap;

use crate::bytecode::{Const, Fn as BcFn, Op};
use crate::value::{Binding, Value};

use super::{Halt, Interp};

pub fn exec(
    ip: &mut Interp,
    func: &BcFn,
    args: &[Value],
    caps: Option<&HashMap<String, Binding>>,
) -> Result<Value, Halt> {
    let mut regs = vec![Value::Void; func.nregs.max(1)];
    if let Some(caps) = caps {
        for (i, name) in func.locals.iter().enumerate() {
            if name.is_empty() || name.starts_with('#') {
                continue;
            }
            if let Some(b) = caps.get(name) {
                regs[i] = b.value.clone();
            }
        }
    }
    for (i, arg) in args.iter().enumerate().take(func.arity) {
        regs[i] = arg.clone();
    }
    let mut pc = 0usize;
    while pc < func.insts.len() {
        let inst = func.insts[pc].clone();
        let stepped = step(ip, func, &inst, &mut regs, pc);
        match stepped {
            Ok(Step::Next(next)) => pc = next,
            Ok(Step::Done(value)) => return Ok(value),
            Err(err) => {
                let thrown = match &err {
                    Halt::Throw { value, .. } => value.clone(),
                    Halt::Runtime(msg) => Value::String(msg.clone()),
                };
                match unwind(func, pc, &mut regs, thrown) {
                    Some(next) => pc = next,
                    None => return Err(err),
                }
            }
        }
    }
    Ok(Value::Void)
}

enum Step {
    Next(usize),
    Done(Value),
}

fn step(
    ip: &mut Interp,
    func: &BcFn,
    inst: &crate::bytecode::Inst,
    regs: &mut [Value],
    pc: usize,
) -> Result<Step, Halt> {
    let next = pc + 1;
    match inst.op {
        Op::LoadConst => {
            regs[inst.a as usize] = const_value(&func.constants[inst.b as usize]);
            Ok(Step::Next(next))
        }
        Op::LoadFn => {
            regs[inst.a as usize] = Value::FnRef {
                name: inst.s.clone(),
                clo: None,
            };
            Ok(Step::Next(next))
        }
        Op::LoadEnum => {
            regs[inst.a as usize] = Value::EnumType(inst.s.clone());
            Ok(Step::Next(next))
        }
        Op::LoadSignal => {
            regs[inst.a as usize] = ip.global_signal(&inst.s, inst.line, inst.col)?;
            Ok(Step::Next(next))
        }
        Op::LoadSingleton => Err(Halt::Runtime(format!(
            "'{}' is available when the script runs in Godot",
            inst.s
        ))),
        Op::Static => Err(Halt::Runtime(format!(
            "'{}.{}' is available when the script runs in Godot",
            inst.module, inst.s
        ))),
        Op::Move => {
            regs[inst.a as usize] = regs[inst.b as usize].clone();
            Ok(Step::Next(next))
        }
        Op::Bin => {
            let lhs = regs[inst.b as usize].clone();
            let rhs = regs[inst.c as usize].clone();
            regs[inst.a as usize] = ip.apply_binop(&inst.s, lhs, rhs, inst.line, inst.col)?;
            Ok(Step::Next(next))
        }
        Op::Unary => {
            let src = regs[inst.b as usize].clone();
            regs[inst.a as usize] = ip.apply_unary(&inst.s, src, inst.line, inst.col)?;
            Ok(Step::Next(next))
        }
        Op::Truthy => {
            let bit = regs[inst.b as usize].truthy();
            regs[inst.a as usize] = Value::Bool(bit);
            Ok(Step::Next(next))
        }
        Op::Jump => Ok(Step::Next(inst.a as usize)),
        Op::JumpF => {
            if regs[inst.a as usize].truthy() {
                Ok(Step::Next(next))
            } else {
                Ok(Step::Next(inst.b as usize))
            }
        }
        Op::JumpT => {
            if regs[inst.a as usize].truthy() {
                Ok(Step::Next(inst.b as usize))
            } else {
                Ok(Step::Next(next))
            }
        }
        Op::Call => {
            let call_args = take(&regs, inst.b, inst.c);
            regs[inst.a as usize] =
                ip.call_qualified(&inst.module, &inst.s, call_args, inst.line, inst.col)?;
            Ok(Step::Next(next))
        }
        Op::CallVal => {
            if inst.c < 1 {
                return Err(ip.runtime("can only call a function", inst.line, inst.col));
            }
            let callee = regs[inst.b as usize].clone();
            let val_args = if inst.c <= 1 {
                Vec::new()
            } else {
                take(&regs, inst.b + 1, inst.c - 1)
            };
            regs[inst.a as usize] = ip.call_fn_value(callee, val_args, inst.line, inst.col)?;
            Ok(Step::Next(next))
        }
        Op::Array => {
            regs[inst.a as usize] = Value::array(take(&regs, inst.b, inst.c));
            Ok(Step::Next(next))
        }
        Op::Range => {
            let start = regs[inst.b as usize].clone();
            let end = regs[inst.c as usize].clone();
            let Value::Int(start_n) = start else {
                return Err(ip.runtime("range start must be Int", inst.line, inst.col));
            };
            let Value::Int(end_n) = end else {
                return Err(ip.runtime("range end must be Int", inst.line, inst.col));
            };
            regs[inst.a as usize] = Value::Range {
                start: start_n,
                end: end_n,
                inclusive: inst.s == "..=",
            };
            Ok(Step::Next(next))
        }
        Op::IndexGet => {
            let obj = regs[inst.b as usize].clone();
            let idx = regs[inst.c as usize].clone();
            regs[inst.a as usize] = ip.index_get(&obj, &idx, inst.line, inst.col)?;
            Ok(Step::Next(next))
        }
        Op::IndexSet => {
            let obj = regs[inst.a as usize].clone();
            let idx = regs[inst.b as usize].clone();
            let rhs = regs[inst.c as usize].clone();
            ip.index_set(&obj, &idx, rhs, inst.line, inst.col)?;
            Ok(Step::Next(next))
        }
        Op::IterItems => {
            let iter = regs[inst.b as usize].clone();
            let items = ip.iter_items(iter, inst.line, inst.col)?;
            regs[inst.a as usize] = Value::array(items);
            Ok(Step::Next(next))
        }
        Op::Len => {
            let arg = regs[inst.b as usize].clone();
            regs[inst.a as usize] = ip.call("", "len", vec![arg], inst.line, inst.col)?;
            Ok(Step::Next(next))
        }
        Op::Map => {
            let kids = take(&regs, inst.b, inst.c);
            regs[inst.a as usize] = ip.map_values(&kids, inst.line, inst.col)?;
            Ok(Step::Next(next))
        }
        Op::Method => {
            let recv = regs[inst.b as usize].clone();
            let method_args = if inst.c <= 1 {
                Vec::new()
            } else {
                take(&regs, inst.b + 1, inst.c - 1)
            };
            regs[inst.a as usize] =
                ip.call_method(recv, &inst.s, method_args, inst.line, inst.col)?;
            Ok(Step::Next(next))
        }
        Op::Super => {
            let recv = regs[inst.b as usize].clone();
            let super_args = if inst.c <= 1 {
                Vec::new()
            } else {
                take(&regs, inst.b + 1, inst.c - 1)
            };
            regs[inst.a as usize] =
                ip.call_super_from(recv, &inst.module, &inst.s, super_args, inst.line, inst.col)?;
            Ok(Step::Next(next))
        }
        Op::Struct => {
            let fields = take(&regs, inst.b, inst.c);
            regs[inst.a as usize] = ip.struct_lit_filled(&inst.s, &fields, inst.line, inst.col)?;
            Ok(Step::Next(next))
        }
        Op::Member => {
            let obj = regs[inst.b as usize].clone();
            regs[inst.a as usize] = ip.read_member(obj, &inst.s, inst.line, inst.col)?;
            Ok(Step::Next(next))
        }
        Op::FieldSet => {
            let obj = regs[inst.a as usize].clone();
            let rhs = regs[inst.b as usize].clone();
            ip.set_field(&obj, &inst.s, rhs, inst.line, inst.col)?;
            Ok(Step::Next(next))
        }
        Op::MatchLit => {
            let hit = ip.match_lit(&regs[inst.b as usize], &regs[inst.c as usize]);
            regs[inst.a as usize] = Value::Bool(hit);
            Ok(Step::Next(next))
        }
        Op::MatchVar => {
            let hit = ip.match_variant(&regs[inst.b as usize], &inst.s, inst.line, inst.col)?;
            regs[inst.a as usize] = Value::Bool(hit);
            Ok(Step::Next(next))
        }
        Op::Payload => {
            if let Some(bound) = ip.match_bind(
                &regs[inst.b as usize],
                inst.c as i64,
                &inst.s,
                inst.line,
                inst.col,
            )? {
                regs[inst.a as usize] = bound;
            }
            Ok(Step::Next(next))
        }
        Op::Throw => Err(Halt::Throw {
            value: regs[inst.a as usize].clone(),
            line: inst.line,
            col: inst.col,
        }),
        Op::Closure => {
            let cap_vals = take(&regs, inst.b, inst.c);
            let Some(proto) = inst.proto.as_ref() else {
                return Err(ip.runtime("invalid closure", inst.line, inst.col));
            };
            regs[inst.a as usize] = ip.make_bc_closure(
                proto,
                inst.code.clone(),
                &inst.names,
                &cap_vals,
                &inst.flags,
            );
            Ok(Step::Next(next))
        }
        Op::Await => {
            let Value::Future(fut) = regs[inst.b as usize].clone() else {
                return Err(ip.runtime("can only await a Future", inst.line, inst.col));
            };
            regs[inst.a as usize] = ip.await_future(&fut, inst.line, inst.col)?;
            Ok(Step::Next(next))
        }
        Op::Ret => Ok(Step::Done(regs[inst.a as usize].clone())),
        Op::RetVoid => Ok(Step::Done(Value::Void)),
        Op::Debug => Ok(Step::Next(next)),
    }
}

fn unwind(func: &BcFn, pc: usize, regs: &mut [Value], thrown: Value) -> Option<usize> {
    for h in &func.handlers {
        if pc >= h.start && pc < h.end && h.dest >= 0 {
            regs[h.dest as usize] = thrown;
            return Some(h.handler);
        }
    }
    None
}

fn take(regs: &[Value], base: i32, argc: i32) -> Vec<Value> {
    if argc <= 0 {
        return Vec::new();
    }
    let start = base as usize;
    let end = start + argc as usize;
    regs[start..end].to_vec()
}

fn const_value(c: &Const) -> Value {
    match c {
        Const::Void => Value::Void,
        Const::Bool(b) => Value::Bool(*b),
        Const::Int(n) => Value::Int(*n),
        Const::Float(n) => Value::Float(*n),
        Const::Str(s) => Value::String(s.clone()),
    }
}
