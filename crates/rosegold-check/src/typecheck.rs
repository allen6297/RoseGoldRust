use crate::checker::{engine_file_pub, Checker, FieldHit};
use crate::stdlib;
use crate::types::{self, type_head};
use rosegold_syntax::*;
use std::collections::{HashMap, HashSet};

struct BoundHit {
    method: Option<TraitMethod>,
    trait_name: String,
}

impl BoundHit {
    fn none() -> Self {
        Self {
            method: None,
            trait_name: String::new(),
        }
    }
    fn ok(&self) -> bool {
        self.method.is_some()
    }
}

pub fn run(env: &Checker) {
    let mut tc = TypeChecker {
        env,
        scopes: Vec::new(),
        current_return: String::new(),
        current_self: String::new(),
        current_super: String::new(),
        current_throws: false,
        current_async: false,
        in_try: false,
        throwing_try: false,
        generic_params: HashSet::new(),
        generic_bounds: HashMap::new(),
        loop_depth: 0,
    };
    tc.run_all();
}

struct TypeChecker<'a> {
    env: &'a Checker,
    scopes: Vec<HashMap<String, String>>,
    current_return: String,
    current_self: String,
    current_super: String,
    current_throws: bool,
    current_async: bool,
    in_try: bool,
    throwing_try: bool,
    generic_params: HashSet<String>,
    generic_bounds: HashMap<String, Vec<String>>,
    loop_depth: i32,
}

impl<'a> TypeChecker<'a> {
    fn fail(&self, line: u32, col: u32, msg: &str) {
        self.env
            .record_diag("type error", &self.env.file, line, col, msg);
    }

    fn is_local(&self, name: &str) -> bool {
        self.scopes.iter().rev().any(|s| s.contains_key(name))
    }

    fn is_subclass_of(&self, child: &str, ancestor: &str) -> bool {
        if child == ancestor {
            return true;
        }
        let mut current = type_head(child);
        let want = type_head(ancestor);
        if current == want {
            return true;
        }
        let mut seen = HashSet::new();
        while !current.is_empty() {
            if !seen.insert(current.clone()) {
                break;
            }
            let pit = self.env.class_parents.borrow().get(&current).cloned();
            let Some(pit) = pit else { break };
            if type_head(&pit) == want {
                return true;
            }
            current = type_head(&pit);
        }
        false
    }

    fn check_access(
        &self,
        vis: Vis,
        kind: &str,
        name: &str,
        defined_on: &str,
        line: u32,
        col: u32,
    ) {
        if vis == Vis::Pub {
            return;
        }
        let ok_same = !self.current_self.is_empty() && self.current_self == defined_on;
        let ok_prot = vis == Vis::Protected
            && !self.current_self.is_empty()
            && self.is_subclass_of(&self.current_self, defined_on);
        if ok_same || ok_prot {
            return;
        }
        self.fail(line, col, &format!("{kind} '{name}' is {}", vis.name()));
    }

    fn require_try(&mut self, throws_ex: bool, name: &str, line: u32, col: u32) {
        if !throws_ex {
            return;
        }
        if self.in_try {
            self.throwing_try = true;
        } else {
            self.fail(
                line,
                col,
                &format!("call to throwing function '{name}' requires 'try'"),
            );
        }
    }

    fn check_type_name(&mut self, ty: &str, line: u32, col: u32) {
        if ty.is_empty() {
            return;
        }
        if is_fn_type(ty) {
            let Some((params, ret)) = parse_fn_type(ty) else {
                return;
            };
            for p in params {
                self.check_type_name(&p, line, col);
            }
            self.check_type_name(&ret, line, col);
            return;
        }
        for a in types::type_arg_list(ty) {
            self.check_type_name(&a, line, col);
        }
        let head = type_head(ty);
        if head.is_empty()
            || head == "_"
            || self.generic_params.contains(&head)
            || builtin_head(&head)
            || head == "Fn"
        {
            return;
        }
        if self.env.find_struct(&head).is_some()
            || self.env.find_trait(&head).is_some()
            || self.env.find_enum(&head).is_some()
            || self.env.all_types.borrow().contains_key(&head)
            || self.env.traits.borrow().contains_key(&head)
        {
            return;
        }
        if let Some(ex) = stdlib::lookup(&head, &self.env.file) {
            let word = if ex.kind == "trait" {
                "trait"
            } else if ex.kind == "fn" {
                "function"
            } else {
                "type"
            };
            self.fail(
                line,
                col,
                &format!(
                    "undefined {word} '{head}'{}",
                    stdlib::import_hint(&head, &self.env.file)
                ),
            );
            return;
        }
        self.fail(line, col, &format!("undefined type '{head}'"));
    }

    fn is_trait_type(&self, ty: &str) -> bool {
        self.env.find_trait(ty).is_some()
    }

    fn compatible(&self, a: &str, b: &str) -> bool {
        if a == b {
            return true;
        }
        if (a == "String" && b == "Str") || (a == "Str" && b == "String") {
            return true;
        }
        if (a == "Int" && b == "Float") || (a == "Float" && b == "Int") {
            return true;
        }
        if (a == "Fn" && is_fn_type(b)) || (b == "Fn" && is_fn_type(a)) {
            return true;
        }
        if is_fn_type(a) && is_fn_type(b) {
            let Some((pa, ra)) = parse_fn_type(a) else {
                return false;
            };
            let Some((pb, rb)) = parse_fn_type(b) else {
                return false;
            };
            if pa.len() != pb.len() {
                return false;
            }
            for (x, y) in pa.iter().zip(pb.iter()) {
                if !self.compatible(x, y) {
                    return false;
                }
            }
            return self.compatible(&ra, &rb);
        }
        if is_array_ty(a) && is_array_ty(b) {
            if a == "Array" {
                return true;
            }
            if b == "Array" {
                return false;
            }
            return self.compatible(&array_elem(a), &array_elem(b));
        }
        if is_future_ty(a) && is_future_ty(b) {
            if a == "Future" {
                return true;
            }
            if b == "Future" {
                return false;
            }
            return self.compatible(&future_elem(a), &future_elem(b));
        }
        if is_map_ty(a) && is_map_ty(b) {
            let aa = types::type_arg_list(a);
            let ab = types::type_arg_list(b);
            if aa.is_empty() {
                return true;
            }
            if ab.is_empty() || aa.len() != 2 || ab.len() != 2 {
                return false;
            }
            return self.compatible(&aa[0], &ab[0]) && self.compatible(&aa[1], &ab[1]);
        }
        if self.is_trait_type(a) {
            if self.is_trait_type(b) && self.compatible_trait(a, b) {
                return true;
            }
            return self.implements_bound(b, a);
        }
        let ha = type_head(a);
        let hb = type_head(b);
        if ha == hb && (a.contains('[') || b.contains('[')) {
            let aa = types::type_arg_list(a);
            let ab = types::type_arg_list(b);
            if aa.is_empty() {
                return true;
            }
            if ab.is_empty() || aa.len() != ab.len() {
                return false;
            }
            for (x, y) in aa.iter().zip(ab.iter()) {
                if !self.compatible(x, y) {
                    return false;
                }
            }
            return true;
        }
        false
    }

    fn assignable(&mut self, expect: &str, got: &str, e: Option<&Expr>) -> bool {
        if self.accepts(expect, got) {
            return true;
        }
        let Some(e) = e else { return false };
        if is_array_ty(expect) && expect != "Array" && got == "Array" && e.kind == ExprKind::Array {
            return true;
        }
        is_map_ty(expect) && expect != "Map" && got == "Map" && e.kind == ExprKind::Map
    }

    fn accepts(&self, expect: &str, got: &str) -> bool {
        self.compatible(expect, got) || self.engine_ancestor(&type_head(expect), &type_head(got))
    }

    fn engine_ancestor(&self, expect: &str, got: &str) -> bool {
        if expect.is_empty() || got.is_empty() || expect == got || !self.engine_class(expect) {
            return false;
        }
        let mut cur = got.to_string();
        let mut seen = HashSet::new();
        for _ in 0..32 {
            let parent = self
                .env
                .class_parents
                .borrow()
                .get(&cur)
                .cloned()
                .unwrap_or_default();
            if parent.is_empty() {
                return false;
            }
            cur = type_head(&parent);
            if !seen.insert(cur.clone()) {
                return false;
            }
            if cur == expect {
                return true;
            }
        }
        false
    }

    fn if_type(&self, a: &str, b: &str) -> String {
        if !known(a) || !known(b) {
            return String::new();
        }
        if a == b {
            return a.to_string();
        }
        if is_string(a) && is_string(b) {
            return "String".into();
        }
        if is_numeric(a) && is_numeric(b) {
            return numeric_result(a, b);
        }
        let ha = type_head(a);
        let hb = type_head(b);
        if self.engine_ancestor(&ha, &hb) {
            return a.to_string();
        }
        if self.engine_ancestor(&hb, &ha) {
            return b.to_string();
        }
        if self.compatible(a, b) {
            return a.to_string();
        }
        String::new()
    }

