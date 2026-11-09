use crate::project::Project;
use crate::stdlib::{self, generic};
use crate::types::{self, is_host_module, type_head};
use indexmap::IndexMap;
use rosegold_syntax::*;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub type FnRc = Rc<FnDecl>;
pub type StructRc = Rc<RefCell<StructDecl>>;
pub type EnumRc = Rc<EnumDecl>;
pub type TraitRc = Rc<TraitDecl>;
pub type ModRc = Rc<RefCell<LoadedMod>>;

#[derive(Clone, Debug)]
pub struct TraitImplInfo {
    pub type_params: Vec<TypeParam>,
    pub type_name: String,
    pub trait_name: String,
    pub line: u32,
}

#[derive(Default)]
pub struct LoadedMod {
    pub fns: HashMap<String, FnRc>,
    pub exports: HashMap<String, FnRc>,
    pub ufcs_fns: HashMap<String, Vec<FnRc>>,
    pub structs: HashMap<String, StructRc>,
    pub export_structs: HashMap<String, StructRc>,
    pub enums: HashMap<String, EnumRc>,
    pub export_enums: HashMap<String, EnumRc>,
    pub modules: HashMap<String, String>,
    pub export_mods: HashMap<String, String>,
    pub from_fns: HashMap<String, FnRc>,
}

pub struct FieldHit {
    pub defined_on: String,
    pub vis: Vis,
}

pub struct Checker {
    pub program: Program,
    pub file: String,
    diags: RefCell<Vec<Diagnostic>>,
    pub fns: RefCell<HashMap<String, FnRc>>,
    pub structs: RefCell<HashMap<String, StructRc>>,
    pub all_types: RefCell<IndexMap<String, StructRc>>,
    pub traits: RefCell<HashMap<String, TraitRc>>,
    pub enums: RefCell<IndexMap<String, EnumRc>>,
    pub class_parents: RefCell<IndexMap<String, String>>,
    class_files: RefCell<HashMap<String, String>>,
    pub class_abstract: RefCell<HashMap<String, bool>>,
    class_final: RefCell<HashMap<String, bool>>,
    field_access: RefCell<HashMap<String, HashMap<String, Vis>>>,
    pub field_defaults: RefCell<HashMap<String, HashMap<String, Expr>>>,
    type_traits: RefCell<HashMap<String, Vec<String>>>,
    pub trait_impls: RefCell<Vec<TraitImplInfo>>,
    pub type_methods: RefCell<IndexMap<String, IndexMap<String, FnRc>>>,
    pub static_methods: RefCell<IndexMap<String, IndexMap<String, FnRc>>>,
    pub type_signals: RefCell<IndexMap<String, IndexMap<String, i32>>>,
    pub signal_arity: RefCell<HashMap<String, i32>>,
    consts: RefCell<HashMap<String, i64>>,
    type_consts: RefCell<HashMap<String, HashMap<String, i64>>>,
    pub singletons: RefCell<HashSet<String>>,
    pub loaded: RefCell<HashMap<String, ModRc>>,
    pub module_binds: RefCell<HashMap<String, String>>,
    loading: RefCell<Vec<String>>,
    pub current_module: RefCell<String>,
    project: RefCell<Option<Project>>,
    project_loaded: Cell<bool>,
}

pub fn check(program: Program, source: &str, file: &str, diags: &mut Vec<Diagnostic>, godot: bool) {
    let taken = std::mem::take(diags);
    let checker = Checker::new(program, file, taken);
    checker.run(godot);
    let mut owned = checker.diags.borrow().clone();
    crate::unused::check(source, file, &checker.program, &mut owned);
    *diags = owned;
}

/// Same pass as [`check`], and the checker is returned so `llvm` can lower it.
pub fn check_owned(
    program: Program,
    source: &str,
    file: &str,
    godot: bool,
) -> (Checker, Vec<Diagnostic>) {
    let checker = Checker::new(program, file, Vec::new());
    checker.run(godot);
    let mut owned = checker.diags.borrow().clone();
    crate::unused::check(source, file, &checker.program, &mut owned);
    (checker, owned)
}

/// Checker pass used by the tree-walk interpreter.
/// Matches Java `Checker.check` (no unused-import pass, not a Godot project load).
pub fn check_interpreted(program: Program, file: &str) -> (Checker, Vec<Diagnostic>) {
    let checker = Checker::new(program, file, Vec::new());
    checker.run(false);
    let diags = checker.diags.borrow().clone();
    (checker, diags)
}

impl Checker {
    fn new(program: Program, file: &str, diags: Vec<Diagnostic>) -> Self {
        Self {
            program,
            file: file.to_string(),
            diags: RefCell::new(diags),
            fns: RefCell::new(HashMap::new()),
            structs: RefCell::new(HashMap::new()),
            all_types: RefCell::new(IndexMap::new()),
            traits: RefCell::new(HashMap::new()),
            enums: RefCell::new(IndexMap::new()),
            class_parents: RefCell::new(IndexMap::new()),
            class_files: RefCell::new(HashMap::new()),
            class_abstract: RefCell::new(HashMap::new()),
            class_final: RefCell::new(HashMap::new()),
            field_access: RefCell::new(HashMap::new()),
            field_defaults: RefCell::new(HashMap::new()),
            type_traits: RefCell::new(HashMap::new()),
            trait_impls: RefCell::new(Vec::new()),
            type_methods: RefCell::new(IndexMap::new()),
            static_methods: RefCell::new(IndexMap::new()),
            type_signals: RefCell::new(IndexMap::new()),
            signal_arity: RefCell::new(HashMap::new()),
            consts: RefCell::new(HashMap::new()),
            type_consts: RefCell::new(HashMap::new()),
            singletons: RefCell::new(HashSet::new()),
            loaded: RefCell::new(HashMap::new()),
            module_binds: RefCell::new(HashMap::new()),
            loading: RefCell::new(Vec::new()),
            current_module: RefCell::new(String::new()),
            project: RefCell::new(None),
            project_loaded: Cell::new(false),
        }
    }

    fn run(&self, godot: bool) {
        self.ingest_entry();
        if godot {
            self.load_module("godot", &self.file.clone(), 1, 1, true);
            self.load_project_scripts();
        }
        let imports = self.program.imports.clone();
        self.load_imports(&imports, &self.file.clone(), None);
        self.apply_inheritance();
        self.check_trait_impls();
        self.check_abstract_final();
        self.bind_trait_signals();
        self.check_constexpr_fns();
        crate::typecheck::run(self);
    }

    pub fn record_diag(&self, kind: &str, at_file: &str, line: u32, col: u32, msg: &str) {
        let file = if at_file.is_empty() {
            self.file.clone()
        } else {
            at_file.to_string()
        };
        let d = Diagnostic::new(&file, line, col, "error", msg, kind);
        let mut diags = self.diags.borrow_mut();
        if diags.iter().any(|prev| {
            prev.file == d.file
                && prev.line == d.line
                && prev.col == d.col
                && prev.message == d.message
        }) {
            return;
        }
        diags.push(d);
    }

    pub fn load_fail(&self, at_file: &str, msg: &str, line: u32, col: u32) {
        self.record_diag("runtime error", at_file, line, col, msg);
    }

    pub fn current_module(&self) -> String {
        self.current_module.borrow().clone()
    }

