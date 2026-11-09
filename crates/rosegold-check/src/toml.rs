use rosegold_syntax::Diagnostic;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub enum Kind {
    String,
    Bool,
    Table,
}

#[derive(Clone, Debug)]
pub struct Value {
    pub kind: Kind,
    pub s: String,
    pub b: bool,
    pub table: HashMap<String, Value>,
    pub order: Vec<String>,
    pub line: u32,
    pub col: u32,
}

impl Value {
    fn string(s: String, line: u32, col: u32) -> Self {
        Self {
            kind: Kind::String,
            s,
            b: false,
            table: HashMap::new(),
            order: Vec::new(),
            line,
            col,
        }
    }

    fn bool(b: bool, line: u32, col: u32) -> Self {
        Self {
            kind: Kind::Bool,
            s: String::new(),
            b,
            table: HashMap::new(),
            order: Vec::new(),
            line,
            col,
        }
    }

    fn table(line: u32, col: u32) -> Self {
        Self {
            kind: Kind::Table,
            s: String::new(),
            b: false,
            table: HashMap::new(),
            order: Vec::new(),
            line,
            col,
        }
    }

    pub fn is_table(&self) -> bool {
        matches!(self.kind, Kind::Table)
    }

    pub fn is_string(&self) -> bool {
        matches!(self.kind, Kind::String)
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.table.get(key)
    }

    pub fn keys(&self) -> &[String] {
        &self.order
    }
}

pub fn parse(source: &str, file: &str) -> Result<Value, Diagnostic> {
    Parser {
        src: source,
        file,
        i: 0,
        line: 1,
        col: 1,
    }
    .parse()
}

struct Parser<'a> {
    src: &'a str,
    file: &'a str,
    i: usize,
    line: u32,
    col: u32,
}

impl<'a> Parser<'a> {
    fn parse(mut self) -> Result<Value, Diagnostic> {
        let mut root = Value::table(1, 1);
        let mut current = String::new();
        self.skip()?;
        while self.i < self.src.len() {
            if self.char_at() == Some('[') {
                current = self.header(&mut root)?;
                self.skip()?;
                continue;
            }
            let key_line = self.line;
            let key_col = self.col;
            let key = self.key()?;
            self.skip_space();
            if self.char_at() != Some('=') {
                return self.fail(key_line, key_col, "expected '=' after key");
            }
            self.bump();
            self.skip()?;
            let val = self.value()?;
            self.put(&mut root, &current, &key, val, key_line, key_col)?;
            self.skip()?;
        }
        Ok(root)
    }

    fn header(&mut self, root: &mut Value) -> Result<String, Diagnostic> {
        let start_line = self.line;
        let start_col = self.col;
        self.bump();
        if self.char_at() == Some('[') {
            return self.fail(start_line, start_col, "array of tables is not supported");
        }
        self.skip_space();
        let mut parts = Vec::new();
        parts.push(self.key()?);
        self.skip_space();
        while self.char_at() == Some('.') {
            self.bump();
            self.skip_space();
            parts.push(self.key()?);
            self.skip_space();
        }
        if self.char_at() != Some(']') {
            return self.fail(start_line, start_col, "expected ']'");
        }
        self.bump();
        self.header_insert(root, &parts, start_line, start_col)?;
        Ok(parts.join("."))
    }

    fn header_insert(
        &mut self,
        root: &mut Value,
        parts: &[String],
        start_line: u32,
        start_col: u32,
    ) -> Result<(), Diagnostic> {
        // Ensure every prefix is a table. Use owned keys to avoid holding borrows.
        let mut prefix: Vec<String> = Vec::new();
        for part in parts {
            if let Some(existing) = lookup_mut(root, &prefix) {
                match existing.table.get(part) {
                    None => {
                        existing.order.push(part.clone());
                        existing
                            .table
                            .insert(part.clone(), Value::table(start_line, start_col));
                    }
                    Some(v) if !v.is_table() => {
                        return self.fail(
                            start_line,
                            start_col,
                            &format!("key '{part}' is not a table"),
                        );
                    }
                    Some(_) => {}
                }
            }
            prefix.push(part.clone());
        }
        Ok(())
    }

    fn key(&mut self) -> Result<String, Diagnostic> {
        self.skip_space();
        if self.i >= self.src.len() {
            return self.fail(self.line, self.col, "expected key");
        }
        if self.char_at() == Some('"') {
            return Ok(self.string()?.s);
        }
        let start_line = self.line;
        let start_col = self.col;
        let Some(c) = self.char_at() else {
            return self.fail(start_line, start_col, "expected key");
        };
        if !is_bare(c) {
            return self.fail(start_line, start_col, "expected key");
        }
        let start = self.i;
        while self.char_at().is_some_and(is_bare) {
            self.bump();
        }
        Ok(self.src[start..self.i].to_string())
    }

