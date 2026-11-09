use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use rosegold_syntax::FnDecl;

pub const MAX_RANGE: i64 = 1_000_000;
pub const MAX_CALL_DEPTH: i32 = 256;
pub const MAX_SYNC_EMIT_DEPTH: i32 = 64;
pub const MAX_REPEAT_UNITS: i64 = 16 * 1024 * 1024;
pub const MAX_SLEEP_MS: i64 = 60_000;
pub const MAX_EVENT_LOOP: i32 = 100_000;

#[derive(Clone)]
pub struct Binding {
    pub value: Value,
    pub is_const: bool,
}

pub struct ClosureData {
    pub func: Rc<FnDecl>,
    pub caps: HashMap<String, Binding>,
    pub code: Option<Rc<crate::bytecode::Fn>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FutState {
    Pending,
    Ready,
    Failed,
}

pub struct FutureData {
    pub state: FutState,
    pub result: Value,
    pub error: Value,
    pub err_line: u32,
    pub err_col: u32,
}

impl FutureData {
    pub fn pending() -> Self {
        Self {
            state: FutState::Pending,
            result: Value::Void,
            error: Value::Void,
            err_line: 1,
            err_col: 1,
        }
    }
}

pub struct StructData {
    pub name: String,
    pub order: Vec<String>,
    pub fields: HashMap<String, Value>,
    pub listeners: HashMap<String, Vec<Value>>,
}

pub struct MapData {
    pub order: Vec<String>,
    pub fields: HashMap<String, Value>,
}

#[derive(Clone)]
pub enum Value {
    Void,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    FnRef {
        name: String,
        clo: Option<Rc<ClosureData>>,
    },
    Array(Rc<RefCell<Vec<Value>>>),
    Range {
        start: i64,
        end: i64,
        inclusive: bool,
    },
    Struct(Rc<RefCell<StructData>>),
    Map(Rc<RefCell<MapData>>),
    SignalRef {
        name: String,
        owner: Option<Rc<RefCell<StructData>>>,
    },
    EnumType(String),
    Enum {
        ty: String,
        variant: String,
        payload: Vec<Value>,
    },
    Future(Rc<RefCell<FutureData>>),
}

impl Value {
    pub fn array(items: Vec<Value>) -> Self {
        Value::Array(Rc::new(RefCell::new(items)))
    }

    pub fn map(data: MapData) -> Self {
        Value::Map(Rc::new(RefCell::new(data)))
    }

    pub fn struc(data: StructData) -> Self {
        Value::Struct(Rc::new(RefCell::new(data)))
    }

    pub fn future(data: FutureData) -> Self {
        Value::Future(Rc::new(RefCell::new(data)))
    }

    pub fn closure(data: ClosureData) -> Self {
        Value::FnRef {
            name: "<fn>".to_string(),
            clo: Some(Rc::new(data)),
        }
    }

    pub fn fn_ref(name: impl Into<String>) -> Self {
        Value::FnRef {
            name: name.into(),
            clo: None,
        }
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self, Value::Int(_) | Value::Float(_))
    }

    pub fn as_f64(&self) -> f64 {
        match self {
            Value::Float(n) => *n,
            Value::Int(n) => *n as f64,
            _ => 0.0,
        }
    }