    pub fn find_module_bind(&self, name: &str) -> Option<String> {
        let cur = self.current_module();
        if !cur.is_empty() {
            let lit = self.loaded.borrow().get(&cur).cloned()?;
            return lit.borrow().modules.get(name).cloned();
        }
        self.module_binds.borrow().get(name).cloned()
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
            if !self.class_abstract.borrow().contains_key(&cur) {
                return false;
            }
            let parent = self.class_parents.borrow().get(&cur).cloned();
            let Some(parent) = parent else { return false };
            if parent.is_empty() {
                return false;
            }
            cur = type_head(&parent);
        }
        false
    }

    pub fn is_engine_class(&self, type_name: &str) -> bool {
        let head = type_head(type_name);
        if !self.class_abstract.borrow().contains_key(&head) || !self.extends_object(&head) {
            return false;
        }
        if head == "Object" {
            return true;
        }
        let methods = self.type_methods.borrow().get(&head).cloned();
        let Some(methods) = methods else { return false };
        methods.values().any(|m| engine_file(&m.file))
    }

    pub fn native_base(&self, type_name: &str) -> String {
        let mut cur = type_head(type_name);
        for _ in 0..32 {
            if cur.is_empty() {
                return String::new();
            }
            if self.is_engine_class(&cur) {
                return cur;
            }
            let parent = self
                .class_parents
                .borrow()
                .get(&cur)
                .cloned()
                .unwrap_or_default();
            if parent.is_empty() {
                return String::new();
            }
            cur = type_head(&parent);
        }
        String::new()
    }

    pub fn find_struct(&self, name: &str) -> Option<StructRc> {
        let head = type_head(name);
        let cur = self.current_module();
        if !cur.is_empty() {
            if let Some(lit) = self.loaded.borrow().get(&cur).cloned() {
                let lit = lit.borrow();
                if let Some(st) = lit.structs.get(&head) {
                    return Some(st.clone());
                }
                let binds = lit.modules.clone();
                drop(lit);
                if let Some(st) = self.imported_struct(&binds, &head, false) {
                    return Some(st);
                }
            }
        }
        if let Some(st) = self.structs.borrow().get(&head).cloned() {
            return Some(st);
        }
        let binds = self.module_binds.borrow().clone();
        self.imported_struct(&binds, &head, true)
    }

    pub fn find_trait(&self, name: &str) -> Option<TraitRc> {
        self.traits.borrow().get(&type_head(name)).cloned()
    }

    pub fn is_data_type(&self, name: &str) -> bool {
        let head = type_head(name);
        if let Some(st) = self.all_types.borrow().get(&head).cloned() {
            return st.borrow().is_data;
        }
        self.find_struct(&head)
            .is_some_and(|st| st.borrow().is_data)
    }

    pub fn find_enum(&self, name: &str) -> Option<EnumRc> {
        let cur = self.current_module();
        if !cur.is_empty() {
            if let Some(lit) = self.loaded.borrow().get(&cur).cloned() {
                let litb = lit.borrow();
                if let Some(en) = litb.enums.get(name) {
                    return Some(en.clone());
                }
                let binds = litb.modules.clone();
                drop(litb);
                if let Some(en) = self.imported_enum(&binds, name, false) {
                    return Some(en);
                }
            }
        }
        if let Some(en) = self.enums.borrow().get(name).cloned() {
            return Some(en);
        }
        let binds = self.module_binds.borrow().clone();
        self.imported_enum(&binds, name, true)
    }

    fn imported_enum(
        &self,
        binds: &HashMap<String, String>,
        name: &str,
        crates_only: bool,
    ) -> Option<EnumRc> {
        for target in self.imported_targets(binds, crates_only) {
            let Some(m) = self.loaded.borrow().get(&target).cloned() else {
                continue;
            };
            let m = m.borrow();
            if let Some(en) = m.export_enums.get(name).or_else(|| m.enums.get(name)) {
                return Some(en.clone());
            }
        }
        None
    }

    fn imported_struct(
        &self,
        binds: &HashMap<String, String>,
        name: &str,
        crates_only: bool,
    ) -> Option<StructRc> {
        for target in self.imported_targets(binds, crates_only) {
            let Some(m) = self.loaded.borrow().get(&target).cloned() else {
                continue;
            };
            let m = m.borrow();
            if let Some(st) = m.export_structs.get(name).or_else(|| m.structs.get(name)) {
                return Some(st.clone());
            }
        }
        None
    }

    fn imported_targets(&self, binds: &HashMap<String, String>, crates_only: bool) -> Vec<String> {
        let mut queue = Vec::new();
        let mut seen = HashSet::new();
        for target in binds.values() {
            if target.is_empty() || !seen.insert(target.clone()) {
                continue;
            }
            if crates_only && !stdlib::is_crate_stdlib(target) {
                continue;
            }
            queue.push(target.clone());
        }
        let mut i = 0;
        while i < queue.len() {
            let name = queue[i].clone();
            i += 1;
            let Some(m) = self.loaded.borrow().get(&name).cloned() else {
                continue;
            };
            let m = m.borrow();
            for target in m.export_mods.values() {
                if !target.is_empty()
                    && seen.insert(target.clone())
                    && (!crates_only || stdlib::is_crate_stdlib(target))
                {
                    queue.push(target.clone());
                }
            }
            if !crates_only {
                for target in m.modules.values() {
                    if !target.is_empty() && seen.insert(target.clone()) {
                        queue.push(target.clone());
                    }
                }
            }
        }
        queue
    }

    pub fn find_local_fn(&self, name: &str) -> Option<FnRc> {
        let cur = self.current_module();
        if !cur.is_empty() {
            let lit = self.loaded.borrow().get(&cur).cloned()?;
            let lit = lit.borrow();
            if let Some(fn_) = lit.fns.get(name) {
                return Some(fn_.clone());
            }
            return lit.from_fns.get(name).cloned();
        }
        self.fns.borrow().get(name).cloned()
    }

    pub fn find_ufcs(&self, name: &str, recv_ty: &str) -> Option<FnRc> {
        let mut candidates: Vec<FnRc> = Vec::new();
        let mut add_cand = |fn_: &FnRc, candidates: &mut Vec<FnRc>| {
            if !fn_.is_ufcs {
                return;
            }
            if !candidates.iter().any(|c| Rc::ptr_eq(c, fn_)) {
                candidates.push(fn_.clone());
            }
        };
        let add_from_mod = |m: &LoadedMod, candidates: &mut Vec<FnRc>| {
            if let Some(ufcs) = m.ufcs_fns.get(name) {
                for fn_ in ufcs {
                    add_cand(fn_, candidates);
                }
                return;
            }
            if let Some(eit) = m.exports.get(name) {
                add_cand(eit, candidates);
            }
            if let Some(fit) = m.fns.get(name) {
                add_cand(fit, candidates);
            }
        };
        let cur = self.current_module();
        let mut queue = Vec::new();
        if !cur.is_empty() {
            if let Some(lit) = self.loaded.borrow().get(&cur).cloned() {
                let lit = lit.borrow();
                add_from_mod(&lit, &mut candidates);
                if let Some(from) = lit.from_fns.get(name) {
                    add_cand(from, &mut candidates);
                }
                queue.extend(lit.modules.values().cloned());
            }
        } else {
            if let Some(fn_) = self.find_local_fn(name) {
                add_cand(&fn_, &mut candidates);
            }
            queue.extend(self.module_binds.borrow().values().cloned());
        }
        let mut seen = HashSet::new();
        let mut i = 0;
        while i < queue.len() {
            let mod_name = queue[i].clone();
            i += 1;
            if !seen.insert(mod_name.clone()) {
                continue;
            }
            let Some(lit) = self.loaded.borrow().get(&mod_name).cloned() else {
                continue;
            };
            let lit = lit.borrow();
            add_from_mod(&lit, &mut candidates);
            queue.extend(lit.export_mods.values().cloned());
            queue.extend(lit.modules.values().cloned());
        }
        if candidates.is_empty() {
            return None;
        }
        if recv_ty.is_empty() || candidates.len() == 1 {
            return Some(candidates[0].clone());
        }
        let want = type_head(recv_ty);
        let mut fallback = None;
        for fn_ in &candidates {
            if fn_.param_types.is_empty() || fn_.param_types[0].is_empty() {
                if fallback.is_none() {
                    fallback = Some(fn_.clone());
                }
                continue;
            }
            if type_head(&fn_.param_types[0]) == want {
                return Some(fn_.clone());
            }
        }
        fallback.or_else(|| Some(candidates[0].clone()))
    }

    pub fn type_signal_names(&self, type_name: &str) -> Vec<String> {
        self.type_signals
            .borrow()
            .get(type_name)
            .map(|slot| slot.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn lookup_type_signal(&self, type_name: &str, name: &str) -> Option<i32> {
        let mut current = type_head(type_name);
        let mut seen = HashSet::new();
        while !current.is_empty() {
            if !seen.insert(current.clone()) {
                break;
            }
            if let Some(slot) = self.type_signals.borrow().get(&current).cloned() {
                if let Some(n) = slot.get(name) {
                    return Some(*n);
                }
            }
            let pit = self.class_parents.borrow().get(&current).cloned();
            current = match pit {
                Some(p) => type_head(&p),
                None => break,
            };
        }
        None
    }

    pub fn lookup_signal(&self, type_name: &str, name: &str) -> Option<SignalDecl> {
        let mut current = type_head(type_name);
        let mut seen = HashSet::new();
        while !current.is_empty() {
            if !seen.insert(current.clone()) {
                break;
            }
            if let Some(st) = self.find_struct(&current) {
                let hit = st.borrow().signals.iter().find(|s| s.name == name).cloned();
                if hit.is_some() {
                    return hit;
                }
            }
            let pit = self.class_parents.borrow().get(&current).cloned();
            current = match pit {
                Some(p) => type_head(&p),
                None => break,
            };
        }
        None
    }

    pub fn lookup_method(&self, type_name: &str, name: &str) -> (String, Option<FnRc>) {
        self.lookup_in(&self.type_methods, type_name, name)
    }

    pub fn lookup_static(&self, type_name: &str, name: &str) -> (String, Option<FnRc>) {
        self.lookup_in(&self.static_methods, type_name, name)
    }

    fn lookup_in(
        &self,
        map: &RefCell<IndexMap<String, IndexMap<String, FnRc>>>,
        type_name: &str,
        name: &str,
    ) -> (String, Option<FnRc>) {
        let mut current = type_head(type_name);
        let mut seen = HashSet::new();
        while !current.is_empty() {
            if !seen.insert(current.clone()) {
                break;
            }
            if let Some(slot) = map.borrow().get(&current).cloned() {
                if let Some(fn_) = slot.get(name) {
                    return (current, Some(fn_.clone()));
                }
            }
            let pit = self.class_parents.borrow().get(&current).cloned();
            current = match pit {
                Some(p) => type_head(&p),
                None => break,
            };
        }
        (String::new(), None)
    }

    pub fn lookup_field(&self, type_name: &str, name: &str) -> FieldHit {
        let mut current = type_head(type_name);
        let mut seen = HashSet::new();
        while !current.is_empty() {
            if !seen.insert(current.clone()) {
                break;
            }
            if let Some(slot) = self.field_access.borrow().get(&current).cloned() {
                if let Some(vis) = slot.get(name) {
                    return FieldHit {
                        defined_on: current,
                        vis: *vis,
                    };
                }
            }
            let pit = self.class_parents.borrow().get(&current).cloned();
            current = match pit {
                Some(p) => type_head(&p),
                None => break,
            };
        }
        FieldHit {
            defined_on: String::new(),
            vis: Vis::Pub,
        }
    }

    pub fn lookup_const(&self, type_name: &str, name: &str) -> Option<i64> {
        if type_name.is_empty() {
            return self.consts.borrow().get(name).copied();
        }
        let mut current = type_head(type_name);
        let mut seen = HashSet::new();
        while !current.is_empty() {
            if !seen.insert(current.clone()) {
                break;
            }
            if let Some(slot) = self.type_consts.borrow().get(&current).cloned() {
                if let Some(n) = slot.get(name) {
                    return Some(*n);
                }
            }
            let parent = self.class_parents.borrow().get(&current).cloned();
            current = match parent {
                Some(p) => type_head(&p),
                None => String::new(),
            };
        }
        None
    }

    fn load_project_scripts(&self) {
        let Some(root) = godot_project_root(&self.file) else {
            return;
        };
        let mut scripts = Vec::new();
        collect_project_scripts(&root, &mut scripts, 0);
        for script in scripts {
            if same_open_file(&script, &self.file) {
                continue;
            }
            self.ingest_project_script(&script);
        }
    }

    fn ingest_project_script(&self, script: &Path) {
        let Ok(text) = std::fs::read_to_string(script) else {
            return;
        };
        let (parsed, parse_diags) = parse_source(&text, &script.to_string_lossy());
        if parse_diags.iter().any(|d| d.severity == "error") {
            return;
        }
        let at = script.to_string_lossy().to_string();
        for st in &parsed.structs {
            if self.all_types.borrow().contains_key(&st.name) {
                continue;
            }
            let rc = Rc::new(RefCell::new(st.clone()));
            self.structs
                .borrow_mut()
                .insert(st.name.clone(), rc.clone());
            self.all_types.borrow_mut().insert(st.name.clone(), rc);
            self.record_field_access(&st.name, &st.fields, &st.field_vis);
            for method in &st.methods {
                let mut m = method.clone();
                stamp(&mut m, &at);
                self.type_methods
                    .borrow_mut()
                    .entry(st.name.clone())
                    .or_default()
                    .insert(m.name.clone(), Rc::new(m));
            }
            self.register_type_signals(&st.name, &st.signals, &at);
        }
        self.ingest_classes(None, &parsed.classes, false, "", &at);
        self.ingest_traits(&parsed.traits, &at);
        self.ingest_enums(None, &parsed.enums, false, "", &at, true);
        self.ingest_impls(&parsed.impls, &at);
    }

    fn ingest_entry(&self) {
        let crate_name = stdlib::crate_name_of_file(&self.file);
        if !crate_name.is_empty() {
            self.ingest_crate_entry(&crate_name);
            return;
        }
        for fn_ in &self.program.fns {
            self.fns
                .borrow_mut()
                .insert(fn_.name.clone(), Rc::new(fn_.clone()));
        }
        for st in &self.program.structs {
            if self.structs.borrow().contains_key(&st.name) {
                self.load_fail(
                    &self.file.clone(),
                    &format!("duplicate struct '{}'", st.name),
                    st.span.line,
                    1,
                );
                continue;
            }
            if self.fns.borrow().contains_key(&st.name) {
                self.load_fail(
                    &self.file.clone(),
                    &format!("struct '{}' conflicts with a function", st.name),
                    st.span.line,
                    1,
                );
                continue;
            }
            let rc = Rc::new(RefCell::new(st.clone()));
            self.structs
                .borrow_mut()
                .insert(st.name.clone(), rc.clone());
            self.all_types.borrow_mut().insert(st.name.clone(), rc);
            self.record_field_access(&st.name, &st.fields, &st.field_vis);
            for m in &st.methods {
                self.type_methods
                    .borrow_mut()
                    .entry(st.name.clone())
                    .or_default()
                    .insert(m.name.clone(), Rc::new(m.clone()));
            }
            for t in &st.impl_traits {
                self.record_trait_impl(
                    &types::self_applied(&st.name, &st.type_params),
                    t,
                    &st.type_params,
                    st.span.line,
                );
            }
            self.register_type_signals(&st.name, &st.signals, &self.file.clone());
        }
        let file = self.file.clone();
        self.ingest_classes(None, &self.program.classes.clone(), false, "", &file);
        self.ingest_traits(&self.program.traits.clone(), &file);
        self.ingest_enums(None, &self.program.enums.clone(), false, "", &file, true);
        self.ingest_impls(&self.program.impls.clone(), &file);
        for sig in &self.program.signals {
            if self.signal_arity.borrow().contains_key(&sig.name) {
                self.load_fail(
                    &file,
                    &format!("duplicate signal '{}'", sig.name),
                    sig.span.line,
                    1,
                );
                continue;
            }
            if self.fns.borrow().contains_key(&sig.name) {
                self.load_fail(
                    &file,
                    &format!("signal '{}' conflicts with a function", sig.name),
                    sig.span.line,
                    1,
                );
                continue;
            }
            if self.structs.borrow().contains_key(&sig.name)
                || self.all_types.borrow().contains_key(&sig.name)
            {
                self.load_fail(
                    &file,
                    &format!("signal '{}' conflicts with a struct", sig.name),
                    sig.span.line,
                    1,
                );
                continue;
            }
            self.signal_arity
                .borrow_mut()
                .insert(sig.name.clone(), sig.params.len() as i32);
        }
        self.ingest_consts(&self.program.consts.clone(), &file);
        for m in &self.program.mods {
            if self.fns.borrow().contains_key(&m.name)
                || self.structs.borrow().contains_key(&m.name)
                || self.all_types.borrow().contains_key(&m.name)
                || self.enums.borrow().contains_key(&m.name)
                || self.signal_arity.borrow().contains_key(&m.name)
            {
                self.load_fail(
                    &file,
                    &format!("module '{}' conflicts with an existing name", m.name),
                    m.span.line,
                    1,
                );
                continue;
            }
            self.register_mod_shells(m, &m.name);
        }
        for m in &self.program.mods {
            self.ingest_loaded_mod(m, &m.name, &file);
            self.module_binds
                .borrow_mut()
                .insert(m.name.clone(), m.name.clone());
        }
    }

    fn register_mod_shells(&self, block: &ModDecl, full_name: &str) {
        self.loaded
            .borrow_mut()
            .entry(full_name.to_string())
            .or_insert_with(|| Rc::new(RefCell::new(LoadedMod::default())));
        for child in &block.mods {
            self.register_mod_shells(child, &format!("{full_name}.{}", child.name));
        }
    }

    fn ingest_consts(&self, items: &[ConstDecl], at_file: &str) {
        for cd in items {
            let conflict = self.fns.borrow().contains_key(&cd.name)
                || self.structs.borrow().contains_key(&cd.name)
                || self.all_types.borrow().contains_key(&cd.name)
                || self.enums.borrow().contains_key(&cd.name)
                || self.signal_arity.borrow().contains_key(&cd.name)
                || self.singletons.borrow().contains(&cd.name);
            if conflict {
                self.load_fail(
                    at_file,
                    &format!("const '{}' conflicts with an existing name", cd.name),
                    cd.span.line,
                    cd.span.col,
                );
                continue;
            }
            let n = cd.value.as_ref().and_then(fold_int);
            if n.is_none() || (!cd.type_name.is_empty() && cd.type_name != "Int") {
                self.load_fail(
                    at_file,
                    &format!("const '{}' must be an Int", cd.name),
                    cd.span.line,
                    cd.span.col,
                );
                continue;
            }
            if self.consts.borrow().contains_key(&cd.name) {
                self.load_fail(
                    at_file,
                    &format!("duplicate const '{}'", cd.name),
                    cd.span.line,
                    cd.span.col,
                );
                continue;
            }
            self.consts.borrow_mut().insert(cd.name.clone(), n.unwrap());
        }
    }

    fn ingest_traits(&self, items: &[TraitDecl], at_file: &str) {
        for t in items {
            if self.traits.borrow().contains_key(&t.name) {
                self.load_fail(
                    at_file,
                    &format!("duplicate trait '{}'", t.name),
                    t.span.line,
                    1,
                );
                continue;
            }
            self.traits
                .borrow_mut()
                .insert(t.name.clone(), Rc::new(t.clone()));
            self.register_type_signals(&t.name, &t.signals, at_file);
        }
    }

    fn ingest_enums(
        &self,
        m: Option<&ModRc>,
        items: &[EnumDecl],
        from_mod: bool,
        mod_name: &str,
        at_file: &str,
        publish_global: bool,
    ) {
        for e in items {
            let rc = Rc::new(e.clone());
            if let Some(m) = m {
                if m.borrow().enums.contains_key(&e.name) {
                    self.load_fail(
                        at_file,
                        &format!("duplicate export '{}' in module '{mod_name}'", e.name),
                        e.span.line,
                        1,
                    );
                }
                m.borrow_mut().enums.insert(e.name.clone(), rc.clone());
            }
            if self.fns.borrow().contains_key(&e.name) {
                self.load_fail(
                    at_file,
                    &format!("enum '{}' conflicts with a function", e.name),
                    e.span.line,
                    1,
                );
            }
            if self.structs.borrow().contains_key(&e.name)
                || self.all_types.borrow().contains_key(&e.name)
            {
                self.load_fail(
                    at_file,
                    &format!("enum '{}' conflicts with a type", e.name),
                    e.span.line,
                    1,
                );
            }
            if m.is_none() || !from_mod || e.is_pub {
                if let Some(m) = m {
                    m.borrow_mut()
                        .export_enums
                        .insert(e.name.clone(), rc.clone());
                }
                if publish_global {
                    if self.enums.borrow().contains_key(&e.name) {
                        self.load_fail(
                            at_file,
                            &format!("duplicate enum '{}'", e.name),
                            e.span.line,
                            1,
                        );
                    }
                    self.enums.borrow_mut().insert(e.name.clone(), rc);
                }
            }
        }
    }

    fn record_type_trait(&self, type_name: &str, trait_name: &str) {
        let key = type_head(type_name);
        let mut map = self.type_traits.borrow_mut();
        let list = map.entry(key).or_default();
        if !list.iter().any(|t| t == trait_name) {
            list.push(trait_name.to_string());
        }
    }

    fn record_trait_impl(
        &self,
        type_name: &str,
        trait_name: &str,
        params: &[TypeParam],
        line: u32,
    ) {
        self.record_type_trait(type_name, trait_name);
        self.trait_impls.borrow_mut().push(TraitImplInfo {
            type_params: params.to_vec(),
            type_name: type_name.to_string(),
            trait_name: trait_name.to_string(),
            line,
        });
    }

    fn register_type_signals(&self, type_name: &str, sigs: &[SignalDecl], at_file: &str) {
        let st = self.all_types.borrow().get(type_name).cloned();
        let methods = self.type_methods.borrow().get(type_name).cloned();
        for sig in sigs {
            if let Some(st) = &st {
                if st.borrow().fields.iter().any(|f| f == &sig.name) {
                    self.load_fail(
                        at_file,
                        &format!(
                            "signal '{}' conflicts with field '{}' on {type_name}",
                            sig.name, sig.name
                        ),
                        sig.span.line,
                        1,
                    );
                }
            }
            if methods.as_ref().is_some_and(|m| m.contains_key(&sig.name)) {
                self.load_fail(
                    at_file,
                    &format!(
                        "signal '{}' conflicts with method '{}' on {type_name}",
                        sig.name, sig.name
                    ),
                    sig.span.line,
                    1,
                );
            }
            let dup = self
                .type_signals
                .borrow()
                .get(type_name)
                .is_some_and(|s| s.contains_key(&sig.name));
            if dup {
                self.load_fail(
                    at_file,
                    &format!("duplicate signal '{}' on {type_name}", sig.name),
                    sig.span.line,
                    1,
                );
                continue;
            }
            self.type_signals
                .borrow_mut()
                .entry(type_name.to_string())
                .or_default()
                .insert(sig.name.clone(), sig.params.len() as i32);
        }
    }

    fn ingest_classes(
        &self,
        m: Option<&ModRc>,
        items: &[ClassDecl],
        from_mod: bool,
        mod_name: &str,
        at_file: &str,
    ) {
        for c in items {
            let shape = shape_of(c);
            let rc = Rc::new(RefCell::new(shape));
            if let Some(m) = m {
                if m.borrow().structs.contains_key(&c.name) {
                    self.load_fail(
                        at_file,
                        &format!("duplicate export '{}' in module '{mod_name}'", c.name),
                        c.span.line,
                        1,
                    );
                }
                m.borrow_mut().structs.insert(c.name.clone(), rc.clone());
            }
            if self.all_types.borrow().contains_key(&c.name) {
                self.load_fail(
                    at_file,
                    &format!("duplicate class '{}'", c.name),
                    c.span.line,
                    1,
                );
                continue;
            }
            if self.fns.borrow().contains_key(&c.name) {
                self.load_fail(
                    at_file,
                    &format!("class '{}' conflicts with a function", c.name),
                    c.span.line,
                    1,
                );
                continue;
            }
            self.all_types
                .borrow_mut()
                .insert(c.name.clone(), rc.clone());
            for f in &c.fields {
                if f.has_default {
                    if let Some(expr) = &f.default_value {
                        self.field_defaults
                            .borrow_mut()
                            .entry(c.name.clone())
                            .or_default()
                            .insert(f.name.clone(), expr.clone());
                    }
                }
                self.field_access
                    .borrow_mut()
                    .entry(c.name.clone())
                    .or_default()
                    .insert(f.name.clone(), f.vis);
            }
            if !c.parent.is_empty() {
                self.class_parents
                    .borrow_mut()
                    .insert(c.name.clone(), c.parent.clone());
            }
            self.class_files
                .borrow_mut()
                .insert(c.name.clone(), at_file.to_string());
            self.class_abstract
                .borrow_mut()
                .insert(c.name.clone(), c.is_abstract);
            self.class_final
                .borrow_mut()
                .insert(c.name.clone(), c.is_final);
            for method in &c.methods {
                let mut method = method.clone();
                method.module = mod_name.to_string();
                stamp(&mut method, at_file);
                let dup = {
                    let tm = self.type_methods.borrow();
                    let sm = self.static_methods.borrow();
                    tm.get(&c.name)
                        .is_some_and(|s| s.contains_key(&method.name))
                        || sm
                            .get(&c.name)
                            .is_some_and(|s| s.contains_key(&method.name))
                };
                if dup {
                    self.load_fail(
                        at_file,
                        &format!("duplicate method '{}' on {}", method.name, c.name),
                        method.span.line,
                        1,
                    );
                }
                let rc_m = Rc::new(method);
                if rc_m.is_static {
                    self.static_methods
                        .borrow_mut()
                        .entry(c.name.clone())
                        .or_default()
                        .insert(rc_m.name.clone(), rc_m);
                } else {
                    self.type_methods
                        .borrow_mut()
                        .entry(c.name.clone())
                        .or_default()
                        .insert(rc_m.name.clone(), rc_m);
                }
            }
            for block in &c.trait_impls {
                let mut params = c.type_params.clone();
                params.extend(block.type_params.clone());
                self.record_trait_impl(
                    &types::self_applied(&c.name, &c.type_params),
                    &block.trait_name,
                    &params,
                    c.span.line,
                );
                for method in &block.methods {
                    let mut method = method.clone();
                    method.module = mod_name.to_string();
                    stamp(&mut method, at_file);
                    if self
                        .type_methods
                        .borrow()
                        .get(&c.name)
                        .is_some_and(|s| s.contains_key(&method.name))
                    {
                        self.load_fail(
                            at_file,
                            &format!("duplicate method '{}' on {}", method.name, c.name),
                            method.span.line,
                            1,
                        );
                    }
                    self.type_methods
                        .borrow_mut()
                        .entry(c.name.clone())
                        .or_default()
                        .insert(method.name.clone(), Rc::new(method));
                }
            }
            for t in &c.impl_traits {
                self.record_trait_impl(
                    &types::self_applied(&c.name, &c.type_params),
                    t,
                    &c.type_params,
                    c.span.line,
                );
            }
            rc.borrow_mut().signals = c.signals.clone();
            self.register_type_signals(&c.name, &c.signals, at_file);
            for cd in &c.consts {
                let method_hit = self
                    .type_methods
                    .borrow()
                    .get(&c.name)
                    .is_some_and(|s| s.contains_key(&cd.name))
                    || self
                        .static_methods
                        .borrow()
                        .get(&c.name)
                        .is_some_and(|s| s.contains_key(&cd.name));
                if method_hit {
                    self.load_fail(
                        at_file,
                        &format!(
                            "const '{}' conflicts with method '{}' on {}",
                            cd.name, cd.name, c.name
                        ),
                        cd.span.line,
                        cd.span.col,
                    );
                    continue;
                }
                if c.fields.iter().any(|f| f.name == cd.name) {
                    self.load_fail(
                        at_file,
                        &format!(
                            "const '{}' conflicts with field '{}' on {}",
                            cd.name, cd.name, c.name
                        ),
                        cd.span.line,
                        cd.span.col,
                    );
                    continue;
                }
                let n = cd.value.as_ref().and_then(fold_int);
                if n.is_none() || (!cd.type_name.is_empty() && cd.type_name != "Int") {
                    self.load_fail(
                        at_file,
                        &format!("const '{}' must be an Int", cd.name),
                        cd.span.line,
                        cd.span.col,
                    );
                    continue;
                }
                if self
                    .type_consts
                    .borrow()
                    .get(&c.name)
                    .is_some_and(|s| s.contains_key(&cd.name))
                {
                    self.load_fail(
                        at_file,
                        &format!("duplicate const '{}' on {}", cd.name, c.name),
                        cd.span.line,
                        cd.span.col,
                    );
                    continue;
                }
                self.type_consts
                    .borrow_mut()
                    .entry(c.name.clone())
                    .or_default()
                    .insert(cd.name.clone(), n.unwrap());
            }
            if m.is_none() || !from_mod || c.is_pub {
                if let Some(m) = m {
                    m.borrow_mut()
                        .export_structs
                        .insert(c.name.clone(), rc.clone());
                }
                if self.structs.borrow().contains_key(&c.name) {
                    self.load_fail(
                        at_file,
                        &format!("duplicate class '{}'", c.name),
                        c.span.line,
                        1,
                    );
                }
                self.structs.borrow_mut().insert(c.name.clone(), rc);
            }
        }
    }

    fn ingest_impls(&self, impls: &[ImplDecl], at_file: &str) {
        for im in impls {
            let head = type_head(&im.type_name);
            let st = self
                .all_types
                .borrow()
                .get(&head)
                .cloned()
                .or_else(|| self.structs.borrow().get(&head).cloned());
            let Some(st) = st else {
                self.load_fail(
                    at_file,
                    &format!(
                        "undefined struct '{head}'{}",
                        stdlib::import_hint(&head, at_file)
                    ),
                    im.span.line,
                    1,
                );
                continue;
            };
            if !im.trait_name.is_empty() {
                let th = type_head(&im.trait_name);
                if !self.traits.borrow().contains_key(&th) {
                    self.load_fail(
                        at_file,
                        &format!(
                            "undefined trait '{th}'{}",
                            stdlib::import_hint(&th, at_file)
                        ),
                        im.span.line,
                        1,
                    );
                } else {
                    let params = if im.type_params.is_empty() {
                        st.borrow().type_params.clone()
                    } else {
                        im.type_params.clone()
                    };
                    self.record_trait_impl(&im.type_name, &im.trait_name, &params, im.span.line);
                }
            }
            let fields = st.borrow().fields.clone();
            let is_data = st.borrow().is_data;
            for method in &im.methods {
                let mut method = method.clone();
                stamp(&mut method, at_file);
                if fields.iter().any(|f| f == &method.name) {
                    self.load_fail(
                        at_file,
                        &format!(
                            "method '{}' conflicts with field '{}' on {head}",
                            method.name, method.name
                        ),
                        method.span.line,
                        1,
                    );
                }
                if self
                    .type_methods
                    .borrow()
                    .get(&head)
                    .is_some_and(|s| s.contains_key(&method.name))
                {
                    self.load_fail(
                        at_file,
                        &format!("duplicate method '{}' on {head}", method.name),
                        method.span.line,
                        1,
                    );
                }
                if method.vis == Vis::Protected && !self.class_abstract.borrow().contains_key(&head)
                {
                    let kind = if is_data { "data" } else { "struct" };
                    self.load_fail(
                        at_file,
                        &format!("protected cannot apply to {kind} method"),
                        method.span.line,
                        1,
                    );
                }
                self.type_methods
                    .borrow_mut()
                    .entry(head.clone())
                    .or_default()
                    .insert(method.name.clone(), Rc::new(method));
            }
        }
    }

    fn ingest_fns(
        &self,
        m: &ModRc,
        fns_list: &[FnDecl],
        from_mod: bool,
        mod_name: &str,
        at_file: &str,
    ) {
        for func in fns_list {
            let mut owned = func.clone();
            owned.module = mod_name.to_string();
            stamp(&mut owned, at_file);
            let name = owned.name.clone();
            let line = owned.span.line;
            let prev = m.borrow().fns.get(&name).cloned();
            if let Some(prev) = prev {
                if owned.is_ufcs && prev.is_ufcs {
                    let rc = Rc::new(owned);
                    let mut bm = m.borrow_mut();
                    let overloads = bm.ufcs_fns.entry(name).or_default();
                    if overloads.is_empty() {
                        overloads.push(prev);
                    }
                    overloads.push(rc);
                    continue;
                }
                self.load_fail(
                    at_file,
                    &format!("duplicate export '{name}' in module '{mod_name}'"),
                    line,
                    1,
                );
                continue;
            }
            let rc = Rc::new(owned);
            let mut bm = m.borrow_mut();
            bm.fns.insert(name.clone(), rc.clone());
            if rc.is_ufcs {
                bm.ufcs_fns
                    .entry(name.clone())
                    .or_default()
                    .push(rc.clone());
            }
            if !from_mod || rc.is_pub {
                bm.exports.insert(name, rc);
            }
        }
    }

    fn ingest_structs(
        &self,
        m: &ModRc,
        items: &[StructDecl],
        from_mod: bool,
        mod_name: &str,
        at_file: &str,
    ) {
        for st in items {
            if m.borrow().structs.contains_key(&st.name) {
                self.load_fail(
                    at_file,
                    &format!("duplicate export '{}' in module '{mod_name}'", st.name),
                    st.span.line,
                    1,
                );
                continue;
            }
            let rc = Rc::new(RefCell::new(st.clone()));
            m.borrow_mut().structs.insert(st.name.clone(), rc.clone());
            self.all_types
                .borrow_mut()
                .insert(st.name.clone(), rc.clone());
            self.record_field_access(&st.name, &st.fields, &st.field_vis);
            for method in &st.methods {
                let mut method = method.clone();
                method.module = mod_name.to_string();
                stamp(&mut method, at_file);
                self.type_methods
                    .borrow_mut()
                    .entry(st.name.clone())
                    .or_default()
                    .insert(method.name.clone(), Rc::new(method));
            }
            for t in &st.impl_traits {
                self.record_trait_impl(
                    &types::self_applied(&st.name, &st.type_params),
                    t,
                    &st.type_params,
                    st.span.line,
                );
            }
            self.register_type_signals(&st.name, &st.signals, at_file);
            if !from_mod || st.is_pub {
                m.borrow_mut()
                    .export_structs
                    .insert(st.name.clone(), rc.clone());
                if self.structs.borrow().contains_key(&st.name) {
                    self.load_fail(
                        at_file,
                        &format!("duplicate struct '{}'", st.name),
                        st.span.line,
                        1,
                    );
                }
                if self.fns.borrow().contains_key(&st.name) {
                    self.load_fail(
                        at_file,
                        &format!("struct '{}' conflicts with a function", st.name),
                        st.span.line,
                        1,
                    );
                }
                self.structs.borrow_mut().insert(st.name.clone(), rc);
            }
        }
    }

    fn load_imports(&self, imports: &[ImportDecl], from_file: &str, owner: Option<&ModRc>) {
        for im in imports {
            self.eval_import(im, from_file, owner);
        }
    }

    fn ingest_nested_mods(
        &self,
        parent: &ModRc,
        mods: &[ModDecl],
        parent_name: &str,
        at_file: &str,
    ) {
        for child in mods {
            let full = format!("{parent_name}.{}", child.name);
            self.ingest_loaded_mod(child, &full, at_file);
            parent
                .borrow_mut()
                .modules
                .insert(child.name.clone(), full.clone());
            if child.is_pub {
                parent
                    .borrow_mut()
                    .export_mods
                    .insert(child.name.clone(), full);
            }
        }
    }

    fn ingest_loaded_mod(&self, block: &ModDecl, full_name: &str, at_file: &str) {
        let dest = self
            .loaded
            .borrow_mut()
            .entry(full_name.to_string())
            .or_insert_with(|| Rc::new(RefCell::new(LoadedMod::default())))
            .clone();
        self.ingest_fns(&dest, &block.fns, true, full_name, at_file);
        self.ingest_structs(&dest, &block.structs, true, full_name, at_file);
        self.ingest_classes(Some(&dest), &block.classes, true, full_name, at_file);
        self.ingest_traits(&block.traits, at_file);
        self.ingest_enums(Some(&dest), &block.enums, true, full_name, at_file, true);
        self.ingest_impls(&block.impls, at_file);
        self.ingest_nested_mods(&dest, &block.mods, full_name, at_file);
        self.load_imports(&block.imports, at_file, Some(&dest));
    }

    fn pick_mod<'a>(&self, p: &'a Program, name: &str) -> Option<&'a ModDecl> {
        let mut named = None;
        let mut only = None;
        let mut mod_count = 0;
        let has_other = !p.fns.is_empty()
            || !p.structs.is_empty()
            || !p.classes.is_empty()
            || !p.traits.is_empty()
            || !p.enums.is_empty()
            || !p.impls.is_empty()
            || !p.signals.is_empty();
        for m in &p.mods {
            mod_count += 1;
            only = Some(m);
            if m.name == name {
                named = Some(m);
            }
        }
        if named.is_some() {
            return named;
        }
        if mod_count == 1 && !has_other {
            return only;
        }
        None
    }

    fn ingest_module(
        &self,
        p: &Program,
        mod_name: &str,
        m: &ModRc,
        at_file: &str,
        publish_global: bool,
    ) {
        if let Some(block) = self.pick_mod(p, mod_name) {
            self.ingest_fns(m, &block.fns, true, mod_name, at_file);
            self.ingest_structs(m, &block.structs, true, mod_name, at_file);
            self.ingest_classes(Some(m), &block.classes, true, mod_name, at_file);
            for s in &p.singletons {
                self.singletons.borrow_mut().insert(s.clone());
            }
            self.ingest_traits(&block.traits, at_file);
            self.ingest_enums(
                Some(m),
                &block.enums,
                true,
                mod_name,
                at_file,
                publish_global,
            );
            self.ingest_impls(&block.impls, at_file);
            self.ingest_consts(&block.consts, at_file);
            self.ingest_nested_mods(m, &block.mods, mod_name, at_file);
            self.load_imports(&block.imports, at_file, Some(m));
        } else {
            self.ingest_fns(m, &p.fns, false, mod_name, at_file);
            self.ingest_structs(m, &p.structs, false, mod_name, at_file);
            self.ingest_classes(Some(m), &p.classes, false, mod_name, at_file);
            for s in &p.singletons {
                self.singletons.borrow_mut().insert(s.clone());
            }
            self.ingest_traits(&p.traits, at_file);
            self.ingest_enums(Some(m), &p.enums, false, mod_name, at_file, publish_global);
            self.ingest_impls(&p.impls, at_file);
            self.ingest_consts(&p.consts, at_file);
            self.load_imports(&p.imports, at_file, Some(m));
        }
    }

    fn project_of(&self) -> Option<Project> {
        if self.project_loaded.get() {
            return self.project.borrow().clone();
        }
        self.project_loaded.set(true);
        let start = if !self.file.is_empty() {
            self.file.clone()
        } else {
            return None;
        };
        match Project::find(Path::new(&start)) {
            Ok(p) => {
                *self.project.borrow_mut() = p.clone();
                p
            }
            Err(d) => {
                self.diags.borrow_mut().push(d);
                None
            }
        }
    }

    fn resolve_module(&self, name: &str, from_file: &str) -> Vec<String> {
        let mut stem = name.to_string();
        if stem.len() > 3 && stem.ends_with(".rg") {
            stem.truncate(stem.len() - 3);
        }
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut add = |p: &Path, out: &mut Vec<String>, seen: &mut HashSet<String>| {
            if !p.is_file() {
                return;
            }
            let id = generic(&stdlib::weakly_canonical(p));
            if !seen.insert(id) {
                return;
            }
            out.push(generic(p));
        };
        let mut add_dir = |dir: &Path, out: &mut Vec<String>, seen: &mut HashSet<String>| {
            for path in stdlib::list_rg_files(dir) {
                add(&path, out, seen);
            }
        };
        if stem == "godot" {
            let dir =
                stdlib::godot_stub_dir(from_file).or_else(|| stdlib::godot_stub_dir(&self.file));
            if let Some(dir) = dir {
                add_dir(&dir, &mut out, &mut seen);
                if !out.is_empty() {
                    return out;
                }
            }
        }
        if stdlib::is_crate_stdlib(&stem) {
            let dir = stdlib::crate_dir(&stem, &self.file)
                .or_else(|| stdlib::crate_dir(&stem, from_file));
            if let Some(dir) = dir {
                if dir.is_file() {
                    add(&dir, &mut out, &mut seen);
                } else {
                    add_dir(&dir, &mut out, &mut seen);
                }
                if !out.is_empty() {
                    return out;
                }
            }
        }
        if let Some(proj) = self.project_of() {
            if let Some(mapped) = proj.module_path(&stem) {
                if mapped.is_dir() {
                    add_dir(&mapped, &mut out, &mut seen);
                } else {
                    add(&mapped, &mut out, &mut seen);
                }
                if !out.is_empty() {
                    return out;
                }
            }
        }
        let dotted = stem.replace('.', std::path::MAIN_SEPARATOR_STR);
        let mut bases = Vec::new();
        let mut push_base = |base: PathBuf, bases: &mut Vec<PathBuf>| {
            let id = generic(&stdlib::weakly_canonical(&base));
            if bases
                .iter()
                .any(|e| generic(&stdlib::weakly_canonical(e)) == id)
            {
                return;
            }
            bases.push(base);
        };
        if !from_file.is_empty() {
            let parent = Path::new(from_file)
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));
            push_base(parent, &mut bases);
        }
        let entry_parent = if self.file.is_empty() {
            PathBuf::from(".")
        } else {
            Path::new(&self.file)
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."))
        };
        push_base(entry_parent, &mut bases);
        for base in &bases {
            let before = out.len();
            add_dir(&base.join(&stem), &mut out, &mut seen);
            if dotted != stem {
                add_dir(&base.join(&dotted), &mut out, &mut seen);
            }
            if base.is_dir() {
                for path in stdlib::list_rg_files(base) {
                    if let Ok(text) = std::fs::read_to_string(&path) {
                        if file_has_mod(&text, &stem) {
                            add(&path, &mut out, &mut seen);
                        }
                    }
                }
            }
            if out.len() > before {
                return out;
            }
        }
        out
    }

    fn ingest_crate_entry(&self, crate_name: &str) {
        self.loading.borrow_mut().push(crate_name.to_string());
        let m = Rc::new(RefCell::new(LoadedMod::default()));
        let files = self.resolve_module(crate_name, &self.file);
        for path in files {
            if stdlib::same_rg_file(&path, &self.file) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                self.loading.borrow_mut().pop();
                self.load_fail(&self.file.clone(), "cannot read module", 1, 1);
                return;
            };
            let (kept, errs) = parse_source(&text, &path);
            for d in errs {
                self.record_diag(&d.kind, &d.file, d.line, d.col, &d.message);
            }
            self.ingest_module(&kept, crate_name, &m, &path, true);
        }
        self.ingest_module(&self.program.clone(), crate_name, &m, &self.file, true);
        self.loading.borrow_mut().pop();
        self.loaded.borrow_mut().insert(crate_name.to_string(), m);
        self.attach_crate_children(crate_name, true);
        let bind = match crate_name.rfind('.') {
            Some(dot) => crate_name[dot + 1..].to_string(),
            None => crate_name.to_string(),
        };
        self.module_binds
            .borrow_mut()
            .insert(bind, crate_name.to_string());
    }

    fn attach_crate_children(&self, parent: &str, publish_global: bool) {
        if !self.loaded.borrow().contains_key(parent) {
            return;
        }
        let Some(dir) = stdlib::crate_dir(parent, &self.file) else {
            return;
        };
        if !dir.is_dir() {
            return;
        }
        let kids = stdlib::list_child_dirs(&dir);
        for kid in kids {
            let full = format!("{parent}.{kid}");
            if self.resolve_module(&full, &self.file).is_empty() {
                continue;
            }
            self.load_module(&full, &self.file, 1, 1, publish_global);
            if !self.loaded.borrow().contains_key(parent) {
                return;
            }
            if !self.loaded.borrow().contains_key(&full) {
                continue;
            }
            if let Some(it) = self.loaded.borrow().get(parent).cloned() {
                it.borrow_mut().modules.insert(kid.clone(), full.clone());
                it.borrow_mut().export_mods.insert(kid, full);
            }
        }
    }

    fn load_module(&self, raw: &str, from_file: &str, line: u32, col: u32, publish_global: bool) {
        let name = stdlib::canonical_stdlib_name(raw);
        if self.loaded.borrow().contains_key(&name) || is_host_module(&name) {
            return;
        }
        if self.loading.borrow().iter().any(|c| c == &name) {
            self.load_fail(
                from_file,
                &format!("cyclic import of module '{name}'"),
                line,
                col,
            );
            return;
        }
        if let Some(dot) = name.rfind('.') {
            let parent = name[..dot].to_string();
            let parent_loaded = self.loaded.borrow().contains_key(&parent);
            if !parent_loaded
                && !is_host_module(&parent)
                && !self.resolve_module(&parent, from_file).is_empty()
            {
                self.load_module(&parent, from_file, line, col, publish_global);
            }
            if self.loaded.borrow().contains_key(&name) {
                return;
            }
        }
        let files = self.resolve_module(&name, from_file);
        if files.is_empty() {
            self.load_fail(
                from_file,
                &format!("module '{name}' not found (tried {name}/ or mod {name})"),
                line,
                col,
            );
            return;
        }
        self.loading.borrow_mut().push(name.clone());
        let m = Rc::new(RefCell::new(LoadedMod::default()));
        for path in &files {
            let Ok(text) = std::fs::read_to_string(path) else {
                self.loading.borrow_mut().pop();
                self.load_fail(from_file, "cannot read module", line, col);
                return;
            };
            let (kept, errs) = parse_source(&text, path);
            for d in errs {
                self.record_diag(&d.kind, &d.file, d.line, d.col, &d.message);
            }
            self.ingest_module(&kept, &name, &m, path, publish_global);
        }
        self.loading.borrow_mut().pop();
        self.loaded.borrow_mut().insert(name.clone(), m);
        self.attach_crate_children(&name, publish_global);
    }

    fn bind_from_import(
        &self,
        im: &ImportDecl,
        mod_rc: &ModRc,
        owner: Option<&ModRc>,
        from_file: &str,
    ) {
        if im.path.len() != 2 {
            self.load_fail(
                from_file,
                "nested from-imports longer than 2 segments are not supported",
                im.span.line,
                im.span.col,
            );
            return;
        }
        let item = &im.path[1];
        let alias = if im.alias.is_empty() {
            item.clone()
        } else {
            im.alias.clone()
        };
        let exports = mod_rc.borrow().exports.get(item).cloned();
        if let Some(fn_) = exports {
            if let Some(owner) = owner {
                if owner.borrow().from_fns.contains_key(&alias)
                    || owner.borrow().fns.contains_key(&alias)
                {
                    self.load_fail(
                        from_file,
                        &format!("duplicate import '{alias}'"),
                        im.span.line,
                        im.span.col,
                    );
                    return;
                }
                owner.borrow_mut().from_fns.insert(alias, fn_);
            } else if self.fns.borrow().contains_key(&alias) {
                self.load_fail(
                    from_file,
                    &format!("duplicate import '{alias}'"),
                    im.span.line,
                    im.span.col,
                );
            } else {
                self.fns.borrow_mut().insert(alias, fn_);
            }
            return;
        }
        let st = mod_rc.borrow().export_structs.get(item).cloned();
        if let Some(st) = st {
            if let Some(owner) = owner {
                owner.borrow_mut().structs.insert(alias.clone(), st.clone());
                owner.borrow_mut().export_structs.insert(alias, st);
            } else {
                let clash = self
                    .structs
                    .borrow()
                    .get(&alias)
                    .is_some_and(|e| !Rc::ptr_eq(e, &st));
                if clash {
                    self.load_fail(
                        from_file,
                        &format!("duplicate struct '{alias}'"),
                        im.span.line,
                        im.span.col,
                    );
                    return;
                }
                self.structs.borrow_mut().insert(alias, st);
            }
            return;
        }
        let en = mod_rc.borrow().export_enums.get(item).cloned();
        if let Some(en) = en {
            if let Some(owner) = owner {
                owner.borrow_mut().enums.insert(alias.clone(), en.clone());
                owner.borrow_mut().export_enums.insert(alias, en);
            } else {
                let clash = self
                    .enums
                    .borrow()
                    .get(&alias)
                    .is_some_and(|e| !Rc::ptr_eq(e, &en));
                if clash {
                    self.load_fail(
                        from_file,
                        &format!("duplicate enum '{alias}'"),
                        im.span.line,
                        im.span.col,
                    );
                    return;
                }
                self.enums.borrow_mut().insert(alias, en);
            }
            return;
        }
        let full = mod_rc.borrow().export_mods.get(item).cloned();
        if let Some(full) = full {
            if let Some(owner) = owner {
                owner.borrow_mut().modules.insert(alias, full);
            } else {
                self.module_binds.borrow_mut().insert(alias, full);
            }
            return;
        }
        self.load_fail(
            from_file,
            &format!("module '{}' has no export '{item}'", im.path[0]),
            im.span.line,
            im.span.col,
        );
    }

    fn check_dotted_export(&self, im: &ImportDecl, from_file: &str) {
        if im.path.len() < 2 {
            return;
        }
        let mut acc = im.path[0].clone();
        for seg in im.path.iter().skip(1) {
            let Some(pit) = self.loaded.borrow().get(&acc).cloned() else {
                return;
            };
            let pit = pit.borrow();
            if pit.modules.contains_key(seg) {
                if !pit.export_mods.contains_key(seg) {
                    self.load_fail(
                        from_file,
                        &format!("module '{acc}' has no export '{seg}'"),
                        im.span.line,
                        im.span.col,
                    );
                    return;
                }
            } else {
                return;
            }
            let next = format!("{acc}.{seg}");
            drop(pit);
            acc = next;
        }
    }

    fn eval_import(&self, im: &ImportDecl, from_file: &str, owner: Option<&ModRc>) {
        if im.path.is_empty() {
            self.load_fail(from_file, "empty import", im.span.line, im.span.col);
            return;
        }
        if is_host_module(&im.path[0]) {
            return;
        }
        if im.is_from {
            let key = stdlib::canonical_stdlib_name(&im.path[0]);
            self.load_module(&key, from_file, im.span.line, im.span.col, owner.is_none());
            let Some(it) = self.loaded.borrow().get(&key).cloned() else {
                return;
            };
            self.bind_from_import(im, &it, owner, from_file);
            return;
        }
        let mut full = im.path[0].clone();
        for seg in im.path.iter().skip(1) {
            full.push('.');
            full.push_str(seg);
        }
        let key = stdlib::canonical_stdlib_name(&full);
        self.load_module(&key, from_file, im.span.line, im.span.col, owner.is_none());
        self.check_dotted_export(im, from_file);
        let bind = if im.alias.is_empty() {
            im.path.last().cloned().unwrap_or_default()
        } else {
            im.alias.clone()
        };
        let set_bind = |name: String, target: String| {
            if let Some(owner) = owner {
                owner.borrow_mut().modules.insert(name, target);
            } else {
                self.module_binds.borrow_mut().insert(name, target);
            }
        };
        set_bind(bind, key.clone());
        if im.path[0] == "std" || key == "std" || (key.len() > 4 && key.starts_with("std.")) {
            set_bind("std".into(), "std".into());
        }
    }

    fn record_field_access(&self, type_name: &str, fields: &[String], vis: &[Vis]) {
        let mut slot = self.field_access.borrow_mut();
        let slot = slot.entry(type_name.to_string()).or_default();
        for (i, f) in fields.iter().enumerate() {
            let v = vis.get(i).copied().unwrap_or(Vis::Pub);
            slot.insert(f.clone(), v);
        }
    }

    fn flatten_type(&self, name: &str, stack: &mut Vec<String>, done: &mut HashSet<String>) {
        if done.contains(name) {
            return;
        }
        let type_line = self
            .all_types
            .borrow()
            .get(name)
            .map(|s| s.borrow().span.line)
            .unwrap_or(1);
        if stack.iter().any(|s| s == name) {
            self.load_fail(
                &self.file.clone(),
                &format!("cycle in class inheritance at '{name}'"),
                type_line,
                1,
            );
            return;
        }
        let parent_applied = self.class_parents.borrow().get(name).cloned();
        let Some(parent_applied) = parent_applied else {
            done.insert(name.to_string());
            return;
        };
        let parent = type_head(&parent_applied);
        if !self.all_types.borrow().contains_key(&parent) {
            if self.type_methods.borrow().contains_key(&parent) {
                done.insert(name.to_string());
                return;
            }
            self.load_fail(
                &self.file.clone(),
                &format!(
                    "class '{name}' extends unknown type '{parent_applied}'{}",
                    stdlib::import_hint(&parent, &self.file)
                ),
                type_line,
                1,
            );
            return;
        }
        if self
            .all_types
            .borrow()
            .get(&parent)
            .is_some_and(|s| s.borrow().is_data)
        {
            self.load_fail(
                &self.file.clone(),
                &format!("class '{name}' cannot extend data type '{parent}'"),
                type_line,
                1,
            );
            return;
        }
        stack.push(name.to_string());
        self.flatten_type(&parent, stack, done);
        stack.pop();
        let parent_rc = self.all_types.borrow().get(&parent).cloned().unwrap();
        let child_rc = self.all_types.borrow().get(name).cloned().unwrap();
        let (mut fields, mut types, mut opts, parent_params) = {
            let p = parent_rc.borrow();
            (
                p.fields.clone(),
                p.field_types.clone(),
                p.field_optional.clone(),
                p.type_params.clone(),
            )
        };
        while types.len() < fields.len() {
            types.push(String::new());
        }
        while opts.len() < fields.len() {
            opts.push(false);
        }
        if types.len() > fields.len() {
            types.truncate(fields.len());
        }
        if opts.len() > fields.len() {
            opts.truncate(fields.len());
        }
        let env = types::type_env_from(&parent_params, &types::type_arg_list(&parent_applied));
        for t in &mut types {
            *t = types::subst_type(t, &env);
        }
        let (child_fields, child_types, child_opts) = {
            let c = child_rc.borrow();
            (
                c.fields.clone(),
                c.field_types.clone(),
                c.field_optional.clone(),
            )
        };
        for (i, f) in child_fields.iter().enumerate() {
            if fields.iter().any(|e| e == f) {
                continue;
            }
            fields.push(f.clone());
            types.push(child_types.get(i).cloned().unwrap_or_default());
            opts.push(child_opts.get(i).copied().unwrap_or(false));
        }
        {
            let mut c = child_rc.borrow_mut();
            c.fields = fields;
            c.field_types = types;
            c.field_optional = opts;
        }
        let mut merged = HashMap::new();
        if let Some(parent_defs) = self.field_defaults.borrow().get(&parent).cloned() {
            merged.extend(parent_defs);
        }
        if let Some(child_defs) = self.field_defaults.borrow().get(name).cloned() {
            merged.extend(child_defs);
        }
        self.field_defaults
            .borrow_mut()
            .insert(name.to_string(), merged);
        done.insert(name.to_string());
    }

    fn apply_inheritance(&self) {
        let mut done = HashSet::new();
        let mut stack = Vec::new();
        let mut names: Vec<String> = self.class_parents.borrow().keys().cloned().collect();
        names.sort_by(|a, b| self.type_line(a).cmp(&self.type_line(b)).then(a.cmp(b)));
        for name in names {
            self.flatten_type(&name, &mut stack, &mut done);
        }
    }

    fn check_trait_impls(&self) {
        let entries: Vec<(String, Vec<String>)> = self
            .type_traits
            .borrow()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (key, traits) in entries {
            let type_line = self
                .all_types
                .borrow()
                .get(&key)
                .map(|s| s.borrow().span.line)
                .unwrap_or(1);
            for trait_name in traits {
                let head = type_head(&trait_name);
                let tr = self.traits.borrow().get(&head).cloned();
                let Some(tr) = tr else {
                    self.load_fail(
                        &self.file.clone(),
                        &format!(
                            "undefined trait '{head}'{}",
                            stdlib::import_hint(&head, &self.file)
                        ),
                        type_line,
                        1,
                    );
                    continue;
                };
                let targs = types::type_arg_list(&trait_name);
                if !targs.is_empty() {
                    let n = tr.type_params.len();
                    if n == 0 {
                        self.load_fail(
                            &self.file.clone(),
                            &format!("'{head}' does not take type arguments"),
                            type_line,
                            1,
                        );
                    } else if targs.len() != n {
                        self.load_fail(
                            &self.file.clone(),
                            &format!(
                                "'{head}' expected {n} type argument(s), got {}",
                                targs.len()
                            ),
                            type_line,
                            1,
                        );
                    }
                }
                let methods = tr.methods.clone();
                for m in methods {
                    if self.lookup_method(&key, &m.name).1.is_none() {
                        let line = if m.span.line > 0 {
                            m.span.line
                        } else {
                            type_line
                        };
                        self.load_fail(
                            &self.file.clone(),
                            &format!(
                                "type '{key}' is missing '{}' for trait '{trait_name}'",
                                m.name
                            ),
                            line,
                            1,
                        );
                    }
                }
            }
        }
    }

    fn type_line(&self, name: &str) -> u32 {
        self.all_types
            .borrow()
            .get(name)
            .map(|s| s.borrow().span.line)
            .unwrap_or(1)
    }

    fn check_abstract_final(&self) {
        let mut parents: Vec<(String, String)> = self
            .class_parents
            .borrow()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        parents.sort_by(|a, b| {
            self.type_line(&a.0)
                .cmp(&self.type_line(&b.0))
                .then(a.0.cmp(&b.0))
        });
        for (child, parent) in &parents {
            if self.class_final.borrow().get(&type_head(parent)) == Some(&true) {
                self.load_fail(
                    &self.file.clone(),
                    &format!(
                        "class '{child}' extends final class '{}'",
                        type_head(parent)
                    ),
                    self.type_line(child),
                    1,
                );
            }
        }
        let abs: Vec<(String, bool)> = self
            .class_abstract
            .borrow()
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();
        for (name, abs_class) in abs {
            let fin_class = self
                .class_final
                .borrow()
                .get(&name)
                .copied()
                .unwrap_or(false);
            let line = self.type_line(&name);
            if abs_class && fin_class {
                self.load_fail(
                    &self.file.clone(),
                    &format!("class '{name}' cannot be both abstract and final"),
                    line,
                    1,
                );
            }
            let methods: Vec<(String, FnDecl)> = self
                .type_methods
                .borrow()
                .get(&name)
                .map(|m| m.iter().map(|(k, v)| (k.clone(), (**v).clone())).collect())
                .unwrap_or_default();
            for (mname, method) in &methods {
                if method.is_abstract && method.is_final {
                    self.load_fail(
                        &self.file.clone(),
                        &format!("method '{mname}' cannot be both abstract and final"),
                        method.span.line,
                        1,
                    );
                }
                if method.is_abstract && !abs_class {
                    self.load_fail(
                        &self.file.clone(),
                        &format!(
                            "class '{name}' has abstract method '{mname}' but is not abstract"
                        ),
                        method.span.line,
                        1,
                    );
                }
                let mut current = self
                    .class_parents
                    .borrow()
                    .get(&name)
                    .map(|p| type_head(p))
                    .unwrap_or_default();
                let mut walked = HashSet::new();
                while !current.is_empty() && walked.insert(current.clone()) {
                    let parent_m = self
                        .type_methods
                        .borrow()
                        .get(&current)
                        .and_then(|t| t.get(mname).cloned());
                    if let Some(parent_m) = parent_m {
                        if parent_m.is_final {
                            self.load_fail(
                                &self.file.clone(),
                                &format!("cannot override final method '{mname}'"),
                                method.span.line,
                                1,
                            );
                        }
                        break;
                    }
                    let pit = self.class_parents.borrow().get(&current).cloned();
                    current = match pit {
                        Some(p) => type_head(&p),
                        None => break,
                    };
                }
            }
            if abs_class {
                continue;
            }
            let mut seen = HashSet::new();
            let mut current = name.clone();
            let mut walked = HashSet::new();
            while !current.is_empty() && walked.insert(current.clone()) {
                let methods: Vec<(String, bool)> = self
                    .type_methods
                    .borrow()
                    .get(&current)
                    .map(|t| t.iter().map(|(k, v)| (k.clone(), v.is_abstract)).collect())
                    .unwrap_or_default();
                for (mname, is_abs) in methods {
                    if !seen.insert(mname.clone()) {
                        continue;
                    }
                    if is_abs {
                        self.load_fail(
                            &self.file.clone(),
                            &format!("type '{name}' is missing abstract method '{mname}'"),
                            line,
                            1,
                        );
                    }
                }
                let pit = self.class_parents.borrow().get(&current).cloned();
                current = match pit {
                    Some(p) => type_head(&p),
                    None => break,
                };
            }
        }
    }

    fn bind_trait_signals(&self) {
        let entries: Vec<(String, Vec<String>)> = self
            .type_traits
            .borrow()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (key, traits) in entries {
            let type_line = self
                .all_types
                .borrow()
                .get(&key)
                .map(|s| s.borrow().span.line)
                .unwrap_or(1);
            let st = self.all_types.borrow().get(&key).cloned();
            for trait_name in traits {
                let Some(tr) = self.traits.borrow().get(&type_head(&trait_name)).cloned() else {
                    continue;
                };
                for sig in &tr.signals {
                    let exists = self
                        .type_signals
                        .borrow()
                        .get(&key)
                        .and_then(|s| s.get(&sig.name).copied());
                    if let Some(n) = exists {
                        if n != sig.params.len() as i32 {
                            self.load_fail(
                                &self.file.clone(),
                                &format!(
                                    "signal '{}' on {key} conflicts with trait '{trait_name}'",
                                    sig.name
                                ),
                                sig.span.line,
                                1,
                            );
                        }
                        continue;
                    }
                    if self
                        .type_methods
                        .borrow()
                        .get(&key)
                        .is_some_and(|m| m.contains_key(&sig.name))
                    {
                        self.load_fail(
                            &self.file.clone(),
                            &format!(
                                "signal '{}' conflicts with method '{}' on {key}",
                                sig.name, sig.name
                            ),
                            type_line,
                            1,
                        );
                    }
                    if let Some(st) = &st {
                        if st.borrow().fields.iter().any(|f| f == &sig.name) {
                            self.load_fail(
                                &self.file.clone(),
                                &format!(
                                    "signal '{}' conflicts with field '{}' on {key}",
                                    sig.name, sig.name
                                ),
                                type_line,
                                1,
                            );
                        }
                    }
                    self.type_signals
                        .borrow_mut()
                        .entry(key.clone())
                        .or_default()
                        .insert(sig.name.clone(), sig.params.len() as i32);
                }
            }
        }
    }

    fn constexpr_fail(&self, line: u32, col: u32, msg: &str) {
        self.record_diag("constexpr error", &self.file.clone(), line, col, msg);
    }

    fn check_constexpr_call(&self, name: &str, line: u32, col: u32) {
        if name == "len" {
            return;
        }
        if name == "print" || name == "assert" || name == "argv" || name == "argv_len" {
            self.constexpr_fail(
                line,
                col,
                &format!("constexpr function cannot call '{name}'"),
            );
            return;
        }
        let Some(fn_) = self.find_local_fn(name) else {
            self.constexpr_fail(
                line,
                col,
                &format!(
                    "constexpr function can only call constexpr functions ('{name}' is unknown)"
                ),
            );
            return;
        };
        if !fn_.is_constexpr {
            self.constexpr_fail(line, col, &format!("constexpr function can only call constexpr functions ('{name}' is not constexpr)"));
        }
    }

    fn check_constexpr_expr(&self, e: &Expr) {
        match e.kind {
            ExprKind::Try => self.constexpr_fail(
                e.span.line,
                e.span.col,
                "constexpr function cannot use 'try'",
            ),
            ExprKind::Lambda => self.constexpr_fail(
                e.span.line,
                e.span.col,
                "constexpr function cannot use closures",
            ),
            ExprKind::Var if e.text == "super" => self.constexpr_fail(
                e.span.line,
                e.span.col,
                "constexpr function cannot use super",
            ),
            ExprKind::Call => {
                if e.text.is_empty() {
                    self.constexpr_fail(
                        e.span.line,
                        e.span.col,
                        "constexpr function cannot call a closure",
                    );
                } else {
                    self.check_constexpr_call(&e.text, e.span.line, e.span.col);
                }
            }
            ExprKind::MethodCall if !e.kids.is_empty() => {
                let recv = &e.kids[0];
                let mut allowed = false;
                if recv.kind == ExprKind::Var {
                    if self.signal_arity.borrow().contains_key(&recv.text) {
                        self.constexpr_fail(
                            e.span.line,
                            e.span.col,
                            "constexpr function cannot use signals",
                        );
                    }
                    if let Some(mod_name) = self.find_module_bind(&recv.text) {
                        let lit = self.loaded.borrow().get(&mod_name).cloned();
                        if lit.is_none() {
                            self.constexpr_fail(
                                e.span.line,
                                e.span.col,
                                &format!("unknown module '{mod_name}'"),
                            );
                        } else if let Some(lit) = lit {
                            let eit = lit.borrow().exports.get(&e.text).cloned();
                            if eit.as_ref().is_none_or(|f| !f.is_constexpr) {
                                self.constexpr_fail(e.span.line, e.span.col, &format!("constexpr function can only call constexpr functions ('{}.{}' is not constexpr)", recv.text, e.text));
                            }
                            allowed = true;
                        }
                    } else if self.find_enum(&recv.text).is_some() {
                        allowed = true;
                    }
                }
                if !allowed {
                    if self.find_ufcs(&e.text, "").is_some_and(|f| f.is_constexpr) {
                        allowed = true;
                    }
                }
                if !allowed && e.text != "len" && e.text != "has" && e.text != "keys" {
                    self.constexpr_fail(
                        e.span.line,
                        e.span.col,
                        &format!("constexpr function cannot call method '{}'", e.text),
                    );
                }
            }
            _ => {}
        }
        for kid in &e.kids {
            self.check_constexpr_expr(kid);
        }
    }

    fn check_constexpr_stmt(&self, stmt: &Stmt) {
        match stmt.kind {
            StmtKind::Var => self.constexpr_fail(
                stmt.span.line,
                stmt.span.col,
                "constexpr function cannot use 'var'",
            ),
            StmtKind::Assign | StmtKind::FieldAssign | StmtKind::IndexAssign => self
                .constexpr_fail(
                    stmt.span.line,
                    stmt.span.col,
                    "constexpr function cannot assign",
                ),
            StmtKind::While => self.constexpr_fail(
                stmt.span.line,
                stmt.span.col,
                "constexpr function cannot use 'while'",
            ),
            StmtKind::For => self.constexpr_fail(
                stmt.span.line,
                stmt.span.col,
                "constexpr function cannot use 'for'",
            ),
            StmtKind::Break => self.constexpr_fail(
                stmt.span.line,
                stmt.span.col,
                "constexpr function cannot use 'break'",
            ),
            StmtKind::Continue => self.constexpr_fail(
                stmt.span.line,
                stmt.span.col,
                "constexpr function cannot use 'continue'",
            ),
            StmtKind::Throw => self.constexpr_fail(
                stmt.span.line,
                stmt.span.col,
                "constexpr function cannot throw",
            ),
            StmtKind::Do => self.constexpr_fail(
                stmt.span.line,
                stmt.span.col,
                "constexpr function cannot use 'do'",
            ),
            StmtKind::Pass | StmtKind::Comment => {}
            StmtKind::Expr | StmtKind::Const | StmtKind::Return => {
                if let Some(e) = &stmt.expr {
                    self.check_constexpr_expr(e);
                }
            }
            StmtKind::If => {
                if let Some(e) = &stmt.expr {
                    self.check_constexpr_expr(e);
                }
                for s in &stmt.body {
                    self.check_constexpr_stmt(s);
                }
                for s in &stmt.else_body {
                    self.check_constexpr_stmt(s);
                }
            }
            StmtKind::Match => {
                if let Some(e) = &stmt.expr {
                    self.check_constexpr_expr(e);
                }
                for arm in &stmt.arms {
                    for s in &arm.body {
                        self.check_constexpr_stmt(s);
                    }
                }
            }
        }
    }

    fn check_constexpr_fn(&self, func: &FnDecl) {
        for stmt in &func.body {
            self.check_constexpr_stmt(stmt);
        }
    }

    fn check_constexpr_fns(&self) {
        for func in &self.program.fns {
            if func.is_constexpr {
                self.check_constexpr_fn(func);
            }
        }
        let methods: Vec<FnDecl> = self
            .type_methods
            .borrow()
            .values()
            .flat_map(|m| m.values().map(|f| (**f).clone()))
            .collect();
        for m in methods {
            if m.is_constexpr {
                self.check_constexpr_fn(&m);
            }
        }
        let prev = self.current_module();
        let mods: Vec<(String, Vec<FnDecl>)> = self
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
        for (key, fns) in mods {
            *self.current_module.borrow_mut() = key;
            for func in fns {
                if func.is_constexpr {
                    self.check_constexpr_fn(&func);
                }
            }
        }
        *self.current_module.borrow_mut() = prev;
    }
}

