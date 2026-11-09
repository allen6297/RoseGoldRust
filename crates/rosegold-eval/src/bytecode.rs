//! Register bytecode. The dump text matches `Bytecode.Fn.dump` in the Java compiler.

use std::rc::Rc;

use rosegold_syntax::FnDecl;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    LoadConst,
    LoadFn,
    LoadEnum,
    LoadSignal,
    LoadSingleton,
    Move,
    Bin,
    Unary,
    Truthy,
    Jump,
    JumpF,
    JumpT,
    Call,
    CallVal,
    Array,
    Range,
    IndexGet,
    IndexSet,
    IterItems,
    Len,
    Map,
    Method,
    Static,
    Super,
    Struct,
    Member,
    FieldSet,
    MatchLit,
    MatchVar,
    Payload,
    Throw,
    Closure,
    Await,
    Ret,
    RetVoid,
    Debug,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Const {
    Void,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl Const {
    pub fn java_dump(&self) -> String {
        match self {
            Const::Void => "Void".to_string(),
            Const::Bool(b) => format!("Bool {b}"),
            Const::Int(n) => format!("Int {n}"),
            Const::Float(n) => format!("Float {}", java_double(*n)),
            Const::Str(s) => format!("String {s}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Inst {
    pub op: Op,
    pub a: i32,
    pub b: i32,
    pub c: i32,
    pub s: String,
    pub module: String,
    pub proto: Option<Rc<FnDecl>>,
    pub code: Option<Rc<Fn>>,
    pub names: Vec<String>,
    pub flags: Vec<bool>,
    pub line: u32,
    pub col: u32,
}

impl Inst {
    pub fn new(op: Op, a: i32, b: i32, c: i32, s: impl Into<String>, line: u32, col: u32) -> Self {
        Self {
            op,
            a,
            b,
            c,
            s: s.into(),
            module: String::new(),
            proto: None,
            code: None,
            names: Vec::new(),
            flags: Vec::new(),
            line: if line == 0 { 1 } else { line },
            col: if col == 0 { 1 } else { col },
        }
    }
}

#[derive(Clone, Debug)]
pub struct Handler {
    pub start: usize,
    pub end: usize,
    pub handler: usize,
    pub dest: i32,
}

#[derive(Clone, Debug)]
pub struct Fn {
    pub name: String,
    pub arity: usize,
    pub nregs: usize,
    pub insts: Vec<Inst>,
    pub handlers: Vec<Handler>,
    pub constants: Vec<Const>,
    pub locals: Vec<String>,
    pub local_const: Vec<bool>,
}

impl Fn {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            arity: 0,
            nregs: 0,
            insts: Vec::new(),
            handlers: Vec::new(),
            constants: Vec::new(),
            locals: Vec::new(),
            local_const: Vec::new(),
        }
    }

    pub fn dump(&self) -> String {
        let mut ss = String::new();
        ss.push_str("fn ");
        ss.push_str(&self.name);
        ss.push('(');
        for i in 0..self.arity {
            if i > 0 {
                ss.push_str(", ");
            }
            let name = self.locals.get(i).map(String::as_str).unwrap_or("");
            if !name.is_empty() {
                ss.push_str(name);
            } else {
                ss.push_str(&format!("r{i}"));
            }
        }
        ss.push_str(") regs=");
        ss.push_str(&self.nregs.to_string());
        ss.push('\n');
        for (i, inst) in self.insts.iter().enumerate() {
            ss.push_str("  ");
            ss.push_str(&i.to_string());
            ss.push_str("  ");
            ss.push_str(&format_inst(inst));
            ss.push('\n');
        }
        for h in &self.handlers {
            ss.push_str(&format!(
                "  handler {}..{} -> {} r{}\n",
                h.start, h.end, h.handler, h.dest
            ));
        }
        for (i, c) in self.constants.iter().enumerate() {
            ss.push_str("  const ");
            ss.push_str(&i.to_string());
            ss.push_str("  ");
            ss.push_str(&c.java_dump());
            ss.push('\n');
        }
        ss
    }
}

fn format_inst(inst: &Inst) -> String {
    let a = inst.a;
    let b = inst.b;
    let c = inst.c;
    match inst.op {
        Op::LoadConst => format!("LOAD_CONST r{a} k{b}"),
        Op::LoadFn => format!("LOAD_FN r{a} {}", inst.s),
        Op::LoadEnum => format!("LOAD_ENUM r{a} {}", inst.s),
        Op::LoadSignal => format!("LOAD_SIGNAL r{a} {}", inst.s),
        Op::LoadSingleton => format!("LOAD_SINGLETON r{a} {}", inst.s),
        Op::Static => format!("STATIC r{a} {}.{} base=r{b} argc={c}", inst.module, inst.s),
        Op::Move => format!("MOVE r{a} r{b}"),
        Op::Bin => format!("BIN {} r{a} r{b} r{c}", inst.s),
        Op::Unary => format!("UNARY {} r{a} r{b}", inst.s),
        Op::Truthy => format!("TRUTHY r{a} r{b}"),
        Op::Jump => format!("JUMP {a}"),
        Op::JumpF => format!("JUMP_F r{a} {b}"),
        Op::JumpT => format!("JUMP_T r{a} {b}"),
        Op::Call => {
            let name = if inst.module.is_empty() {
                inst.s.clone()
            } else {
                format!("{}.{}", inst.module, inst.s)
            };
            format!("CALL {name} r{a} base={b} argc={c}")
        }
        Op::CallVal => format!("CALL_VAL r{a} base={b} argc={c}"),
        Op::Array => format!("ARRAY r{a} base={b} argc={c}"),
        Op::Range => {
            let tag = if inst.s == "..=" { "_IN" } else { "" };
            format!("RANGE{tag} r{a} r{b} r{c}")
        }
        Op::IndexGet => format!("INDEX_GET r{a} r{b} r{c}"),
        Op::IndexSet => format!("INDEX_SET r{a} r{b} r{c}"),
        Op::IterItems => format!("ITER_ITEMS r{a} r{b}"),
        Op::Len => format!("LEN r{a} r{b}"),
        Op::Map => format!("MAP r{a} base={b} argc={c}"),
        Op::Method => format!("METHOD {} r{a} base={b} argc={c}", inst.s),
        Op::Super => {
            let start = if inst.module.is_empty() {
                String::new()
            } else {
                format!("{}.", inst.module)
            };
            format!("SUPER {start}{} r{a} base={b} argc={c}", inst.s)
        }
        Op::Struct => format!("STRUCT {} r{a} base={b} argc={c}", inst.s),
        Op::Member => format!("MEMBER {} r{a} r{b}", inst.s),
        Op::FieldSet => format!("FIELD_SET {} r{a} r{b}", inst.s),
        Op::MatchLit => format!("MATCH_LIT r{a} r{b} r{c}"),
        Op::MatchVar => format!("MATCH_VAR r{a} r{b} {}", inst.s),
        Op::Payload => {
            let tail = if inst.s.is_empty() {
                c.to_string()
            } else {
                inst.s.clone()
            };
            format!("PAYLOAD r{a} r{b} {tail}")
        }
        Op::Throw => format!("THROW r{a}"),
        Op::Closure => format!("CLOSURE r{a} base={b} argc={c}"),
        Op::Await => format!("AWAIT r{a} r{b}"),
        Op::Ret => format!("RET r{a}"),
        Op::RetVoid => "RET_VOID".to_string(),
        Op::Debug => format!("DEBUG {}:{}", inst.line, inst.col),
    }
}

fn java_double(n: f64) -> String {
    if n.is_nan() {
        return "NaN".to_string();
    }
    if n.is_infinite() {
        return if n.is_sign_positive() {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        };
    }
    let s = format!("{n}");
    if !s.contains('.') && !s.contains('e') && !s.contains('E') {
        format!("{s}.0")
    } else {
        s
    }
}