    pub fn truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Int(n) => *n != 0,
            Value::Float(n) => *n != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::FnRef { .. }
            | Value::Array(_)
            | Value::Range { .. }
            | Value::Struct(_)
            | Value::Map(_)
            | Value::SignalRef { .. }
            | Value::EnumType(_)
            | Value::Enum { .. }
            | Value::Future(_) => true,
            Value::Void => false,
        }
    }

    pub fn type_name(&self) -> String {
        match self {
            Value::Struct(rec) => rec.borrow().name.clone(),
            Value::Enum { ty, .. } | Value::EnumType(ty) => ty.clone(),
            Value::Array(_) => "Array".to_string(),
            Value::Map(_) => "Map".to_string(),
            Value::String(_) => "String".to_string(),
            Value::Future(_) => "Future".to_string(),
            Value::Int(_) => "Int".to_string(),
            Value::Float(_) => "Float".to_string(),
            Value::Bool(_) => "Bool".to_string(),
            _ => String::new(),
        }
    }

    pub fn equals_value(&self, other: &Value) -> bool {
        self.equals_rec(other, &mut HashSet::new(), &mut HashSet::new())
    }

    fn equals_rec(
        &self,
        other: &Value,
        seen_a: &mut HashSet<usize>,
        seen_b: &mut HashSet<usize>,
    ) -> bool {
        if std::mem::discriminant(self) != std::mem::discriminant(other) {
            if self.is_numeric() && other.is_numeric() {
                return numeric_eq(self.as_f64(), other.as_f64());
            }
            return false;
        }
        match (self, other) {
            (Value::Void, Value::Void) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => numeric_eq(*a, *b),
            (Value::String(a), Value::String(b)) | (Value::EnumType(a), Value::EnumType(b)) => {
                a == b
            }
            (Value::FnRef { name: na, clo: ca }, Value::FnRef { name: nb, clo: cb }) => {
                if ca.is_some() || cb.is_some() {
                    match (ca, cb) {
                        (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                        _ => false,
                    }
                } else {
                    na == nb
                }
            }
            (
                Value::Range {
                    start: a,
                    end: ae,
                    inclusive: ai,
                },
                Value::Range {
                    start: b,
                    end: be,
                    inclusive: bi,
                },
            ) => a == b && ae == be && ai == bi,
            (Value::Array(a), Value::Array(b)) => {
                if Rc::ptr_eq(a, b) {
                    return true;
                }
                let (av, bv) = (a.borrow(), b.borrow());
                if av.len() != bv.len() {
                    return false;
                }
                let pa = Rc::as_ptr(a) as usize;
                let pb = Rc::as_ptr(b) as usize;
                if !seen_a.insert(pa) || !seen_b.insert(pb) {
                    return false;
                }
                for (x, y) in av.iter().zip(bv.iter()) {
                    if !x.equals_rec(y, seen_a, seen_b) {
                        return false;
                    }
                }
                true
            }
            (Value::Struct(a), Value::Struct(b)) => {
                if Rc::ptr_eq(a, b) {
                    return true;
                }
                let (av, bv) = (a.borrow(), b.borrow());
                if av.name != bv.name || av.fields.len() != bv.fields.len() {
                    return false;
                }
                let pa = Rc::as_ptr(a) as usize;
                let pb = Rc::as_ptr(b) as usize;
                if !seen_a.insert(pa) || !seen_b.insert(pb) {
                    return false;
                }
                for (k, v) in &av.fields {
                    match bv.fields.get(k) {
                        Some(rhs) if v.equals_rec(rhs, seen_a, seen_b) => {}
                        _ => return false,
                    }
                }
                true
            }
            (Value::Map(a), Value::Map(b)) => {
                if Rc::ptr_eq(a, b) {
                    return true;
                }
                let (av, bv) = (a.borrow(), b.borrow());
                if av.fields.len() != bv.fields.len() {
                    return false;
                }
                let pa = Rc::as_ptr(a) as usize;
                let pb = Rc::as_ptr(b) as usize;
                if !seen_a.insert(pa) || !seen_b.insert(pb) {
                    return false;
                }
                for (k, v) in &av.fields {
                    match bv.fields.get(k) {
                        Some(rhs) if v.equals_rec(rhs, seen_a, seen_b) => {}
                        _ => return false,
                    }
                }
                true
            }
            (
                Value::SignalRef {
                    name: na,
                    owner: oa,
                },
                Value::SignalRef {
                    name: nb,
                    owner: ob,
                },
            ) => {
                if na != nb {
                    return false;
                }
                match (oa, ob) {
                    (None, None) => true,
                    (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                    _ => false,
                }
            }
            (Value::Future(a), Value::Future(b)) => Rc::ptr_eq(a, b),
            (
                Value::Enum {
                    ty: ta,
                    variant: va,
                    payload: pa,
                },
                Value::Enum {
                    ty: tb,
                    variant: vb,
                    payload: pb,
                },
            ) => {
                if ta != tb || va != vb || pa.len() != pb.len() {
                    return false;
                }
                pa.iter()
                    .zip(pb.iter())
                    .all(|(x, y)| x.equals_rec(y, seen_a, seen_b))
            }
            _ => false,
        }
    }

    pub fn to_print(&self) -> String {
        self.to_print_rec(&mut HashSet::new())
    }

    fn to_print_rec(&self, seen: &mut HashSet<usize>) -> String {
        match self {
            Value::Void => String::new(),
            Value::Bool(b) => b.to_string(),
            Value::Int(n) => n.to_string(),
            Value::Float(n) => format_float_print(*n),
            Value::String(s) => s.clone(),
            Value::FnRef { name, clo } => {
                if clo.is_some() || name.is_empty() {
                    "<fn>".to_string()
                } else {
                    name.clone()
                }
            }
            Value::Range {
                start,
                end,
                inclusive,
            } => {
                format!("{start}{}{end}", if *inclusive { "..=" } else { ".." })
            }
            Value::Array(items) => {
                let ptr = Rc::as_ptr(items) as usize;
                if !seen.insert(ptr) {
                    return "[...]".to_string();
                }
                let items = items.borrow();
                let mut out = String::from("[");
                for (n, item) in items.iter().enumerate() {
                    if n > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&item.to_print_rec(seen));
                }
                out.push(']');
                out
            }
            Value::Struct(rec) => {
                let ptr = Rc::as_ptr(rec) as usize;
                let rec = rec.borrow();
                if !seen.insert(ptr) {
                    return format!("{} {{ ... }}", rec.name);
                }
                let mut parts = String::new();
                for field in &rec.order {
                    let Some(v) = rec.fields.get(field) else {
                        continue;
                    };
                    if !parts.is_empty() {
                        parts.push_str(", ");
                    }
                    parts.push_str(field);
                    parts.push_str(": ");
                    parts.push_str(&v.to_print_rec(seen));
                }
                format!("{} {{ {parts} }}", rec.name)
            }
            Value::Map(dict) => {
                let ptr = Rc::as_ptr(dict) as usize;
                if !seen.insert(ptr) {
                    return "{...}".to_string();
                }
                let dict = dict.borrow();
                let mut out = String::from("{");
                for (n, key) in dict.order.iter().enumerate() {
                    let Some(v) = dict.fields.get(key) else {
                        continue;
                    };
                    if n > 0 {
                        out.push_str(", ");
                    }
                    out.push('"');
                    out.push_str(key);
                    out.push_str("\": ");
                    out.push_str(&v.to_print_rec(seen));
                }
                out.push('}');
                out
            }
            Value::SignalRef { name, owner } => match owner {
                Some(rec) => format!("{}.{}", rec.borrow().name, name),
                None => name.clone(),
            },
            Value::Future(_) => "<Future>".to_string(),
            Value::EnumType(name) => format!("enum {name}"),
            Value::Enum {
                ty,
                variant,
                payload,
            } => {
                if payload.is_empty() {
                    return format!("{ty}.{variant}");
                }
                let mut out = format!("{ty}.{variant}(");
                for (i, v) in payload.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&v.to_print_rec(seen));
                }
                out.push(')');
                out
            }
        }
    }
}

