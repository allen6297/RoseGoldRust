use crate::value::{json_float, MapData, Value};

const MAX_DEPTH: i32 = 64;

pub fn parse(src: &str) -> Result<Value, String> {
    let mut p = Parser {
        chars: src.chars().collect(),
        i: 0,
    };
    let v = p.parse_value(0)?;
    p.skip();
    if p.i != p.chars.len() {
        return Err("invalid JSON".to_string());
    }
    Ok(v)
}

pub fn stringify(v: &Value) -> Result<String, String> {
    let mut out = String::new();
    stringify_into(v, &mut out, 0)?;
    Ok(out)
}

fn stringify_into(v: &Value, out: &mut String, depth: i32) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("json nesting too deep".to_string());
    }
    match v {
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(n) => out.push_str(&n.to_string()),
        Value::Float(n) => {
            if !n.is_finite() {
                return Err("cannot stringify non-finite Float".to_string());
            }
            out.push_str(&json_float(*n));
        }
        Value::String(s) => {
            out.push('"');
            escape(s, out);
            out.push('"');
        }
        Value::Array(items) => {
            out.push('[');
            let items = items.borrow();
            for (n, item) in items.iter().enumerate() {
                if n > 0 {
                    out.push(',');
                }
                stringify_into(item, out, depth + 1)?;
            }
            out.push(']');
        }
        Value::Map(dict) => {
            out.push('{');
            let dict = dict.borrow();
            let mut first = true;
            for key in &dict.order {
                let Some(val) = dict.fields.get(key) else {
                    continue;
                };
                if !first {
                    out.push(',');
                }
                first = false;
                out.push('"');
                escape(key, out);
                out.push_str("\":");
                stringify_into(val, out, depth + 1)?;
            }
            out.push('}');
        }
        Value::Struct(rec) => {
            out.push('{');
            let rec = rec.borrow();
            let mut first = true;
            for key in &rec.order {
                let Some(val) = rec.fields.get(key) else {
                    continue;
                };
                if !first {
                    out.push(',');
                }
                first = false;
                out.push('"');
                escape(key, out);
                out.push_str("\":");
                stringify_into(val, out, depth + 1)?;
            }
            out.push('}');
        }
        Value::Enum { variant, .. } => {
            out.push('"');
            escape(variant, out);
            out.push('"');
        }
        _ => return Err("cannot stringify value".to_string()),
    }
    Ok(())
}

fn escape(s: &str, out: &mut String) {
    const HEX: &[u8] = b"0123456789abcdef";
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let n = c as u32;
                out.push_str("\\u00");
                out.push(HEX[(n >> 4) as usize] as char);
                out.push(HEX[(n & 0xf) as usize] as char);
            }
            c => out.push(c),
        }
    }
}

struct Parser {
    chars: Vec<char>,
    i: usize,
}

