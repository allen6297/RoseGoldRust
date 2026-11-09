use crate::token::{Diagnostic, Span, Tok, Token};

struct Lexer<'a> {
    chars: Vec<char>,
    file: &'a str,
    errors: &'a mut Vec<Diagnostic>,
    i: usize,
    line: u32,
    col: u32,
    offset: usize,
}

pub fn tokenize(source: &str, file: &str, errors: &mut Vec<Diagnostic>) -> Vec<Token> {
    let mut lexer = Lexer {
        chars: source.chars().collect(),
        file,
        errors,
        i: 0,
        line: 1,
        col: 1,
        offset: 0,
    };
    let mut tokens = Vec::new();
    loop {
        match lexer.next() {
            Ok(token) => {
                let done = token.kind == Tok::Eof;
                tokens.push(token);
                if done {
                    break;
                }
            }
            Err(()) => {
                tokens.push(Token::eof(lexer.span_here()));
                break;
            }
        }
    }
    tokens
}

impl<'a> Lexer<'a> {
    fn span_here(&self) -> Span {
        Span::new(self.line, self.col, self.offset)
    }

    fn span_at(&self, line: u32, col: u32, offset: usize) -> Span {
        Span::new(line, col, offset)
    }

    fn record(&mut self, msg: &str, line: u32, col: u32) {
        let d = Diagnostic::parse_error(self.file, line, col, msg);
        if self.errors.iter().any(|prev| {
            prev.file == d.file
                && prev.line == d.line
                && prev.col == d.col
                && prev.message == d.message
        }) {
            return;
        }
        self.errors.push(d);
    }

    fn fail<T>(&mut self, msg: &str, line: u32, col: u32) -> Result<T, ()> {
        self.record(msg, line, col);
        self.i = self.chars.len();
        Err(())
    }

    fn peek_at(&self, off: usize) -> char {
        self.chars.get(self.i + off).copied().unwrap_or('\0')
    }

    fn peek(&self) -> char {
        self.peek_at(0)
    }

    fn advance(&mut self) -> char {
        let c = self.chars[self.i];
        self.i += 1;
        self.offset += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        c
    }

    fn skip(&mut self) {
        while matches!(self.peek(), ' ' | '\t' | '\n' | '\r' | '\u{000c}') {
            self.advance();
        }
    }

    fn line_comment(&mut self, line: u32, col: u32, offset: usize) -> Token {
        let mut text = String::new();
        text.push(self.advance());
        loop {
            text.push(self.advance());
            if self.peek() == '\0' || self.peek() == '\n' {
                break;
            }
        }
        Token::new(Tok::LineComment, text, self.span_at(line, col, offset))
    }

    fn hash_line_comment(&mut self, line: u32, col: u32, offset: usize) -> Token {
        let mut text = String::new();
        loop {
            text.push(self.advance());
            if self.peek() == '\0' || self.peek() == '\n' {
                break;
            }
        }
        Token::new(Tok::LineComment, text, self.span_at(line, col, offset))
    }

    fn block_comment(&mut self, line: u32, col: u32, offset: usize) -> Result<Token, ()> {
        let mut text = String::new();
        text.push(self.advance());
        text.push(self.advance());
        loop {
            if self.peek() == '\0' {
                return self.fail("unterminated block comment", line, col);
            }
            if self.peek() == '#' && self.peek_at(1) == '/' {
                text.push(self.advance());
                text.push(self.advance());
                break;
            }
            text.push(self.advance());
        }
        Ok(Token::new(
            Tok::BlockComment,
            text,
            self.span_at(line, col, offset),
        ))
    }