    fn bind(&mut self, name: &str, ty: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), ty.to_string());
        }
    }

    fn lookup(&self, name: &str) -> String {
        for scope in self.scopes.iter().rev() {
            if let Some(ty) = scope.get(name) {
                return ty.clone();
            }
        }
        if name != "self" && !self.current_self.is_empty() {
            let ft = self.field_type(&self.current_self, name);
            if !ft.is_empty() || self.has_field(&self.current_self, name) {
                return ft;
            }
            if self
                .env
                .lookup_type_signal(&self.current_self, name)
                .is_some()
            {
                return "Signal".into();
            }
        }
        if self.env.lookup_const("", name).is_some() {
            return "Int".into();
        }
        String::new()
    }

    fn has_field(&self, type_name: &str, name: &str) -> bool {
        let mut current = type_head(type_name);
        let mut seen = HashSet::new();
        while !current.is_empty() {
            if !seen.insert(current.clone()) {
                break;
            }
            if let Some(st) = self.env.all_types.borrow().get(&current).cloned() {
                if st.borrow().fields.iter().any(|f| f == name) {
                    return true;
                }
            }
            let pit = self.env.class_parents.borrow().get(&current).cloned();
            current = match pit {
                Some(p) => type_head(&p),
                None => break,
            };
        }
        false
    }

    fn field_type(&self, type_name: &str, name: &str) -> String {
        let mut applied = type_name.to_string();
        let mut seen = HashSet::new();
        while !applied.is_empty() {
            let current = type_head(&applied);
            if !seen.insert(current.clone()) {
                break;
            }
            if let Some(st) = self.env.all_types.borrow().get(&current).cloned() {
                let st = st.borrow();
                for (i, f) in st.fields.iter().enumerate() {
                    if f != name {
                        continue;
                    }
                    if i < st.field_types.len() {
                        let tenv =
                            types::type_env_from(&st.type_params, &types::type_arg_list(&applied));
                        return types::subst_type(&st.field_types[i], &tenv);
                    }
                    return String::new();
                }
            }
            applied = self.applied_parent(&applied);
        }
        self.live_prop(type_name, name)
    }

    fn live_prop(&self, type_name: &str, name: &str) -> String {
        if !self.engine_node(type_name) {
            return String::new();
        }
        if name == "position" || name == "scale" {
            "Vec2".into()
        } else if name == "rotation" {
            "Float".into()
        } else {
            String::new()
        }
    }

    fn engine_node(&self, type_name: &str) -> bool {
        let Some(methods) = self
            .env
            .type_methods
            .borrow()
            .get(&type_head(type_name))
            .cloned()
        else {
            return false;
        };
        methods.values().any(|m| engine_file_pub(&m.file))
    }

    fn engine_class(&self, type_name: &str) -> bool {
        let head = type_head(type_name);
        if !self.env.class_abstract.borrow().contains_key(&head) || !self.extends_object(&head) {
            return false;
        }
        head == "Object" || self.engine_node(&head)
    }

    fn godot_object(&self, type_name: &str) -> bool {
        self.extends_object(&type_head(type_name))
    }

    fn extends_object(&self, head: &str) -> bool {
        let mut cur = head.to_string();
        for _ in 0..32 {
            if cur.is_empty() {
                return false;
            }
            if cur == "Object" {
                return true;
            }
            if !self.env.class_abstract.borrow().contains_key(&cur) {
                return false;
            }
            let parent = self
                .env
                .class_parents
                .borrow()
                .get(&cur)
                .cloned()
                .unwrap_or_default();
            if parent.is_empty() {
                return false;
            }
            cur = type_head(&parent);
        }
        false
    }

    fn applied_parent(&self, applied: &str) -> String {
        let pit = self
            .env
            .class_parents
            .borrow()
            .get(&type_head(applied))
            .cloned()
            .unwrap_or_default();
        if pit.is_empty() {
            return String::new();
        }
        let params = self
            .env
            .all_types
            .borrow()
            .get(&type_head(applied))
            .map(|s| s.borrow().type_params.clone())
            .unwrap_or_default();
        types::subst_type(
            &pit,
            &types::type_env_from(&params, &types::type_arg_list(applied)),
        )
    }

    fn defined(&self, name: &str) -> bool {
        if name == "self" || name == "super" {
            return true;
        }
        if !self.lookup(name).is_empty()
            || (!self.current_self.is_empty()
                && (self.has_field(&self.current_self, name)
                    || self
                        .env
                        .lookup_type_signal(&self.current_self, name)
                        .is_some()))
        {
            return true;
        }
        if self.is_local(name) {
            return true;
        }
        if self.env.find_local_fn(name).is_some() || self.env.fns.borrow().contains_key(name) {
            return true;
        }
        if matches!(name, "print" | "len" | "assert" | "argv" | "argv_len") {
            return true;
        }
        if self.env.singletons.borrow().contains(name) {
            return true;
        }
        if self.env.all_types.borrow().contains_key(name)
            || self.env.structs.borrow().contains_key(name)
            || self.env.enums.borrow().contains_key(name)
            || self.env.traits.borrow().contains_key(name)
            || self.env.signal_arity.borrow().contains_key(name)
        {
            return true;
        }
        if self.env.find_enum(name).is_some() || self.env.find_struct(name).is_some() {
            return true;
        }
        self.env.find_module_bind(name).is_some() || types::is_host_module(name)
    }

    fn try_module_path(&self, e: &Expr) -> Option<String> {
        if e.kind == ExprKind::Var {
            return self.env.find_module_bind(&e.text);
        }
        if e.kind != ExprKind::Member || e.kids.is_empty() {
            return None;
        }
        let parent = self.try_module_path(&e.kids[0])?;
        let lit = self.env.loaded.borrow().get(&parent).cloned()?;
        let lit = lit.borrow();
        let internal = self.env.current_module() == parent;
        let map = if internal {
            &lit.modules
        } else {
            &lit.export_mods
        };
        map.get(&e.text).cloned()
    }

    fn infer_array(&mut self, e: &Expr) -> String {
        if e.kids.is_empty() {
            return "Array".into();
        }
        let mut elem = String::new();
        for kid in &e.kids {
            let t = self.infer(kid);
            if !known(&t) {
                return "Array".into();
            }
            if elem.is_empty() {
                elem = t;
            } else if elem == "Int" && t == "Float" {
                elem = t;
            } else if !(elem == t || self.compatible(&elem, &t)) {
                return "Array".into();
            }
        }
        format!("Array[{elem}]")
    }

    fn infer(&mut self, e: &Expr) -> String {
        match e.kind {
            ExprKind::Int => "Int".into(),
            ExprKind::Float => "Float".into(),
            ExprKind::String | ExprKind::Format => self.infer_format_or_string(e),
            ExprKind::Bool => "Bool".into(),
            ExprKind::Null => String::new(),
            ExprKind::Var => self.infer_var(e),
            ExprKind::Unary => self.infer_unary(e),
            ExprKind::Binary => self.infer_binary(e),
            ExprKind::Call => self.infer_call_expr(e),
            ExprKind::Lambda => e
                .lambda
                .as_ref()
                .map(|f| encode_fn_decl(f))
                .unwrap_or_else(|| "Fn".into()),
            ExprKind::MethodCall => self.infer_method(e),
            ExprKind::Member => self.infer_member(e),
            ExprKind::Index => self.infer_index(e),
            ExprKind::Array => self.infer_array(e),
            ExprKind::Map => "Map".into(),
            ExprKind::Range => "Range".into(),
            ExprKind::StructLit => self.infer_struct_lit(e),
            ExprKind::Try => e.kids.first().map(|k| self.infer(k)).unwrap_or_default(),
            ExprKind::Await => self.infer_await(e),
            ExprKind::Cast => self.cast_target(e),
            ExprKind::Is => "Bool".into(),
            ExprKind::If => {
                if e.kids.len() < 3 {
                    String::new()
                } else {
                    let a = self.infer(&e.kids[1]);
                    let b = self.infer(&e.kids[2]);
                    self.if_type(&a, &b)
                }
            }
            ExprKind::New => {
                if self
                    .env
                    .all_types
                    .borrow()
                    .contains_key(&type_head(&e.text))
                {
                    e.text.clone()
                } else {
                    String::new()
                }
            }
            ExprKind::NodePath => "Node".into(),
        }
    }

    fn infer_format_or_string(&mut self, e: &Expr) -> String {
        if e.kind == ExprKind::String {
            return "String".into();
        }
        for kid in &e.kids {
            let ty = self.infer(kid);
            if !known(&ty) || (!is_string(&ty) && !is_numeric(&ty)) {
                return String::new();
            }
        }
        "String".into()
    }

    fn infer_var(&self, e: &Expr) -> String {
        let ty = self.lookup(&e.text);
        if known(&ty) {
            return ty;
        }
        if let Some(fn_) = self.env.find_local_fn(&e.text) {
            return encode_fn_decl(&fn_);
        }
        if let Some(top) = self.env.fns.borrow().get(&e.text).cloned() {
            return encode_fn_decl(&top);
        }
        if let Some(en) = self.env.find_enum(&e.text) {
            return en.name.clone();
        }
        if self.env.singletons.borrow().contains(&e.text) {
            return e.text.clone();
        }
        String::new()
    }

    fn infer_unary(&mut self, e: &Expr) -> String {
        if e.text == "!" {
            return "Bool".into();
        }
        let Some(kid) = e.kids.first() else {
            return String::new();
        };
        let ty = self.infer(kid);
        if e.text == "~" {
            return if ty == "Int" {
                "Int".into()
            } else {
                String::new()
            };
        }
        if e.text == "-" {
            if is_numeric(&ty) {
                return ty;
            }
            let head = type_head(&ty);
            return if value_math(&head) {
                head
            } else {
                String::new()
            };
        }
        String::new()
    }

    fn infer_binary(&mut self, e: &Expr) -> String {
        if e.kids.len() < 2 {
            return String::new();
        }
        let l = self.infer(&e.kids[0]);
        let r = self.infer(&e.kids[1]);
        if !known(&l) || !known(&r) {
            return String::new();
        }
        let value = value_op(&e.text, &l, &r);
        if !value.is_empty() {
            return value;
        }
        if e.text == "+" && is_numeric(&l) && is_numeric(&r) {
            return numeric_result(&l, &r);
        }
        if e.text == "+" && is_string(&l) && is_string(&r) {
            return "String".into();
        }
        if matches!(e.text.as_str(), "-" | "*" | "/" | "%") && is_numeric(&l) && is_numeric(&r) {
            return numeric_result(&l, &r);
        }
        if matches!(e.text.as_str(), "&" | "|" | "^" | "<<" | ">>") && l == "Int" && r == "Int" {
            return "Int".into();
        }
        if matches!(
            e.text.as_str(),
            "==" | "!=" | "<" | ">" | "<=" | ">=" | "&&" | "||"
        ) {
            return "Bool".into();
        }
        String::new()
    }

    fn infer_call_expr(&mut self, e: &Expr) -> String {
        if e.text.is_empty() && !e.kids.is_empty() {
            if e.kids[0].kind == ExprKind::Lambda {
                if let Some(fn_) = &e.kids[0].lambda {
                    if fn_.type_params.is_empty() {
                        return wrap_async(fn_, &fn_.return_type);
                    }
                    let args = rest(&e.kids);
                    let tenv =
                        self.infer_env(&fn_.type_params, &e.type_args, &fn_.param_types, &args);
                    return wrap_async(fn_, &types::subst_type(&fn_.return_type, &tenv));
                }
            }
            let ty = self.infer(&e.kids[0]);
            if let Some((_, ret)) = parse_fn_type(&ty) {
                return ret;
            }
            return String::new();
        }
        let ty = self.lookup(&e.text);
        if let Some((_, ret)) = parse_fn_type(&ty) {
            return ret;
        }
        match e.text.as_str() {
            "len" | "argv_len" => return "Int".into(),
            "argv" => return "String".into(),
            "print" | "assert" => return "Void".into(),
            _ => {}
        }
        if let Some(fn_) = self.env.find_local_fn(&e.text) {
            if fn_.type_params.is_empty() {
                return wrap_async(&fn_, &fn_.return_type);
            }
            let tenv = self.infer_env(&fn_.type_params, &e.type_args, &fn_.param_types, &e.kids);
            return wrap_async(&fn_, &types::subst_type(&fn_.return_type, &tenv));
        }
        self.infer_call(&e.text)
    }

    fn infer_call(&self, name: &str) -> String {
        match name {
            "len" | "argv_len" => return "Int".into(),
            "argv" => return "String".into(),
            "print" | "assert" => return "Void".into(),
            _ => {}
        }
        if let Some(fn_) = self.env.find_local_fn(name) {
            return wrap_async(&fn_, &fn_.return_type);
        }
        if !self.current_self.is_empty() {
            if let Some(fn_) = self.env.lookup_method(&self.current_self, name).1 {
                return wrap_async(&fn_, &fn_.return_type);
            }
        }
        String::new()
    }

    fn infer_member(&mut self, e: &Expr) -> String {
        if let Some(recv) = e.kids.first() {
            if recv.kind == ExprKind::Var
                && self.env.find_struct(&recv.text).is_some()
                && !known(&self.lookup(&recv.text))
                && self.env.lookup_const(&recv.text, &e.text).is_some()
            {
                return "Int".into();
            }
            if recv.kind == ExprKind::Var && self.env.find_enum(&recv.text).is_some() {
                return recv.text.clone();
            }
        }
        let obj = e.kids.first().map(|k| self.infer(k)).unwrap_or_default();
        if is_array_ty(&obj) || is_map_ty(&obj) || obj == "String" || obj == "Str" {
            return if e.text == "len" {
                "Int".into()
            } else {
                String::new()
            };
        }
        if known(&obj) {
            if self.env.lookup_type_signal(&obj, &e.text).is_some() {
                return "Signal".into();
            }
            return self.field_type(&obj, &e.text);
        }
        String::new()
    }

    fn infer_index(&mut self, e: &Expr) -> String {
        let Some(obj_e) = e.kids.first() else {
            return String::new();
        };
        let obj = self.infer(obj_e);
        if is_string(&obj) {
            return "String".into();
        }
        if is_array_ty(&obj) {
            return array_elem(&obj);
        }
        String::new()
    }

    fn infer_await(&mut self, e: &Expr) -> String {
        let Some(kid) = e.kids.first() else {
            return String::new();
        };
        let ty = self.infer(kid);
        if ty == "Signal" || !known(&ty) || !is_future_ty(&ty) {
            return String::new();
        }
        future_elem(&ty)
    }

    fn infer_struct_lit(&mut self, e: &Expr) -> String {
        let Some(st) = self.env.find_struct(&e.text) else {
            return e.text.clone();
        };
        let st = st.borrow().clone();
        if st.type_params.is_empty() {
            return e.text.clone();
        }
        if !e.type_args.is_empty() {
            return types::type_apply(&e.text, &e.type_args);
        }
        let mut tenv = HashMap::new();
        let prev = self.generic_params.clone();
        for p in &st.type_params {
            self.generic_params.insert(p.name.clone());
        }
        for i in 0..e.names.len().min(e.kids.len()) {
            let pat = types::type_of_field(&st.fields, &st.field_types, &e.names[i]);
            let got = self.infer(&e.kids[i]);
            self.unify_type(&pat, &got, &mut tenv);
        }
        self.generic_params = prev;
        let mut args = Vec::new();
        for p in &st.type_params {
            let it = tenv.get(&p.name).cloned().unwrap_or_default();
            if it.is_empty() || it == p.name || !known(&it) {
                return e.text.clone();
            }
            args.push(it);
        }
        types::type_apply(&e.text, &args)
    }

    fn infer_method(&mut self, e: &Expr) -> String {
        if e.kids.is_empty() {
            return String::new();
        }
        let recv = &e.kids[0];
        if recv.kind == ExprKind::Var
            && self.env.find_struct(&recv.text).is_some()
            && !known(&self.lookup(&recv.text))
            && !self.env.singletons.borrow().contains(&recv.text)
        {
            if let Some(st) = self.env.lookup_static(&recv.text, &e.text).1 {
                if !st.return_type.is_empty() {
                    return st.return_type.clone();
                }
            }
        }
        if recv.kind == ExprKind::Var && recv.text == "Future" {
            if e.text == "all" {
                if e.kids.len() < 2 {
                    return "Future[Array]".into();
                }
                let arg_ty = self.infer(&e.kids[1]);
                if is_array_ty(&arg_ty) {
                    let elem = array_elem(&arg_ty);
                    if is_future_ty(&elem) {
                        let inner = future_elem(&elem);
                        if inner.is_empty() {
                            return "Future[Array]".into();
                        }
                        return format!("Future[Array[{inner}]]");
                    }
                    if elem.is_empty() || arg_ty == "Array" {
                        return "Future[Array]".into();
                    }
                }
                return "Future[Array]".into();
            }
            if e.text == "race" {
                if e.kids.len() < 2 {
                    return "Future".into();
                }
                let arg_ty = self.infer(&e.kids[1]);
                if is_array_ty(&arg_ty) {
                    let elem = array_elem(&arg_ty);
                    if is_future_ty(&elem) {
                        let inner = future_elem(&elem);
                        return if inner.is_empty() {
                            "Future".into()
                        } else {
                            format!("Future[{inner}]")
                        };
                    }
                }
                return "Future".into();
            }
        }
        if recv.kind == ExprKind::Var && types::is_host_module(&recv.text) {
            return host_return(&recv.text, &e.text);
        }
        if recv.kind == ExprKind::Var && self.env.signal_arity.borrow().contains_key(&recv.text) {
            return "Void".into();
        }
        if recv.kind == ExprKind::Var
            && !self.current_self.is_empty()
            && self
                .env
                .lookup_type_signal(&self.current_self, &recv.text)
                .is_some()
        {
            return "Void".into();
        }
        let recv_ty = self.infer(recv);
        if recv_ty == "Signal" {
            return "Void".into();
        }
        if recv.kind == ExprKind::Var {
            if let Some(mod_name) = self.env.find_module_bind(&recv.text) {
                if let Some(lit) = self.env.loaded.borrow().get(&mod_name).cloned() {
                    if let Some(eit) = lit.borrow().exports.get(&e.text).cloned() {
                        return self.imported_return(&eit, &rest(&e.kids), &e.type_args);
                    }
                }
            }
            if self.env.find_enum(&recv.text).is_some() {
                return recv.text.clone();
            }
        }
        if let Some(mod_path) = self.try_module_path(recv) {
            if let Some(lit) = self.env.loaded.borrow().get(&mod_path).cloned() {
                if let Some(eit) = lit.borrow().exports.get(&e.text).cloned() {
                    return self.imported_return(&eit, &rest(&e.kids), &e.type_args);
                }
            }
        }
        let obj = self.infer(recv);
        if is_array_ty(&obj) {
            return match e.text.as_str() {
                "len" => "Int".into(),
                "pop" => array_elem(&obj),
                "push" => "Void".into(),
                _ => String::new(),
            };
        }
        if is_map_ty(&obj) {
            return match e.text.as_str() {
                "len" => "Int".into(),
                "has" => "Bool".into(),
                "keys" => "Array[String]".into(),
                "insert" => "Void".into(),
                "remove" => String::new(),
                _ => String::new(),
            };
        }
        if is_string(&obj) && e.text == "len" {
            return "Int".into();
        }
        if is_future_ty(&obj) && e.text == "cancel" {
            return "Void".into();
        }
        if known(&obj) {
            let mut hit = self.trait_object_method(&obj, &e.text);
            if !hit.ok() {
                hit = self.bound_method(&obj, &e.text);
            }
            if let Some(m) = hit.method {
                return types::subst_type(&m.return_type, &self.trait_env(&hit.trait_name));
            }
            let lookup_on = if is_callable(&obj) {
                "Fn"
            } else {
                obj.as_str()
            };
            if let Some((defined, fn_)) = self
                .env
                .lookup_method(lookup_on, &e.text)
                .1
                .map(|f| (self.env.lookup_method(lookup_on, &e.text).0, f))
            {
                let _ = defined;
            }
            if let (defined, Some(fn_)) = self.env.lookup_method(
                if is_callable(&obj) {
                    "Fn"
                } else {
                    obj.as_str()
                },
                &e.text,
            ) {
                let mut tenv = self.env_for_applied(&self.recv_applied(&obj, &defined));
                if !fn_.type_params.is_empty() {
                    let patterns: Vec<String> = fn_.param_types.iter().skip(1).cloned().collect();
                    let args = rest(&e.kids);
                    tenv.extend(self.infer_env(&fn_.type_params, &e.type_args, &patterns, &args));
                }
                return wrap_async(&fn_, &types::subst_type(&fn_.return_type, &tenv));
            }
        }
        if let Some(ufcs) = self.env.find_ufcs(&e.text, &recv_ty) {
            if ufcs.type_params.is_empty() {
                return wrap_async(&ufcs, &ufcs.return_type);
            }
            let mut vals = vec![e
                .kids
                .first()
                .cloned()
                .unwrap_or_else(|| Expr::new(ExprKind::Null, Span::default()))];
            if e.kids.len() > 1 {
                vals.extend(e.kids[1..].iter().cloned());
            }
            let tenv = self.infer_env(&ufcs.type_params, &e.type_args, &ufcs.param_types, &vals);
            return wrap_async(&ufcs, &types::subst_type(&ufcs.return_type, &tenv));
        }
        String::new()
    }

    fn cast_target(&mut self, e: &Expr) -> String {
        if e.text.is_empty() || e.kids.is_empty() {
            return String::new();
        }
        let head = type_head(&e.text);
        if self.env.all_types.borrow().get(&head).is_none() || !self.engine_class(&head) {
            return String::new();
        }
        let src = self.infer(&e.kids[0]);
        if known(&src) && !self.godot_object(&src) {
            return String::new();
        }
        e.text.clone()
    }

    fn check_binop(&mut self, e: &Expr) {
        if e.kids.len() < 2 {
            return;
        }
        let l = self.infer(&e.kids[0]);
        let r = self.infer(&e.kids[1]);
        if !known(&l) || !known(&r) || !value_op(&e.text, &l, &r).is_empty() {
            return;
        }
        match e.text.as_str() {
            "+" => {
                if (is_numeric(&l) && is_numeric(&r)) || (is_string(&l) && is_string(&r)) {
                    return;
                }
                self.fail(e.span.line, e.span.col, &format!("cannot add {l} and {r}"));
            }
            "-" | "*" | "/" | "%" => {
                if is_numeric(&l) && is_numeric(&r) {
                    return;
                }
                let verb = match e.text.as_str() {
                    "-" => "subtract",
                    "*" => "multiply",
                    "/" => "divide",
                    _ => "modulo",
                };
                self.fail(
                    e.span.line,
                    e.span.col,
                    &format!("cannot {verb} {l} and {r}"),
                );
            }
            "<" | ">" | "<=" | ">=" => {
                if is_numeric(&l) && is_numeric(&r) {
                    return;
                }
                self.fail(
                    e.span.line,
                    e.span.col,
                    &format!("cannot compare {l} and {r}"),
                );
            }
            "&" | "|" | "^" | "<<" | ">>" => {
                if l == "Int" && r == "Int" {
                    return;
                }
                self.fail(e.span.line, e.span.col, "bitwise operators need Int");
            }
            _ => {}
        }
    }

    fn check_compound(
        &mut self,
        op: &str,
        lt: &str,
        rt: &str,
        line: u32,
        col: u32,
        rhs: Option<&Expr>,
    ) {
        if op.is_empty() || op == "=" {
            if known(lt) && known(rt) {
                let ok = match rhs {
                    Some(rhs) => self.assignable(lt, rt, Some(rhs)),
                    None => self.accepts(lt, rt),
                };
                if !ok {
                    self.fail(line, col, &format!("cannot assign {rt} to {lt}"));
                }
            }
            return;
        }
        if !known(lt) || !known(rt) {
            return;
        }
        let bin = &op[..op.len() - 1];
        let value = value_op(bin, lt, rt);
        if !value.is_empty() {
            if !self.compatible(lt, &value) {
                self.fail(line, col, &format!("cannot assign {value} to {lt}"));
            }
            return;
        }
        if bin == "+" {
            if (is_numeric(lt) && is_numeric(rt)) || (is_string(lt) && is_string(rt)) {
                return;
            }
            self.fail(line, col, &format!("cannot add {lt} and {rt}"));
            return;
        }
        if matches!(bin, "-" | "*" | "/" | "%") {
            if is_numeric(lt) && is_numeric(rt) {
                return;
            }
            let verb = match bin {
                "-" => "subtract",
                "*" => "multiply",
                "/" => "divide",
                _ => "modulo",
            };
            self.fail(line, col, &format!("cannot {verb} {lt} and {rt}"));
            return;
        }
        if matches!(bin, "|" | "&" | "^" | "<<" | ">>") && !(lt == "Int" && rt == "Int") {
            self.fail(line, col, "bitwise operators need Int");
        }
    }

    fn check_index(&mut self, e: &Expr) {
        if e.kids.len() < 2 {
            return;
        }
        let obj = self.infer(&e.kids[0]);
        let idx = self.infer(&e.kids[1]);
        if !known(&obj) {
            return;
        }
        if is_array_ty(&obj) || is_string(&obj) {
            if known(&idx) && idx != "Int" {
                self.fail(e.span.line, e.span.col, "index must be Int");
            }
            return;
        }
        if is_map_ty(&obj) {
            if known(&idx) && !is_string(&idx) {
                self.fail(e.span.line, e.span.col, "map key must be String");
            }
            return;
        }
        let with = if known(&idx) { idx } else { "unknown".into() };
        self.fail(
            e.span.line,
            e.span.col,
            &format!("cannot index {obj} with {with}"),
        );
    }

    fn check_arg_types(
        &mut self,
        func: &FnDecl,
        args: &[Expr],
        is_method: bool,
        line: u32,
        col: u32,
    ) {
        self.check_arg_types_env(func, args, is_method, line, col, &HashMap::new());
    }

    /// A call through `import`, including `collections.contains`. Type
    /// arguments are inferred from the values, the same as a function in
    /// the same file.
    fn check_imported_fn(
        &mut self,
        func: &FnDecl,
        args: &[Expr],
        type_args: &[String],
        label: &str,
        line: u32,
        col: u32,
    ) {
        self.check_call_arity(func, label, args.len() as i32, false, line, col);
        self.check_type_arg_count(&func.type_params, type_args, line, col, label);
        if !func.type_params.is_empty() {
            let tenv = self.infer_env(&func.type_params, type_args, &func.param_types, args);
            self.check_bounds(&func.type_params, &tenv, line, col);
            self.check_arg_types_env(func, args, false, line, col, &tenv);
        } else {
            if !type_args.is_empty() {
                self.fail(
                    line,
                    col,
                    &format!("'{label}' does not take type arguments"),
                );
            }
            self.check_arg_types(func, args, false, line, col);
        }
        self.require_try(func.throws_ex, label, line, col);
    }

    fn imported_return(&mut self, func: &FnDecl, args: &[Expr], type_args: &[String]) -> String {
        if func.type_params.is_empty() {
            return wrap_async(func, &func.return_type);
        }
        let tenv = self.infer_env(&func.type_params, type_args, &func.param_types, args);
        wrap_async(func, &types::subst_type(&func.return_type, &tenv))
    }

    fn check_arg_types_env(
        &mut self,
        func: &FnDecl,
        args: &[Expr],
        is_method: bool,
        line: u32,
        col: u32,
        tenv: &HashMap<String, String>,
    ) {
        let off = if is_method { 1 } else { 0 };
        for (i, arg) in args.iter().enumerate() {
            if i + off >= func.param_types.len() {
                break;
            }
            let expect = types::subst_type(&func.param_types[i + off], tenv);
            let got = self.infer(arg);
            if known(&expect) && known(&got) && !self.assignable(&expect, &got, Some(arg)) {
                self.fail(
                    line,
                    col,
                    &format!("cannot pass {got} to '{}', expected {expect}", func.name),
                );
            }
            self.check_array_elems(&expect, Some(arg), line, col);
        }
    }

    fn check_arity(&self, label: &str, expected: i32, got: i32, line: u32, col: u32) {
        if expected != got {
            self.fail(
                line,
                col,
                &format!("{label} expected {expected} args, got {got}"),
            );
        }
    }

    fn check_call_arity(
        &self,
        func: &FnDecl,
        label: &str,
        got: i32,
        method: bool,
        line: u32,
        col: u32,
    ) {
        let min = types::min_arity(&func.params, &func.param_defaults, method);
        let max = types::max_arity(&func.params, method);
        if engine_vararg(func) {
            if got < min {
                self.fail(
                    line,
                    col,
                    &format!("{label} expected at least {min} args, got {got}"),
                );
            }
            return;
        }
        if got >= min && got <= max {
            return;
        }
        if min == max {
            self.fail(
                line,
                col,
                &format!("{label} expected {max} args, got {got}"),
            );
        } else {
            self.fail(
                line,
                col,
                &format!("{label} expected {min} to {max} args, got {got}"),
            );
        }
    }

    fn unify_type(&self, pattern: &str, got: &str, tenv: &mut HashMap<String, String>) -> bool {
        if pattern.is_empty() || !known(got) {
            return true;
        }
        let ph = type_head(pattern);
        let pa = types::type_arg_list(pattern);
        if pa.is_empty() && self.generic_params.contains(&ph) {
            match tenv.get(&ph).cloned() {
                None => {
                    tenv.insert(ph, got.to_string());
                    return true;
                }
                Some(it) if it == ph => {
                    tenv.insert(ph, got.to_string());
                    return true;
                }
                Some(it) => return self.compatible(&it, got),
            }
        }
        if is_array_ty(pattern) && is_array_ty(got) {
            let pe = array_elem(pattern);
            let ge = array_elem(got);
            if pe.is_empty() || ge.is_empty() {
                return true;
            }
            return self.unify_type(&pe, &ge, tenv);
        }
        if is_map_ty(pattern) && is_map_ty(got) {
            return true;
        }
        let gh = type_head(got);
        let ga = types::type_arg_list(got);
        if ph != gh {
            return self.compatible(pattern, got);
        }
        if pa.is_empty() || ga.is_empty() {
            return true;
        }
        if pa.len() != ga.len() {
            return false;
        }
        for (p, g) in pa.iter().zip(ga.iter()) {
            if !self.unify_type(p, g, tenv) {
                return false;
            }
        }
        true
    }

    fn infer_env(
        &mut self,
        params: &[TypeParam],
        explicit: &[String],
        patterns: &[String],
        vals: &[Expr],
    ) -> HashMap<String, String> {
        let prev = self.generic_params.clone();
        for p in params {
            self.generic_params.insert(p.name.clone());
        }
        let mut tenv = if explicit.is_empty() {
            HashMap::new()
        } else {
            types::type_env_from(params, explicit)
        };
        let n = patterns.len().min(vals.len());
        for i in 0..n {
            let got = self.infer(&vals[i]);
            self.unify_type(&patterns[i], &got, &mut tenv);
        }
        self.generic_params = prev;
        tenv
    }

    fn recv_applied(&self, obj: &str, defined_on: &str) -> String {
        let mut applied = obj.to_string();
        let mut seen = HashSet::new();
        while !applied.is_empty() && seen.insert(type_head(&applied)) {
            if type_head(&applied) == type_head(defined_on) {
                return applied;
            }
            applied = self.applied_parent(&applied);
        }
        defined_on.to_string()
    }

    fn env_for_applied(&self, applied: &str) -> HashMap<String, String> {
        let params = if let Some(st) = self.env.find_struct(applied) {
            st.borrow().type_params.clone()
        } else if let Some(st) = self
            .env
            .all_types
            .borrow()
            .get(&type_head(applied))
            .cloned()
        {
            st.borrow().type_params.clone()
        } else {
            Vec::new()
        };
        types::type_env_from(&params, &types::type_arg_list(applied))
    }

    fn trait_env(&self, applied: &str) -> HashMap<String, String> {
        let params = self
            .env
            .find_trait(applied)
            .map(|t| t.type_params.clone())
            .unwrap_or_default();
        types::type_env_from(&params, &types::type_arg_list(applied))
    }

    fn compatible_trait(&self, imp: &str, bound: &str) -> bool {
        if type_head(imp) != type_head(bound) {
            return false;
        }
        let ia = types::type_arg_list(imp);
        let ba = types::type_arg_list(bound);
        if ia.is_empty() || ba.is_empty() {
            return true;
        }
        if ia.len() != ba.len() {
            return false;
        }
        ia.iter().zip(ba.iter()).all(|(a, b)| self.compatible(a, b))
    }

    fn implements_bound(&self, concrete: &str, bound: &str) -> bool {
        if self.is_trait_type(concrete) && self.compatible_trait(concrete, bound) {
            return true;
        }
        let impls = self.env.trait_impls.borrow().clone();
        let mut applied = concrete.to_string();
        let mut seen = HashSet::new();
        while !applied.is_empty() && seen.insert(type_head(&applied)) {
            let head = type_head(&applied);
            for im in &impls {
                if type_head(&im.type_name) != head {
                    continue;
                }
                let mut tenv = self.env_for_applied(&applied);
                if self.unify_with_params(&im.type_params, &im.type_name, &applied, &mut tenv)
                    && self.compatible_trait(&types::subst_type(&im.trait_name, &tenv), bound)
                {
                    return true;
                }
            }
            applied = self.applied_parent(&applied);
        }
        false
    }

    fn unify_with_params(
        &self,
        params: &[TypeParam],
        pattern: &str,
        applied: &str,
        tenv: &mut HashMap<String, String>,
    ) -> bool {
        let mut gp = self.generic_params.clone();
        for p in params {
            gp.insert(p.name.clone());
        }
        // unify_type reads self.generic_params. Temporarily we need those params visible.
        // Use a local copy by calling a free function.
        unify_type_with(&gp, pattern, applied, tenv, &|a, b| self.compatible(a, b))
    }

    fn check_trait_bound(&mut self, bound: &str, line: u32, col: u32) {
        let head = type_head(bound);
        let Some(tr) = self.env.find_trait(&head) else {
            self.fail(
                line,
                col,
                &format!(
                    "undefined trait '{head}'{}",
                    stdlib::import_hint(&head, &self.env.file)
                ),
            );
            return;
        };
        let args = types::type_arg_list(bound);
        if args.is_empty() {
            return;
        }
        if tr.type_params.is_empty() {
            self.fail(line, col, &format!("'{head}' does not take type arguments"));
        } else if args.len() != tr.type_params.len() {
            self.fail(
                line,
                col,
                &format!(
                    "'{head}' expected {} type argument(s), got {}",
                    tr.type_params.len(),
                    args.len()
                ),
            );
        }
    }

    fn check_type_arg_count(
        &self,
        params: &[TypeParam],
        args: &[String],
        line: u32,
        col: u32,
        who: &str,
    ) {
        if args.is_empty() {
            return;
        }
        if params.is_empty() {
            self.fail(line, col, &format!("'{who}' does not take type arguments"));
            return;
        }
        if args.len() != params.len() {
            self.fail(
                line,
                col,
                &format!(
                    "'{who}' expected {} type argument(s), got {}",
                    params.len(),
                    args.len()
                ),
            );
        }
    }

    fn check_bounds(
        &mut self,
        params: &[TypeParam],
        tenv: &HashMap<String, String>,
        line: u32,
        col: u32,
    ) {
        for p in params {
            let Some(eit) = tenv.get(&p.name).cloned() else {
                continue;
            };
            if eit == p.name || !known(&eit) {
                continue;
            }
            for bound in &p.bounds {
                if self.implements_bound(&eit, bound) {
                    continue;
                }
                if self.generic_params.contains(&type_head(&eit)) {
                    let mut ok = false;
                    if let Some(git) = self.generic_bounds.get(&type_head(&eit)) {
                        ok = git.iter().any(|b| self.compatible_trait(b, bound));
                    }
                    if ok {
                        continue;
                    }
                }
                self.fail(line, col, &format!("type {eit} does not implement {bound}"));
            }
        }
    }

    fn bound_method(&self, type_name: &str, name: &str) -> BoundHit {
        let Some(bounds) = self.generic_bounds.get(&type_head(type_name)).cloned() else {
            return BoundHit::none();
        };
        for trait_name in bounds {
            let Some(tr) = self.env.find_trait(&trait_name) else {
                continue;
            };
            if let Some(m) = tr.methods.iter().find(|m| m.name == name).cloned() {
                return BoundHit {
                    method: Some(m),
                    trait_name,
                };
            }
        }
        BoundHit::none()
    }

    fn trait_object_method(&self, type_name: &str, name: &str) -> BoundHit {
        let Some(tr) = self.env.find_trait(type_name) else {
            return BoundHit::none();
        };
        if let Some(m) = tr.methods.iter().find(|m| m.name == name).cloned() {
            return BoundHit {
                method: Some(m),
                trait_name: type_name.to_string(),
            };
        }
        BoundHit::none()
    }

    fn check_array_elems(&mut self, expect_ty: &str, e: Option<&Expr>, line: u32, col: u32) {
        let Some(e) = e else { return };
        if !is_array_ty(expect_ty) || e.kind != ExprKind::Array {
            return;
        }
        let elem = array_elem(expect_ty);
        if !known(&elem) {
            return;
        }
        for kid in &e.kids {
            let got = self.infer(kid);
            if known(&got) && !self.assignable(&elem, &got, Some(kid)) {
                self.fail(
                    line,
                    col,
                    &format!("cannot pass {got} to {expect_ty}, expected {elem}"),
                );
            }
        }
    }

    fn check_bound_args(&mut self, hit: &BoundHit, recv: &str, args: &[Expr], line: u32, col: u32) {
        let Some(m) = &hit.method else { return };
        let expect = if m.params.is_empty() {
            0
        } else {
            m.params.len() as i32 - 1
        };
        self.check_arity(
            &format!("{recv}.{}", m.name),
            expect,
            args.len() as i32,
            line,
            col,
        );
        let tenv = self.trait_env(&hit.trait_name);
        for (i, arg) in args.iter().enumerate() {
            if i + 1 >= m.param_types.len() {
                break;
            }
            let expect_ty = types::subst_type(&m.param_types[i + 1], &tenv);
            let got = self.infer(arg);
            if known(&expect_ty) && known(&got) && !self.assignable(&expect_ty, &got, Some(arg)) {
                self.fail(
                    line,
                    col,
                    &format!("cannot pass {got} to '{}', expected {expect_ty}", m.name),
                );
            }
        }
        let throws = m.throws_ex;
        let name = m.name.clone();
        self.require_try(throws, &name, line, col);
    }

    fn merge_method_env(
        &mut self,
        func: &FnDecl,
        e: &Expr,
        args: &[Expr],
        tenv: &mut HashMap<String, String>,
    ) {
        if func.type_params.is_empty() {
            if !e.type_args.is_empty() {
                self.fail(
                    e.span.line,
                    e.span.col,
                    &format!("'{}' does not take type arguments", func.name),
                );
            }
            return;
        }
        self.check_type_arg_count(
            &func.type_params,
            &e.type_args,
            e.span.line,
            e.span.col,
            &func.name,
        );
        let patterns: Vec<String> = func.param_types.iter().skip(1).cloned().collect();
        let inferred = self.infer_env(&func.type_params, &e.type_args, &patterns, args);
        tenv.extend(inferred);
        self.check_bounds(&func.type_params, tenv, e.span.line, e.span.col);
    }

    fn check_fn_type_call(&mut self, ty: &str, args: &[Expr], line: u32, col: u32, label: &str) {
        let Some((params, _)) = parse_fn_type(ty) else {
            return;
        };
        self.check_arity(label, params.len() as i32, args.len() as i32, line, col);
        for (i, arg) in args.iter().enumerate() {
            if i >= params.len() {
                break;
            }
            let got = self.infer(arg);
            if known(&params[i]) && known(&got) && !self.assignable(&params[i], &got, Some(arg)) {
                self.fail(
                    line,
                    col,
                    &format!("cannot pass {got} to '{label}', expected {}", params[i]),
                );
            }
            let expect = params[i].clone();
            self.check_array_elems(&expect, Some(arg), line, col);
        }
    }

    fn check_call(&mut self, e: &Expr) {
        let name = e.text.as_str();
        if e.text.is_empty() {
            if e.kids.is_empty() {
                return;
            }
            if e.kids[0].kind == ExprKind::Lambda {
                if let Some(fn_) = e.kids[0].lambda.clone() {
                    let args = rest(&e.kids);
                    self.check_call_arity(
                        &fn_,
                        "<fn>",
                        args.len() as i32,
                        false,
                        e.span.line,
                        e.span.col,
                    );
                    self.check_type_arg_count(
                        &fn_.type_params,
                        &e.type_args,
                        e.span.line,
                        e.span.col,
                        "<fn>",
                    );
                    if !fn_.type_params.is_empty() {
                        let tenv =
                            self.infer_env(&fn_.type_params, &e.type_args, &fn_.param_types, &args);
                        self.check_bounds(&fn_.type_params, &tenv, e.span.line, e.span.col);
                        self.check_arg_types_env(
                            &fn_,
                            &args,
                            false,
                            e.span.line,
                            e.span.col,
                            &tenv,
                        );
                    } else {
                        if !e.type_args.is_empty() {
                            self.fail(
                                e.span.line,
                                e.span.col,
                                "'<fn>' does not take type arguments",
                            );
                        }
                        self.check_arg_types(&fn_, &args, false, e.span.line, e.span.col);
                    }
                    self.require_try(fn_.throws_ex, "<fn>", e.span.line, e.span.col);
                    return;
                }
            }
            let ty = self.infer(&e.kids[0]);
            if is_fn_type(&ty) {
                self.check_fn_type_call(&ty, &rest(&e.kids), e.span.line, e.span.col, "<fn>");
                return;
            }
            if known(&ty) && !is_callable(&ty) {
                self.fail(e.span.line, e.span.col, "can only call a function");
            }
            return;
        }
        let ty = self.lookup(name);
        if is_fn_type(&ty) {
            self.check_fn_type_call(&ty, &e.kids, e.span.line, e.span.col, name);
            return;
        }
        if ty == "Fn" {
            return;
        }
        if known(&ty) && !is_callable(&ty) {
            self.fail(e.span.line, e.span.col, "can only call a function");
            return;
        }
        let n = e.kids.len() as i32;
        match name {
            "print" => return,
            "len" => {
                self.check_arity("len", 1, n, e.span.line, e.span.col);
                if n == 1 {
                    let ty = self.infer(&e.kids[0]);
                    if known(&ty) && !is_array_ty(&ty) && !is_map_ty(&ty) && !is_string(&ty) {
                        self.fail(e.span.line, e.span.col, "len expects Array, String, or Map");
                    }
                }
                return;
            }
            "assert" => {
                self.check_arity("assert", 1, n, e.span.line, e.span.col);
                return;
            }
            "argv" => {
                self.check_arity("argv", 1, n, e.span.line, e.span.col);
                return;
            }
            "argv_len" => {
                self.check_arity("argv_len", 0, n, e.span.line, e.span.col);
                return;
            }
            _ => {}
        }
        if let Some(fn_) = self.env.find_local_fn(name) {
            self.check_call_arity(&fn_, name, n, false, e.span.line, e.span.col);
            self.check_type_arg_count(
                &fn_.type_params,
                &e.type_args,
                e.span.line,
                e.span.col,
                name,
            );
            if !fn_.type_params.is_empty() {
                let tenv =
                    self.infer_env(&fn_.type_params, &e.type_args, &fn_.param_types, &e.kids);
                self.check_bounds(&fn_.type_params, &tenv, e.span.line, e.span.col);
                self.check_arg_types_env(&fn_, &e.kids, false, e.span.line, e.span.col, &tenv);
            } else {
                if !e.type_args.is_empty() {
                    self.fail(
                        e.span.line,
                        e.span.col,
                        &format!("'{name}' does not take type arguments"),
                    );
                }
                self.check_arg_types(&fn_, &e.kids, false, e.span.line, e.span.col);
            }
            self.require_try(fn_.throws_ex, name, e.span.line, e.span.col);
            return;
        }
        if !self.current_self.is_empty() {
            let hit = self.bound_method(&self.current_self.clone(), name);
            if hit.ok() {
                let self_name = self.current_self.clone();
                self.check_bound_args(&hit, &self_name, &e.kids, e.span.line, e.span.col);
                return;
            }
            if let (defined, Some(fn_)) = self.env.lookup_method(&self.current_self, name) {
                self.check_access(fn_.vis, "method", name, &defined, e.span.line, e.span.col);
                let label = format!("{}.{name}", self.current_self);
                self.check_call_arity(&fn_, &label, n, true, e.span.line, e.span.col);
                let mut tenv =
                    self.env_for_applied(&self.recv_applied(&self.current_self, &defined));
                self.merge_method_env(&fn_, e, &e.kids, &mut tenv);
                self.check_arg_types_env(&fn_, &e.kids, true, e.span.line, e.span.col, &tenv);
                self.require_try(fn_.throws_ex, name, e.span.line, e.span.col);
                return;
            }
        }
        self.fail(
            e.span.line,
            e.span.col,
            &format!(
                "unknown function '{name}'{}",
                stdlib::import_hint(name, &self.env.file)
            ),
        );
    }

    fn check_host_call(&mut self, module: &str, name: &str, n: i32, line: u32, col: u32) {
        let label = format!("{module}.{name}");
        let miss = |this: &Self| this.fail(line, col, &format!("unknown function {module}.{name}"));
        match module {
            "checks" => {
                if name == "eq" || name == "neq" || name == "eq_string" {
                    self.check_arity(&label, 2, n, line, col);
                } else if name == "that" || name == "truthy" {
                    self.check_arity(&label, 1, n, line, col);
                } else {
                    miss(self);
                }
            }
            "process" => {
                if name == "argv" {
                    self.check_arity(&label, 1, n, line, col);
                } else if name == "argc" {
                    self.check_arity(&label, 0, n, line, col);
                } else {
                    miss(self);
                }
            }
            "__math" => match name {
                "pow" | "powf" | "atan2" => self.check_arity(&label, 2, n, line, col),
                "random" => self.check_arity(&label, 0, n, line, col),
                "rand_int" | "sin" | "cos" | "sqrt" | "to_int" | "to_float" | "floor" | "ceil" => {
                    self.check_arity(&label, 1, n, line, col)
                }
                _ => miss(self),
            },
            "__str" => match name {
                "slice" | "replace" => self.check_arity(&label, 3, n, line, col),
                "repeat" | "contains" | "starts_with" | "ends_with" | "split" | "find" => {
                    self.check_arity(&label, 2, n, line, col)
                }
                "length" | "is_empty" | "upper" | "lower" | "trim" => {
                    self.check_arity(&label, 1, n, line, col)
                }
                _ => miss(self),
            },
            "__io" => match name {
                "write_text" => {
                    self.check_arity(&label, 2, n, line, col);
                    self.require_try(true, name, line, col);
                }
                "read_text" | "read_lines" => {
                    self.check_arity(&label, 1, n, line, col);
                    self.require_try(true, name, line, col);
                }
                "exists" | "remove" => self.check_arity(&label, 1, n, line, col),
                _ => miss(self),
            },
            "__uuid" => match name {
                "v4" => self.check_arity(&label, 0, n, line, col),
                "parse" => {
                    self.check_arity(&label, 1, n, line, col);
                    self.require_try(true, name, line, col);
                }
                "valid" => self.check_arity(&label, 1, n, line, col),
                _ => miss(self),
            },
            "__time" => {
                if name == "now" {
                    self.check_arity(&label, 0, n, line, col);
                } else if name == "sleep" || name == "delay" {
                    self.check_arity(&label, 1, n, line, col);
                } else {
                    miss(self);
                }
            }
            "__path" => {
                if name == "join" {
                    self.check_arity(&label, 2, n, line, col);
                } else if name == "parent" || name == "stem" {
                    self.check_arity(&label, 1, n, line, col);
                } else {
                    miss(self);
                }
            }
            "__json" => {
                if name == "parse" {
                    self.check_arity(&label, 1, n, line, col);
                    self.require_try(true, name, line, col);
                } else if name == "valid" || name == "stringify" {
                    self.check_arity(&label, 1, n, line, col);
                } else {
                    miss(self);
                }
            }
            "__regex" => match name {
                "replace" => self.check_arity(&label, 3, n, line, col),
                "is_match" | "find" | "find_match" | "captures" | "findall" | "split" => {
                    self.check_arity(&label, 2, n, line, col)
                }
                "valid" => self.check_arity(&label, 1, n, line, col),
                _ => miss(self),
            },
            "__ui" => self.check_ui(name, n, line, col, &label),
            "hooks" => match name {
                "before_break" | "before_place" | "after_break" | "after_place" | "fill_place"
                | "fill_remove" | "on_select" | "on_screen" | "break_speed" => {
                    self.check_arity(&label, 2, n, line, col)
                }
                _ => miss(self),
            },
            "events" => match name {
                "break" | "place" | "break_block" | "place_block" | "broken" | "placed"
                | "fill_place" | "fill_remove" | "select_slot" | "open_screen" | "break_speed"
                | "pickup_item" | "break_drop" | "loot" | "entity_spawn" | "entity_hurt"
                | "entity_death" | "entity_tick" | "use_item" | "use_block" | "use_entity"
                | "attack" | "chat" | "respawn" | "craft" | "smelt" | "repair" | "enchant"
                | "brew" | "smith" | "weave" | "tick" | "dawn" | "dusk" | "jump" | "sprint"
                | "land" | "command" | "structure_discovered" => {
                    self.check_arity(&label, 2, n, line, col)
                }
                _ => miss(self),
            },
            "world" => match name {
                "block" | "get_block" | "get_meta" | "get_fill" | "fill_level" | "is_air"
                | "is_compound" => {
                    self.check_arity(&label, 3, n, line, col)
                }
                "write" | "set_block" | "set_meta" | "set_fill" => {
                    self.check_arity(&label, 4, n, line, col)
                }
                "hardness" | "light" | "replaces" | "slot" | "count" => {
                    self.check_arity(&label, 1, n, line, col)
                }
                "time_of_day" | "is_day" | "season" | "has_time" => {
                    self.check_arity(&label, 0, n, line, col)
                }
                _ => miss(self),
            },
            "player" => match name {
                "stack" | "count" => self.check_arity(&label, 1, n, line, col),
                "held" | "selected" | "present" | "x" | "y" | "z" => {
                    self.check_arity(&label, 0, n, line, col)
                }
                _ => miss(self),
            },
            "recipe" => match name {
                "register" | "shapeless" | "shaped" | "smelt" | "smoke" | "blast" => {
                    self.check_arity(&label, 2, n, line, col)
                }
                _ => miss(self),
            },
            "loot" => match name {
                "block" | "chest" | "entity" => self.check_arity(&label, 2, n, line, col),
                _ => miss(self),
            },
            "item" => match name {
                "register" | "places" | "stack" => self.check_arity(&label, 2, n, line, col),
                "exists" | "hardness" | "light" | "replaces" | "max_count" => {
                    self.check_arity(&label, 1, n, line, col)
                }
                _ => miss(self),
            },
            "block" => match name {
                "register" | "texture" | "model" | "hardness" | "light" => {
                    self.check_arity(&label, 2, n, line, col)
                }
                _ => miss(self),
            },
            "entity" => match name {
                "register" | "model" | "hostile" | "spawn_egg" | "hearts" | "hurtbox"
                | "speed" => self.check_arity(&label, 2, n, line, col),
                "spawn" | "despawn" => self.check_arity(&label, 4, n, line, col),
                _ => miss(self),
            },
            "block_entity" => match name {
                "bind" => self.check_arity(&label, 1, n, line, col),
                "has" | "clear" => self.check_arity(&label, 3, n, line, col),
                "get" | "get_int" => self.check_arity(&label, 4, n, line, col),
                "set" | "set_int" => self.check_arity(&label, 5, n, line, col),
                _ => miss(self),
            },
            "modapi" => match name {
                "note" | "registry" => self.check_arity(&label, 1, n, line, col),
                "content" => self.check_arity(&label, 3, n, line, col),
                _ => miss(self),
            },
            "gui" => match name {
                "screen" => self.check_arity(&label, 5, n, line, col),
                "element" => self.check_arity(&label, 11, n, line, col),
                "remove" | "read" => self.check_arity(&label, 2, n, line, col),
                "press" => self.check_arity(&label, 3, n, line, col),
                "open" => self.check_arity(&label, 1, n, line, col),
                _ => miss(self),
            },
            "chat" => match name {
                "say" => self.check_arity(&label, 1, n, line, col),
                _ => miss(self),
            },
            "settings" => match name {
                "option" => self.check_arity(&label, 5, n, line, col),
                "watch" => self.check_arity(&label, 2, n, line, col),
                "choices" => self.check_arity(&label, 2, n, line, col),
                "value" => self.check_arity(&label, 1, n, line, col),
                _ => miss(self),
            },
            _ => {}
        }
    }

    fn check_ui(&mut self, name: &str, n: i32, line: u32, col: u32, label: &str) {
        match name {
            "open" => {
                self.check_arity(label, 4, n, line, col);
                self.require_try(true, name, line, col);
            }
            "fill" | "line" | "stroke_rect" | "image_rgb" => {
                self.check_arity(label, 6, n, line, col)
            }
            "fill_round" | "stroke_round" => self.check_arity(label, 7, n, line, col),
            "clip_push" | "text" => self.check_arity(label, 5, n, line, col),
            "image" => self.check_arity(label, 4, n, line, col),
            "text_width" | "image_width" | "image_height" | "clipboard_set" | "close" | "show"
            | "hide" | "poll" | "next_frame" | "alive" | "title" | "width" | "height"
            | "present" | "mouse_x" | "mouse_y" | "mouse_down" | "take_click"
            | "take_right_click" | "take_key" | "key_code" | "key_text" | "take_scroll"
            | "scroll_dx" | "scroll_dy" | "clip_pop" => self.check_arity(label, 1, n, line, col),
            "set_size" | "feed_click" | "feed_right_click" | "feed_key" | "feed_scroll"
            | "feed_mouse" => self.check_arity(label, 3, n, line, col),
            "set_title" | "clear" | "set_frame" | "feed_down" | "cursor" => {
                self.check_arity(label, 2, n, line, col)
            }
            "run" | "count" | "backend" | "platform" | "wait" | "font_height" | "clipboard_get" => {
                self.check_arity(label, 0, n, line, col)
            }
            _ => self.fail(line, col, &format!("unknown function __ui.{name}")),
        }
    }

    fn check_ufcs(&mut self, e: &Expr, n: i32) -> bool {
        let recv_ty = e.kids.first().map(|k| self.infer(k)).unwrap_or_default();
        let Some(fn_) = self.env.find_ufcs(&e.text, &recv_ty) else {
            return false;
        };
        let args = rest(&e.kids);
        self.check_call_arity(&fn_, &e.text, n, true, e.span.line, e.span.col);
        self.check_type_arg_count(
            &fn_.type_params,
            &e.type_args,
            e.span.line,
            e.span.col,
            &fn_.name,
        );
        if !fn_.type_params.is_empty() {
            let mut vals = Vec::new();
            if let Some(k) = e.kids.first() {
                vals.push(k.clone());
            }
            vals.extend(args.iter().cloned());
            let tenv = self.infer_env(&fn_.type_params, &e.type_args, &fn_.param_types, &vals);
            self.check_bounds(&fn_.type_params, &tenv, e.span.line, e.span.col);
            if !fn_.param_types.is_empty() && !e.kids.is_empty() {
                let expect = types::subst_type(&fn_.param_types[0], &tenv);
                let got = self.infer(&e.kids[0]);
                if known(&expect)
                    && known(&got)
                    && !self.assignable(&expect, &got, Some(&e.kids[0]))
                {
                    self.fail(
                        e.span.line,
                        e.span.col,
                        &format!("cannot pass {got} to '{}', expected {expect}", fn_.name),
                    );
                }
            }
            self.check_arg_types_env(&fn_, &args, true, e.span.line, e.span.col, &tenv);
        } else {
            if !fn_.param_types.is_empty() && !e.kids.is_empty() {
                let expect = fn_.param_types[0].clone();
                let got = self.infer(&e.kids[0]);
                if known(&expect)
                    && known(&got)
                    && !self.assignable(&expect, &got, Some(&e.kids[0]))
                {
                    self.fail(
                        e.span.line,
                        e.span.col,
                        &format!("cannot pass {got} to '{}', expected {expect}", fn_.name),
                    );
                }
            }
            self.check_arg_types(&fn_, &args, true, e.span.line, e.span.col);
        }
        let throws = fn_.throws_ex;
        let name = fn_.name.clone();
        self.require_try(throws, &name, e.span.line, e.span.col);
        true
    }

    fn check_value_method(&mut self, obj: &str, e: &Expr, n: i32) {
        if is_array_ty(obj) {
            if e.text == "len" || e.text == "pop" {
                self.check_arity(&format!("{obj}.{}", e.text), 0, n, e.span.line, e.span.col);
                return;
            }
            if e.text == "push" {
                self.check_arity(&format!("{obj}.{}", e.text), 1, n, e.span.line, e.span.col);
                if n == 1 && e.kids.len() >= 2 {
                    let elem = array_elem(obj);
                    let got = self.infer(&e.kids[1]);
                    if known(&elem)
                        && known(&got)
                        && !self.assignable(&elem, &got, Some(&e.kids[1]))
                    {
                        self.fail(
                            e.span.line,
                            e.span.col,
                            &format!("cannot pass {got} to 'push', expected {elem}"),
                        );
                    }
                    self.check_array_elems(&elem, Some(&e.kids[1]), e.span.line, e.span.col);
                }
                return;
            }
            if self.check_ufcs(e, n) {
                return;
            }
            self.fail(
                e.span.line,
                e.span.col,
                &format!("Array has no method '{}'", e.text),
            );
            return;
        }
        if is_map_ty(obj) {
            match e.text.as_str() {
                "len" | "keys" => {
                    self.check_arity(&format!("{obj}.{}", e.text), 0, n, e.span.line, e.span.col);
                    return;
                }
                "has" | "remove" => {
                    self.check_arity(&format!("{obj}.{}", e.text), 1, n, e.span.line, e.span.col);
                    return;
                }
                "insert" => {
                    self.check_arity(&format!("{obj}.{}", e.text), 2, n, e.span.line, e.span.col);
                    return;
                }
                _ => {}
            }
            if self.check_ufcs(e, n) {
                return;
            }
            self.fail(
                e.span.line,
                e.span.col,
                &format!("Map has no method '{}'", e.text),
            );
            return;
        }
        if is_string(obj) {
            if e.text == "len" {
                self.check_arity(&format!("{obj}.{}", e.text), 0, n, e.span.line, e.span.col);
                return;
            }
            if self.check_ufcs(e, n) {
                return;
            }
            self.fail(
                e.span.line,
                e.span.col,
                &format!("String has no method '{}'", e.text),
            );
            return;
        }
        if is_future_ty(obj) {
            if e.text == "cancel" {
                self.check_arity(&format!("{obj}.{}", e.text), 0, n, e.span.line, e.span.col);
                return;
            }
            self.fail(
                e.span.line,
                e.span.col,
                &format!("Future has no method '{}'", e.text),
            );
        }
    }

    fn check_method(&mut self, e: &Expr) {
        if e.kids.is_empty() {
            return;
        }
        let recv_kind = e.kids[0].kind;
        let recv_text = e.kids[0].text.clone();
        let n = (e.kids.len() as i32) - 1;
        let args = rest(&e.kids);
        if recv_kind == ExprKind::Var && recv_text == "Future" {
            if e.text != "all" && e.text != "race" {
                self.fail(
                    e.span.line,
                    e.span.col,
                    &format!("Future has no method '{}'", e.text),
                );
                return;
            }
            self.check_arity(&format!("Future.{}", e.text), 1, n, e.span.line, e.span.col);
            if n == 1 {
                let arg_ty = self.infer(&e.kids[1]);
                if known(&arg_ty) && !is_array_ty(&arg_ty) {
                    self.fail(
                        e.span.line,
                        e.span.col,
                        &format!("Future.{} expects Array of Future, got {arg_ty}", e.text),
                    );
                } else if known(&arg_ty) && is_array_ty(&arg_ty) {
                    let elem = array_elem(&arg_ty);
                    if known(&elem) && !is_future_ty(&elem) {
                        self.fail(
                            e.span.line,
                            e.span.col,
                            &format!(
                                "Future.{} expects Array of Future, got Array[{elem}]",
                                e.text
                            ),
                        );
                    }
                }
            }
            return;
        }
        if recv_kind == ExprKind::Var
            && self.env.find_struct(&recv_text).is_some()
            && !known(&self.lookup(&recv_text))
            && !self.env.singletons.borrow().contains(&recv_text)
        {
            let (defined, found) = self.env.lookup_static(&recv_text, &e.text);
            let Some(fn_) = found else {
                self.fail(
                    e.span.line,
                    e.span.col,
                    &format!("{recv_text} has no static method '{}'", e.text),
                );
                return;
            };
            self.check_access(
                fn_.vis,
                "method",
                &e.text,
                &defined,
                e.span.line,
                e.span.col,
            );
            self.check_call_arity(
                &fn_,
                &format!("{recv_text}.{}", e.text),
                n,
                false,
                e.span.line,
                e.span.col,
            );
            self.check_arg_types(&fn_, &args, false, e.span.line, e.span.col);
            self.require_try(fn_.throws_ex, &e.text, e.span.line, e.span.col);
            return;
        }
        if recv_kind == ExprKind::Var && recv_text == "super" {
            if self.current_super.is_empty() {
                self.fail(
                    e.span.line,
                    e.span.col,
                    "super is only valid in a method of a class that extends another",
                );
                return;
            }
            let (defined, found) = self.env.lookup_method(&self.current_super, &e.text);
            let Some(fn_) = found else {
                self.fail(
                    e.span.line,
                    e.span.col,
                    &format!("struct {} has no method '{}'", self.current_super, e.text),
                );
                return;
            };
            if fn_.is_abstract {
                self.fail(
                    e.span.line,
                    e.span.col,
                    &format!("cannot call abstract method '{}'", e.text),
                );
                return;
            }
            self.check_access(
                fn_.vis,
                "method",
                &e.text,
                &defined,
                e.span.line,
                e.span.col,
            );
            let label = format!("{}.{}", self.current_super, e.text);
            self.check_call_arity(&fn_, &label, n, true, e.span.line, e.span.col);
            let mut tenv = self.env_for_applied(&self.recv_applied(&self.current_super, &defined));
            self.merge_method_env(&fn_, e, &args, &mut tenv);
            self.check_arg_types_env(&fn_, &args, true, e.span.line, e.span.col, &tenv);
            self.require_try(fn_.throws_ex, &e.text, e.span.line, e.span.col);
            return;
        }
        if recv_kind == ExprKind::Var {
            if !self.current_self.is_empty() {
                if let Some(arity) = self.env.lookup_type_signal(&self.current_self, &recv_text) {
                    self.check_sig(&recv_text, arity, e, n);
                    return;
                }
            }
            if let Some(arity) = self.env.signal_arity.borrow().get(&recv_text).copied() {
                self.check_sig(&recv_text, arity, e, n);
                return;
            }
        }
        if e.kids[0].kind == ExprKind::Member && !e.kids[0].kids.is_empty() {
            let obj_ty = self.infer(&e.kids[0].kids[0]);
            if known(&obj_ty) {
                if let Some(arity) = self.env.lookup_type_signal(&obj_ty, &e.kids[0].text) {
                    let sig = e.kids[0].text.clone();
                    self.check_sig(&sig, arity, e, n);
                    return;
                }
            }
        }
        if self.infer(&e.kids[0]) == "Signal" {
            if e.text == "connect" || e.text == "disconnect" {
                self.check_arity(&format!("signal {}", e.text), 1, n, e.span.line, e.span.col);
                return;
            }
            if e.text == "emit" || e.text == "emit_deferred" {
                return;
            }
            self.fail(
                e.span.line,
                e.span.col,
                &format!("signal has no method '{}'", e.text),
            );
            return;
        }
        if recv_kind == ExprKind::Var && types::is_host_module(&recv_text) {
            self.check_host_call(&recv_text, &e.text, n, e.span.line, e.span.col);
            return;
        }
        if recv_kind == ExprKind::Var {
            if let Some(mod_name) = self.env.find_module_bind(&recv_text) {
                let lit = self.env.loaded.borrow().get(&mod_name).cloned();
                let Some(lit) = lit else {
                    self.fail(
                        e.span.line,
                        e.span.col,
                        &format!("unknown module '{mod_name}'"),
                    );
                    return;
                };
                let eit = lit.borrow().exports.get(&e.text).cloned();
                let Some(eit) = eit else {
                    self.fail(
                        e.span.line,
                        e.span.col,
                        &format!("module '{recv_text}' has no export '{}'", e.text),
                    );
                    return;
                };
                self.check_imported_fn(
                    &eit,
                    &args,
                    &e.type_args,
                    &format!("{recv_text}.{}", e.text),
                    e.span.line,
                    e.span.col,
                );
                return;
            }
            if let Some(en) = self.env.find_enum(&recv_text) {
                let found = en.variants.iter().find(|v| v.name == e.text).cloned();
                let Some(found) = found else {
                    self.fail(
                        e.span.line,
                        e.span.col,
                        &format!("enum {} has no variant '{}'", en.name, e.text),
                    );
                    return;
                };
                if n != found.arity {
                    self.fail(
                        e.span.line,
                        e.span.col,
                        &format!("{}.{} takes {} argument(s)", en.name, e.text, found.arity),
                    );
                }
                return;
            }
        }
        if let Some(mod_path) = self.try_module_path(&e.kids[0]) {
            if let Some(lit) = self.env.loaded.borrow().get(&mod_path).cloned() {
                let eit = lit.borrow().exports.get(&e.text).cloned();
                let Some(eit) = eit else {
                    self.fail(
                        e.span.line,
                        e.span.col,
                        &format!("module '{mod_path}' has no export '{}'", e.text),
                    );
                    return;
                };
                let label = format!("{mod_path}.{}", e.text);
                self.check_imported_fn(&eit, &args, &e.type_args, &label, e.span.line, e.span.col);
                return;
            }
        }
        let obj = self.infer(&e.kids[0]);
        if is_array_ty(&obj) || is_map_ty(&obj) || is_string(&obj) || is_future_ty(&obj) {
            let shown = if is_string(&obj) {
                "String".into()
            } else {
                obj
            };
            self.check_value_method(&shown, e, n);
            return;
        }
        if known(&obj) {
            let mut hit = self.trait_object_method(&obj, &e.text);
            if !hit.ok() {
                hit = self.bound_method(&obj, &e.text);
            }
            if hit.ok() {
                self.check_bound_args(&hit, &obj, &args, e.span.line, e.span.col);
                return;
            }
            let lookup_ty = if is_callable(&obj) {
                "Fn".to_string()
            } else {
                obj.clone()
            };
            let (defined, found) = self.env.lookup_method(&lookup_ty, &e.text);
            if let Some(fn_) = found {
                self.check_access(
                    fn_.vis,
                    "method",
                    &e.text,
                    &defined,
                    e.span.line,
                    e.span.col,
                );
                let label = format!("{obj}.{}", e.text);
                self.check_call_arity(&fn_, &label, n, true, e.span.line, e.span.col);
                let mut tenv = self.env_for_applied(&self.recv_applied(&obj, &defined));
                self.merge_method_env(&fn_, e, &args, &mut tenv);
                self.check_arg_types_env(&fn_, &args, true, e.span.line, e.span.col, &tenv);
                self.require_try(fn_.throws_ex, &e.text, e.span.line, e.span.col);
                return;
            }
            if self.check_ufcs(e, n) {
                return;
            }
            if is_callable(&obj) {
                self.fail(
                    e.span.line,
                    e.span.col,
                    &format!("function has no method '{}'", e.text),
                );
                return;
            }
            let word = if self.is_trait_type(&obj) {
                "trait"
            } else {
                "struct"
            };
            self.fail(
                e.span.line,
                e.span.col,
                &format!("{word} {obj} has no method '{}'", e.text),
            );
            return;
        }
        self.check_ufcs(e, n);
    }

    fn check_sig(&mut self, signal: &str, expect: i32, e: &Expr, n: i32) {
        if e.text == "connect" || e.text == "disconnect" {
            self.check_arity(
                &format!("signal '{signal}' {}", e.text),
                1,
                n,
                e.span.line,
                e.span.col,
            );
            if n == 1 && e.kids.len() >= 2 {
                let ty = self.infer(&e.kids[1]);
                if known(&ty) && !is_callable(&ty) {
                    self.fail(
                        e.span.line,
                        e.span.col,
                        &format!("signal '{signal}' {} expects a function", e.text),
                    );
                }
            }
            return;
        }
        if e.text == "emit" || e.text == "emit_deferred" {
            if expect != n {
                self.fail(
                    e.span.line,
                    e.span.col,
                    &format!("signal '{signal}' expected {expect} argument(s), got {n}"),
                );
            }
            return;
        }
        self.fail(
            e.span.line,
            e.span.col,
            &format!("signal '{signal}' has no method '{}'", e.text),
        );
    }

    fn check_new(&mut self, e: &Expr) {
        let head = type_head(&e.text);
        if self.env.find_trait(&head).is_some() && self.env.all_types.borrow().get(&head).is_none()
        {
            self.fail(
                e.span.line,
                e.span.col,
                &format!("cannot construct trait '{head}'"),
            );
            return;
        }
        if head.is_empty() || self.env.all_types.borrow().get(&head).is_none() {
            self.fail(e.span.line, e.span.col, &format!("unknown type '{head}'"));
            return;
        }
        if self.env.class_abstract.borrow().get(&head) == Some(&true) {
            self.fail(
                e.span.line,
                e.span.col,
                &format!("cannot construct abstract class '{head}'"),
            );
            return;
        }
        if self.engine_class(&head) || !self.env.native_base(&head).is_empty() {
            return;
        }
        let mut lit = Expr::new(ExprKind::StructLit, e.span);
        lit.text = head;
        self.walk_expr(&lit);
    }

    fn check_cast(&mut self, e: &Expr) {
        let head = type_head(&e.text);
        if head.is_empty() || self.env.all_types.borrow().get(&head).is_none() {
            self.fail(e.span.line, e.span.col, &format!("unknown type '{head}'"));
            return;
        }
        if self.env.native_base(&head).is_empty() {
            self.fail(e.span.line, e.span.col, "can only cast to an engine type");
            return;
        }
        let src = e.kids.first().map(|k| self.infer(k)).unwrap_or_default();
        if known(&src) && !self.godot_object(&src) {
            self.fail(
                e.span.line,
                e.span.col,
                &format!("cannot cast {src} to {head}"),
            );
        }
    }

    fn check_is(&mut self, e: &Expr) {
        let head = type_head(&e.text);
        if head.is_empty() || self.env.all_types.borrow().get(&head).is_none() {
            self.fail(e.span.line, e.span.col, &format!("unknown type '{head}'"));
            return;
        }
        if self.env.native_base(&head).is_empty() {
            self.fail(e.span.line, e.span.col, "can only test an engine type");
            return;
        }
        let src = e.kids.first().map(|k| self.infer(k)).unwrap_or_default();
        if known(&src) && !self.godot_object(&src) {
            self.fail(e.span.line, e.span.col, &format!("cannot test {src}"));
        }
    }

    fn walk_expr(&mut self, e: &Expr) {
        match e.kind {
            ExprKind::Try => {
                if e.kids.is_empty() {
                    return;
                }
                let prev = self.in_try;
                self.in_try = true;
                self.walk_expr(&e.kids[0]);
                self.in_try = prev;
                return;
            }
            ExprKind::Cast => {
                if let Some(k) = e.kids.first() {
                    self.walk_expr(k);
                }
                self.check_cast(e);
                return;
            }
            ExprKind::Is => {
                if let Some(k) = e.kids.first() {
                    self.walk_expr(k);
                }
                self.check_is(e);
                return;
            }
            ExprKind::New => {
                self.check_new(e);
                return;
            }
            ExprKind::NodePath => {
                if self.current_self.is_empty()
                    || self
                        .env
                        .lookup_method(&self.current_self, "get_node")
                        .1
                        .is_none()
                {
                    self.fail(e.span.line, e.span.col, "node path requires a Node");
                }
                return;
            }
            ExprKind::Await => {
                if e.kids.is_empty() {
                    return;
                }
                self.walk_expr(&e.kids[0]);
                let ty = self.infer(&e.kids[0]);
                if ty == "Signal" {
                    if !e.boolean_value {
                        self.fail(
                            e.span.line,
                            e.span.col,
                            "await signal must be its own statement",
                        );
                    }
                    return;
                }
                if !self.current_async {
                    self.fail(
                        e.span.line,
                        e.span.col,
                        "'await' is only allowed in async functions",
                    );
                }
                if known(&ty) && !is_future_ty(&ty) {
                    self.fail(
                        e.span.line,
                        e.span.col,
                        &format!("can only await Future, got {ty}"),
                    );
                }
                return;
            }
            ExprKind::Lambda => {
                if let Some(lam) = e.lambda.clone() {
                    let self_ty = self.current_self.clone();
                    self.check_fn(&lam, &self_ty);
                }
                return;
            }
            ExprKind::Var => {
                if !self.defined(&e.text) {
                    self.fail(
                        e.span.line,
                        e.span.col,
                        &format!(
                            "undefined variable '{}'{}",
                            e.text,
                            stdlib::import_hint(&e.text, &self.env.file)
                        ),
                    );
                } else if !self.is_local(&e.text) && !self.current_self.is_empty() {
                    let found = self.env.lookup_field(&self.current_self, &e.text);
                    if !found.defined_on.is_empty() {
                        self.check_access(
                            found.vis,
                            "field",
                            &e.text,
                            &found.defined_on,
                            e.span.line,
                            e.span.col,
                        );
                    }
                }
                return;
            }
            ExprKind::Call => {
                for kid in &e.kids {
                    self.walk_expr(kid);
                }
                self.check_call(e);
                return;
            }
            ExprKind::MethodCall => {
                for (i, kid) in e.kids.iter().enumerate() {
                    if i == 0
                        && kid.kind == ExprKind::Var
                        && (kid.text == "Future" || self.env.find_struct(&kid.text).is_some())
                    {
                        continue;
                    }
                    self.walk_expr(kid);
                }
                self.check_method(e);
                return;
            }
            ExprKind::Member => {
                self.walk_member(e);
                return;
            }
            ExprKind::Index => {
                for kid in &e.kids {
                    self.walk_expr(kid);
                }
                self.check_index(e);
                return;
            }
            ExprKind::Range => {
                for kid in &e.kids {
                    self.walk_expr(kid);
                }
                if e.kids.len() >= 2 {
                    let start = self.infer(&e.kids[0]);
                    let end = self.infer(&e.kids[1]);
                    if known(&start) && start != "Int" {
                        self.fail(
                            e.kids[0].span.line,
                            e.kids[0].span.col,
                            "range start must be Int",
                        );
                    }
                    if known(&end) && end != "Int" {
                        self.fail(
                            e.kids[1].span.line,
                            e.kids[1].span.col,
                            "range end must be Int",
                        );
                    }
                }
                return;
            }
            ExprKind::StructLit => {
                self.walk_struct_lit(e);
                return;
            }
            _ => {}
        }
        for kid in &e.kids {
            self.walk_expr(kid);
        }
        if e.kind == ExprKind::Binary {
            self.check_binop(e);
        }
        if e.kind == ExprKind::If && e.kids.len() >= 3 {
            let a = self.infer(&e.kids[1]);
            let b = self.infer(&e.kids[2]);
            if known(&a) && known(&b) && self.if_type(&a, &b).is_empty() {
                self.fail(
                    e.span.line,
                    e.span.col,
                    &format!("if branches are {a} and {b}"),
                );
            }
        }
        if e.kind == ExprKind::Format {
            for kid in &e.kids {
                let ty = self.infer(kid);
                if known(&ty) && !is_string(&ty) && !is_numeric(&ty) {
                    self.fail(
                        e.span.line,
                        e.span.col,
                        &format!("cannot add String and {ty}"),
                    );
                }
            }
        }
        if e.kind == ExprKind::Unary && e.text == "-" {
            if let Some(kid) = e.kids.first() {
                let ty = self.infer(kid);
                if known(&ty) && !is_numeric(&ty) && !value_math(&type_head(&ty)) {
                    self.fail(e.span.line, e.span.col, &format!("cannot negate {ty}"));
                }
            }
        }
        if e.kind == ExprKind::Unary && e.text == "~" {
            if let Some(kid) = e.kids.first() {
                let ty = self.infer(kid);
                if known(&ty) && ty != "Int" {
                    self.fail(e.span.line, e.span.col, "bitwise not needs Int");
                }
            }
        }
    }

    fn walk_member(&mut self, e: &Expr) {
        for kid in &e.kids {
            self.walk_expr(kid);
        }
        if let Some(recv) = e.kids.first() {
            if recv.kind == ExprKind::Var {
                if let Some(en) = self.env.find_enum(&recv.text) {
                    if !en.variants.iter().any(|v| v.name == e.text) {
                        self.fail(
                            e.span.line,
                            e.span.col,
                            &format!("enum {} has no variant '{}'", en.name, e.text),
                        );
                    }
                    return;
                }
                if self.try_module_path(e).is_some()
                    || self.env.find_module_bind(&recv.text).is_some()
                    || types::is_host_module(&recv.text)
                    || self.env.signal_arity.borrow().contains_key(&recv.text)
                {
                    return;
                }
            }
        }
        if self.try_module_path(e).is_some() {
            return;
        }
        let obj = e.kids.first().map(|k| self.infer(k)).unwrap_or_default();
        if is_array_ty(&obj) || is_map_ty(&obj) || is_string(&obj) {
            if e.text != "len" {
                let what = if is_array_ty(&obj) {
                    "Array".to_string()
                } else if is_map_ty(&obj) {
                    "Map".to_string()
                } else {
                    obj.clone()
                };
                self.fail(
                    e.span.line,
                    e.span.col,
                    &format!("{what} has no member '{}'", e.text),
                );
            }
            return;
        }
        if known(&obj) && self.env.lookup_type_signal(&obj, &e.text).is_some() {
            return;
        }
        if known(&obj) && self.is_trait_type(&obj) {
            self.fail(
                e.span.line,
                e.span.col,
                &format!("trait {obj} has no field '{}'", e.text),
            );
            return;
        }
        if known(&obj) && !self.has_field(&obj, &e.text) && self.live_prop(&obj, &e.text).is_empty()
        {
            self.fail(
                e.span.line,
                e.span.col,
                &format!("struct {obj} has no field '{}'", e.text),
            );
        } else if known(&obj) {
            let found: FieldHit = self.env.lookup_field(&obj, &e.text);
            if !found.defined_on.is_empty() {
                self.check_access(
                    found.vis,
                    "field",
                    &e.text,
                    &found.defined_on,
                    e.span.line,
                    e.span.col,
                );
            }
        }
    }

    fn walk_struct_lit(&mut self, e: &Expr) {
        for kid in &e.kids {
            self.walk_expr(kid);
        }
        let Some(st_rc) = self.env.find_struct(&e.text) else {
            if self.env.find_trait(&e.text).is_some() {
                self.fail(
                    e.span.line,
                    e.span.col,
                    &format!("cannot construct trait '{}'", e.text),
                );
            } else {
                self.fail(
                    e.span.line,
                    e.span.col,
                    &format!(
                        "undefined struct '{}'{}",
                        e.text,
                        stdlib::import_hint(&e.text, &self.env.file)
                    ),
                );
            }
            return;
        };
        if self.env.class_abstract.borrow().get(&e.text) == Some(&true) {
            self.fail(
                e.span.line,
                e.span.col,
                &format!("cannot construct abstract class '{}'", e.text),
            );
            return;
        }
        let st = st_rc.borrow().clone();
        self.check_type_arg_count(
            &st.type_params,
            &e.type_args,
            e.span.line,
            e.span.col,
            &st.name,
        );
        let mut tenv = HashMap::new();
        if !st.type_params.is_empty() {
            let prev = self.generic_params.clone();
            for p in &st.type_params {
                self.generic_params.insert(p.name.clone());
            }
            if !e.type_args.is_empty() {
                tenv = types::type_env_from(&st.type_params, &e.type_args);
            }
            for i in 0..e.names.len().min(e.kids.len()) {
                let pat = types::type_of_field(&st.fields, &st.field_types, &e.names[i]);
                let got = self.infer(&e.kids[i]);
                self.unify_type(&pat, &got, &mut tenv);
            }
            self.generic_params = prev;
            self.check_bounds(&st.type_params, &tenv, e.span.line, e.span.col);
        }
        let mut seen = HashSet::new();
        for (i, field) in e.names.iter().enumerate() {
            if !st.fields.iter().any(|f| f == field) {
                self.fail(
                    e.span.line,
                    e.span.col,
                    &format!("unknown field '{field}' on {}", st.name),
                );
            }
            if !seen.insert(field.clone()) {
                self.fail(
                    e.span.line,
                    e.span.col,
                    &format!("duplicate field '{field}' on {}", st.name),
                );
            }
            if i < e.kids.len() {
                let expect = types::subst_type(
                    &types::type_of_field(&st.fields, &st.field_types, field),
                    &tenv,
                );
                let got = self.infer(&e.kids[i]);
                if known(&expect)
                    && known(&got)
                    && !self.assignable(&expect, &got, Some(&e.kids[i]))
                {
                    self.fail(
                        e.span.line,
                        e.span.col,
                        &format!("cannot assign {got} to field '{field}', expected {expect}"),
                    );
                }
                self.check_array_elems(&expect, Some(&e.kids[i]), e.span.line, e.span.col);
            }
        }
        let defs = self.env.field_defaults.borrow().get(&st.name).cloned();
        for field in &e.names {
            let found = self.env.lookup_field(&st.name, field);
            if !found.defined_on.is_empty() && found.defined_on != st.name {
                self.check_access(
                    found.vis,
                    "field",
                    field,
                    &found.defined_on,
                    e.span.line,
                    e.span.col,
                );
            }
        }
        for field in &st.fields {
            if e.names.iter().any(|n| n == field) {
                continue;
            }
            let has_def = defs.as_ref().is_some_and(|d| d.contains_key(field));
            if !has_def && !types::is_optional_field(&st.fields, &st.field_optional, field) {
                self.fail(
                    e.span.line,
                    e.span.col,
                    &format!("missing field '{field}' on {}", st.name),
                );
            }
        }
    }

    fn walk_block(&mut self, stmts: &mut [Stmt]) {
        for i in 0..stmts.len() {
            self.walk_stmt(&mut stmts[i]);
            let go = stmts[i]
                .expr
                .as_ref()
                .is_some_and(|e| e.kind == ExprKind::Await && e.boolean_value);
            if !go {
                continue;
            }
            let mut expr = stmts[i].expr.clone().unwrap();
            if !self.bind_await_args(&mut expr) {
                continue;
            }
            stmts[i].expr = Some(expr);
            let mut rest = stmts[i + 1..].to_vec();
            self.walk_block(&mut rest);
            return;
        }
    }

    fn drain_scopes(&mut self, mark: usize) -> Vec<HashMap<String, String>> {
        let mut extra = Vec::new();
        while self.scopes.len() > mark {
            extra.insert(0, self.scopes.pop().unwrap());
        }
        extra
    }

    fn bind_await_args(&mut self, await_expr: &mut Expr) -> bool {
        if await_expr.kids.is_empty() {
            return false;
        }
        let Some(sig) = self.await_signal(&await_expr.kids[0]) else {
            return false;
        };
        if sig.params.is_empty() {
            return false;
        }
        await_expr.names.clear();
        await_expr.type_args.clear();
        self.scopes.push(HashMap::new());
        for (i, name) in sig.params.iter().enumerate() {
            let ty = sig.param_types.get(i).cloned().unwrap_or_default();
            if !ty.is_empty() {
                self.check_type_name(&ty, await_expr.span.line, await_expr.span.col);
            }
            if !name.is_empty() {
                self.bind(name, &ty);
            }
            await_expr.names.push(name.clone());
            await_expr.type_args.push(ty);
        }
        true
    }

    fn await_signal(&mut self, signal: &Expr) -> Option<SignalDecl> {
        if signal.kind == ExprKind::Member && !signal.kids.is_empty() {
            let ty = self.infer(&signal.kids[0]);
            return self.env.lookup_signal(&ty, &signal.text);
        }
        if signal.kind == ExprKind::Var && !self.current_self.is_empty() {
            return self.env.lookup_signal(&self.current_self, &signal.text);
        }
        None
    }

    fn walk_stmt(&mut self, stmt: &mut Stmt) {
        match stmt.kind {
            StmtKind::Pass | StmtKind::Comment => {}
            StmtKind::Break => {
                if self.loop_depth == 0 {
                    self.fail(stmt.span.line, stmt.span.col, "break outside loop");
                }
            }
            StmtKind::Continue => {
                if self.loop_depth == 0 {
                    self.fail(stmt.span.line, stmt.span.col, "continue outside loop");
                }
            }
            StmtKind::Throw => {
                if !self.current_throws {
                    self.fail(
                        stmt.span.line,
                        stmt.span.col,
                        "throw outside throwing function",
                    );
                }
                if let Some(e) = stmt.expr.clone() {
                    self.walk_expr(&e);
                }
            }
            StmtKind::Do => {
                let mark = self.scopes.len();
                self.walk_block(&mut stmt.body);
                let from_body = self.drain_scopes(mark);
                if !stmt.name.is_empty() {
                    self.scopes.push(HashMap::new());
                    self.bind(&stmt.name.clone(), "");
                    let catch_top = self.scopes.len();
                    self.walk_block(&mut stmt.else_body);
                    let from_else = self.drain_scopes(catch_top);
                    self.scopes.pop();
                    self.scopes.extend(from_body);
                    self.scopes.extend(from_else);
                } else {
                    self.scopes.extend(from_body);
                }
            }
            StmtKind::Expr => {
                if let Some(expr) = &mut stmt.expr {
                    if expr.kind == ExprKind::Await
                        && !expr.kids.is_empty()
                        && self.infer(&expr.kids[0]) == "Signal"
                    {
                        expr.boolean_value = true;
                    }
                }
                if let Some(expr) = stmt.expr.clone() {
                    self.walk_expr(&expr);
                    let ty = self.infer(&expr);
                    if known(&ty) && is_future_ty(&ty) {
                        self.fail(
                            stmt.span.line,
                            stmt.span.col,
                            "unused Future; await or bind it",
                        );
                    }
                }
            }
            StmtKind::Var | StmtKind::Const => {
                if let Some(e) = stmt.expr.clone() {
                    self.walk_expr(&e);
                }
                let got = stmt
                    .expr
                    .as_ref()
                    .map(|e| self.infer(e))
                    .unwrap_or_default();
                if !stmt.type_name.is_empty() {
                    self.check_type_name(&stmt.type_name, stmt.span.line, stmt.span.col);
                    if !is_fn_type(&stmt.type_name) {
                        if let Some(st) = self.env.find_struct(&stmt.type_name) {
                            let params = st.borrow().type_params.clone();
                            let who = type_head(&stmt.type_name);
                            self.check_type_arg_count(
                                &params,
                                &types::type_arg_list(&stmt.type_name),
                                stmt.span.line,
                                stmt.span.col,
                                &who,
                            );
                        } else if let Some(tr) = self.env.find_trait(&stmt.type_name) {
                            let params = tr.type_params.clone();
                            let who = type_head(&stmt.type_name);
                            self.check_type_arg_count(
                                &params,
                                &types::type_arg_list(&stmt.type_name),
                                stmt.span.line,
                                stmt.span.col,
                                &who,
                            );
                        }
                    }
                }
                if known(&stmt.type_name)
                    && known(&got)
                    && !self.assignable(&stmt.type_name, &got, stmt.expr.as_ref())
                {
                    self.fail(
                        stmt.span.line,
                        stmt.span.col,
                        &format!(
                            "variable annotated as {} but initializer looks like {got}",
                            stmt.type_name
                        ),
                    );
                }
                if !stmt.type_name.is_empty() {
                    let ty = stmt.type_name.clone();
                    self.check_array_elems(&ty, stmt.expr.as_ref(), stmt.span.line, stmt.span.col);
                }
                let bind_ty = if known(&stmt.type_name) {
                    stmt.type_name.clone()
                } else {
                    got
                };
                let name = stmt.name.clone();
                self.bind(&name, &bind_ty);
            }
            StmtKind::Assign => {
                if let Some(e) = stmt.expr.clone() {
                    self.walk_expr(&e);
                }
                if !self.defined(&stmt.name) {
                    self.fail(
                        stmt.span.line,
                        stmt.span.col,
                        &format!(
                            "undefined variable '{}'{}",
                            stmt.name,
                            stdlib::import_hint(&stmt.name, &self.env.file)
                        ),
                    );
                    return;
                }
                if self.env.lookup_const("", &stmt.name).is_some() && !self.is_local(&stmt.name) {
                    self.fail(
                        stmt.span.line,
                        stmt.span.col,
                        &format!("cannot assign to const '{}'", stmt.name),
                    );
                    return;
                }
                if !self.is_local(&stmt.name) && !self.current_self.is_empty() {
                    let found = self.env.lookup_field(&self.current_self, &stmt.name);
                    if !found.defined_on.is_empty() {
                        self.check_access(
                            found.vis,
                            "field",
                            &stmt.name,
                            &found.defined_on,
                            stmt.span.line,
                            stmt.span.col,
                        );
                        if self.env.is_data_type(&found.defined_on)
                            || self.env.is_data_type(&self.current_self)
                        {
                            self.fail(
                                stmt.span.line,
                                stmt.span.col,
                                &format!("cannot assign to data field '{}'", stmt.name),
                            );
                        }
                    }
                }
                let lt = self.lookup(&stmt.name);
                let rt = stmt
                    .expr
                    .as_ref()
                    .map(|e| self.infer(e))
                    .unwrap_or_default();
                let op = stmt.op.clone();
                let line = stmt.span.line;
                let col = stmt.span.col;
                let expr = stmt.expr.clone();
                self.check_compound(&op, &lt, &rt, line, col, expr.as_ref());
                if op.is_empty() || op == "=" {
                    self.check_array_elems(&lt, stmt.expr.as_ref(), line, col);
                }
            }
            StmtKind::FieldAssign => {
                if stmt.target.as_ref().is_some_and(|t| {
                    t.kind == ExprKind::Var
                        && self.env.find_struct(&t.text).is_some()
                        && self.env.lookup_const(&t.text, &stmt.name).is_some()
                }) {
                    let recv = stmt.target.as_ref().unwrap().text.clone();
                    self.fail(
                        stmt.span.line,
                        stmt.span.col,
                        &format!("cannot assign to const '{recv}.{}'", stmt.name),
                    );
                    return;
                }
                if let Some(t) = stmt.target.clone() {
                    self.walk_expr(&t);
                }
                if let Some(e) = stmt.expr.clone() {
                    self.walk_expr(&e);
                }
                let obj = stmt
                    .target
                    .as_ref()
                    .map(|t| self.infer(t))
                    .unwrap_or_default();
                if known(&obj) && self.is_trait_type(&obj) {
                    self.fail(
                        stmt.span.line,
                        stmt.span.col,
                        &format!("trait {obj} has no field '{}'", stmt.name),
                    );
                } else if known(&obj)
                    && !self.has_field(&obj, &stmt.name)
                    && self.live_prop(&obj, &stmt.name).is_empty()
                {
                    self.fail(
                        stmt.span.line,
                        stmt.span.col,
                        &format!("struct {obj} has no field '{}'", stmt.name),
                    );
                } else if known(&obj) {
                    let found = self.env.lookup_field(&obj, &stmt.name);
                    if !found.defined_on.is_empty() {
                        self.check_access(
                            found.vis,
                            "field",
                            &stmt.name,
                            &found.defined_on,
                            stmt.span.line,
                            stmt.span.col,
                        );
                    }
                    if self.env.is_data_type(&obj)
                        || (!found.defined_on.is_empty()
                            && self.env.is_data_type(&found.defined_on))
                    {
                        self.fail(
                            stmt.span.line,
                            stmt.span.col,
                            &format!("cannot assign to data field '{}'", stmt.name),
                        );
                    }
                }
                let lt = if known(&obj) {
                    self.field_type(&obj, &stmt.name)
                } else {
                    String::new()
                };
                let rt = stmt
                    .expr
                    .as_ref()
                    .map(|e| self.infer(e))
                    .unwrap_or_default();
                let op = stmt.op.clone();
                self.check_compound(
                    &op,
                    &lt,
                    &rt,
                    stmt.span.line,
                    stmt.span.col,
                    stmt.expr.as_ref(),
                );
                if op.is_empty() || op == "=" {
                    self.check_array_elems(&lt, stmt.expr.as_ref(), stmt.span.line, stmt.span.col);
                }
            }
            StmtKind::IndexAssign => {
                if let Some(t) = stmt.target.clone() {
                    self.walk_expr(&t);
                }
                if let Some(e) = stmt.expr.clone() {
                    self.walk_expr(&e);
                }
                if let Some(t) = stmt.target.clone() {
                    if t.kids.len() >= 2 {
                        self.check_index(&t);
                    }
                    if t.kind == ExprKind::Index && t.kids.len() >= 2 {
                        let obj = self.infer(&t.kids[0]);
                        let elem = array_elem(&obj);
                        let rt = stmt
                            .expr
                            .as_ref()
                            .map(|e| self.infer(e))
                            .unwrap_or_default();
                        if known(&elem)
                            && known(&rt)
                            && !self.assignable(&elem, &rt, stmt.expr.as_ref())
                        {
                            self.fail(
                                stmt.span.line,
                                stmt.span.col,
                                &format!("cannot assign {rt} to {elem}"),
                            );
                        }
                        self.check_array_elems(
                            &elem,
                            stmt.expr.as_ref(),
                            stmt.span.line,
                            stmt.span.col,
                        );
                    }
                }
            }
            StmtKind::Return => {
                let prev = self.throwing_try;
                self.throwing_try = false;
                if let Some(e) = stmt.expr.clone() {
                    self.walk_expr(&e);
                }
                if self.throwing_try && !self.current_throws {
                    self.fail(
                        stmt.span.line,
                        stmt.span.col,
                        "returning a throwing call requires 'throws'",
                    );
                }
                self.throwing_try = prev || self.throwing_try;
                let got = stmt
                    .expr
                    .as_ref()
                    .map(|e| self.infer(e))
                    .unwrap_or_default();
                let ret = self.current_return.clone();
                if known(&ret) && known(&got) && !self.assignable(&ret, &got, stmt.expr.as_ref()) {
                    self.fail(
                        stmt.span.line,
                        stmt.span.col,
                        &format!("cannot return {got} from {ret} function"),
                    );
                }
                self.check_array_elems(&ret, stmt.expr.as_ref(), stmt.span.line, stmt.span.col);
            }
            StmtKind::If => {
                if let Some(e) = stmt.expr.clone() {
                    self.walk_expr(&e);
                }
                let mark = self.scopes.len();
                self.walk_block(&mut stmt.body);
                let from_body = self.drain_scopes(mark);
                self.walk_block(&mut stmt.else_body);
                let from_else = self.drain_scopes(mark);
                self.scopes.extend(from_body);
                self.scopes.extend(from_else);
            }
            StmtKind::While => {
                if let Some(e) = stmt.expr.clone() {
                    self.walk_expr(&e);
                }
                self.loop_depth += 1;
                let mark = self.scopes.len();
                self.walk_block(&mut stmt.body);
                self.drain_scopes(mark);
                self.loop_depth -= 1;
            }
            StmtKind::For => {
                if let Some(e) = stmt.expr.clone() {
                    self.walk_expr(&e);
                }
                self.scopes.push(HashMap::new());
                let it = stmt
                    .expr
                    .as_ref()
                    .map(|e| self.infer(e))
                    .unwrap_or_default();
                let item = if is_array_ty(&it) {
                    array_elem(&it)
                } else if is_map_ty(&it) || is_string(&it) {
                    "String".into()
                } else if it == "Int" || it == "Range" {
                    "Int".into()
                } else {
                    String::new()
                };
                let name = stmt.name.clone();
                self.bind(&name, &item);
                self.loop_depth += 1;
                let mark = self.scopes.len();
                self.walk_block(&mut stmt.body);
                self.drain_scopes(mark);
                self.scopes.pop();
                self.loop_depth -= 1;
            }
            StmtKind::Match => self.walk_match(stmt),
        }
    }

    fn walk_match(&mut self, stmt: &mut Stmt) {
        if let Some(e) = stmt.expr.clone() {
            self.walk_expr(&e);
        }
        let ty = stmt
            .expr
            .as_ref()
            .map(|e| self.infer(e))
            .unwrap_or_default();
        let en = if known(&ty) {
            self.env.find_enum(&ty)
        } else {
            None
        };
        let variants: Vec<String> = en
            .as_ref()
            .map(|en| en.variants.iter().map(|v| v.name.clone()).collect())
            .unwrap_or_default();
        let en_name = en.as_ref().map(|en| en.name.clone()).unwrap_or_default();
        let mut covered = HashSet::new();
        let mut wild = false;
        let mut escaped = Vec::new();
        for arm in &mut stmt.arms {
            if arm.pat == Pat::Wildcard {
                wild = true;
            } else if arm.pat == Pat::Variant && en.is_some() {
                if !variants.iter().any(|v| v == &arm.name) {
                    self.fail(
                        arm.span.line,
                        arm.span.col,
                        &format!("enum {en_name} has no variant '{}'", arm.name),
                    );
                } else {
                    covered.insert(arm.name.clone());
                }
            }
            self.scopes.push(HashMap::new());
            for b in &arm.binds.clone() {
                if !b.is_empty() {
                    self.bind(b, "");
                }
            }
            let arm_top = self.scopes.len();
            self.walk_block(&mut arm.body);
            escaped.extend(self.drain_scopes(arm_top));
            self.scopes.pop();
        }
        self.scopes.extend(escaped);
        if en.is_some() && !wild {
            let missing: Vec<String> = variants
                .into_iter()
                .filter(|v| !covered.contains(v))
                .collect();
            if !missing.is_empty() {
                let mut msg = format!("match of {en_name} is missing variant");
                if missing.len() > 1 {
                    msg.push('s');
                }
                for (i, name) in missing.iter().enumerate() {
                    msg.push_str(if i == 0 { " '" } else { ", '" });
                    msg.push_str(name);
                    msg.push('\'');
                }
                self.fail(stmt.span.line, stmt.span.col, &msg);
            }
        }
    }

    fn check_fn(&mut self, func: &FnDecl, self_type: &str) {
        if func.is_vararg && !engine_vararg(func) {
            self.fail(func.span.line, 1, "vararg applies to an engine method");
        }
        if func.is_abstract {
            return;
        }
        let prev_ret = self.current_return.clone();
        let prev_self = self.current_self.clone();
        let prev_super = self.current_super.clone();
        let prev_throws = self.current_throws;
        let prev_async = self.current_async;
        let prev_loop = self.loop_depth;
        self.current_return = func.return_type.clone();
        self.current_self = self_type.to_string();
        self.current_super.clear();
        self.current_throws = func.throws_ex;
        self.current_async = func.is_async;
        self.loop_depth = 0;
        if func.is_constexpr && func.throws_ex {
            self.fail(func.span.line, 1, "constexpr function cannot throw");
        }
        if func.is_constexpr && func.is_async {
            self.fail(func.span.line, 1, "constexpr function cannot be async");
        }
        let prev_params = self.generic_params.clone();
        let prev_bounds = self.generic_bounds.clone();
        self.push_params(&func.type_params, func.span.line);
        if !self_type.is_empty() {
            if let Some(st) = self.env.find_struct(self_type) {
                let params = st.borrow().type_params.clone();
                self.push_params(&params, func.span.line);
            }
            let impls = self.env.trait_impls.borrow().clone();
            for im in impls {
                if type_head(&im.type_name) == type_head(self_type) {
                    self.push_params(&im.type_params, func.span.line);
                }
            }
            if let Some(pit) = self
                .env
                .class_parents
                .borrow()
                .get(&type_head(self_type))
                .cloned()
            {
                self.current_super = pit;
            }
        }
        self.check_type_name(&func.return_type, func.span.line, 1);
        self.scopes.push(HashMap::new());
        for i in 0..func.params.len() {
            let mut ty = func.param_types.get(i).cloned().unwrap_or_default();
            if func.params[i] == "self" && !self_type.is_empty() {
                ty = self_type.to_string();
            }
            if !ty.is_empty() {
                self.check_type_name(&ty, func.span.line, 1);
                if !is_fn_type(&ty) {
                    if let Some(st) = self.env.find_struct(&ty) {
                        let params = st.borrow().type_params.clone();
                        let who = type_head(&ty);
                        self.check_type_arg_count(
                            &params,
                            &types::type_arg_list(&ty),
                            func.span.line,
                            1,
                            &who,
                        );
                    } else if let Some(tr) = self.env.find_trait(&ty) {
                        let who = type_head(&ty);
                        self.check_type_arg_count(
                            &tr.type_params,
                            &types::type_arg_list(&ty),
                            func.span.line,
                            1,
                            &who,
                        );
                    }
                }
            }
            self.bind(&func.params[i], &ty);
        }
        for i in 0..func.params.len() {
            let Some(d) = func.param_defaults.get(i).and_then(|d| d.clone()) else {
                continue;
            };
            self.walk_expr(&d);
            let expect = func.param_types.get(i).cloned().unwrap_or_default();
            let got = self.infer(&d);
            if known(&expect) && known(&got) && !self.assignable(&expect, &got, Some(&d)) {
                self.fail(
                    d.span.line,
                    d.span.col,
                    &format!(
                        "cannot use {got} as the default for '{}', expected {expect}",
                        func.params[i]
                    ),
                );
            }
        }
        let mark = self.scopes.len();
        let mut body = func.body.clone();
        self.walk_block(&mut body);
        self.drain_scopes(mark);
        self.scopes.pop();
        self.generic_params = prev_params;
        self.generic_bounds = prev_bounds;
        self.current_return = prev_ret;
        self.current_self = prev_self;
        self.current_super = prev_super;
        self.current_throws = prev_throws;
        self.current_async = prev_async;
        self.loop_depth = prev_loop;
    }

    fn push_params(&mut self, params: &[TypeParam], line: u32) {
        for p in params {
            self.generic_params.insert(p.name.clone());
            if !p.bounds.is_empty() {
                self.generic_bounds.insert(p.name.clone(), p.bounds.clone());
            }
            for b in &p.bounds.clone() {
                self.check_trait_bound(b, line, 1);
            }
        }
    }

    fn run_all(&mut self) {
        for st in &self.env.program.structs.clone() {
            let prev = self.generic_params.clone();
            for p in &st.type_params {
                self.generic_params.insert(p.name.clone());
            }
            for ty in &st.field_types {
                self.check_type_name(ty, st.span.line, 1);
            }
            self.generic_params = prev;
        }
        for c in &self.env.program.classes.clone() {
            let prev = self.generic_params.clone();
            for p in &c.type_params {
                self.generic_params.insert(p.name.clone());
            }
            for f in &c.fields {
                self.check_type_name(&f.ty, c.span.line, 1);
                let head = type_head(&f.ty);
                if f.has_range && head != "Int" && head != "Float" {
                    self.fail(f.span.line, f.span.col, "range needs Int or Float");
                }
                if f.has_enum && head != "Int" {
                    self.fail(f.span.line, f.span.col, "enum needs Int");
                }
                if f.has_flags && head != "Int" {
                    self.fail(f.span.line, f.span.col, "flags needs Int");
                }
                if f.has_file && head != "String" {
                    self.fail(f.span.line, f.span.col, "file needs String");
                }
                if f.has_dir && head != "String" {
                    self.fail(f.span.line, f.span.col, "dir needs String");
                }
                if f.has_multiline && head != "String" {
                    self.fail(f.span.line, f.span.col, "multiline needs String");
                }
            }
            self.generic_params = prev;
        }
        let crate_name = stdlib::crate_name_of_file(&self.env.file);
        let prev_entry = self.env.current_module();
        if !crate_name.is_empty() {
            *self.env.current_module.borrow_mut() = crate_name;
        }
        let fns = self.env.program.fns.clone();
        for func in &fns {
            self.check_fn(func, "");
        }
        *self.env.current_module.borrow_mut() = prev_entry;
        let methods: Vec<(String, FnDecl, String)> = self
            .env
            .type_methods
            .borrow()
            .iter()
            .flat_map(|(ty, ms)| {
                ms.values()
                    .map(|f| (ty.clone(), (**f).clone(), f.module.clone()))
            })
            .collect();
        for (ty, func, module) in methods {
            let prev = self.env.current_module();
            *self.env.current_module.borrow_mut() = module;
            self.check_fn(&func, &ty);
            *self.env.current_module.borrow_mut() = prev;
        }
        let statics: Vec<(FnDecl, String)> = self
            .env
            .static_methods
            .borrow()
            .values()
            .flat_map(|ms| ms.values().map(|f| ((**f).clone(), f.module.clone())))
            .collect();
        for (func, module) in statics {
            let prev = self.env.current_module();
            *self.env.current_module.borrow_mut() = module;
            self.check_fn(&func, "");
            *self.env.current_module.borrow_mut() = prev;
        }
        let prev = self.env.current_module();
        let loaded: Vec<(String, Vec<FnDecl>)> = self
            .env
            .loaded
            .borrow()
            .iter()
            .map(|(k, m)| {
                (
                    k.clone(),
                    m.borrow().fns.values().map(|f| (**f).clone()).collect(),
                )
            })
            .collect();
        for (key, fns) in loaded {
            *self.env.current_module.borrow_mut() = key;
            for func in fns {
                self.check_fn(&func, "");
            }
        }
        *self.env.current_module.borrow_mut() = prev;
    }
}