fn numeric_eq(a: f64, b: f64) -> bool {
    a == b || (a - b).abs() < 1e-9
}

pub fn format_float_print(real: f64) -> String {
    if real.is_finite() {
        let r = real.round_ties_even();
        if real == r {
            return java_long_cast(real).to_string();
        }
    }
    let mut s = java_double_to_string(real);
    if let Some(stripped) = s.strip_suffix(".0") {
        s = stripped.to_string();
    }
    s
}

pub fn json_float(n: f64) -> String {
    if n == 0.0 {
        return if n.is_sign_negative() {
            "-0".to_string()
        } else {
            "0".to_string()
        };
    }
    let s = java_double_to_string(n);
    if !s.contains('.') && !s.contains('e') && !s.contains('E') {
        format!("{s}.0")
    } else {
        s
    }
}

fn java_long_cast(real: f64) -> i64 {
    if real.is_nan() {
        return 0;
    }
    if real >= 9.223372036854776e18 {
        return i64::MAX;
    }
    if real <= -9.223372036854776e18 {
        return i64::MIN;
    }
    real as i64
}

pub fn java_double_to_string(n: f64) -> String {
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
    if s == "inf" {
        return "Infinity".to_string();
    }
    if s == "-inf" {
        return "-Infinity".to_string();
    }
    if let Some((mant, exp)) = s.split_once('e') {
        let exp_n: i32 = exp.parse().unwrap_or(0);
        let mant = if mant.contains('.') {
            mant.to_string()
        } else {
            format!("{mant}.0")
        };
        return format!("{mant}E{exp_n}");
    }
    s
}

