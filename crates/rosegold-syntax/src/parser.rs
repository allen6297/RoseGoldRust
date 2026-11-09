use crate::ast::*;
use crate::lexer::tokenize;
use crate::token::{Diagnostic, Span, Tok, Token};

struct ParseError;

#[derive(Default)]
struct FnAttrs {
    is_test: bool,
    is_deprecated: bool,
    is_constexpr: bool,
    is_ufcs: bool,
    is_optional: bool,
    is_onready: bool,
}

impl FnAttrs {
    fn any(&self) -> bool {
        self.is_test
            || self.is_deprecated
            || self.is_constexpr
            || self.is_ufcs
            || self.is_optional
            || self.is_onready
    }

    fn fn_like(&self) -> bool {
        self.is_test || self.is_deprecated || self.is_constexpr || self.is_ufcs
    }
}

#[derive(Default)]
struct MemberPrefix {
    attrs: FnAttrs,
    is_abstract: bool,
    is_final: bool,
    is_static: bool,
    exported: bool,
    has_range: bool,
    range_min: String,
    range_max: String,
    range_step: String,
    range_suffixes: Vec<String>,
    has_enum: bool,
    enum_names: Vec<String>,
    has_flags: bool,
    flag_names: Vec<String>,
    has_file: bool,
    file_filters: Vec<String>,
    has_dir: bool,
    has_multiline: bool,
    vis: Vis,
    saw_vis: bool,
}

impl Default for Vis {
    fn default() -> Self {
        Vis::Pub
    }
}

struct Parser {
    tokens: Vec<Token>,
    file: String,
    errors: Vec<Diagnostic>,
    i: usize,
}

pub fn parse_source(source: &str, file: &str) -> (Program, Vec<Diagnostic>) {
    let mut errors = Vec::new();
    let tokens = tokenize(source, file, &mut errors);
    let mut parser = Parser {
        tokens,
        file: file.to_string(),
        errors,
        i: 0,
    };
    let program = parser.parse();
    (program, parser.errors)
}

fn is_comment(kind: Tok) -> bool {
    kind == Tok::LineComment || kind == Tok::BlockComment
}

fn is_assign_tok(kind: Tok) -> bool {
    matches!(
        kind,
        Tok::Eq
            | Tok::PlusEq
            | Tok::MinusEq
            | Tok::StarEq
            | Tok::SlashEq
            | Tok::PercentEq
            | Tok::AmpEq
            | Tok::PipeEq
            | Tok::CaretEq
            | Tok::ShlEq
            | Tok::ShrEq
    )
}

fn assign_text(kind: Tok) -> &'static str {
    match kind {
        Tok::PlusEq => "+=",
        Tok::MinusEq => "-=",
        Tok::StarEq => "*=",
        Tok::SlashEq => "/=",
        Tok::PercentEq => "%=",
        Tok::AmpEq => "&=",
        Tok::PipeEq => "|=",
        Tok::CaretEq => "^=",
        Tok::ShlEq => "<<=",
        Tok::ShrEq => ">>=",
        _ => "=",
    }
}

fn wrap_args(name: &str, args: &[String]) -> String {
    if args.is_empty() {
        return name.to_string();
    }
    let mut out = String::from(name);
    out.push('[');
    for (n, arg) in args.iter().enumerate() {
        if n > 0 {
            out.push_str(", ");
        }
        out.push_str(arg);
    }
    out.push(']');
    out
}

fn blank_struct(name: &str, span: Span) -> StructDecl {
    StructDecl {
        name: name.to_string(),
        type_params: Vec::new(),
        fields: Vec::new(),
        field_types: Vec::new(),
        field_optional: Vec::new(),
        field_vis: Vec::new(),
        methods: Vec::new(),
        impl_traits: Vec::new(),
        signals: Vec::new(),
        is_pub: true,
        is_data: false,
        span,
    }
}

fn bind_self(func: &mut FnDecl) {
    if func.params.first().map(String::as_str) != Some("self") {
        func.params.insert(0, "self".to_string());
        func.param_types.insert(0, String::new());
        func.param_defaults.insert(0, None);
    }
}

impl Parser {
    fn span_of(&self, token: &Token) -> Span {
        token.span
    }

    fn next_significant(&self, mut from: usize) -> usize {
        while from < self.tokens.len() && is_comment(self.tokens[from].kind) {
            from += 1;
        }
        from
    }

    fn discard_comments(&mut self) {
        while self.i < self.tokens.len() && is_comment(self.tokens[self.i].kind) {
            self.i += 1;
        }
    }

    fn peek(&mut self) -> Token {
        self.discard_comments();
        if self.i >= self.tokens.len() {
            let span = self
                .tokens
                .last()
                .map(|t| t.span)
                .unwrap_or(Span::new(1, 1, 0));
            return Token::eof(span);
        }
        self.tokens[self.i].clone()
    }

    fn prev(&self) -> Token {
        self.tokens[self.i - 1].clone()
    }

    fn check(&mut self, kind: Tok) -> bool {
        self.peek().kind == kind
    }

    fn match_tok(&mut self, kind: Tok) -> bool {
        if !self.check(kind) {
            return false;
        }
        self.i += 1;
        true
    }

    fn advance(&mut self) -> Token {
        self.discard_comments();
        if self.i >= self.tokens.len() {
            let span = self
                .tokens
                .last()
                .map(|t| t.span)
                .unwrap_or(Span::new(1, 1, 0));
            return Token::eof(span);
        }
        let token = self.tokens[self.i].clone();
        self.i += 1;
        token
    }

    fn record(&mut self, msg: &str) {
        let token = self.peek();
        let diag = Diagnostic::parse_error(&self.file, token.line(), token.col(), msg);
        if self.errors.iter().any(|prev| {
            prev.file == diag.file
                && prev.line == diag.line
                && prev.col == diag.col
                && prev.message == diag.message
        }) {
            return;
        }
        self.errors.push(diag);
    }

    fn error_here(&mut self, msg: &str) -> ParseError {
        self.record(msg);
        ParseError
    }

    fn error_note(&mut self, msg: &str) {
        self.record(msg);
    }

    fn synchronize_stmt(&mut self) {
        while !self.check(Tok::Eof) && !self.check(Tok::RBrace) && !self.check(Tok::Semi) {
            self.advance();
        }
        if self.check(Tok::Semi) {
            self.advance();
        }
    }

    fn at_item_start(&mut self) -> bool {
        self.check(Tok::Function)
            || self.check(Tok::Struct)
            || self.check(Tok::Data)
            || self.check(Tok::Class)
            || self.check(Tok::Trait)
            || self.check(Tok::Enum)
            || self.check(Tok::Implements)
            || self.check(Tok::Signal)
            || self.check(Tok::Module)
            || self.check(Tok::Import)
            || self.check(Tok::From)
            || self.check(Tok::At)
            || self.check(Tok::Pub)
            || self.check(Tok::Private)
            || self.check(Tok::Protected)
            || self.check(Tok::Abstract)
            || self.check(Tok::Final)
    }

    fn synchronize_item(&mut self) {
        if self.check(Tok::Eof) {
            return;
        }
        self.advance();
        while !self.check(Tok::Eof) {
            if self.at_item_start() {
                return;
            }
            if self.check(Tok::RBrace) || self.check(Tok::Semi) {
                self.advance();
                return;
            }
            let before = self.i;
            self.advance();
            if self.i == before {
                return;
            }
        }
    }

    fn expect(&mut self, kind: Tok, msg: &str) -> Result<Token, ParseError> {
        if !self.check(kind) {
            return Err(self.error_here(msg));
        }
        Ok(self.advance())
    }

    fn expect_semi(&mut self, stmt: &mut Stmt) -> Result<(), ParseError> {
        let semi = self.expect(Tok::Semi, "expected ';'")?;
        if self.i < self.tokens.len()
            && is_comment(self.tokens[self.i].kind)
            && self.tokens[self.i].line() == semi.line()
        {
            stmt.trailing_comment = self.tokens[self.i].text.clone();
            self.i += 1;
        }
        Ok(())
    }

    fn parse_type(&mut self) -> Result<String, ParseError> {
        if self.check(Tok::Function) {
            self.expect(Tok::Function, "expected 'fn'")?;
            self.expect(Tok::LParen, "expected '('")?;
            let mut params = Vec::new();
            if !self.check(Tok::RParen) {
                loop {
                    params.push(self.parse_type()?);
                    if !self.match_tok(Tok::Comma) {
                        break;
                    }
                }
            }
            self.expect(Tok::RParen, "expected ')'")?;
            let mut ret = "Void".to_string();
            if self.match_tok(Tok::Colon) {
                ret = self.parse_type()?;
            }
            let mut out = String::from("fn(");
            for (n, param) in params.iter().enumerate() {
                if n > 0 {
                    out.push_str(", ");
                }
                out.push_str(param);
            }
            out.push_str("):");
            out.push_str(&ret);
            return Ok(out);
        }
        let name = self.expect(Tok::Identifier, "expected type name")?;
        let mut ty = name.text.clone();
        if self.match_tok(Tok::LArrow) {
            ty = wrap_args(&ty, &self.parse_type_arg_list(Tok::RArrow, "expected '>'")?);
        } else if self.match_tok(Tok::LBracket) {
            let mut args = Vec::new();
            if !self.check(Tok::RBracket) {
                loop {
                    args.push(self.parse_type()?);
                    if !self.match_tok(Tok::Comma) {
                        break;
                    }
                }
            } else {
                self.error_note("expected type argument");
            }
            self.expect(Tok::RBracket, "expected ']' after type argument")?;
            ty = wrap_args(&ty, &args);
        }
        Ok(ty)
    }