fn stamp(func: &mut FnDecl, at: &str) {
    if func.file.is_empty() {
        func.file = at.to_string();
    }
}

fn fold_int(e: &Expr) -> Option<i64> {
    match e.kind {
        ExprKind::Int => Some(e.number),
        ExprKind::Unary if e.text == "-" => {
            let kid = e.kids.first()?;
            if kid.kind != ExprKind::Int {
                return None;
            }
            kid.number.checked_neg()
        }
        _ => None,
    }
}

fn shape_of(c: &ClassDecl) -> StructDecl {
    let mut shape = c.shape.clone();
    shape.name = c.name.clone();
    shape.is_pub = c.is_pub;
    shape.span = c.span;
    shape.type_params = c.type_params.clone();
    shape.fields.clear();
    shape.field_types.clear();
    shape.field_optional.clear();
    shape.field_vis.clear();
    shape.methods.clear();
    for f in &c.fields {
        shape.fields.push(f.name.clone());
        shape.field_types.push(f.ty.clone());
        shape.field_optional.push(f.optional);
    }
    shape.signals = c.signals.clone();
    shape.is_data = false;
    shape
}

fn engine_file(file: &str) -> bool {
    file.replace('\\', "/").ends_with("/godot/lib.rg")
}

pub fn engine_file_pub(file: &str) -> bool {
    engine_file(file)
}