pub fn utf16_units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

pub fn utf16_len(s: &str) -> i64 {
    s.encode_utf16().count() as i64
}

pub fn from_units(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

pub fn utf16_slice(s: &str, start: i64, end: i64) -> String {
    let units = utf16_units(s);
    let len = units.len() as i64;
    let mut start = start.max(0).min(len) as usize;
    let mut end = end.max(0).min(len) as usize;
    if end < start {
        end = start;
    }
    if start > units.len() {
        start = units.len();
    }
    from_units(&units[start..end])
}

pub fn utf16_index_char(s: &str, index: usize) -> Option<String> {
    let units = utf16_units(s);
    units.get(index).map(|u| from_units(&[*u]))
}

pub fn utf16_find(hay: &str, needle: &str) -> i64 {
    match hay.find(needle) {
        Some(byte) => hay[..byte].encode_utf16().count() as i64,
        None => -1,
    }
}

pub fn java_trim(s: &str) -> String {
    s.trim_matches(|c: char| (c as u32) <= 0x20).to_string()
}

pub fn java_upper(s: &str) -> String {
    java_case(s, true)
}

pub fn java_lower(s: &str) -> String {
    java_case(s, false)
}

fn java_case(s: &str, upper: bool) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let mapped: String = if upper {
            c.to_uppercase().collect()
        } else {
            c.to_lowercase().collect()
        };
        let mut chars = mapped.chars();
        match (chars.next(), chars.next()) {
            (Some(one), None) if one.len_utf16() == c.len_utf16() => out.push(one),
            _ => out.push(c),
        }
    }
    out
}

pub fn zero_of_type(ty: &str) -> Value {
    let head = match ty.find('[') {
        Some(p) => &ty[..p],
        None => ty,
    };
    match head {
        "Int" => Value::Int(0),
        "Float" => Value::Float(0.0),
        "String" | "Str" => Value::String(String::new()),
        "Bool" => Value::Bool(false),
        "Array" => Value::array(Vec::new()),
        "Map" => Value::map(MapData {
            order: Vec::new(),
            fields: HashMap::new(),
        }),
        _ => Value::Void,
    }
}

pub fn same_fn(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::FnRef { name: na, clo: ca }, Value::FnRef { name: nb, clo: cb }) => {
            if ca.is_some() || cb.is_some() {
                match (ca, cb) {
                    (Some(x), Some(y)) => Rc::ptr_eq(x, y),
                    _ => false,
                }
            } else {
                na == nb
            }
        }
        _ => false,
    }
}