    fn parse_type_arg_list(
        &mut self,
        close: Tok,
        close_msg: &str,
    ) -> Result<Vec<String>, ParseError> {
        let mut args = Vec::new();
        if !self.check(close) {
            loop {
                args.push(self.parse_type()?);
                if !self.match_tok(Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(close, close_msg)?;
        Ok(args)
    }

    fn parse_type_params(&mut self) -> Result<Vec<TypeParam>, ParseError> {
        if !self.check(Tok::LBracket) && !self.check(Tok::LArrow) {
            return Ok(Vec::new());
        }
        let angle = self.match_tok(Tok::LArrow);
        if !angle {
            self.expect(Tok::LBracket, "expected '['")?;
        }
        let mut out = Vec::new();
        let empty = if angle {
            self.check(Tok::RArrow)
        } else {
            self.check(Tok::RBracket)
        };
        if empty {
            self.error_note("expected type parameter");
            if angle {
                self.expect(Tok::RArrow, "expected '>'")?;
            } else {
                self.expect(Tok::RBracket, "expected ']'")?;
            }
            return Ok(out);
        }
        loop {
            let name = self.expect(Tok::Identifier, "expected type parameter")?;
            let mut param = TypeParam {
                name: name.text.clone(),
                bounds: Vec::new(),
            };
            if self.match_tok(Tok::Colon) {
                loop {
                    param.bounds.push(self.parse_type()?);
                    if !self.match_tok(Tok::Plus) {
                        break;
                    }
                }
            }
            if out.iter().any(|prev: &TypeParam| prev.name == param.name) {
                self.error_note(&format!("duplicate type parameter '{}'", param.name));
            }
            out.push(param);
            if !self.match_tok(Tok::Comma) {
                break;
            }
        }
        if angle {
            self.expect(Tok::RArrow, "expected '>' after type parameters")?;
        } else {
            self.expect(Tok::RBracket, "expected ']' after type parameters")?;
        }
        Ok(out)
    }

    fn parse_optional_type(&mut self) -> Result<String, ParseError> {
        if !self.match_tok(Tok::Colon) {
            return Ok(String::new());
        }
        self.parse_type()
    }

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let token = self.peek();
        if self.match_tok(Tok::Integer) {
            let mut expr = Expr::new(ExprKind::Int, token.span);
            expr.number = token.number;
            return Ok(expr);
        }
        if self.match_tok(Tok::Float) {
            let mut expr = Expr::new(ExprKind::Float, token.span);
            expr.real = token.real;
            expr.text = token.text;
            return Ok(expr);
        }
        if self.match_tok(Tok::String) {
            let mut expr = Expr::new(ExprKind::String, token.span);
            expr.text = token.text;
            return Ok(expr);
        }
        if self.match_tok(Tok::FString) {
            return self.parse_format(&token);
        }
        if self.match_tok(Tok::True) || self.match_tok(Tok::False) {
            let mut expr = Expr::new(ExprKind::Bool, token.span);
            expr.boolean_value = token.kind == Tok::True;
            return Ok(expr);
        }
        if self.match_tok(Tok::Null) {
            return Ok(Expr::new(ExprKind::Null, token.span));
        }
        if self.check(Tok::Dollar) || (self.check(Tok::Percent) && self.unique_name_ahead()) {
            let op = self.peek();
            let unique = op.kind == Tok::Percent;
            self.match_tok(op.kind);
            let mut expr = self.parse_node_path(&op)?;
            expr.boolean_value = unique;
            return Ok(expr);
        }
        if self.match_tok(Tok::Identifier) || self.match_tok(Tok::Super) {
            let mut expr = Expr::new(ExprKind::Var, token.span);
            expr.text = if token.kind == Tok::Super {
                "super".to_string()
            } else {
                token.text
            };
            return Ok(expr);
        }
        if self.match_tok(Tok::If) {
            return self.parse_if_expr(&token);
        }
        if self.match_tok(Tok::LParen) {
            let inner = self.parse_expr()?;
            self.expect(Tok::RParen, "expected ')'")?;
            return Ok(inner);
        }
        if self.match_tok(Tok::LBracket) {
            let mut expr = Expr::new(ExprKind::Array, token.span);
            if !self.check(Tok::RBracket) {
                loop {
                    expr.kids.push(self.parse_expr()?);
                    if !self.match_tok(Tok::Comma) {
                        break;
                    }
                }
            }
            self.expect(Tok::RBracket, "expected ']' after array")?;
            return Ok(expr);
        }
        if self.match_tok(Tok::LBrace) {
            let mut expr = Expr::new(ExprKind::Map, token.span);
            if !self.check(Tok::RBrace) {
                loop {
                    expr.kids.push(self.parse_expr()?);
                    self.expect(Tok::Colon, "expected ':' after map key")?;
                    expr.kids.push(self.parse_expr()?);
                    if !self.match_tok(Tok::Comma) || self.check(Tok::RBrace) {
                        break;
                    }
                }
            }
            self.expect(Tok::RBrace, "expected '}' after map")?;
            return Ok(expr);
        }
        if self.check(Tok::Async) {
            let n = self.next_significant(self.i + 1);
            if n < self.tokens.len() && self.tokens[n].kind == Tok::Function {
                let m = self.next_significant(n + 1);
                if m < self.tokens.len() {
                    let kind = self.tokens[m].kind;
                    if kind == Tok::LParen || kind == Tok::LBracket || kind == Tok::LArrow {
                        return self.parse_lambda();
                    }
                }
            }
        }
        if self.check(Tok::Function) {
            let n = self.next_significant(self.i + 1);
            if n < self.tokens.len() {
                let kind = self.tokens[n].kind;
                if kind == Tok::LParen || kind == Tok::LBracket || kind == Tok::LArrow {
                    return self.parse_lambda();
                }
            }
        }
        Err(self.error_here("expected expression"))
    }

    fn unique_name_ahead(&self) -> bool {
        let j = self.next_significant(self.i + 1);
        if j >= self.tokens.len() {
            return false;
        }
        matches!(self.tokens[j].kind, Tok::Identifier | Tok::String)
    }

    fn parse_node_path(&mut self, op: &Token) -> Result<Expr, ParseError> {
        let mut expr = Expr::new(ExprKind::NodePath, op.span);
        if self.match_tok(Tok::String) {
            expr.text = self.prev().text;
            return Ok(expr);
        }
        let name = self.expect(Tok::Identifier, "expected node name")?;
        let mut path = name.text;
        while self.match_tok(Tok::Slash) {
            let next = self.expect(Tok::Identifier, "expected node name")?;
            path.push('/');
            path.push_str(&next.text);
        }
        expr.text = path;
        Ok(expr)
    }

    fn looks_like_generic_apply(&self) -> bool {
        let at = self.next_significant(self.i);
        if at >= self.tokens.len() || self.tokens[at].kind != Tok::LBracket {
            return false;
        }
        let mut j = self.next_significant(at + 1);
        if j >= self.tokens.len() || self.tokens[j].kind != Tok::Identifier {
            return false;
        }
        let mut depth = 1i32;
        j += 1;
        while j < self.tokens.len() && depth > 0 {
            let kind = self.tokens[j].kind;
            if is_comment(kind) {
                j += 1;
                continue;
            }
            if kind == Tok::LBracket {
                depth += 1;
            } else if kind == Tok::RBracket {
                depth -= 1;
            } else if !matches!(
                kind,
                Tok::Identifier | Tok::Comma | Tok::LArrow | Tok::RArrow
            ) {
                return false;
            }
            j += 1;
        }
        if depth != 0 || j >= self.tokens.len() {
            return false;
        }
        j = self.next_significant(j);
        j < self.tokens.len() && matches!(self.tokens[j].kind, Tok::LBrace | Tok::LParen)
    }

    fn parse_bracket_type_args(&mut self) -> Result<Vec<String>, ParseError> {
        self.expect(Tok::LBracket, "expected '['")?;
        let mut args = Vec::new();
        if self.check(Tok::RBracket) {
            self.error_note("expected type argument");
        } else {
            loop {
                args.push(self.parse_type()?);
                if !self.match_tok(Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(Tok::RBracket, "expected ']' after type arguments")?;
        Ok(args)
    }

    fn is_struct_lit_start(&self) -> bool {
        let at = self.next_significant(self.i);
        if at >= self.tokens.len() || self.tokens[at].kind != Tok::LBrace {
            return false;
        }
        let a = self.next_significant(at + 1);
        if a >= self.tokens.len() {
            return false;
        }
        if self.tokens[a].kind == Tok::RBrace {
            return true;
        }
        let b = self.next_significant(a + 1);
        if b >= self.tokens.len() {
            return false;
        }
        self.tokens[a].kind == Tok::Identifier && self.tokens[b].kind == Tok::Colon
    }

    fn parse_struct_lit(&mut self, type_name: &Expr) -> Result<Expr, ParseError> {
        self.expect(Tok::LBrace, "expected '{'")?;
        let mut lit = Expr::new(ExprKind::StructLit, type_name.span);
        lit.text = type_name.text.clone();
        if !self.check(Tok::RBrace) {
            loop {
                if self.check(Tok::RBrace) {
                    break;
                }
                let field = self.expect(Tok::Identifier, "expected field name")?;
                self.expect(Tok::Colon, "expected ':' after field name")?;
                lit.names.push(field.text);
                lit.kids.push(self.parse_expr()?);
                if !self.match_tok(Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(Tok::RBrace, "expected '}' after struct fields")?;
        Ok(lit)
    }

    fn finish_call(&mut self, call: &mut Expr) -> Result<(), ParseError> {
        if !self.check(Tok::RParen) {
            loop {
                call.kids.push(self.parse_expr()?);
                if !self.match_tok(Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(Tok::RParen, "expected ')'")?;
        Ok(())
    }

    fn parse_call(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.match_tok(Tok::Dot) {
                // `break` is a keyword, and `events.break` still needs that word as a name.
                let name = if self.check(Tok::Break) {
                    self.advance();
                    self.prev()
                } else {
                    self.expect(Tok::Identifier, "expected name after '.'")?
                };
                let targs = if self.looks_like_generic_apply() {
                    self.parse_bracket_type_args()?
                } else {
                    Vec::new()
                };
                if self.match_tok(Tok::LParen) {
                    let mut call = Expr::new(ExprKind::MethodCall, name.span);
                    call.text = name.text;
                    call.type_args = targs;
                    call.kids.push(expr);
                    self.finish_call(&mut call)?;
                    expr = call;
                } else if !targs.is_empty() {
                    return Err(self.error_here("expected '(' after type arguments"));
                } else {
                    let mut mem = Expr::new(ExprKind::Member, name.span);
                    mem.text = name.text;
                    mem.kids.push(expr);
                    expr = mem;
                }
            } else if matches!(expr.kind, ExprKind::Var | ExprKind::Lambda)
                && self.looks_like_generic_apply()
            {
                let args = self.parse_bracket_type_args()?;
                if expr.kind == ExprKind::Var && self.is_struct_lit_start() {
                    let mut lit = self.parse_struct_lit(&expr)?;
                    lit.type_args = args;
                    expr = lit;
                } else if self.match_tok(Tok::LParen) {
                    let mut call = Expr::new(ExprKind::Call, expr.span);
                    call.type_args = args;
                    if expr.kind == ExprKind::Var {
                        call.text = expr.text.clone();
                    } else {
                        call.kids.push(expr.clone());
                    }
                    self.finish_call(&mut call)?;
                    expr = call;
                } else {
                    return Err(self.error_here("expected '(' or '{' after type arguments"));
                }
            } else if self.match_tok(Tok::LParen) {
                let mut call = Expr::new(ExprKind::Call, expr.span);
                if expr.kind == ExprKind::Var {
                    call.text = expr.text.clone();
                } else {
                    call.kids.push(expr.clone());
                }
                self.finish_call(&mut call)?;
                expr = call;
            } else if self.match_tok(Tok::LBracket) {
                let mut idx = Expr::new(ExprKind::Index, expr.span);
                idx.kids.push(expr.clone());
                idx.kids.push(self.parse_expr()?);
                self.expect(Tok::RBracket, "expected ']' after index")?;
                expr = idx;
            } else if expr.kind == ExprKind::Var && self.is_struct_lit_start() {
                expr = self.parse_struct_lit(&expr)?;
            } else if self.match_tok(Tok::As) {
                let op = self.prev();
                let mut cast = Expr::new(ExprKind::Cast, op.span);
                cast.text = self.parse_type()?;
                cast.kids.push(expr);
                expr = cast;
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        if self.check(Tok::Identifier) && self.peek().text == "new" {
            let n = self.next_significant(self.i + 1);
            if n < self.tokens.len() && self.tokens[n].kind == Tok::Identifier {
                let op = self.peek();
                self.match_tok(Tok::Identifier);
                let mut expr = Expr::new(ExprKind::New, op.span);
                expr.text = self.parse_type()?;
                return Ok(expr);
            }
        }
        if self.match_tok(Tok::Await) {
            let op = self.prev();
            let mut expr = Expr::new(ExprKind::Await, op.span);
            expr.kids.push(self.parse_unary()?);
            return Ok(expr);
        }
        if self.match_tok(Tok::Try) {
            let op = self.prev();
            let mut expr = Expr::new(ExprKind::Try, op.span);
            expr.kids.push(self.parse_unary()?);
            return Ok(expr);
        }
        if self.match_tok(Tok::Minus) || self.match_tok(Tok::Bang) || self.match_tok(Tok::Tilde) {
            let op = self.prev();
            let mut expr = Expr::new(ExprKind::Unary, op.span);
            expr.text = op.text;
            expr.kids.push(self.parse_unary()?);
            return Ok(expr);
        }
        self.parse_call()
    }

    fn bin_loop(
        &mut self,
        mut left: Expr,
        ops: &[Tok],
        mut rhs: impl FnMut(&mut Self) -> Result<Expr, ParseError>,
    ) -> Result<Expr, ParseError> {
        loop {
            let mut hit = false;
            for op in ops {
                if self.match_tok(*op) {
                    hit = true;
                    break;
                }
            }
            if !hit {
                return Ok(left);
            }
            let op = self.prev();
            let mut expr = Expr::new(ExprKind::Binary, op.span);
            expr.text = op.text;
            expr.kids.push(left);
            expr.kids.push(rhs(self)?);
            left = expr;
        }
    }

    fn parse_factor(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_unary()?;
        self.bin_loop(left, &[Tok::Star, Tok::Slash, Tok::Percent], |p| {
            p.parse_unary()
        })
    }

    fn parse_term(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_factor()?;
        self.bin_loop(left, &[Tok::Plus, Tok::Minus], |p| p.parse_factor())
    }

    fn parse_shift(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_term()?;
        self.bin_loop(left, &[Tok::Shl, Tok::Shr], |p| p.parse_term())
    }

    fn parse_bit_and(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_shift()?;
        self.bin_loop(left, &[Tok::Amp], |p| p.parse_shift())
    }

    fn parse_bit_xor(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_bit_and()?;
        self.bin_loop(left, &[Tok::Caret], |p| p.parse_bit_and())
    }

    fn parse_bit_or(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_bit_xor()?;
        self.bin_loop(left, &[Tok::Pipe], |p| p.parse_bit_xor())
    }

    fn parse_comparison(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_bit_or()?;
        let left = self.bin_loop(
            left,
            &[Tok::LArrow, Tok::RArrow, Tok::LtEq, Tok::GtEq],
            |p| p.parse_bit_or(),
        )?;
        if self.match_tok(Tok::Is) {
            let op = self.prev();
            let mut expr = Expr::new(ExprKind::Is, op.span);
            expr.text = self.parse_type()?;
            expr.kids.push(left);
            return Ok(expr);
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_comparison()?;
        self.bin_loop(left, &[Tok::EqEq, Tok::NotEq], |p| p.parse_comparison())
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_equality()?;
        self.bin_loop(left, &[Tok::AndAnd], |p| p.parse_equality())
    }

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_and()?;
        self.bin_loop(left, &[Tok::OrOr], |p| p.parse_and())
    }

    fn parse_range(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_or()?;
        if self.match_tok(Tok::DotDot) || self.match_tok(Tok::DotDotEq) {
            let op = self.prev();
            let mut expr = Expr::new(ExprKind::Range, op.span);
            expr.boolean_value = op.kind == Tok::DotDotEq;
            expr.text = op.text;
            expr.kids.push(left);
            expr.kids.push(self.parse_or()?);
            return Ok(expr);
        }
        Ok(left)
    }

    fn parse_format(&mut self, token: &Token) -> Result<Expr, ParseError> {
        let mut expr = Expr::new(ExprKind::Format, token.span);
        let holes = &token.holes;
        for (h, hole) in holes.iter().enumerate() {
            let text = token.texts.get(h).cloned().unwrap_or_default();
            self.add_format_lit(&mut expr, &text, token);
            let line = token.hole_line.get(h).copied().unwrap_or(token.line());
            let col = token.hole_col.get(h).copied().unwrap_or(token.col());
            expr.kids.push(self.parse_hole(hole, line, col)?);
            expr.names.push("hole".to_string());
        }
        if holes.is_empty() {
            let tail = token.texts.first().cloned().unwrap_or_default();
            self.add_format_lit(&mut expr, &tail, token);
        } else if let Some(tail) = token.texts.last() {
            self.add_format_lit(&mut expr, tail, token);
        }
        if expr.kids.is_empty() {
            self.add_format_lit(&mut expr, "", token);
        }
        Ok(expr)
    }

    fn add_format_lit(&self, expr: &mut Expr, text: &str, token: &Token) {
        if text.is_empty() {
            return;
        }
        let mut lit = Expr::new(ExprKind::String, token.span);
        lit.text = text.to_string();
        expr.kids.push(lit);
        expr.names.push("lit".to_string());
    }

    fn parse_hole(&mut self, src: &str, line: u32, col: u32) -> Result<Expr, ParseError> {
        let mut tokens = tokenize(src, &self.file, &mut self.errors);
        for token in &mut tokens {
            if token.kind == Tok::Eof {
                continue;
            }
            if token.line() == 1 {
                token.span.col += col.saturating_sub(1);
            }
            token.span.line += line.saturating_sub(1);
        }
        let saved_tokens = std::mem::replace(&mut self.tokens, tokens);
        let saved_i = self.i;
        self.i = 0;
        let hole = self.parse_expr();
        let trailing_ok = hole.is_ok() && (self.check(Tok::Eof) || self.check(Tok::Semi));
        let result = if let Ok(expr) = hole {
            if !trailing_ok {
                Err(self.error_here("unexpected tokens in format value"))
            } else {
                Ok(expr)
            }
        } else {
            hole
        };
        self.tokens = saved_tokens;
        self.i = saved_i;
        result
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_range()
    }

    fn parse_if_expr(&mut self, if_tok: &Token) -> Result<Expr, ParseError> {
        let mut expr = Expr::new(ExprKind::If, if_tok.span);
        expr.kids.push(self.parse_range()?);
        self.expect(Tok::LBrace, "expected '{'")?;
        expr.kids.push(self.parse_expr()?);
        self.expect(Tok::RBrace, "expected '}'")?;
        if self.match_tok(Tok::Elif) {
            let el = self.prev();
            expr.kids.push(self.parse_if_expr(&el)?);
        } else if self.match_tok(Tok::Else) {
            self.expect(Tok::LBrace, "expected '{'")?;
            expr.kids.push(self.parse_expr()?);
            self.expect(Tok::RBrace, "expected '}'")?;
        } else {
            return Err(self.error_here("if expression needs else"));
        }
        Ok(expr)
    }

    fn parse_block(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.expect(Tok::LBrace, "expected '{'")?;
        let mut stmts = Vec::new();
        loop {
            let leading = self.take_leading_comments();
            if self.check(Tok::RBrace) || self.check(Tok::Eof) {
                if !leading.is_empty() {
                    let mut comment = Stmt::new(StmtKind::Comment, Span::new(1, 1, 0));
                    comment.leading_comments = leading;
                    stmts.push(comment);
                }
                break;
            }
            match self.parse_stmt() {
                Ok(mut stmt) => {
                    stmt.leading_comments.splice(0..0, leading);
                    stmts.push(stmt);
                }
                Err(_) => self.synchronize_stmt(),
            }
        }
        self.expect(Tok::RBrace, "expected '}'")?;
        Ok(stmts)
    }

    fn take_leading_comments(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        while self.i < self.tokens.len() && is_comment(self.tokens[self.i].kind) {
            out.push(self.tokens[self.i].text.clone());
            self.i += 1;
        }
        out
    }

    fn parse_if_at(&mut self, span: Span) -> Result<Stmt, ParseError> {
        let mut stmt = Stmt::new(StmtKind::If, span);
        stmt.expr = Some(self.parse_expr()?);
        stmt.body = self.parse_block()?;
        if self.match_tok(Tok::Elif) {
            let el = self.prev();
            stmt.else_body.push(self.parse_if_at(el.span)?);
        } else if self.match_tok(Tok::Else) {
            stmt.else_body = self.parse_block()?;
        }
        Ok(stmt)
    }

    fn parse_stmt(&mut self) -> Result<Stmt, ParseError> {
        let token = self.peek();
        if self.match_tok(Tok::If) {
            return self.parse_if_at(token.span);
        }
        if self.match_tok(Tok::Do) {
            let mut stmt = Stmt::new(StmtKind::Do, token.span);
            stmt.body = self.parse_block()?;
            if self.match_tok(Tok::Catch) {
                stmt.name = self.expect(Tok::Identifier, "expected catch binding")?.text;
                stmt.else_body = self.parse_block()?;
            }
            return Ok(stmt);
        }
        if self.match_tok(Tok::Throw) {
            let mut stmt = Stmt::new(StmtKind::Throw, token.span);
            stmt.expr = Some(self.parse_expr()?);
            self.expect_semi(&mut stmt)?;
            return Ok(stmt);
        }
        if self.match_tok(Tok::While) {
            let mut stmt = Stmt::new(StmtKind::While, token.span);
            stmt.expr = Some(self.parse_expr()?);
            stmt.body = self.parse_block()?;
            return Ok(stmt);
        }
        if self.match_tok(Tok::For) {
            let mut stmt = Stmt::new(StmtKind::For, token.span);
            stmt.name = self.expect(Tok::Identifier, "expected loop variable")?.text;
            self.expect(Tok::In, "expected 'in' after loop variable")?;
            stmt.expr = Some(self.parse_expr()?);
            stmt.body = self.parse_block()?;
            return Ok(stmt);
        }
        if self.check(Tok::Match) || self.check(Tok::Switch) {
            return self.parse_match_stmt();
        }
        if self.match_tok(Tok::Return) {
            let mut stmt = Stmt::new(StmtKind::Return, token.span);
            if !self.check(Tok::Semi) {
                stmt.expr = Some(self.parse_expr()?);
                stmt.has_expr = true;
            }
            self.expect_semi(&mut stmt)?;
            return Ok(stmt);
        }
        if self.match_tok(Tok::Pass) || self.match_tok(Tok::Break) || self.match_tok(Tok::Continue)
        {
            let kind = self.prev().kind;
            let stmt_kind = if kind == Tok::Pass {
                StmtKind::Pass
            } else if kind == Tok::Break {
                StmtKind::Break
            } else {
                StmtKind::Continue
            };
            let mut stmt = Stmt::new(stmt_kind, token.span);
            self.expect_semi(&mut stmt)?;
            return Ok(stmt);
        }
        if self.match_tok(Tok::Variable) || self.match_tok(Tok::Constant) {
            let is_const = self.prev().kind == Tok::Constant;
            let name = self.expect(Tok::Identifier, "expected variable name")?;
            let ty = self.parse_optional_type()?;
            self.expect(Tok::Eq, "expected '='")?;
            let mut stmt = Stmt::new(
                if is_const {
                    StmtKind::Const
                } else {
                    StmtKind::Var
                },
                name.span,
            );
            stmt.name = name.text;
            stmt.type_name = ty;
            stmt.expr = Some(self.parse_expr()?);
            self.expect_semi(&mut stmt)?;
            return Ok(stmt);
        }
        if self.check(Tok::Identifier) {
            let n = self.next_significant(self.i + 1);
            if n < self.tokens.len() && is_assign_tok(self.tokens[n].kind) {
                let name = self.advance();
                let op_tok = self.advance().kind;
                let mut stmt = Stmt::new(StmtKind::Assign, name.span);
                stmt.name = name.text;
                stmt.op = assign_text(op_tok).to_string();
                stmt.expr = Some(self.parse_expr()?);
                self.expect_semi(&mut stmt)?;
                return Ok(stmt);
            }
        }
        let expr = self.parse_expr()?;
        let mut stmt = Stmt::new(StmtKind::Expr, expr.span);
        let mut op = String::new();
        if expr.kind == ExprKind::Member && self.match_assign(&mut op) {
            stmt.kind = StmtKind::FieldAssign;
            stmt.name = expr.text.clone();
            stmt.op = op;
            stmt.target = expr.kids.first().cloned();
            stmt.expr = Some(self.parse_expr()?);
        } else if expr.kind == ExprKind::Index && self.match_assign(&mut op) {
            stmt.kind = StmtKind::IndexAssign;
            stmt.op = op;
            stmt.target = Some(expr);
            stmt.expr = Some(self.parse_expr()?);
        } else {
            stmt.expr = Some(expr);
        }
        self.expect_semi(&mut stmt)?;
        Ok(stmt)
    }

    fn match_assign(&mut self, op: &mut String) -> bool {
        if !is_assign_tok(self.peek().kind) {
            return false;
        }
        *op = assign_text(self.peek().kind).to_string();
        self.advance();
        true
    }

    fn parse_match_arm(&mut self) -> Result<MatchArm, ParseError> {
        let token = self.peek();
        let mut arm = MatchArm {
            pat: Pat::Variant,
            name: String::new(),
            number: 0,
            real: 0.0,
            boolean_value: false,
            text: String::new(),
            binds: Vec::new(),
            field_names: Vec::new(),
            body: Vec::new(),
            span: token.span,
        };
        if self.check(Tok::Integer) {
            arm.pat = Pat::Int;
            arm.number = self.advance().number;
        } else if self.check(Tok::Float) {
            arm.pat = Pat::Float;
            let ft = self.advance();
            arm.real = ft.real;
            arm.text = ft.text;
        } else if self.check(Tok::String) {
            arm.pat = Pat::String;
            arm.text = self.advance().text;
        } else if self.match_tok(Tok::True) || self.match_tok(Tok::False) {
            arm.pat = Pat::Bool;
            arm.boolean_value = self.prev().kind == Tok::True;
        } else {
            let name = self.expect(Tok::Identifier, "expected match pattern")?;
            if name.text == "_" {
                arm.pat = Pat::Wildcard;
            } else {
                arm.pat = Pat::Variant;
                arm.name = name.text;
                while self.match_tok(Tok::Dot) {
                    arm.name = self
                        .expect(Tok::Identifier, "expected name after '.'")?
                        .text;
                }
                if self.match_tok(Tok::LParen) {
                    if !self.check(Tok::RParen) {
                        loop {
                            if self.check(Tok::Identifier) {
                                let n = self.next_significant(self.i + 1);
                                if n < self.tokens.len() && self.tokens[n].kind == Tok::Colon {
                                    arm.field_names.push(self.advance().text);
                                    self.expect(Tok::Colon, "expected ':' after field name")?;
                                    arm.binds.push(
                                        self.expect(Tok::Identifier, "expected binding name")?.text,
                                    );
                                } else {
                                    arm.field_names.push(String::new());
                                    arm.binds.push(
                                        self.expect(Tok::Identifier, "expected binding name")?.text,
                                    );
                                }
                            } else {
                                arm.field_names.push(String::new());
                                arm.binds.push(
                                    self.expect(Tok::Identifier, "expected binding name")?.text,
                                );
                            }
                            if !self.match_tok(Tok::Comma) || self.check(Tok::RParen) {
                                break;
                            }
                        }
                    }
                    self.expect(Tok::RParen, "expected ')' after pattern binding")?;
                }
            }
        }
        arm.body = self.parse_block()?;
        Ok(arm)
    }

    fn parse_match_stmt(&mut self) -> Result<Stmt, ParseError> {
        let kw = self.advance();
        let mut stmt = Stmt::new(StmtKind::Match, kw.span);
        stmt.expr = Some(self.parse_expr()?);
        self.expect(Tok::LBrace, "expected '{' before match arms")?;
        while !self.check(Tok::RBrace) && !self.check(Tok::Eof) {
            stmt.arms.push(self.parse_match_arm()?);
        }
        self.expect(Tok::RBrace, "expected '}' after match arms")?;
        self.match_tok(Tok::Semi);
        Ok(stmt)
    }

    fn parse_enum(&mut self) -> Result<EnumDecl, ParseError> {
        let enum_tok = self.expect(Tok::Enum, "expected 'enum'")?;
        let name = self.expect(Tok::Identifier, "expected enum name")?;
        let mut decl = EnumDecl {
            name: name.text,
            variants: Vec::new(),
            is_pub: true,
            span: enum_tok.span,
        };
        if self.match_tok(Tok::LArrow) {
            let mut depth = 1i32;
            while depth > 0 && !self.check(Tok::Eof) {
                if self.match_tok(Tok::LArrow) {
                    depth += 1;
                } else if self.match_tok(Tok::RArrow) {
                    depth -= 1;
                } else {
                    self.advance();
                }
            }
        }
        self.expect(Tok::LBrace, "expected '{'")?;
        while !self.check(Tok::RBrace) && !self.check(Tok::Eof) {
            self.match_tok(Tok::Pub);
            let vname = self.expect(Tok::Identifier, "expected variant name")?;
            let mut variant = EnumVariant {
                name: vname.text.clone(),
                field_names: Vec::new(),
                field_types: Vec::new(),
                arity: 0,
            };
            if self.match_tok(Tok::LParen) {
                if !self.check(Tok::RParen) {
                    loop {
                        if self.check(Tok::Identifier) {
                            let n = self.next_significant(self.i + 1);
                            if n < self.tokens.len() && self.tokens[n].kind == Tok::Colon {
                                variant.field_names.push(self.advance().text);
                                self.expect(Tok::Colon, "expected ':' after field name")?;
                                variant.field_types.push(self.parse_type()?);
                            } else {
                                variant.field_names.push(String::new());
                                variant.field_types.push(self.parse_type()?);
                            }
                        } else {
                            variant.field_names.push(String::new());
                            variant.field_types.push(self.parse_type()?);
                        }
                        variant.arity += 1;
                        if !self.match_tok(Tok::Comma) || self.check(Tok::RParen) {
                            break;
                        }
                    }
                }
                self.expect(Tok::RParen, "expected ')' after variant types")?;
            }
            if decl.variants.iter().any(|prev| prev.name == variant.name) {
                self.error_note(&format!("duplicate variant '{}'", variant.name));
            }
            decl.variants.push(variant);
            if !self.match_tok(Tok::Comma) {
                break;
            }
        }
        self.expect(Tok::RBrace, "expected '}' after enum variants")?;
        Ok(decl)
    }

    fn parse_fn(
        &mut self,
        attrs: &FnAttrs,
        is_pub: bool,
        abstract_method: bool,
    ) -> Result<FnDecl, ParseError> {
        let is_async = self.match_tok(Tok::Async);
        let fn_tok = self.expect(Tok::Function, "expected 'fn'")?;
        let name = self.expect(Tok::Identifier, "expected function name")?;
        let mut func = FnDecl::new(name.text, fn_tok.span);
        func.is_test = attrs.is_test;
        func.is_deprecated = attrs.is_deprecated;
        func.is_constexpr = attrs.is_constexpr;
        func.is_ufcs = attrs.is_ufcs;
        func.is_pub = is_pub;
        func.is_abstract = abstract_method;
        func.is_async = is_async;
        func.type_params = self.parse_type_params()?;
        self.fill_fn_sig(&mut func)?;
        if abstract_method {
            if is_async {
                self.error_note("abstract methods cannot be async");
            }
            self.expect(Tok::Semi, "expected ';' after abstract method")?;
            return Ok(func);
        }
        func.body = self.parse_block()?;
        Ok(func)
    }

    fn fill_fn_sig(&mut self, func: &mut FnDecl) -> Result<(), ParseError> {
        self.expect(Tok::LParen, "expected '('")?;
        if !self.check(Tok::RParen) {
            let mut seen_default = false;
            loop {
                if self.match_tok(Tok::Ellipsis) {
                    func.is_vararg = true;
                    break;
                }
                let param = self.expect(Tok::Identifier, "expected parameter name")?;
                func.param_types.push(self.parse_optional_type()?);
                func.params.push(param.text);
                if self.match_tok(Tok::Eq) {
                    seen_default = true;
                    func.param_defaults.push(Some(self.parse_expr()?));
                } else {
                    if seen_default {
                        self.error_note("default arguments must come last");
                    }
                    func.param_defaults.push(None);
                }
                if !self.match_tok(Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(Tok::RParen, "expected ')'")?;
        func.throws_ex = self.match_tok(Tok::Throws);
        func.return_type = self.parse_optional_type()?;
        Ok(())
    }

    fn parse_lambda(&mut self) -> Result<Expr, ParseError> {
        let is_async = self.match_tok(Tok::Async);
        let fn_tok = self.expect(Tok::Function, "expected 'fn'")?;
        let mut func = FnDecl::new("<fn>", fn_tok.span);
        func.is_async = is_async;
        func.type_params = self.parse_type_params()?;
        self.fill_fn_sig(&mut func)?;
        func.body = self.parse_block()?;
        let mut expr = Expr::new(ExprKind::Lambda, fn_tok.span);
        expr.lambda = Some(Box::new(func));
        Ok(expr)
    }

    fn check_method_name(&mut self, fields: &[String], methods: &[FnDecl], name: &str) {
        if fields.iter().any(|field| field == name) {
            self.error_note(&format!("method '{name}' conflicts with field '{name}'"));
        }
        if methods.iter().any(|method| method.name == name) {
            self.error_note(&format!("duplicate method '{name}'"));
        }
    }

    fn check_signal_decl(
        &mut self,
        fields: &[String],
        methods: &[FnDecl],
        signals: &[SignalDecl],
        name: &str,
    ) {
        if fields.iter().any(|field| field == name) {
            self.error_note(&format!("signal '{name}' conflicts with field '{name}'"));
        }
        if methods.iter().any(|method| method.name == name) {
            self.error_note(&format!("signal '{name}' conflicts with method '{name}'"));
        }
        if signals.iter().any(|signal| signal.name == name) {
            self.error_note(&format!("duplicate signal '{name}'"));
        }
    }

    fn parse_one_attr(&mut self, attrs: &mut FnAttrs) -> Result<bool, ParseError> {
        if !self.match_tok(Tok::At) {
            return Ok(false);
        }
        let attr = self.expect(Tok::Identifier, "expected attribute name")?;
        match attr.text.as_str() {
            "test" => attrs.is_test = true,
            "deprecated" => attrs.is_deprecated = true,
            "constexpr" => attrs.is_constexpr = true,
            "ufcs" => attrs.is_ufcs = true,
            "optional" => attrs.is_optional = true,
            "onready" => attrs.is_onready = true,
            other => self.error_note(&format!("unknown attribute @{other}")),
        }
        Ok(true)
    }

    fn reject_method_attrs(&mut self, attrs: &FnAttrs) {
        if attrs.is_test {
            self.error_note("@test cannot apply to method");
        }
        if attrs.is_ufcs {
            self.error_note("@ufcs cannot apply to method");
        }
        if attrs.is_optional {
            self.error_note("@optional cannot apply to method");
        }
        if attrs.is_onready {
            self.error_note("@onready applies to a class var");
        }
    }

    fn reject_class_mods(&mut self, is_abstract: bool, is_final: bool, what: &str) {
        if is_abstract {
            self.error_note(&format!("abstract cannot apply to {what}"));
        }
        if is_final {
            self.error_note(&format!("final cannot apply to {what}"));
        }
    }

    fn reject_protected(&mut self, is_protected: bool, what: &str) {
        if is_protected {
            self.error_note(&format!("protected cannot apply to {what}"));
        }
    }

    fn take_vis(&mut self, prefix: &mut MemberPrefix, vis: Vis) {
        if prefix.saw_vis {
            self.error_note("cannot combine visibility modifiers");
        }
        prefix.saw_vis = true;
        prefix.vis = vis;
    }

    fn parse_member_prefix(&mut self, prefix: &mut MemberPrefix) -> Result<(), ParseError> {
        loop {
            if self.parse_one_attr(&mut prefix.attrs)? {
                continue;
            }
            if self.match_tok(Tok::Pub) {
                self.take_vis(prefix, Vis::Pub);
                continue;
            }
            if self.match_tok(Tok::Private) {
                self.take_vis(prefix, Vis::Private);
                continue;
            }
            if self.match_tok(Tok::Protected) {
                self.take_vis(prefix, Vis::Protected);
                continue;
            }
            if self.match_tok(Tok::Abstract) {
                prefix.is_abstract = true;
                continue;
            }
            if self.match_tok(Tok::Final) {
                prefix.is_final = true;
                continue;
            }
            if self.match_tok(Tok::Static) {
                prefix.is_static = true;
                continue;
            }
            if self.match_tok(Tok::Export) {
                prefix.exported = true;
                if self.check(Tok::Identifier) && self.peek().text == "range" {
                    self.match_tok(Tok::Identifier);
                    self.expect(Tok::LParen, "expected '('")?;
                    prefix.range_min = self.parse_range_number()?;
                    self.expect(Tok::Comma, "expected ','")?;
                    prefix.range_max = self.parse_range_number()?;
                    if self.match_tok(Tok::Comma) {
                        if self.check(Tok::String) {
                            self.parse_range_suffixes(&mut prefix.range_suffixes)?;
                        } else {
                            prefix.range_step = self.parse_range_number()?;
                            if self.match_tok(Tok::Comma) {
                                if self.check(Tok::String) {
                                    self.parse_range_suffixes(&mut prefix.range_suffixes)?;
                                } else {
                                    self.error_note("range takes 2 or 3 numbers");
                                }
                            }
                        }
                    }
                    self.expect(Tok::RParen, "expected ')'")?;
                    prefix.has_range = true;
                } else if self.match_tok(Tok::Enum) {
                    prefix.has_enum =
                        self.parse_quoted_names(&mut prefix.enum_names, "enum", "name")?;
                } else if self.check(Tok::Identifier) && self.peek().text == "flags" {
                    self.match_tok(Tok::Identifier);
                    prefix.has_flags =
                        self.parse_quoted_names(&mut prefix.flag_names, "flags", "name")?;
                } else if self.check(Tok::Identifier) && self.peek().text == "file" {
                    self.match_tok(Tok::Identifier);
                    prefix.has_file =
                        self.parse_quoted_names(&mut prefix.file_filters, "file", "filter")?;
                } else if self.check(Tok::Identifier) && self.peek().text == "dir" {
                    self.match_tok(Tok::Identifier);
                    prefix.has_dir = true;
                } else if self.check(Tok::Identifier) && self.peek().text == "multiline" {
                    self.match_tok(Tok::Identifier);
                    prefix.has_multiline = true;
                }
                continue;
            }
            break;
        }
        Ok(())
    }

    fn parse_quoted_names(
        &mut self,
        names: &mut Vec<String>,
        kind: &str,
        item: &str,
    ) -> Result<bool, ParseError> {
        self.expect(Tok::LParen, "expected '('")?;
        if self.check(Tok::RParen) {
            self.error_note(&format!("{kind} needs a {item}"));
        } else {
            loop {
                let name = self.expect(Tok::String, &format!("expected {kind} {item}"))?;
                if name.text.contains(',') {
                    self.error_note(&format!("{kind} {item} cannot contain a comma"));
                } else if !name.text.is_empty() {
                    names.push(name.text);
                } else {
                    self.error_note(&format!("{kind} needs a {item}"));
                }
                if !self.match_tok(Tok::Comma) || self.check(Tok::RParen) {
                    break;
                }
            }
        }
        self.expect(Tok::RParen, "expected ')'")?;
        Ok(!names.is_empty())
    }

    fn range_suffix_ok(&mut self, text: &str, out: &[String]) -> bool {
        if text == "or_greater" || text == "or_less" {
            if out.iter().any(|prev| prev == text) {
                self.error_note("range suffix is repeated");
                return false;
            }
            return true;
        }
        if let Some(rest) = text.strip_prefix("suffix:") {
            if rest.is_empty() {
                self.error_note("range suffix needs text after suffix:");
                return false;
            }
            if text.contains(',') {
                self.error_note("range suffix cannot contain a comma");
                return false;
            }
            if out.iter().any(|prev| prev.starts_with("suffix:")) {
                self.error_note("range suffix is repeated");
                return false;
            }
            return true;
        }
        self.error_note("range suffix must be \"or_greater\", \"or_less\", or \"suffix:...\"");
        false
    }

    fn parse_range_suffixes(&mut self, out: &mut Vec<String>) -> Result<(), ParseError> {
        while self.check(Tok::String) {
            let token = self.expect(Tok::String, "expected range suffix")?;
            if self.range_suffix_ok(&token.text, out) {
                out.push(token.text);
            }
            if !self.match_tok(Tok::Comma) {
                break;
            }
        }
        Ok(())
    }

    fn parse_range_number(&mut self) -> Result<String, ParseError> {
        let neg = self.match_tok(Tok::Minus);
        let token = self.peek();
        if !self.match_tok(Tok::Integer) && !self.match_tok(Tok::Float) {
            return Err(self.error_here("range bounds must be numbers"));
        }
        Ok(format!("{}{}", if neg { "-" } else { "" }, token.text))
    }

    fn parse_method(&mut self, trait_impl: bool) -> Result<FnDecl, ParseError> {
        let mut prefix = MemberPrefix::default();
        self.parse_member_prefix(&mut prefix)?;
        self.reject_class_mods(prefix.is_abstract, prefix.is_final, "impl method");
        if trait_impl && prefix.vis != Vis::Pub {
            self.error_note("trait impl methods cannot be private or protected");
        }
        self.reject_method_attrs(&prefix.attrs);
        let mut func = self.parse_fn(&prefix.attrs, true, false)?;
        func.vis = prefix.vis;
        bind_self(&mut func);
        Ok(func)
    }

    fn parse_struct(&mut self, is_data: bool) -> Result<StructDecl, ParseError> {
        let tok = if is_data {
            self.expect(Tok::Data, "expected 'data'")?
        } else {
            self.expect(Tok::Struct, "expected 'struct'")?
        };
        let name = self.expect(
            Tok::Identifier,
            if is_data {
                "expected data name"
            } else {
                "expected struct name"
            },
        )?;
        let mut decl = blank_struct(&name.text, tok.span);
        decl.is_data = is_data;
        decl.type_params = self.parse_type_params()?;
        let kind = if is_data { "data" } else { "struct" };
        if self.match_tok(Tok::Implements) {
            loop {
                decl.impl_traits.push(self.parse_type()?);
                if !self.match_tok(Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(Tok::LBrace, "expected '{'")?;
        while !self.check(Tok::RBrace) && !self.check(Tok::Eof) {
            if let Err(_) = self.parse_struct_member(&mut decl, is_data, kind) {
                self.synchronize_stmt();
            }
        }
        self.expect(Tok::RBrace, "expected '}'")?;
        Ok(decl)
    }

    fn parse_struct_member(
        &mut self,
        decl: &mut StructDecl,
        is_data: bool,
        kind: &str,
    ) -> Result<(), ParseError> {
        let mut prefix = MemberPrefix::default();
        self.parse_member_prefix(&mut prefix)?;
        if self.check(Tok::Function) || self.check(Tok::Async) {
            if prefix.is_static {
                self.error_note("static applies to class methods");
            }
            self.reject_class_mods(
                prefix.is_abstract,
                prefix.is_final,
                &format!("{kind} method"),
            );
            if prefix.vis == Vis::Protected {
                self.error_note(&format!("protected cannot apply to {kind} method"));
            }
            self.reject_method_attrs(&prefix.attrs);
            let mut func = self.parse_fn(&prefix.attrs, true, false)?;
            func.vis = prefix.vis;
            bind_self(&mut func);
            self.check_method_name(&decl.fields, &decl.methods, &func.name);
            if decl.signals.iter().any(|sig| sig.name == func.name) {
                self.error_note(&format!(
                    "method '{}' conflicts with signal '{}'",
                    func.name, func.name
                ));
            }
            decl.methods.push(func);
            return Ok(());
        }
        if self.check(Tok::Signal) {
            self.reject_class_mods(prefix.is_abstract, prefix.is_final, "signal");
            self.reject_protected(prefix.vis == Vis::Protected, "signal");
            if prefix.attrs.any() {
                self.error_note("attributes cannot apply to signal");
            }
            if is_data {
                self.error_note("data cannot declare signals");
            }
            let sig = self.parse_signal()?;
            self.check_signal_decl(&decl.fields, &decl.methods, &decl.signals, &sig.name);
            if !is_data {
                decl.signals.push(sig);
            }
            return Ok(());
        }
        self.reject_class_mods(prefix.is_abstract, prefix.is_final, "field");
        if prefix.vis == Vis::Protected {
            self.error_note(&format!("protected cannot apply to {kind} field"));
        }
        if prefix.attrs.fn_like() {
            self.error_note("attributes cannot apply to field");
        }
        if prefix.attrs.is_onready {
            self.error_note("@onready applies to a class var");
        }
        let field = self.expect(Tok::Identifier, "expected field name")?;
        let ty = self.parse_optional_type()?;
        self.expect(Tok::Semi, "expected ';'")?;
        if decl.fields.iter().any(|name| name == &field.text) {
            self.error_note(&format!("duplicate field '{}'", field.text));
        }
        if decl.methods.iter().any(|method| method.name == field.text) {
            self.error_note(&format!(
                "field '{}' conflicts with method '{}'",
                field.text, field.text
            ));
        }
        if decl.signals.iter().any(|sig| sig.name == field.text) {
            self.error_note(&format!(
                "field '{}' conflicts with signal '{}'",
                field.text, field.text
            ));
        }
        decl.fields.push(field.text);
        decl.field_types.push(ty);
        decl.field_optional.push(prefix.attrs.is_optional);
        decl.field_vis.push(prefix.vis);
        Ok(())
    }

    fn parse_impl_decl(&mut self) -> Result<ImplDecl, ParseError> {
        let impl_tok = self.expect(Tok::Implements, "expected 'impl'")?;
        let mut decl = ImplDecl {
            type_params: self.parse_type_params()?,
            type_name: String::new(),
            trait_name: String::new(),
            methods: Vec::new(),
            span: impl_tok.span,
        };
        let first = self.parse_type()?;
        if self.match_tok(Tok::For) {
            decl.trait_name = first;
            decl.type_name = self.parse_type()?;
        } else {
            decl.type_name = first;
        }
        self.expect(Tok::LBrace, "expected '{'")?;
        while !self.check(Tok::RBrace) && !self.check(Tok::Eof) {
            match self.parse_method(!decl.trait_name.is_empty()) {
                Ok(func) => {
                    self.check_method_name(&[], &decl.methods, &func.name);
                    decl.methods.push(func);
                }
                Err(_) => self.synchronize_stmt(),
            }
        }
        self.expect(Tok::RBrace, "expected '}'")?;
        Ok(decl)
    }

    fn parse_trait_method(&mut self) -> Result<TraitMethod, ParseError> {
        let fn_tok = self.expect(Tok::Function, "expected 'fn'")?;
        let name = self.expect(Tok::Identifier, "expected function name")?;
        self.expect(Tok::LParen, "expected '('")?;
        let mut method = TraitMethod {
            name: name.text,
            params: Vec::new(),
            param_types: Vec::new(),
            return_type: String::new(),
            throws_ex: false,
            span: fn_tok.span,
        };
        if !self.check(Tok::RParen) {
            loop {
                let param = self.expect(Tok::Identifier, "expected parameter name")?;
                method.param_types.push(self.parse_optional_type()?);
                method.params.push(param.text);
                if !self.match_tok(Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(Tok::RParen, "expected ')'")?;
        method.throws_ex = self.match_tok(Tok::Throws);
        method.return_type = self.parse_optional_type()?;
        self.expect(
            Tok::Semi,
            "expected ';' after trait method (signatures only)",
        )?;
        if method.params.first().map(String::as_str) != Some("self") {
            method.params.insert(0, "self".to_string());
            method.param_types.insert(0, String::new());
        }
        Ok(method)
    }

    fn parse_trait(&mut self) -> Result<TraitDecl, ParseError> {
        let trait_tok = self.expect(Tok::Trait, "expected 'trait'")?;
        let name = self.expect(Tok::Identifier, "expected trait name")?;
        let mut decl = TraitDecl {
            name: name.text,
            type_params: self.parse_type_params()?,
            methods: Vec::new(),
            signals: Vec::new(),
            is_pub: true,
            span: trait_tok.span,
        };
        self.expect(Tok::LBrace, "expected '{'")?;
        while !self.check(Tok::RBrace) && !self.check(Tok::Eof) {
            if let Err(_) = self.parse_trait_item(&mut decl) {
                self.synchronize_stmt();
            }
        }
        self.expect(Tok::RBrace, "expected '}'")?;
        Ok(decl)
    }

    fn parse_trait_item(&mut self, decl: &mut TraitDecl) -> Result<(), ParseError> {
        self.match_tok(Tok::Pub);
        if self.check(Tok::Signal) {
            decl.signals.push(self.parse_signal()?);
            return Ok(());
        }
        if self.check(Tok::Function) {
            let method = self.parse_trait_method()?;
            if decl.methods.iter().any(|prev| prev.name == method.name) {
                self.error_note(&format!("duplicate method '{}'", method.name));
            }
            decl.methods.push(method);
            return Ok(());
        }
        if self.check(Tok::Variable) || self.check(Tok::Constant) {
            self.error_note("traits cannot declare vars or consts");
            self.synchronize_stmt();
            return Ok(());
        }
        Err(self.error_here("expected fn signature or signal in trait"))
    }

    fn parse_const_decl(&mut self) -> Result<ConstDecl, ParseError> {
        self.expect(Tok::Constant, "expected 'const'")?;
        let name = self.expect(Tok::Identifier, "expected const name")?;
        let mut decl = ConstDecl {
            name: name.text,
            type_name: self.parse_optional_type()?,
            value: None,
            span: name.span,
        };
        self.expect(Tok::Eq, "expected '='")?;
        decl.value = Some(self.parse_expr()?);
        self.expect(Tok::Semi, "expected ';'")?;
        Ok(decl)
    }

    fn parse_signal(&mut self) -> Result<SignalDecl, ParseError> {
        let sig_tok = self.expect(Tok::Signal, "expected 'signal'")?;
        let name = self.expect(Tok::Identifier, "expected signal name")?;
        self.expect(Tok::LParen, "expected '('")?;
        let mut sig = SignalDecl {
            name: name.text,
            params: Vec::new(),
            param_types: Vec::new(),
            is_pub: true,
            span: sig_tok.span,
        };
        if !self.check(Tok::RParen) {
            loop {
                let param = self.expect(Tok::Identifier, "expected parameter name")?;
                sig.params.push(param.text);
                sig.param_types.push(self.parse_optional_type()?);
                if self.match_tok(Tok::Eq) {
                    let _ = self.parse_expr()?;
                }
                if !self.match_tok(Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(Tok::RParen, "expected ')'")?;
        self.expect(Tok::Semi, "expected ';'")?;
        Ok(sig)
    }

    fn parse_class(&mut self) -> Result<ClassDecl, ParseError> {
        let class_tok = self.expect(Tok::Class, "expected 'class'")?;
        let name = self.expect(Tok::Identifier, "expected class name")?;
        let mut decl = ClassDecl {
            name: name.text.clone(),
            type_params: self.parse_type_params()?,
            parent: String::new(),
            impl_traits: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            trait_impls: Vec::new(),
            signals: Vec::new(),
            consts: Vec::new(),
            shape: blank_struct(&name.text, class_tok.span),
            is_pub: true,
            is_abstract: false,
            is_final: false,
            span: class_tok.span,
        };
        if self.match_tok(Tok::Extends) {
            decl.parent = self.parse_type()?;
        }
        if self.match_tok(Tok::Implements) {
            loop {
                decl.impl_traits.push(self.parse_type()?);
                if !self.match_tok(Tok::Comma) {
                    break;
                }
            }
        }
        self.expect(Tok::LBrace, "expected '{'")?;
        let mut field_names = Vec::new();
        while !self.check(Tok::RBrace) && !self.check(Tok::Eof) {
            if let Err(_) = self.parse_class_item(&mut decl, &mut field_names) {
                self.synchronize_stmt();
            }
        }
        self.expect(Tok::RBrace, "expected '}'")?;
        self.install_onready(&mut decl);
        decl.shape.name = decl.name.clone();
        decl.shape.span = decl.span;
        decl.shape.type_params = decl.type_params.clone();
        for field in &decl.fields {
            decl.shape.fields.push(field.name.clone());
            decl.shape.field_types.push(field.ty.clone());
            decl.shape.field_optional.push(field.optional);
        }
        decl.shape.signals = decl.signals.clone();
        Ok(decl)
    }

    fn install_onready(&mut self, decl: &mut ClassDecl) {
        let ready: Vec<ClassField> = decl
            .fields
            .iter()
            .filter(|f| f.onready && f.has_default && f.default_value.is_some())
            .cloned()
            .collect();
        if ready.is_empty() {
            return;
        }
        let existing = decl
            .methods
            .iter()
            .position(|m| m.name == "_ready" && !m.is_static && !m.is_abstract);
        if existing.is_none() {
            let mut func = FnDecl::new("_ready", decl.span);
            func.generated = true;
            bind_self(&mut func);
            decl.methods.push(func);
        }
        let index = decl
            .methods
            .iter()
            .position(|m| m.name == "_ready" && !m.is_static && !m.is_abstract)
            .unwrap();
        let mut assigns = Vec::new();
        for field in ready {
            let mut stmt = Stmt::new(StmtKind::Assign, field.span);
            stmt.name = field.name;
            stmt.expr = field.default_value;
            stmt.op = "=".to_string();
            stmt.generated = true;
            assigns.push(stmt);
        }
        let body = std::mem::take(&mut decl.methods[index].body);
        decl.methods[index].body = assigns;
        decl.methods[index].body.extend(body);
    }

    fn parse_class_item(
        &mut self,
        decl: &mut ClassDecl,
        field_names: &mut Vec<String>,
    ) -> Result<(), ParseError> {
        let mut prefix = MemberPrefix::default();
        self.parse_member_prefix(&mut prefix)?;
        if prefix.exported && !self.check(Tok::Variable) {
            self.error_note("export applies to var");
        }
        if prefix.is_static && !self.check(Tok::Function) && !self.check(Tok::Async) {
            self.error_note("static applies to fn");
        }
        if self.check(Tok::Constant) {
            if prefix.attrs.any() {
                self.error_note("attributes cannot apply to const");
            }
            self.reject_class_mods(prefix.is_abstract, prefix.is_final, "const");
            if prefix.vis != Vis::Pub {
                self.error_note("const is public");
            }
            let constant = self.parse_const_decl()?;
            if decl.consts.iter().any(|prev| prev.name == constant.name) {
                self.error_note(&format!("duplicate const '{}'", constant.name));
            }
            if field_names.iter().any(|prev| prev == &constant.name) {
                self.error_note(&format!(
                    "const '{}' conflicts with field '{}'",
                    constant.name, constant.name
                ));
            }
            if decl
                .methods
                .iter()
                .any(|method| method.name == constant.name)
            {
                self.error_note(&format!(
                    "const '{}' conflicts with method '{}'",
                    constant.name, constant.name
                ));
            }
            if decl.signals.iter().any(|sig| sig.name == constant.name) {
                self.error_note(&format!(
                    "const '{}' conflicts with signal '{}'",
                    constant.name, constant.name
                ));
            }
            decl.consts.push(constant);
            return Ok(());
        }
        if self.check(Tok::Implements) {
            if prefix.attrs.any() {
                self.error_note("attributes cannot apply to impl");
            }
            self.reject_class_mods(prefix.is_abstract, prefix.is_final, "impl");
            if prefix.saw_vis {
                self.error_note("visibility cannot apply to impl");
            }
            self.expect(Tok::Implements, "expected 'impl'")?;
            let mut block = NestedImpl {
                type_params: self.parse_type_params()?,
                trait_name: String::new(),
                methods: Vec::new(),
            };
            block.trait_name = self.parse_type()?;
            if self.match_tok(Tok::For) {
                self.error_note("impl inside a class is `impl Trait { … }` (no `for`)");
            }
            self.expect(Tok::LBrace, "expected '{' after trait name")?;
            while !self.check(Tok::RBrace) && !self.check(Tok::Eof) {
                let func = self.parse_method(true)?;
                self.check_method_name(field_names, &decl.methods, &func.name);
                for existing in &decl.trait_impls {
                    self.check_method_name(&[], &existing.methods, &func.name);
                }
                block.methods.push(func);
            }
            self.expect(Tok::RBrace, "expected '}'")?;
            decl.trait_impls.push(block);
            return Ok(());
        }
        if self.check(Tok::Function) || self.check(Tok::Async) {
            self.reject_method_attrs(&prefix.attrs);
            if prefix.is_abstract && prefix.is_final {
                self.error_note("method cannot be both abstract and final");
            }
            if prefix.is_abstract && prefix.attrs.is_constexpr {
                self.error_note("@constexpr cannot apply to abstract method");
            }
            let mut func = self.parse_fn(&prefix.attrs, true, prefix.is_abstract)?;
            func.is_final = prefix.is_final;
            func.is_static = prefix.is_static;
            func.vis = prefix.vis;
            if prefix.is_static && prefix.is_abstract {
                self.error_note("static method cannot be abstract");
            }
            if !prefix.is_static {
                bind_self(&mut func);
            }
            let name = func.name.clone();
            self.check_method_name(field_names, &decl.methods, &name);
            for existing in &decl.trait_impls {
                self.check_method_name(&[], &existing.methods, &name);
            }
            if decl.signals.iter().any(|sig| sig.name == name) {
                self.error_note(&format!("method '{name}' conflicts with signal '{name}'"));
            }
            if decl.consts.iter().any(|constant| constant.name == name) {
                self.error_note(&format!("method '{name}' conflicts with const '{name}'"));
            }
            decl.methods.push(func);
            return Ok(());
        }
        if self.check(Tok::Signal) {
            self.reject_class_mods(prefix.is_abstract, prefix.is_final, "signal");
            self.reject_protected(prefix.vis == Vis::Protected, "signal");
            if prefix.attrs.any() {
                self.error_note("attributes cannot apply to signal");
            }
            let sig = self.parse_signal()?;
            self.check_signal_decl(field_names, &decl.methods, &decl.signals, &sig.name);
            for block in &decl.trait_impls {
                if block.methods.iter().any(|method| method.name == sig.name) {
                    self.error_note(&format!(
                        "signal '{}' conflicts with method '{}'",
                        sig.name, sig.name
                    ));
                }
            }
            if decl.consts.iter().any(|constant| constant.name == sig.name) {
                self.error_note(&format!(
                    "signal '{}' conflicts with const '{}'",
                    sig.name, sig.name
                ));
            }
            decl.signals.push(sig);
            return Ok(());
        }
        if !self.check(Tok::Variable) {
            return Err(self.error_here("expected var, const, fn, signal, or impl in class body"));
        }
        self.reject_class_mods(prefix.is_abstract, prefix.is_final, "field");
        if prefix.attrs.fn_like() {
            self.error_note("attributes cannot apply to field");
        }
        self.expect(Tok::Variable, "expected 'var'")?;
        let field_tok = self.expect(Tok::Identifier, "expected field name")?;
        let mut field = ClassField {
            name: field_tok.text.clone(),
            ty: self.parse_optional_type()?,
            has_default: false,
            optional: prefix.attrs.is_optional,
            exported: prefix.exported,
            has_range: prefix.has_range,
            range_min: prefix.range_min,
            range_max: prefix.range_max,
            range_step: prefix.range_step,
            range_suffixes: prefix.range_suffixes,
            has_enum: prefix.has_enum,
            enum_names: prefix.enum_names,
            has_flags: prefix.has_flags,
            flag_names: prefix.flag_names,
            has_file: prefix.has_file,
            file_filters: prefix.file_filters,
            has_dir: prefix.has_dir,
            has_multiline: prefix.has_multiline,
            onready: prefix.attrs.is_onready,
            vis: prefix.vis,
            default_value: None,
            span: field_tok.span,
        };
        if self.match_tok(Tok::Eq) {
            field.has_default = true;
            field.default_value = Some(self.parse_expr()?);
        }
        if field.onready && !field.has_default {
            self.error_note("@onready requires an initializer");
        }
        self.expect(Tok::Semi, "expected ';'")?;
        if field_names.iter().any(|prev| prev == &field.name) {
            self.error_note(&format!("duplicate field '{}'", field.name));
        }
        self.check_method_name(field_names, &decl.methods, &field.name);
        if decl
            .consts
            .iter()
            .any(|constant| constant.name == field.name)
        {
            self.error_note(&format!(
                "field '{}' conflicts with const '{}'",
                field.name, field.name
            ));
        }
        for block in &decl.trait_impls {
            if block.methods.iter().any(|method| method.name == field.name) {
                self.error_note(&format!(
                    "field '{}' conflicts with method '{}'",
                    field.name, field.name
                ));
            }
        }
        if decl.signals.iter().any(|sig| sig.name == field.name) {
            self.error_note(&format!(
                "field '{}' conflicts with signal '{}'",
                field.name, field.name
            ));
        }
        field_names.push(field.name.clone());
        decl.fields.push(field);
        Ok(())
    }

    fn parse_import(&mut self) -> Result<ImportDecl, ParseError> {
        let token = self.peek();
        let mut decl = ImportDecl {
            path: Vec::new(),
            alias: String::new(),
            is_from: false,
            span: token.span,
        };
        if self.match_tok(Tok::From) {
            decl.is_from = true;
            let module = self.expect(Tok::Identifier, "expected module name")?;
            self.expect(Tok::Import, "expected 'import' after module name")?;
            let item = self.expect(Tok::Identifier, "expected imported name")?;
            decl.path.push(module.text);
            decl.path.push(item.text);
            while self.match_tok(Tok::Dot) {
                decl.path.push(
                    self.expect(Tok::Identifier, "expected name after '.'")?
                        .text,
                );
            }
            if self.match_tok(Tok::As) {
                decl.alias = self.expect(Tok::Identifier, "expected alias")?.text;
            }
            self.expect(Tok::Semi, "expected ';' after import")?;
            return Ok(decl);
        }
        self.expect(Tok::Import, "expected 'import'")?;
        decl.path
            .push(self.expect(Tok::Identifier, "expected module name")?.text);
        while self.match_tok(Tok::Dot) {
            decl.path.push(
                self.expect(Tok::Identifier, "expected module name after '.'")?
                    .text,
            );
        }
        if self.match_tok(Tok::As) {
            decl.alias = self.expect(Tok::Identifier, "expected alias")?.text;
        }
        self.expect(Tok::Semi, "expected ';' after import")?;
        Ok(decl)
    }

    fn parse_decl(&mut self, in_mod: bool) -> Result<(Decl, Vec<String>), ParseError> {
        let mut leading = self.take_leading_comments();
        if !in_mod && self.check(Tok::Identifier) && self.peek().text == "singleton" {
            self.advance();
            let name = self.expect(Tok::Identifier, "expected singleton name")?;
            self.expect(Tok::Semi, "expected ';' after singleton")?;
            return Ok((Decl::Singleton(name.text), leading));
        }
        let mut is_pub = !in_mod;
        let mut is_abstract = false;
        let mut is_final = false;
        let mut is_protected = false;
        let mut saw_vis = false;
        let mut attrs = FnAttrs::default();
        loop {
            if self.parse_one_attr(&mut attrs)? {
                continue;
            }
            if self.match_tok(Tok::Pub) {
                if saw_vis {
                    self.error_note("cannot combine visibility modifiers");
                }
                saw_vis = true;
                is_pub = true;
                is_protected = false;
                continue;
            }
            if self.match_tok(Tok::Private) {
                if saw_vis {
                    self.error_note("cannot combine visibility modifiers");
                }
                saw_vis = true;
                is_pub = false;
                is_protected = false;
                continue;
            }
            if self.match_tok(Tok::Protected) {
                if saw_vis {
                    self.error_note("cannot combine visibility modifiers");
                }
                saw_vis = true;
                is_protected = true;
                continue;
            }
            if self.match_tok(Tok::Abstract) {
                is_abstract = true;
                continue;
            }
            if self.match_tok(Tok::Final) {
                is_final = true;
                continue;
            }
            break;
        }
        let (decl, _) = self.finish_decl(
            in_mod,
            is_pub,
            is_abstract,
            is_final,
            is_protected,
            saw_vis,
            &attrs,
        )?;
        Ok((decl, leading))
    }

    fn finish_decl(
        &mut self,
        in_mod: bool,
        is_pub: bool,
        is_abstract: bool,
        is_final: bool,
        is_protected: bool,
        saw_vis: bool,
        attrs: &FnAttrs,
    ) -> Result<(Decl, Vec<String>), ParseError> {
        if self.check(Tok::Import) || self.check(Tok::From) {
            if attrs.any() {
                self.error_note("attributes cannot apply to import");
            }
            self.reject_class_mods(is_abstract, is_final, "import");
            self.reject_protected(is_protected, "import");
            return Ok((Decl::Import(self.parse_import()?), Vec::new()));
        }
        if self.check(Tok::Module) {
            if attrs.any() {
                self.error_note("attributes cannot apply to mod");
            }
            self.reject_class_mods(is_abstract, is_final, "mod");
            self.reject_protected(is_protected, "mod");
            let mut nested = self.parse_mod()?;
            nested.is_pub = is_pub;
            return Ok((Decl::Mod(nested), Vec::new()));
        }
        if self.check(Tok::Struct) || self.check(Tok::Data) {
            let is_data = self.check(Tok::Data);
            let kind = if is_data { "data" } else { "struct" };
            self.reject_item_attrs(attrs, kind);
            self.reject_class_mods(is_abstract, is_final, kind);
            self.reject_protected(is_protected, kind);
            let mut decl = self.parse_struct(is_data)?;
            decl.is_pub = is_pub;
            return Ok((Decl::Struct(decl), Vec::new()));
        }
        if self.check(Tok::Class) {
            self.reject_item_attrs(attrs, "class");
            if is_abstract && is_final {
                self.error_note("class cannot be both abstract and final");
            }
            self.reject_protected(is_protected, "class");
            let mut decl = self.parse_class()?;
            decl.is_pub = is_pub;
            decl.is_abstract = is_abstract;
            decl.is_final = is_final;
            decl.shape.is_pub = is_pub;
            return Ok((Decl::Class(decl), Vec::new()));
        }
        if self.check(Tok::Trait) {
            self.reject_item_attrs(attrs, "trait");
            self.reject_class_mods(is_abstract, is_final, "trait");
            self.reject_protected(is_protected, "trait");
            let mut decl = self.parse_trait()?;
            decl.is_pub = is_pub;
            return Ok((Decl::Trait(decl), Vec::new()));
        }
        if self.check(Tok::Enum) {
            self.reject_item_attrs(attrs, "enum");
            self.reject_class_mods(is_abstract, is_final, "enum");
            self.reject_protected(is_protected, "enum");
            let mut decl = self.parse_enum()?;
            decl.is_pub = is_pub;
            return Ok((Decl::Enum(decl), Vec::new()));
        }
        if self.check(Tok::Implements) {
            if attrs.any() {
                self.error_note("attributes cannot apply to impl");
            }
            self.reject_class_mods(is_abstract, is_final, "impl");
            self.reject_protected(is_protected, "impl");
            return Ok((Decl::Impl(self.parse_impl_decl()?), Vec::new()));
        }
        if self.check(Tok::Signal) {
            if attrs.any() {
                self.error_note("attributes cannot apply to signal");
            }
            self.reject_class_mods(is_abstract, is_final, "signal");
            self.reject_protected(is_protected, "signal");
            let mut sig = self.parse_signal()?;
            sig.is_pub = is_pub;
            return Ok((Decl::Signal(sig), Vec::new()));
        }
        if self.check(Tok::Constant) {
            if attrs.any() {
                self.error_note("attributes cannot apply to const");
            }
            self.reject_class_mods(is_abstract, is_final, "const");
            self.reject_protected(is_protected, "const");
            if saw_vis && !is_pub {
                self.error_note("private cannot apply to const");
            }
            let _ = in_mod;
            return Ok((Decl::Const(self.parse_const_decl()?), Vec::new()));
        }
        self.reject_class_mods(is_abstract, is_final, "function");
        self.reject_protected(is_protected, "function");
        let func = self.parse_fn(attrs, is_pub, false)?;
        if attrs.is_optional {
            self.error_note("@optional cannot apply to function");
        }
        if attrs.is_onready {
            self.error_note("@onready applies to a class var");
        }
        Ok((Decl::Fn(func), Vec::new()))
    }

    fn reject_item_attrs(&mut self, attrs: &FnAttrs, kind: &str) {
        if attrs.is_test {
            self.error_note(&format!("@test cannot apply to {kind}"));
        }
        if attrs.is_constexpr {
            self.error_note(&format!("@constexpr cannot apply to {kind}"));
        }
        if attrs.is_ufcs {
            self.error_note(&format!("@ufcs cannot apply to {kind}"));
        }
        if attrs.is_optional {
            self.error_note(&format!("@optional cannot apply to {kind}"));
        }
        if attrs.is_onready {
            self.error_note("@onready applies to a class var");
        }
    }

    fn parse_mod(&mut self) -> Result<ModDecl, ParseError> {
        let mod_tok = self.expect(Tok::Module, "expected 'mod'")?;
        let name = self.expect(Tok::Identifier, "expected module name")?;
        let mut decl = ModDecl {
            name: name.text,
            span: mod_tok.span,
            is_pub: true,
            ..ModDecl::default()
        };
        self.expect(Tok::LBrace, "expected '{' after module name")?;
        loop {
            let j = self.next_significant(self.i);
            if j >= self.tokens.len()
                || self.tokens[j].kind == Tok::Eof
                || self.tokens[j].kind == Tok::RBrace
            {
                break;
            }
            match self.parse_decl(true) {
                Ok((item, leading)) => decl.push(item, leading),
                Err(_) => self.synchronize_item(),
            }
        }
        decl.trailing_comments = self.take_leading_comments();
        self.expect(Tok::RBrace, "expected '}' after module body")?;
        Ok(decl)
    }

    fn parse(&mut self) -> Program {
        let mut program = Program::default();
        loop {
            let j = self.next_significant(self.i);
            if j >= self.tokens.len() || self.tokens[j].kind == Tok::Eof {
                break;
            }
            match self.parse_decl(false) {
                Ok((decl, leading)) => program.push(decl, leading),
                Err(_) => self.synchronize_item(),
            }
        }
        program.trailing_comments = self.take_leading_comments();
        program
    }
}