    fn ident_or_kw(&mut self, line: u32, col: u32, offset: usize) -> Token {
        let mut text = String::new();
        while is_alnum(self.peek()) || self.peek() == '_' {
            text.push(self.advance());
        }
        let kind = match text.as_str() {
            "fn" => Tok::Function,
            "import" => Tok::Import,
            "from" => Tok::From,
            "as" => Tok::As,
            "is" => Tok::Is,
            "pub" => Tok::Pub,
            "abstract" => Tok::Abstract,
            "final" => Tok::Final,
            "static" => Tok::Static,
            "private" => Tok::Private,
            "protected" => Tok::Protected,
            "mod" => Tok::Module,
            "class" => Tok::Class,
            "trait" => Tok::Trait,
            "extends" => Tok::Extends,
            "for" => Tok::For,
            "in" => Tok::In,
            "super" => Tok::Super,
            "enum" => Tok::Enum,
            "match" => Tok::Match,
            "switch" => Tok::Switch,
            "var" => Tok::Variable,
            "const" => Tok::Constant,
            "export" => Tok::Export,
            "struct" => Tok::Struct,
            "data" => Tok::Data,
            "impl" => Tok::Implements,
            "signal" => Tok::Signal,
            "return" => Tok::Return,
            "pass" => Tok::Pass,
            "break" => Tok::Break,
            "continue" => Tok::Continue,
            "if" => Tok::If,
            "elif" => Tok::Elif,
            "else" => Tok::Else,
            "while" => Tok::While,
            "try" => Tok::Try,
            "do" => Tok::Do,
            "throws" => Tok::Throws,
            "throw" => Tok::Throw,
            "catch" => Tok::Catch,
            "async" => Tok::Async,
            "await" => Tok::Await,
            "true" => Tok::True,
            "false" => Tok::False,
            "null" => Tok::Null,
            _ => Tok::Identifier,
        };
        Token::new(kind, text, self.span_at(line, col, offset))
    }

    fn string_lit(&mut self, line: u32, col: u32, offset: usize) -> Result<Token, ()> {
        self.advance();
        let mut text = String::new();
        loop {
            let c = self.peek();
            if c == '\0' {
                return self.fail("unterminated string", line, col);
            }
            if c == '"' {
                self.advance();
                break;
            }
            if c == '\\' {
                text.push_str(&self.read_escape(line, col)?);
            } else {
                text.push(self.advance());
            }
        }
        Ok(Token::new(
            Tok::String,
            text,
            self.span_at(line, col, offset),
        ))
    }

    fn f_string(&mut self, line: u32, col: u32, offset: usize) -> Result<Token, ()> {
        self.advance();
        self.advance();
        let mut token = Token::new(Tok::FString, "", self.span_at(line, col, offset));
        let mut lit = String::new();
        loop {
            let c = self.peek();
            if c == '\0' {
                return self.fail("unterminated string", line, col);
            }
            if c == '"' {
                self.advance();
                break;
            }
            if c == '\\' {
                lit.push_str(&self.read_escape(line, col)?);
                continue;
            }
            if c == '{' {
                if self.peek_at(1) == '{' {
                    self.advance();
                    self.advance();
                    lit.push('{');
                    continue;
                }
                self.advance();
                token.texts.push(std::mem::take(&mut lit));
                token.hole_line.push(self.line);
                token.hole_col.push(self.col);
                token.holes.push(self.read_hole()?);
                continue;
            }
            if c == '}' {
                if self.peek_at(1) == '}' {
                    self.advance();
                    self.advance();
                    lit.push('}');
                    continue;
                }
                return self.fail("unclosed format value", self.line, self.col);
            }
            lit.push(self.advance());
        }
        token.texts.push(lit);
        Ok(token)
    }