    fn value(&mut self) -> Result<Value, Diagnostic> {
        let Some(c) = self.char_at() else {
            return self.fail(self.line, self.col, "expected value");
        };
        if c == '"' {
            return self.string();
        }
        if c == 't' || c == 'f' {
            return self.bool_val();
        }
        if c == '{' {
            return self.fail(self.line, self.col, "inline tables are not supported");
        }
        self.fail(self.line, self.col, "expected string or boolean")
    }

    fn string(&mut self) -> Result<Value, Diagnostic> {
        let start_line = self.line;
        let start_col = self.col;
        self.bump();
        let mut out = String::new();
        while self.i < self.src.len() {
            let c = self.char_at().unwrap();
            if c == '"' {
                self.bump();
                return Ok(Value::string(out, start_line, start_col));
            }
            if c == '\n' {
                return self.fail(start_line, start_col, "unterminated string");
            }
            if c == '\\' {
                self.bump();
                let Some(e) = self.char_at() else {
                    return self.fail(start_line, start_col, "unterminated string");
                };
                self.bump();
                match e {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    '"' => out.push('"'),
                    '\\' => out.push('\\'),
                    _ => return self.fail(start_line, start_col, "unknown string escape"),
                }
                continue;
            }
            out.push(c);
            self.bump();
        }
        self.fail(start_line, start_col, "unterminated string")
    }

    fn bool_val(&mut self) -> Result<Value, Diagnostic> {
        let start_line = self.line;
        let start_col = self.col;
        if self.match_word("true") {
            return Ok(Value::bool(true, start_line, start_col));
        }
        if self.match_word("false") {
            return Ok(Value::bool(false, start_line, start_col));
        }
        self.fail(start_line, start_col, "expected string or boolean")
    }

    fn match_word(&mut self, word: &str) -> bool {
        if self.i + word.len() > self.src.len() {
            return false;
        }
        if &self.src[self.i..self.i + word.len()] != word {
            return false;
        }
        let after = self.i + word.len();
        if after < self.src.len() && is_bare(self.src[after..].chars().next().unwrap()) {
            return false;
        }
        for _ in 0..word.len() {
            self.bump();
        }
        true
    }

    fn put(
        &mut self,
        root: &mut Value,
        current: &str,
        key: &str,
        val: Value,
        key_line: u32,
        key_col: u32,
    ) -> Result<(), Diagnostic> {
        let table = if current.is_empty() {
            root
        } else {
            let parts: Vec<String> = current.split('.').map(|s| s.to_string()).collect();
            match lookup_mut(root, &parts) {
                Some(t) => t,
                None => return self.fail(key_line, key_col, "expected key"),
            }
        };
        if table.table.contains_key(key) {
            return self.fail(key_line, key_col, &format!("duplicate key '{key}'"));
        }
        table.order.push(key.to_string());
        table.table.insert(key.to_string(), val);
        Ok(())
    }

    fn skip(&mut self) -> Result<(), Diagnostic> {
        while self.i < self.src.len() {
            let c = self.char_at().unwrap();
            if c == ' ' || c == '\t' || c == '\r' || c == '\n' {
                self.bump();
                continue;
            }
            if c == '#' {
                while self.i < self.src.len() && self.char_at() != Some('\n') {
                    self.bump();
                }
                continue;
            }
            break;
        }
        Ok(())
    }

    fn skip_space(&mut self) {
        while let Some(c) = self.char_at() {
            if c == ' ' || c == '\t' || c == '\r' {
                self.bump();
                continue;
            }
            break;
        }
    }

    fn bump(&mut self) {
        if self.char_at() == Some('\n') {
            self.line += 1;
            self.col = 1;
            self.i += 1;
            return;
        }
        if let Some(c) = self.char_at() {
            self.i += c.len_utf8();
            self.col += 1;
        }
    }

    fn char_at(&self) -> Option<char> {
        self.src[self.i..].chars().next()
    }

    fn fail<T>(&self, line: u32, col: u32, message: &str) -> Result<T, Diagnostic> {
        Err(Diagnostic::new(
            self.file, line, col, "error", message, "error",
        ))
    }
}

fn lookup_mut<'a>(root: &'a mut Value, parts: &[String]) -> Option<&'a mut Value> {
    let mut cur = root;
    for part in parts {
        cur = cur.table.get_mut(part)?;
    }
    Some(cur)
}

fn is_bare(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}