fn known(ty: &str) -> bool {
    !ty.is_empty() && ty != "None" && ty != "Self" && ty != "Void"
}

fn builtin_head(head: &str) -> bool {
    matches!(
        head,
        "Int"
            | "Float"
            | "String"
            | "Str"
            | "Bool"
            | "Void"
            | "Array"
            | "Map"
            | "Range"
            | "None"
            | "Self"
            | "Future"
    )
}

fn is_array_ty(ty: &str) -> bool {
    ty == "Array" || (ty.len() > 6 && ty.starts_with("Array[") && ty.ends_with(']'))
}
fn array_elem(ty: &str) -> String {
    if ty.len() > 6 && ty.starts_with("Array[") && ty.ends_with(']') {
        ty[6..ty.len() - 1].to_string()
    } else {
        String::new()
    }
}
fn is_future_ty(ty: &str) -> bool {
    ty == "Future" || (ty.len() > 7 && ty.starts_with("Future[") && ty.ends_with(']'))
}
fn future_elem(ty: &str) -> String {
    if ty.len() > 7 && ty.starts_with("Future[") && ty.ends_with(']') {
        ty[7..ty.len() - 1].to_string()
    } else {
        String::new()
    }
}
fn is_map_ty(ty: &str) -> bool {
    ty == "Map" || (ty.len() > 4 && ty.starts_with("Map[") && ty.ends_with(']'))
}
fn is_numeric(ty: &str) -> bool {
    ty == "Int" || ty == "Float"
}
fn is_string(ty: &str) -> bool {
    ty == "String" || ty == "Str"
}
fn vector_type(t: &str) -> bool {
    matches!(
        t,
        "Vec2" | "Vector2i" | "Vector3" | "Vector3i" | "Vector4" | "Vector4i"
    )
}
fn float_math(t: &str) -> bool {
    matches!(
        t,
        "Vec2" | "Vector3" | "Vector4" | "Color" | "Quaternion" | "Transform2D"
    )
}
fn int_math(t: &str) -> bool {
    matches!(t, "Vector2i" | "Vector3i" | "Vector4i")
}
fn value_math(t: &str) -> bool {
    float_math(t) || int_math(t)
}
fn joins_number(left: &str, right: &str) -> bool {
    (is_string(left) && is_numeric(right)) || (is_numeric(left) && is_string(right))
}
fn value_op(op: &str, left: &str, right: &str) -> String {
    let l = type_head(left);
    let r = type_head(right);
    if op == "+" || op == "-" {
        if op == "+" && joins_number(&l, &r) {
            return "String".into();
        }
        if l == r && value_math(&l) && l != "Transform2D" {
            return l;
        }
        return String::new();
    }
    if op == "*" {
        if (l == "Transform2D" && r == "Vec2") || (l == "Vec2" && r == "Transform2D") {
            return "Vec2".into();
        }
        if l == r && value_math(&l) {
            return l;
        }
        if float_math(&l) && (r == "Float" || r == "Int") {
            return l;
        }
        if int_math(&l) && r == "Int" {
            return l;
        }
        if (l == "Float" || l == "Int") && float_math(&r) {
            return r;
        }
        if l == "Int" && int_math(&r) {
            return r;
        }
        return String::new();
    }
    if op == "/" {
        if l == r && vector_type(&l) {
            return l;
        }
        if float_math(&l) && (r == "Float" || r == "Int") {
            return l;
        }
        if int_math(&l) && r == "Int" {
            return l;
        }
        return String::new();
    }
    if matches!(op, "<" | ">" | "<=" | ">=") && l == r && vector_type(&l) {
        return "Bool".into();
    }
    String::new()
}
fn numeric_result(l: &str, r: &str) -> String {
    if l == "Float" || r == "Float" {
        "Float".into()
    } else {
        "Int".into()
    }
}
fn is_fn_type(ty: &str) -> bool {
    ty.starts_with("fn(")
}
fn is_callable(ty: &str) -> bool {
    ty == "Fn" || is_fn_type(ty)
}