    fn read_hole(&mut self) -> Result<String, ()> {
        let hole_line = self.line;
        let hole_col = self.col;
        let mut hole = String::new();
        let mut depth = 1i32;
        while depth > 0 {
            let c = self.peek();
            if c == '\0' {
                return self.fail("unclosed format value", hole_line, hole_col);
            }
            if c == '"' {
                hole.push_str(&self.read_raw_string()?);
                continue;
            }
            if c == '/' && self.peek_at(1) == '/' {
                while self.peek() != '\0' && self.peek() != '\n' {
                    hole.push(self.advance());
                }
                continue;
            }
            if c == '#' {
                while self.peek() != '\0' && self.peek() != '\n' {
                    hole.push(self.advance());
                }
                continue;
            }
            if c == '/' && self.peek_at(1) == '#' {
                hole.push(self.advance());
                hole.push(self.advance());
                loop {
                    if self.peek() == '\0' {
                        return self.fail("unterminated block comment", hole_line, hole_col);
                    }
                    if self.peek() == '#' && self.peek_at(1) == '/' {
                        hole.push(self.advance());
                        hole.push(self.advance());
                        break;
                    }
                    hole.push(self.advance());
                }
                continue;
            }
            if c == '{' {
                depth += 1;
                hole.push(self.advance());
                continue;
            }
            if c == '}' {
                depth -= 1;
                self.advance();
                if depth == 0 {
                    break;
                }
                hole.push('}');
                continue;
            }
            hole.push(self.advance());
        }
        if hole.trim().is_empty() {
            return self.fail("empty format value", hole_line, hole_col);
        }
        Ok(hole)
    }

    fn read_raw_string(&mut self) -> Result<String, ()> {
        let mut s = String::new();
        s.push(self.advance());
        loop {
            let c = self.peek();
            if c == '\0' {
                return self.fail("unterminated string", self.line, self.col);
            }
            if c == '\\' {
                s.push(self.advance());
                if self.peek() == '\0' {
                    return self.fail("unterminated string", self.line, self.col);
                }
                s.push(self.advance());
                continue;
            }
            s.push(self.advance());
            if c == '"' {
                break;
            }
        }
        Ok(s)
    }

    fn read_escape(&mut self, line: u32, col: u32) -> Result<String, ()> {
        self.advance();
        let e = self.peek();
        if e == '\0' {
            return self.fail("unterminated string", line, col);
        }
        self.advance();
        if e == 'n' {
            return Ok("\n".to_string());
        }
        if e == 't' {
            return Ok("\t".to_string());
        }
        if e == 'r' {
            return Ok("\r".to_string());
        }
        if e == 'x' {
            let d1 = hex(self.peek());
            if d1 < 0 {
                return self.fail("invalid \\x escape", line, col);
            }
            self.advance();
            let d2 = hex(self.peek());
            if d2 < 0 {
                return self.fail("invalid \\x escape", line, col);
            }
            self.advance();
            return Ok(char::from_u32(((d1 as u32) << 4) | d2 as u32)
                .unwrap_or('\0')
                .to_string());
        }
        Ok(e.to_string())
    }