impl Parser {
    fn parse_value(&mut self, depth: i32) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return Err("JSON nesting too deep".to_string());
        }
        self.skip();
        let c = self.peek();
        if c == '"' {
            return Ok(Value::String(self.parse_string()?));
        }
        if c == '{' {
            self.getc()?;
            let mut data = MapData {
                order: Vec::new(),
                fields: std::collections::HashMap::new(),
            };
            self.skip();
            if self.peek() == '}' {
                self.getc()?;
                return Ok(Value::map(data));
            }
            loop {
                self.skip();
                if self.peek() != '"' {
                    return Err("invalid JSON".to_string());
                }
                let key = self.parse_string()?;
                self.skip();
                if self.getc()? != ':' {
                    return Err("invalid JSON".to_string());
                }
                let val = self.parse_value(depth + 1)?;
                if !data.fields.contains_key(&key) {
                    data.order.push(key.clone());
                }
                data.fields.insert(key, val);
                self.skip();
                let sep = self.getc()?;
                if sep == '}' {
                    return Ok(Value::map(data));
                }
                if sep != ',' {
                    return Err("invalid JSON".to_string());
                }
            }
        }
        if c == '[' {
            self.getc()?;
            let mut items = Vec::new();
            self.skip();
            if self.peek() == ']' {
                self.getc()?;
                return Ok(Value::array(items));
            }
            loop {
                items.push(self.parse_value(depth + 1)?);
                self.skip();
                let sep = self.getc()?;
                if sep == ']' {
                    return Ok(Value::array(items));
                }
                if sep != ',' {
                    return Err("invalid JSON".to_string());
                }
            }
        }
        if c == 't' {
            if !self.match_lit("true") {
                return Err("invalid JSON".to_string());
            }
            return Ok(Value::Bool(true));
        }
        if c == 'f' {
            if !self.match_lit("false") {
                return Err("invalid JSON".to_string());
            }
            return Ok(Value::Bool(false));
        }
        if c == 'n' {
            if !self.match_lit("null") {
                return Err("invalid JSON".to_string());
            }
            return Err("json null is not supported".to_string());
        }
        if c == '-' || c.is_ascii_digit() {
            return self.parse_number();
        }
        Err("invalid JSON".to_string())
    }

    fn parse_number(&mut self) -> Result<Value, String> {
        let start = self.i;
        if self.peek() == '-' {
            self.i += 1;
        }
        if self.peek() == '0' {
            self.i += 1;
            if self.peek().is_ascii_digit() {
                return Err("invalid JSON".to_string());
            }
        } else if self.peek().is_ascii_digit() {
            while self.peek().is_ascii_digit() {
                self.i += 1;
            }
        } else {
            return Err("invalid JSON".to_string());
        }
        let mut frac = false;
        if self.peek() == '.' {
            frac = true;
            self.i += 1;
            if !self.peek().is_ascii_digit() {
                return Err("invalid JSON".to_string());
            }
            while self.peek().is_ascii_digit() {
                self.i += 1;
            }
        }
        if self.peek() == 'e' || self.peek() == 'E' {
            frac = true;
            self.i += 1;
            if self.peek() == '+' || self.peek() == '-' {
                self.i += 1;
            }
            if !self.peek().is_ascii_digit() {
                return Err("invalid JSON".to_string());
            }
            while self.peek().is_ascii_digit() {
                self.i += 1;
            }
        }
        let tok: String = self.chars[start..self.i].iter().collect();
        if !frac {
            if let Ok(n) = tok.parse::<i64>() {
                return Ok(Value::Int(n));
            }
        }
        match tok.parse::<f64>() {
            Ok(n) if n.is_finite() => Ok(Value::Float(n)),
            _ => Err("invalid JSON".to_string()),
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        if self.getc()? != '"' {
            return Err("invalid JSON".to_string());
        }
        let mut units: Vec<u16> = Vec::new();
        loop {
            let c = self.getc()?;
            if c == '"' {
                return Ok(String::from_utf16_lossy(&units));
            }
            if c == '\\' {
                let e = self.getc()?;
                match e {
                    '"' | '\\' | '/' => push_char(&mut units, e),
                    'b' => push_char(&mut units, '\u{0008}'),
                    'f' => push_char(&mut units, '\u{000c}'),
                    'n' => push_char(&mut units, '\n'),
                    'r' => push_char(&mut units, '\r'),
                    't' => push_char(&mut units, '\t'),
                    'u' => {
                        let cp = self.hex4()?;
                        if cp <= 0xFFFF {
                            units.push(cp as u16);
                        } else if char::from_u32(cp as u32).is_some() {
                            push_char(&mut units, char::from_u32(cp as u32).unwrap());
                        } else {
                            units.push(cp as u16);
                        }
                    }
                    _ => return Err("invalid JSON".to_string()),
                }
            } else if (c as u32) < 0x20 {
                return Err("invalid JSON".to_string());
            } else {
                push_char(&mut units, c);
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let mut n = 0u32;
        for _ in 0..4 {
            let c = self.getc()?;
            n <<= 4;
            if c.is_ascii_digit() {
                n += (c as u32) - ('0' as u32);
            } else if ('a'..='f').contains(&c) {
                n += (c as u32) - ('a' as u32) + 10;
            } else if ('A'..='F').contains(&c) {
                n += (c as u32) - ('A' as u32) + 10;
            } else {
                return Err("invalid JSON".to_string());
            }
        }
        Ok(n)
    }

    fn match_lit(&mut self, lit: &str) -> bool {
        let chars: Vec<char> = lit.chars().collect();
        if self.i + chars.len() > self.chars.len() {
            return false;
        }
        if self.chars[self.i..self.i + chars.len()] != chars[..] {
            return false;
        }
        self.i += chars.len();
        true
    }

    fn skip(&mut self) {
        while self.i < self.chars.len() && is_space(self.chars[self.i]) {
            self.i += 1;
        }
    }

    fn peek(&self) -> char {
        if self.i < self.chars.len() {
            self.chars[self.i]
        } else {
            '\0'
        }
    }

    fn getc(&mut self) -> Result<char, String> {
        if self.i >= self.chars.len() {
            return Err("invalid JSON".to_string());
        }
        let c = self.chars[self.i];
        self.i += 1;
        Ok(c)
    }
}

fn push_char(units: &mut Vec<u16>, c: char) {
    let mut buf = [0u16; 2];
    units.extend_from_slice(c.encode_utf16(&mut buf));
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{000c}' | '\u{000b}')
}