fn parse_fn_type(ty: &str) -> Option<(Vec<String>, String)> {
    if !is_fn_type(ty) {
        return None;
    }
    let chars: Vec<char> = ty.chars().collect();
    let mut i = 3usize;
    let mut depth = 1i32;
    let mut cur = String::new();
    let mut params = Vec::new();
    let flush = |cur: &mut String, params: &mut Vec<String>| {
        let s = cur.as_str();
        let a = s.find(|c: char| c != ' ');
        let b = s.rfind(|c: char| c != ' ');
        if let (Some(a), Some(b)) = (a, b) {
            params.push(s[a..=b].to_string());
        }
        cur.clear();
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '(' || c == '[' {
            depth += 1;
            cur.push(c);
        } else if c == ')' || c == ']' {
            depth -= 1;
            if depth == 0 && c == ')' {
                flush(&mut cur, &mut params);
                i += 1;
                if i < chars.len() && chars[i] == ':' {
                    i += 1;
                    let r: String = chars[i..].iter().collect();
                    let a = r.find(|c: char| c != ' ');
                    let b = r.rfind(|c: char| c != ' ');
                    let ret = match (a, b) {
                        (Some(a), Some(b)) => r[a..=b].to_string(),
                        _ => "Void".into(),
                    };
                    return Some((params, ret));
                }
                return Some((params, "Void".into()));
            }
            cur.push(c);
        } else if c == ',' && depth == 1 {
            flush(&mut cur, &mut params);
        } else {
            cur.push(c);
        }
        i += 1;
    }
    None
}