    fn next(&mut self) -> Result<Token, ()> {
        self.skip();
        let start_line = self.line;
        let start_col = self.col;
        let start_off = self.offset;
        let c = self.peek();
        if c == '\0' {
            return Ok(Token::eof(self.span_at(start_line, start_col, start_off)));
        }
        if c == '/' && self.peek_at(1) == '/' {
            return Ok(self.line_comment(start_line, start_col, start_off));
        }
        if c == '#' {
            return Ok(self.hash_line_comment(start_line, start_col, start_off));
        }
        if c == '/' && self.peek_at(1) == '#' {
            return self.block_comment(start_line, start_col, start_off);
        }
        if c == 'f' && self.peek_at(1) == '"' {
            return self.f_string(start_line, start_col, start_off);
        }
        if is_alpha(c) || c == '_' {
            return Ok(self.ident_or_kw(start_line, start_col, start_off));
        }
        if is_digit(c) {
            let mut digits = String::new();
            let mut overflowed = false;
            overflowed = self.take_digits(&mut digits, overflowed);
            if self.peek() == '.' && is_digit(self.peek_at(1)) {
                digits.push(self.advance());
                overflowed = self.take_digits(&mut digits, overflowed);
                let mut token = Token::new(
                    Tok::Float,
                    digits.clone(),
                    self.span_at(start_line, start_col, start_off),
                );
                if overflowed {
                    self.record("float literal too large", start_line, start_col);
                } else {
                    match digits.parse::<f64>() {
                        Ok(real) if real.is_finite() => token.real = real,
                        _ => self.record("float literal too large", start_line, start_col),
                    }
                }
                return Ok(token);
            }
            let mut token = Token::new(
                Tok::Integer,
                digits.clone(),
                self.span_at(start_line, start_col, start_off),
            );
            if overflowed || digits.len() > 19 {
                self.record("integer literal too large", start_line, start_col);
            } else {
                match digits.parse::<i64>() {
                    Ok(n) => token.number = n,
                    Err(_) => self.record("integer literal too large", start_line, start_col),
                }
            }
            return Ok(token);
        }
        if c == '"' {
            return self.string_lit(start_line, start_col, start_off);
        }
        self.advance();
        let n = self.peek();
        if c == '=' && n == '=' {
            self.advance();
            return Ok(self.make(Tok::EqEq, "==", start_line, start_col, start_off));
        }
        if c == '!' && n == '=' {
            self.advance();
            return Ok(self.make(Tok::NotEq, "!=", start_line, start_col, start_off));
        }
        if c == '<' && n == '<' && self.peek_at(1) == '=' {
            self.advance();
            self.advance();
            return Ok(self.make(Tok::ShlEq, "<<=", start_line, start_col, start_off));
        }
        if c == '<' && n == '<' {
            self.advance();
            return Ok(self.make(Tok::Shl, "<<", start_line, start_col, start_off));
        }
        if c == '<' && n == '=' {
            self.advance();
            return Ok(self.make(Tok::LtEq, "<=", start_line, start_col, start_off));
        }
        if c == '>' && n == '>' && self.peek_at(1) == '=' {
            self.advance();
            self.advance();
            return Ok(self.make(Tok::ShrEq, ">>=", start_line, start_col, start_off));
        }
        if c == '>' && n == '>' {
            self.advance();
            return Ok(self.make(Tok::Shr, ">>", start_line, start_col, start_off));
        }
        if c == '>' && n == '=' {
            self.advance();
            return Ok(self.make(Tok::GtEq, ">=", start_line, start_col, start_off));
        }
        if c == '&' && n == '&' {
            self.advance();
            return Ok(self.make(Tok::AndAnd, "&&", start_line, start_col, start_off));
        }
        if c == '&' && n == '=' {
            self.advance();
            return Ok(self.make(Tok::AmpEq, "&=", start_line, start_col, start_off));
        }
        if c == '&' {
            return Ok(self.make(Tok::Amp, "&", start_line, start_col, start_off));
        }
        if c == '|' && n == '|' {
            self.advance();
            return Ok(self.make(Tok::OrOr, "||", start_line, start_col, start_off));
        }
        if c == '|' && n == '=' {
            self.advance();
            return Ok(self.make(Tok::PipeEq, "|=", start_line, start_col, start_off));
        }
        if c == '|' {
            return Ok(self.make(Tok::Pipe, "|", start_line, start_col, start_off));
        }
        if c == '^' && n == '=' {
            self.advance();
            return Ok(self.make(Tok::CaretEq, "^=", start_line, start_col, start_off));
        }
        if c == '^' {
            return Ok(self.make(Tok::Caret, "^", start_line, start_col, start_off));
        }
        if c == '~' {
            return Ok(self.make(Tok::Tilde, "~", start_line, start_col, start_off));
        }
        if c == '+' && n == '=' {
            self.advance();
            return Ok(self.make(Tok::PlusEq, "+=", start_line, start_col, start_off));
        }
        if c == '-' && n == '=' {
            self.advance();
            return Ok(self.make(Tok::MinusEq, "-=", start_line, start_col, start_off));
        }
        if c == '*' && n == '=' {
            self.advance();
            return Ok(self.make(Tok::StarEq, "*=", start_line, start_col, start_off));
        }
        if c == '/' && n == '=' {
            self.advance();
            return Ok(self.make(Tok::SlashEq, "/=", start_line, start_col, start_off));
        }
        if c == '%' && n == '=' {
            self.advance();
            return Ok(self.make(Tok::PercentEq, "%=", start_line, start_col, start_off));
        }
        if c == '.' && n == '.' {
            self.advance();
            if self.peek() == '=' {
                self.advance();
                return Ok(self.make(Tok::DotDotEq, "..=", start_line, start_col, start_off));
            }
            if self.peek() == '.' {
                self.advance();
                return Ok(self.make(Tok::Ellipsis, "...", start_line, start_col, start_off));
            }
            return Ok(self.make(Tok::DotDot, "..", start_line, start_col, start_off));
        }
        let made = match c {
            '@' => Some(self.make(Tok::At, "@", start_line, start_col, start_off)),
            '$' => Some(self.make(Tok::Dollar, "$", start_line, start_col, start_off)),
            ':' => Some(self.make(Tok::Colon, ":", start_line, start_col, start_off)),
            '.' => Some(self.make(Tok::Dot, ".", start_line, start_col, start_off)),
            ',' => Some(self.make(Tok::Comma, ",", start_line, start_col, start_off)),
            ';' => Some(self.make(Tok::Semi, ";", start_line, start_col, start_off)),
            '(' => Some(self.make(Tok::LParen, "(", start_line, start_col, start_off)),
            ')' => Some(self.make(Tok::RParen, ")", start_line, start_col, start_off)),
            '{' => Some(self.make(Tok::LBrace, "{", start_line, start_col, start_off)),
            '}' => Some(self.make(Tok::RBrace, "}", start_line, start_col, start_off)),
            '[' => Some(self.make(Tok::LBracket, "[", start_line, start_col, start_off)),
            ']' => Some(self.make(Tok::RBracket, "]", start_line, start_col, start_off)),
            '+' => Some(self.make(Tok::Plus, "+", start_line, start_col, start_off)),
            '-' => Some(self.make(Tok::Minus, "-", start_line, start_col, start_off)),
            '*' => Some(self.make(Tok::Star, "*", start_line, start_col, start_off)),
            '/' => Some(self.make(Tok::Slash, "/", start_line, start_col, start_off)),
            '%' => Some(self.make(Tok::Percent, "%", start_line, start_col, start_off)),
            '=' => Some(self.make(Tok::Eq, "=", start_line, start_col, start_off)),
            '<' => Some(self.make(Tok::LArrow, "<", start_line, start_col, start_off)),
            '>' => Some(self.make(Tok::RArrow, ">", start_line, start_col, start_off)),
            '!' => Some(self.make(Tok::Bang, "!", start_line, start_col, start_off)),
            _ => None,
        };
        if let Some(token) = made {
            return Ok(token);
        }
        self.record(
            &format!("unexpected character '{c}'"),
            start_line,
            start_col,
        );
        if self.i < self.chars.len() {
            self.advance();
        }
        self.next()
    }

    fn take_digits(&mut self, digits: &mut String, mut overflowed: bool) -> bool {
        while is_digit(self.peek()) {
            if digits.len() >= 256 {
                overflowed = true;
                while is_digit(self.peek()) {
                    self.advance();
                }
                break;
            }
            digits.push(self.advance());
        }
        overflowed
    }

    fn make(&self, kind: Tok, text: &str, line: u32, col: u32, offset: usize) -> Token {
        Token::new(kind, text, self.span_at(line, col, offset))
    }
}

fn hex(h: char) -> i32 {
    match h {
        '0'..='9' => h as i32 - '0' as i32,
        'a'..='f' => h as i32 - 'a' as i32 + 10,
        'A'..='F' => h as i32 - 'A' as i32 + 10,
        _ => -1,
    }
}

fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
}

fn is_alpha(c: char) -> bool {
    c.is_ascii_alphabetic()
}

fn is_alnum(c: char) -> bool {
    is_alpha(c) || is_digit(c)
}
