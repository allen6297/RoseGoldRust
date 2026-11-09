/// Source position. `line` and `col` are 1-based, matching the Java lexer.
/// `offset` is a byte index into the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub offset: usize,
}

impl Span {
    pub fn new(line: u32, col: u32, offset: usize) -> Self {
        Self { line, col, offset }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tok {
    Eof,
    Identifier,
    Integer,
    Float,
    String,
    FString,
    LineComment,
    BlockComment,
    Module,
    Class,
    Trait,
    Enum,
    Function,
    Struct,
    Data,
    Implements,
    Extends,
    For,
    In,
    Super,
    Import,
    From,
    As,
    Is,
    Pub,
    Abstract,
    Final,
    Static,
    Private,
    Protected,
    Variable,
    Constant,
    Export,
    Signal,
    Return,
    Pass,
    Continue,
    Break,
    If,
    Elif,
    Else,
    Switch,
    Match,
    While,
    Try,
    Do,
    Throws,
    Throw,
    Catch,
    Async,
    Await,
    True,
    False,
    Null,
    At,
    Dollar,
    Colon,
    Dot,
    Comma,
    Semi,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LArrow,
    RArrow,
    LtEq,
    GtEq,
    DotDot,
    DotDotEq,
    Ellipsis,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eq,
    EqEq,
    NotEq,
    PlusEq,
    MinusEq,
    StarEq,
    SlashEq,
    PercentEq,
    AmpEq,
    PipeEq,
    CaretEq,
    ShlEq,
    ShrEq,
    AndAnd,
    OrOr,
    Amp,
    Pipe,
    Caret,
    Tilde,
    Shl,
    Shr,
    Bang,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: Tok,
    pub text: String,
    pub number: i64,
    pub real: f64,
    pub span: Span,
    pub texts: Vec<String>,
    pub holes: Vec<String>,
    pub hole_line: Vec<u32>,
    pub hole_col: Vec<u32>,
}

impl Token {
    pub fn new(kind: Tok, text: impl Into<String>, span: Span) -> Self {
        Self {
            kind,
            text: text.into(),
            number: 0,
            real: 0.0,
            span,
            texts: Vec::new(),
            holes: Vec::new(),
            hole_line: Vec::new(),
            hole_col: Vec::new(),
        }
    }

    pub fn eof(span: Span) -> Self {
        Self::new(Tok::Eof, "", span)
    }

    pub fn line(&self) -> u32 {
        self.span.line
    }

    pub fn col(&self) -> u32 {
        self.span.col
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub file: String,
    pub line: u32,
    pub col: u32,
    pub severity: String,
    pub message: String,
    pub kind: String,
}

impl Diagnostic {
    pub fn new(
        file: impl Into<String>,
        line: u32,
        col: u32,
        severity: impl Into<String>,
        message: impl Into<String>,
        kind: impl Into<String>,
    ) -> Self {
        Self {
            file: file.into(),
            line: line.max(1),
            col: col.max(1),
            severity: severity.into(),
            message: message.into(),
            kind: kind.into(),
        }
    }

    pub fn parse_error(file: &str, line: u32, col: u32, message: impl Into<String>) -> Self {
        Self::new(file, line, col, "error", message, "parse error")
    }

    pub fn to_human(&self) -> String {
        let mut out = String::new();
        if !self.file.is_empty() {
            out.push_str(&self.file);
            out.push(':');
        }
        out.push_str(&format!(
            "{}:{}: {}: {}",
            self.line, self.col, self.severity, self.message
        ));
        out
    }
}