fn encode_fn_type(params: &[String], ret: &str) -> String {
    let mut t = String::from("fn(");
    for (i, p) in params.iter().enumerate() {
        if i > 0 {
            t.push_str(", ");
        }
        t.push_str(p);
    }
    t.push_str("):");
    t.push_str(if ret.is_empty() { "Void" } else { ret });
    t
}

fn wrap_async(func: &FnDecl, inner: &str) -> String {
    let t = if inner.is_empty() { "Void" } else { inner };
    if func.is_async {
        format!("Future[{t}]")
    } else {
        t.to_string()
    }
}

fn encode_fn_decl(func: &FnDecl) -> String {
    if func.param_types.iter().any(|p| p.is_empty()) {
        return "Fn".into();
    }
    encode_fn_type(&func.param_types, &wrap_async(func, &func.return_type))
}

fn rest(kids: &[Expr]) -> Vec<Expr> {
    if kids.len() <= 1 {
        Vec::new()
    } else {
        kids[1..].to_vec()
    }
}

fn engine_vararg(func: &FnDecl) -> bool {
    func.is_vararg && engine_file_pub(&func.file)
}

fn host_return(module: &str, name: &str) -> String {
    if module == "process" && name == "argv" {
        return "String".into();
    }
    if module == "process" && name == "argc" {
        return "Int".into();
    }
    match module {
        "__math" => {
            if matches!(
                name,
                "sin"
                    | "cos"
                    | "atan2"
                    | "sqrt"
                    | "powf"
                    | "to_float"
                    | "floor"
                    | "ceil"
                    | "random"
            ) {
                "Float".into()
            } else {
                "Int".into()
            }
        }
        "__str" => match name {
            "length" | "find" => "Int".into(),
            "contains" | "starts_with" | "ends_with" | "is_empty" => "Bool".into(),
            "split" => "Array[String]".into(),
            _ => "String".into(),
        },
        "__io" => match name {
            "exists" | "remove" => "Bool".into(),
            "read_text" => "String".into(),
            "read_lines" => "Array[String]".into(),
            _ => "Void".into(),
        },
        "__uuid" => {
            if name == "valid" {
                "Bool".into()
            } else {
                "String".into()
            }
        }
        "__time" => {
            if name == "now" {
                "Int".into()
            } else if name == "delay" {
                "Future[Void]".into()
            } else {
                "Void".into()
            }
        }
        "__path" => "String".into(),
        "__json" => {
            if name == "valid" {
                "Bool".into()
            } else if name == "stringify" {
                "String".into()
            } else {
                String::new()
            }
        }
        "__regex" => match name {
            "valid" | "is_match" => "Bool".into(),
            "find" => "Int".into(),
            "find_match" | "replace" => "String".into(),
            "captures" | "findall" | "split" => "Array[String]".into(),
            _ => "Void".into(),
        },
        "__ui" => match name {
            "open" => "Int".into(),
            "title" | "backend" | "platform" | "key_text" | "clipboard_get" => "String".into(),
            "alive" | "poll" | "mouse_down" | "take_click" | "take_right_click" | "take_key"
            | "take_scroll" => "Bool".into(),
            "next_frame" => "Future[Bool]".into(),
            "width" | "height" | "count" | "mouse_x" | "mouse_y" | "key_code" | "scroll_dx"
            | "scroll_dy" | "text_width" | "font_height" | "image_width" | "image_height" => {
                "Int".into()
            }
            _ => "Void".into(),
        },
        "world" => match name {
            "hardness" | "time_of_day" => "Float".into(),
            "light" | "count" | "get_meta" | "fill_level" => "Int".into(),
            "write" | "set_block" | "set_meta" | "set_fill" | "is_air" | "is_compound"
            | "is_day" | "has_time" => "Bool".into(),
            "block" | "get_block" | "get_fill" | "replaces" | "slot" | "season" => "String".into(),
            _ => "Void".into(),
        },
        "player" => match name {
            "held" | "stack" => "String".into(),
            "selected" | "count" => "Int".into(),
            "present" => "Bool".into(),
            "x" | "y" | "z" => "Float".into(),
            _ => "Void".into(),
        },
        "recipe" => match name {
            "register" | "shapeless" | "shaped" | "smelt" | "smoke" | "blast" => "Bool".into(),
            _ => "Void".into(),
        },
        "loot" => match name {
            "block" | "chest" | "entity" => "Bool".into(),
            _ => "Void".into(),
        },
        "item" => match name {
            "hardness" => "Float".into(),
            "light" | "max_count" => "Int".into(),
            "exists" | "places" | "stack" => "Bool".into(),
            "register" | "replaces" => "String".into(),
            _ => "Void".into(),
        },
        "block" => match name {
            "register" => "String".into(),
            "texture" | "model" | "hardness" | "light" => "Bool".into(),
            _ => "Void".into(),
        },
        "entity" => match name {
            "register" => "String".into(),
            "model" | "hostile" | "spawn_egg" | "hearts" | "hurtbox" | "speed" | "spawn"
            | "despawn" => "Bool".into(),
            _ => "Void".into(),
        },
        "block_entity" => match name {
            "get" => "String".into(),
            "get_int" => "Int".into(),
            "bind" | "set" | "set_int" | "clear" | "has" => "Bool".into(),
            _ => "Void".into(),
        },
        "modapi" => match name {
            "content" => "String".into(),
            _ => "Void".into(),
        },
        "gui" => match name {
            "screen" | "element" | "remove" | "open" => "Bool".into(),
            "read" => "String".into(),
            _ => "Void".into(),
        },
        "chat" => match name {
            "say" => "Bool".into(),
            _ => "Void".into(),
        },
        "settings" => match name {
            "option" => "Bool".into(),
            "value" => "String".into(),
            _ => "Void".into(),
        },
        _ => "Void".into(),
    }
}

