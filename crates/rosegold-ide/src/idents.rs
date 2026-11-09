use rosegold_syntax::{tokenize, Tok, Token};

use crate::pos::{self, is_ws};

#[derive(Clone, Debug)]
pub struct Ident {
    pub name: String,
    pub qualifier: String,
    pub start: usize,
    pub end: usize,
}

pub fn at(source: &str, offset: usize) -> Option<Ident> {
    let tokens = tokens_of(source);
    let (line1, col1) = pos::line_col(source, offset);
    for i in 0..tokens.len() {
        let t = &tokens[i];
        if t.kind != Tok::Identifier && t.kind != Tok::True && t.kind != Tok::False {
            continue;
        }
        if t.line() != line1 {
            continue;
        }
        let start = t.col();
        let end = t.col() + t.text.chars().count() as u32;
        if col1 < start || col1 > end {
            continue;
        }
        let mut ident = Ident {
            name: t.text.clone(),
            qualifier: String::new(),
            start: pos::offset(source, t.line(), t.col()),
            end: 0,
        };
        ident.end = ident.start + t.text.chars().count();
        if i >= 2 && tokens[i - 1].kind == Tok::Dot && tokens[i - 2].kind == Tok::Identifier {
            ident.qualifier = tokens[i - 2].text.clone();
        }
        return Some(ident);
    }
    None
}

pub fn ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub fn ltrim(s: &str) -> &str {
    for (idx, c) in s.char_indices() {
        if !is_ws(c) {
            return &s[idx..];
        }
    }
    ""
}

pub fn after_import_or_from(prefix: &str) -> bool {
    let line = ltrim(prefix);
    line.starts_with("import ") || line == "import" || line.starts_with("from ") || line == "from"
}

pub fn from_import_crate(prefix: &str) -> String {
    let line = ltrim(prefix);
    if !line.starts_with("from ") {
        return String::new();
    }
    let rest: Vec<char> = ltrim(&line[5..]).chars().collect();
    let mut i = 0usize;
    while i < rest.len() {
        let c = rest[i];
        if ident_char(c) {
            i += 1;
        } else if c == '.' && i + 1 < rest.len() && ident_char(rest[i + 1]) {
            i += 1;
        } else {
            break;
        }
    }
    if i == 0 {
        return String::new();
    }
    let mut j = i;
    while j < rest.len() && is_ws(rest[j]) {
        j += 1;
    }
    if j + 6 > rest.len() {
        return String::new();
    }
    let word: String = rest[j..j + 6].iter().collect();
    if word != "import" {
        return String::new();
    }
    if j + 6 < rest.len() && ident_char(rest[j + 6]) {
        return String::new();
    }
    rest[..i].iter().collect()
}

pub fn name_token(source: &str, line: u32, name: &str) -> Option<Token> {
    if name.is_empty() {
        return None;
    }
    for t in tokens_of(source) {
        if t.line() == line && t.kind == Tok::Identifier && t.text == name {
            return Some(t);
        }
    }
    None
}

pub fn tokens_of(source: &str) -> Vec<Token> {
    let mut junk = Vec::new();
    tokenize(source, "", &mut junk)
}

pub fn line_prefix(source: &str, offset: usize) -> String {
    let chars: Vec<char> = source.chars().collect();
    let offset = offset.min(chars.len());
    let mut start = offset;
    while start > 0 && chars[start - 1] != '\n' {
        start -= 1;
    }
    chars[start..offset].iter().collect()
}
