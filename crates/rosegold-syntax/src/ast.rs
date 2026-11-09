use crate::token::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vis {
    Pub,
    Protected,
    Private,
}

impl Vis {
    pub fn name(self) -> &'static str {
        match self {
            Vis::Private => "private",
            Vis::Protected => "protected",
            Vis::Pub => "pub",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Import,
    Fn,
    Struct,
    Class,
    Trait,
    Enum,
    Impl,
    Signal,
    Mod,
    Singleton,
    Const,
}

#[derive(Clone, Debug)]
pub struct TypeParam {
    pub name: String,
    pub bounds: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExprKind {
    Int,
    Float,
    String,
    Bool,
    Var,
    Unary,
    Binary,
    Call,
    MethodCall,
    Member,
    Index,
    Array,
    Map,
    StructLit,
    Range,
    Try,
    Lambda,
    Await,
    Cast,
    Is,
    If,
    Format,
    New,
    NodePath,
    Null,
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub kind: ExprKind,
    pub number: i64,
    pub real: f64,
    pub boolean_value: bool,
    pub text: String,
    pub module: String,
    pub names: Vec<String>,
    pub type_args: Vec<String>,
    pub kids: Vec<Expr>,
    pub lambda: Option<Box<FnDecl>>,
    pub span: Span,
}

impl Expr {
    pub fn new(kind: ExprKind, span: Span) -> Self {
        Self {
            kind,
            number: 0,
            real: 0.0,
            boolean_value: false,
            text: String::new(),
            module: String::new(),
            names: Vec::new(),
            type_args: Vec::new(),
            kids: Vec::new(),
            lambda: None,
            span,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pat {
    Wildcard,
    Int,
    Float,
    String,
    Bool,
    Variant,
}

#[derive(Clone, Debug)]
pub struct MatchArm {
    pub pat: Pat,
    pub name: String,
    pub number: i64,
    pub real: f64,
    pub boolean_value: bool,
    pub text: String,
    pub binds: Vec<String>,
    pub field_names: Vec<String>,
    pub body: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StmtKind {
    Expr,
    Var,
    Const,
    Assign,
    FieldAssign,
    IndexAssign,
    Return,
    If,
    While,
    For,
    Match,
    Pass,
    Break,
    Continue,
    Throw,
    Do,
    Comment,
}

#[derive(Clone, Debug)]
pub struct Stmt {
    pub kind: StmtKind,
    pub name: String,
    pub expr: Option<Expr>,
    pub target: Option<Expr>,
    pub body: Vec<Stmt>,
    pub else_body: Vec<Stmt>,
    pub arms: Vec<MatchArm>,
    pub type_name: String,
    pub op: String,
    pub leading_comments: Vec<String>,
    pub trailing_comment: String,
    pub has_expr: bool,
    pub generated: bool,
    pub span: Span,
}

impl Stmt {
    pub fn new(kind: StmtKind, span: Span) -> Self {
        Self {
            kind,
            name: String::new(),
            expr: None,
            target: None,
            body: Vec::new(),
            else_body: Vec::new(),
            arms: Vec::new(),
            type_name: String::new(),
            op: "=".to_string(),
            leading_comments: Vec::new(),
            trailing_comment: String::new(),
            has_expr: false,
            generated: false,
            span,
        }
    }
}

#[derive(Clone, Debug)]
pub struct FnDecl {
    pub name: String,
    pub type_params: Vec<TypeParam>,
    pub params: Vec<String>,
    pub param_types: Vec<String>,
    pub param_defaults: Vec<Option<Expr>>,
    pub return_type: String,
    pub body: Vec<Stmt>,
    pub module: String,
    pub is_test: bool,
    pub is_deprecated: bool,
    pub is_constexpr: bool,
    pub is_ufcs: bool,
    pub throws_ex: bool,
    pub is_async: bool,
    pub is_pub: bool,
    pub is_abstract: bool,
    pub is_final: bool,
    pub is_static: bool,
    pub is_vararg: bool,
    pub generated: bool,
    pub vis: Vis,
    pub span: Span,
    pub file: String,
    /// Continuation used when a suspending loop is lowered to bytecode.
    pub loop_break: Vec<Stmt>,
    pub loop_continue: Vec<Stmt>,
}

impl FnDecl {
    pub fn new(name: impl Into<String>, span: Span) -> Self {
        Self {
            name: name.into(),
            type_params: Vec::new(),
            params: Vec::new(),
            param_types: Vec::new(),
            param_defaults: Vec::new(),
            return_type: String::new(),
            body: Vec::new(),
            module: String::new(),
            is_test: false,
            is_deprecated: false,
            is_constexpr: false,
            is_ufcs: false,
            throws_ex: false,
            is_async: false,
            is_pub: true,
            is_abstract: false,
            is_final: false,
            is_static: false,
            is_vararg: false,
            generated: false,
            vis: Vis::Pub,
            span,
            file: String::new(),
            loop_break: Vec::new(),
            loop_continue: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct SignalDecl {
    pub name: String,
    pub params: Vec<String>,
    pub param_types: Vec<String>,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct StructDecl {
    pub name: String,
    pub type_params: Vec<TypeParam>,
    pub fields: Vec<String>,
    pub field_types: Vec<String>,
    pub field_optional: Vec<bool>,
    pub field_vis: Vec<Vis>,
    pub methods: Vec<FnDecl>,
    pub impl_traits: Vec<String>,
    pub signals: Vec<SignalDecl>,
    pub is_pub: bool,
    pub is_data: bool,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ImplDecl {
    pub type_params: Vec<TypeParam>,
    pub type_name: String,
    pub trait_name: String,
    pub methods: Vec<FnDecl>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ClassField {
    pub name: String,
    pub ty: String,
    pub has_default: bool,
    pub optional: bool,
    pub exported: bool,
    pub has_range: bool,
    pub range_min: String,
    pub range_max: String,
    pub range_step: String,
    pub range_suffixes: Vec<String>,
    pub has_enum: bool,
    pub enum_names: Vec<String>,
    pub has_flags: bool,
    pub flag_names: Vec<String>,
    pub has_file: bool,
    pub file_filters: Vec<String>,
    pub has_dir: bool,
    pub has_multiline: bool,
    pub onready: bool,
    pub vis: Vis,
    pub default_value: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct NestedImpl {
    pub type_params: Vec<TypeParam>,
    pub trait_name: String,
    pub methods: Vec<FnDecl>,
}

#[derive(Clone, Debug)]
pub struct ClassDecl {
    pub name: String,
    pub type_params: Vec<TypeParam>,
    pub parent: String,
    pub impl_traits: Vec<String>,
    pub fields: Vec<ClassField>,
    pub methods: Vec<FnDecl>,
    pub trait_impls: Vec<NestedImpl>,
    pub signals: Vec<SignalDecl>,
    pub consts: Vec<ConstDecl>,
    pub shape: StructDecl,
    pub is_pub: bool,
    pub is_abstract: bool,
    pub is_final: bool,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct TraitMethod {
    pub name: String,
    pub params: Vec<String>,
    pub param_types: Vec<String>,
    pub return_type: String,
    pub throws_ex: bool,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct TraitDecl {
    pub name: String,
    pub type_params: Vec<TypeParam>,
    pub methods: Vec<TraitMethod>,
    pub signals: Vec<SignalDecl>,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct EnumVariant {
    pub name: String,
    pub field_names: Vec<String>,
    pub field_types: Vec<String>,
    pub arity: i32,
}

#[derive(Clone, Debug)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub is_pub: bool,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ConstDecl {
    pub name: String,
    pub type_name: String,
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct ImportDecl {
    pub path: Vec<String>,
    pub alias: String,
    pub is_from: bool,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct OrderedItem {
    pub kind: ItemKind,
    pub index: usize,
    pub leading_comments: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ModDecl {
    pub name: String,
    pub imports: Vec<ImportDecl>,
    pub fns: Vec<FnDecl>,
    pub structs: Vec<StructDecl>,
    pub classes: Vec<ClassDecl>,
    pub traits: Vec<TraitDecl>,
    pub enums: Vec<EnumDecl>,
    pub impls: Vec<ImplDecl>,
    pub signals: Vec<SignalDecl>,
    pub consts: Vec<ConstDecl>,
    pub mods: Vec<ModDecl>,
    pub items: Vec<OrderedItem>,
    pub singletons: Vec<String>,
    pub trailing_comments: Vec<String>,
    pub is_pub: bool,
    pub span: Span,
}

impl Default for Span {
    fn default() -> Self {
        Self {
            line: 1,
            col: 1,
            offset: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Decl {
    Import(ImportDecl),
    Fn(FnDecl),
    Struct(StructDecl),
    Class(ClassDecl),
    Trait(TraitDecl),
    Enum(EnumDecl),
    Impl(ImplDecl),
    Signal(SignalDecl),
    Const(ConstDecl),
    Mod(ModDecl),
    Singleton(String),
}

#[derive(Clone, Debug, Default)]
pub struct Program {
    pub singletons: Vec<String>,
    pub imports: Vec<ImportDecl>,
    pub fns: Vec<FnDecl>,
    pub structs: Vec<StructDecl>,
    pub classes: Vec<ClassDecl>,
    pub traits: Vec<TraitDecl>,
    pub enums: Vec<EnumDecl>,
    pub impls: Vec<ImplDecl>,
    pub signals: Vec<SignalDecl>,
    pub consts: Vec<ConstDecl>,
    pub mods: Vec<ModDecl>,
    pub items: Vec<OrderedItem>,
    pub trailing_comments: Vec<String>,
}

fn note(items: &mut Vec<OrderedItem>, kind: ItemKind, index: usize, leading: Vec<String>) {
    items.push(OrderedItem {
        kind,
        index,
        leading_comments: leading,
    });
}

impl Program {
    pub fn push(&mut self, decl: Decl, leading: Vec<String>) {
        match decl {
            Decl::Import(d) => {
                self.imports.push(d);
                note(
                    &mut self.items,
                    ItemKind::Import,
                    self.imports.len() - 1,
                    leading,
                );
            }
            Decl::Fn(d) => {
                self.fns.push(d);
                note(&mut self.items, ItemKind::Fn, self.fns.len() - 1, leading);
            }
            Decl::Struct(d) => {
                self.structs.push(d);
                note(
                    &mut self.items,
                    ItemKind::Struct,
                    self.structs.len() - 1,
                    leading,
                );
            }
            Decl::Class(d) => {
                self.classes.push(d);
                note(
                    &mut self.items,
                    ItemKind::Class,
                    self.classes.len() - 1,
                    leading,
                );
            }
            Decl::Trait(d) => {
                self.traits.push(d);
                note(
                    &mut self.items,
                    ItemKind::Trait,
                    self.traits.len() - 1,
                    leading,
                );
            }
            Decl::Enum(d) => {
                self.enums.push(d);
                note(
                    &mut self.items,
                    ItemKind::Enum,
                    self.enums.len() - 1,
                    leading,
                );
            }
            Decl::Impl(d) => {
                self.impls.push(d);
                note(
                    &mut self.items,
                    ItemKind::Impl,
                    self.impls.len() - 1,
                    leading,
                );
            }
            Decl::Signal(d) => {
                self.signals.push(d);
                note(
                    &mut self.items,
                    ItemKind::Signal,
                    self.signals.len() - 1,
                    leading,
                );
            }
            Decl::Const(d) => {
                self.consts.push(d);
                note(
                    &mut self.items,
                    ItemKind::Const,
                    self.consts.len() - 1,
                    leading,
                );
            }
            Decl::Mod(d) => {
                self.mods.push(d);
                note(&mut self.items, ItemKind::Mod, self.mods.len() - 1, leading);
            }
            Decl::Singleton(name) => {
                self.singletons.push(name);
                note(
                    &mut self.items,
                    ItemKind::Singleton,
                    self.singletons.len() - 1,
                    leading,
                );
            }
        }
    }
}

impl ModDecl {
    pub fn push(&mut self, decl: Decl, leading: Vec<String>) {
        match decl {
            Decl::Import(d) => {
                self.imports.push(d);
                note(
                    &mut self.items,
                    ItemKind::Import,
                    self.imports.len() - 1,
                    leading,
                );
            }
            Decl::Fn(d) => {
                self.fns.push(d);
                note(&mut self.items, ItemKind::Fn, self.fns.len() - 1, leading);
            }
            Decl::Struct(d) => {
                self.structs.push(d);
                note(
                    &mut self.items,
                    ItemKind::Struct,
                    self.structs.len() - 1,
                    leading,
                );
            }
            Decl::Class(d) => {
                self.classes.push(d);
                note(
                    &mut self.items,
                    ItemKind::Class,
                    self.classes.len() - 1,
                    leading,
                );
            }
            Decl::Trait(d) => {
                self.traits.push(d);
                note(
                    &mut self.items,
                    ItemKind::Trait,
                    self.traits.len() - 1,
                    leading,
                );
            }
            Decl::Enum(d) => {
                self.enums.push(d);
                note(
                    &mut self.items,
                    ItemKind::Enum,
                    self.enums.len() - 1,
                    leading,
                );
            }
            Decl::Impl(d) => {
                self.impls.push(d);
                note(
                    &mut self.items,
                    ItemKind::Impl,
                    self.impls.len() - 1,
                    leading,
                );
            }
            Decl::Signal(d) => {
                self.signals.push(d);
                note(
                    &mut self.items,
                    ItemKind::Signal,
                    self.signals.len() - 1,
                    leading,
                );
            }
            Decl::Const(d) => {
                self.consts.push(d);
                note(
                    &mut self.items,
                    ItemKind::Const,
                    self.consts.len() - 1,
                    leading,
                );
            }
            Decl::Mod(d) => {
                self.mods.push(d);
                note(&mut self.items, ItemKind::Mod, self.mods.len() - 1, leading);
            }
            Decl::Singleton(name) => {
                self.singletons.push(name);
                note(
                    &mut self.items,
                    ItemKind::Singleton,
                    self.singletons.len() - 1,
                    leading,
                );
            }
        }
    }
}