fn unify_type_with(
    generic_params: &HashSet<String>,
    pattern: &str,
    got: &str,
    tenv: &mut HashMap<String, String>,
    compatible: &dyn Fn(&str, &str) -> bool,
) -> bool {
    if pattern.is_empty() || !known(got) {
        return true;
    }
    let ph = type_head(pattern);
    let pa = types::type_arg_list(pattern);
    if pa.is_empty() && generic_params.contains(&ph) {
        match tenv.get(&ph).cloned() {
            None => {
                tenv.insert(ph, got.to_string());
                return true;
            }
            Some(it) if it == ph => {
                tenv.insert(ph, got.to_string());
                return true;
            }
            Some(it) => return compatible(&it, got),
        }
    }
    if is_array_ty(pattern) && is_array_ty(got) {
        let pe = array_elem(pattern);
        let ge = array_elem(got);
        if pe.is_empty() || ge.is_empty() {
            return true;
        }
        return unify_type_with(generic_params, &pe, &ge, tenv, compatible);
    }
    if is_map_ty(pattern) && is_map_ty(got) {
        return true;
    }
    let gh = type_head(got);
    let ga = types::type_arg_list(got);
    if ph != gh {
        return compatible(pattern, got);
    }
    if pa.is_empty() || ga.is_empty() {
        return true;
    }
    if pa.len() != ga.len() {
        return false;
    }
    pa.iter()
        .zip(ga.iter())
        .all(|(p, g)| unify_type_with(generic_params, p, g, tenv, compatible))
}