fn godot_project_root(path: &str) -> Option<PathBuf> {
    if path.is_empty() {
        return None;
    }
    let mut dir = PathBuf::from(path);
    if !dir.is_dir() {
        dir.pop();
    }
    for _ in 0..8 {
        if dir.join("project.godot").is_file() {
            return Some(dir);
        }
        if !dir.pop() {
            break;
        }
    }
    None
}

fn collect_project_scripts(dir: &Path, out: &mut Vec<PathBuf>, depth: i32) {
    if depth > 12 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let child = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if child.is_dir() {
            if name == ".godot" || name == ".git" || name == "bin" {
                continue;
            }
            collect_project_scripts(&child, out, depth + 1);
        } else if name.ends_with(".rg") {
            out.push(child);
        }
    }
}

fn same_open_file(script: &Path, open: &str) -> bool {
    if open.is_empty() {
        return false;
    }
    let open_p = Path::new(open);
    if script.exists() && open_p.exists() && stdlib::same_file(script, open_p) {
        return true;
    }
    stdlib::lexical_abs_string(script).eq_ignore_ascii_case(&stdlib::lexical_abs_string(open_p))
}

fn file_has_mod(source: &str, name: &str) -> bool {
    let mut junk = Vec::new();
    let tokens = rosegold_syntax::tokenize(source, "", &mut junk);
    for i in 0..tokens.len().saturating_sub(1) {
        if tokens[i].kind == Tok::Module
            && tokens[i + 1].kind == Tok::Identifier
            && tokens[i + 1].text == name
        {
            return true;
        }
    }
    false
}

pub fn godot_project(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    let mut dir = PathBuf::from(path);
    if dir.is_file() || !dir.is_dir() {
        if !dir.pop() {
            return false;
        }
    }
    for _ in 0..8 {
        if dir.join("project.godot").is_file() {
            return true;
        }
        if !dir.pop() {
            break;
        }
    }
    false
}
