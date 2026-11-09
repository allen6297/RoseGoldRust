use std::collections::HashMap;
use std::io::{BufRead, Write};

use rosegold_check::check_source;

use crate::completions;
use crate::diagnostics;
use crate::format::{self, Options};
use crate::goto;
use crate::hover;
use crate::pos;
use crate::rename;
use crate::signatures;
use crate::usages;

enum Val {
    Null,
    Bool(bool),
    Num(i64),
    Str(String),
}

pub fn serve<R: BufRead, W: Write>(mut input: R, output: &mut W) {
    let mut line = String::new();
    loop {
        line.clear();
        match input.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(err) => {
                let _ = writeln!(output, "{}", fail(0, &err.to_string()));
                let _ = output.flush();
                break;
            }
        }
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }
        if line.chars().all(pos::is_ws) {
            continue;
        }
        let response = handle(&line);
        let _ = writeln!(output, "{response}");
        let _ = output.flush();
    }
}

pub fn handle(line: &str) -> String {
    let req = match parse_object(line) {
        Ok(req) => req,
        Err(err) => return fail(0, &err),
    };
    let id = long_field(&req, "id");
    let method = str_field(&req, "method");
    match method.as_str() {
        "ping" => ok(id, ""),
        "check" => check(id, &req),
        "hover" => hover_req(id, &req),
        "complete" => complete(id, &req),
        "goto" => goto_req(id, &req),
        "signature" => signature(id, &req),
        "usages" => usages_req(id, &req),
        "rename" => rename_req(id, &req),
        "format" => format_req(id, &req),
        _ => fail(id, "unknown method"),
    }
}

fn check(id: i64, req: &HashMap<String, Val>) -> String {
    let source = str_field(req, "source");
    let path = str_field(req, "path");
    let diags = check_source(&source, &path);
    let mut sb = String::new();
    sb.push_str("{\"id\":");
    sb.push_str(&id.to_string());
    sb.push_str(",\"ok\":true,\"diagnostics\":[");
    for (i, d) in diags.iter().enumerate() {
        if i > 0 {
            sb.push(',');
        }
        let extra = diagnostics::span_hint(&source, &path, d);
        sb.push_str("{\"file\":\"");
        sb.push_str(&esc(&d.file));
        sb.push_str("\",\"line\":");
        sb.push_str(&d.line.to_string());
        sb.push_str(",\"col\":");
        sb.push_str(&d.col.to_string());
        sb.push_str(",\"endLine\":");
        sb.push_str(&extra.end_line.to_string());
        sb.push_str(",\"endCol\":");
        sb.push_str(&extra.end_col.to_string());
        sb.push_str(",\"severity\":\"");
        sb.push_str(&esc(&d.severity));
        sb.push_str("\",\"kind\":\"");
        sb.push_str(&esc(&d.kind));
        sb.push_str("\",\"message\":\"");
        sb.push_str(&esc(&d.message));
        sb.push('"');
        if !extra.hint.is_empty() {
            sb.push_str(",\"hint\":\"");
            sb.push_str(&esc(&extra.hint));
            sb.push('"');
        }
        sb.push('}');
    }
    sb.push_str("]}");
    sb
}

fn hover_req(id: i64, req: &HashMap<String, Val>) -> String {
    let info = hover::at(
        &str_field(req, "source"),
        &str_field(req, "path"),
        int_field(req, "offset"),
    );
    let Some(info) = info else {
        return ok(id, "\"hover\":null");
    };
    let mut sb = String::new();
    sb.push_str("\"hover\":{");
    str_field_json(&mut sb, "name", &info.name, true);
    str_field_json(&mut sb, "kind", &info.kind, true);
    str_field_json(&mut sb, "signature", &info.signature, true);
    str_field_json(&mut sb, "doc", &info.doc, true);
    sb.push_str("\"deprecated\":");
    sb.push_str(if info.deprecated { "true" } else { "false" });
    sb.push(',');
    str_field_json(&mut sb, "crate", &info.crate_name, true);
    sb.push_str("\"start\":");
    sb.push_str(&info.start.to_string());
    sb.push(',');
    sb.push_str("\"end\":");
    sb.push_str(&info.end.to_string());
    sb.push('}');
    ok(id, &sb)
}

fn complete(id: i64, req: &HashMap<String, Val>) -> String {
    let items = completions::suggest(
        &str_field(req, "source"),
        &str_field(req, "path"),
        int_field(req, "offset"),
    );
    let mut sb = String::new();
    sb.push_str("\"items\":[");
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            sb.push(',');
        }
        sb.push('{');
        str_field_json(&mut sb, "label", &item.label, true);
        str_field_json(&mut sb, "kind", &item.kind, true);
        str_field_json(&mut sb, "detail", &item.detail, false);
        sb.push('}');
    }
    sb.push(']');
    ok(id, &sb)
}

fn goto_req(id: i64, req: &HashMap<String, Val>) -> String {
    let loc = goto::at(
        &str_field(req, "source"),
        &str_field(req, "path"),
        int_field(req, "offset"),
    );
    let Some(loc) = loc else {
        return ok(id, "\"loc\":null");
    };
    ok(
        id,
        &format!(
            "\"loc\":{}",
            loc_json(&loc.path, &loc.name, "", false, loc.line, loc.col, loc.start, loc.end)
        ),
    )
}

fn signature(id: i64, req: &HashMap<String, Val>) -> String {
    let info = signatures::at(
        &str_field(req, "source"),
        &str_field(req, "path"),
        int_field(req, "offset"),
    );
    let Some(info) = info else {
        return ok(id, "\"signature\":null");
    };
    let mut sb = String::new();
    sb.push_str("\"signature\":{");
    str_field_json(&mut sb, "name", &info.name, true);
    str_field_json(&mut sb, "qualifier", &info.qualifier, true);
    str_field_json(&mut sb, "label", &info.label, true);
    sb.push_str("\"params\":[");
    for (i, param) in info.params.iter().enumerate() {
        if i > 0 {
            sb.push(',');
        }
        sb.push('"');
        sb.push_str(&esc(param));
        sb.push('"');
    }
    sb.push_str("],\"activeParam\":");
    sb.push_str(&info.active_param.to_string());
    sb.push_str(",\"paramStarts\":");
    sb.push_str(&int_array(&info.param_starts));
    sb.push_str(",\"paramEnds\":");
    sb.push_str(&int_array(&info.param_ends));
    sb.push('}');
    ok(id, &sb)
}

fn usages_req(id: i64, req: &HashMap<String, Val>) -> String {
    let hits = usages::including_crates(
        &str_field(req, "source"),
        &str_field(req, "path"),
        int_field(req, "offset"),
    );
    ok(id, &format!("\"hits\":{}", hits_json(&hits)))
}

fn rename_req(id: i64, req: &HashMap<String, Val>) -> String {
    let new_name = str_field(req, "newName");
    if !rename::is_valid_name(&new_name) {
        return fail(id, "invalid name");
    }
    let hits = rename::rewrite_hits(
        &str_field(req, "source"),
        &str_field(req, "path"),
        int_field(req, "offset"),
    );
    ok(id, &format!("\"hits\":{}", hits_json(&hits)))
}

fn format_req(id: i64, req: &HashMap<String, Val>) -> String {
    let mut opts = Options::default();
    if bool_field(req, "compact") {
        opts.blank_between_items = false;
    }
    if req.contains_key("comments") {
        opts.keep_comments = bool_field(req, "comments");
    }
    let result = format::format_source(&str_field(req, "source"), &str_field(req, "path"), &opts);
    if !result.ok {
        let message = if result.message.is_empty() {
            "format failed".to_string()
        } else {
            result.message
        };
        return fail(id, &message);
    }
    ok(id, &format!("\"text\":\"{}\"", esc(&result.out)))
}

fn hits_json(hits: &[usages::Hit]) -> String {
    let mut sb = String::new();
    sb.push('[');
    for (i, hit) in hits.iter().enumerate() {
        if i > 0 {
            sb.push(',');
        }
        sb.push_str(&loc_json(
            &hit.path,
            &hit.name,
            &hit.qualifier,
            hit.write,
            hit.line,
            hit.col,
            hit.start,
            hit.end,
        ));
    }
    sb.push(']');
    sb
}

fn loc_json(
    path: &str,
    name: &str,
    qualifier: &str,
    write: bool,
    line: u32,
    col: u32,
    start: usize,
    end: usize,
) -> String {
    let mut sb = String::new();
    sb.push('{');
    str_field_json(&mut sb, "path", path, true);
    str_field_json(&mut sb, "name", name, true);
    str_field_json(&mut sb, "qualifier", qualifier, true);
    sb.push_str("\"write\":");
    sb.push_str(if write { "true" } else { "false" });
    sb.push(',');
    sb.push_str("\"line\":");
    sb.push_str(&line.to_string());
    sb.push(',');
    sb.push_str("\"col\":");
    sb.push_str(&col.to_string());
    sb.push(',');
    sb.push_str("\"start\":");
    sb.push_str(&start.to_string());
    sb.push(',');
    sb.push_str("\"end\":");
    sb.push_str(&end.to_string());
    sb.push('}');
    sb
}

fn int_array(values: &[i32]) -> String {
    let mut sb = String::new();
    sb.push('[');
    for (i, value) in values.iter().enumerate() {
        if i > 0 {
            sb.push(',');
        }
        sb.push_str(&value.to_string());
    }
    sb.push(']');
    sb
}

fn ok(id: i64, fields: &str) -> String {
    if fields.is_empty() {
        format!("{{\"id\":{id},\"ok\":true}}")
    } else {
        format!("{{\"id\":{id},\"ok\":true,{fields}}}")
    }
}

fn fail(id: i64, message: &str) -> String {
    format!(
        "{{\"id\":{id},\"ok\":false,\"error\":\"{}\"}}",
        esc(message)
    )
}

fn str_field_json(sb: &mut String, key: &str, value: &str, comma: bool) {
    sb.push('"');
    sb.push_str(key);
    sb.push_str("\":\"");
    sb.push_str(&esc(value));
    sb.push('"');
    if comma {
        sb.push(',');
    }
}

fn esc(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn str_field(req: &HashMap<String, Val>, key: &str) -> String {
    match req.get(key) {
        Some(Val::Str(s)) => s.clone(),
        Some(Val::Num(n)) => n.to_string(),
        Some(Val::Bool(b)) => b.to_string(),
        Some(Val::Null) | None => String::new(),
    }
}

fn long_field(req: &HashMap<String, Val>, key: &str) -> i64 {
    match req.get(key) {
        Some(Val::Num(n)) => *n,
        _ => 0,
    }
}

fn int_field(req: &HashMap<String, Val>, key: &str) -> usize {
    let n = long_field(req, key);
    if n > i32::MAX as i64 {
        return i32::MAX as usize;
    }
    if n < 0 {
        return 0;
    }
    n as usize
}

fn bool_field(req: &HashMap<String, Val>, key: &str) -> bool {
    matches!(req.get(key), Some(Val::Bool(true)))
}

fn parse_object(src: &str) -> Result<HashMap<String, Val>, String> {
    let mut js = Js { s: src, i: 0 };
    js.skip();
    if js.peek() != '{' {
        return Err("expected object".to_string());
    }
    js.i += 1;
    let mut map = HashMap::new();
    js.skip();
    if js.peek() == '}' {
        return Ok(map);
    }
    loop {
        let key = js.string()?;
        js.skip();
        if js.peek() != ':' {
            return Err("expected ':'".to_string());
        }
        js.i += 1;
        let value = js.value()?;
        map.insert(key, value);
        js.skip();
        let c = js.peek();
        if c == ',' {
            js.i += 1;
            continue;
        }
        if c == '}' {
            return Ok(map);
        }
        return Err("expected ',' or '}'".to_string());
    }
}

struct Js<'a> {
    s: &'a str,
    i: usize,
}

impl<'a> Js<'a> {
    fn skip(&mut self) {
        let bytes = self.s.as_bytes();
        while self.i < bytes.len() {
            match bytes[self.i] {
                b' ' | b'\t' | b'\n' | b'\r' => self.i += 1,
                _ => break,
            }
        }
    }

    fn peek(&mut self) -> char {
        self.skip();
        self.s[self.i..].chars().next().unwrap_or('\0')
    }

    fn value(&mut self) -> Result<Val, String> {
        let c = self.peek();
        if c == '"' {
            return Ok(Val::Str(self.string()?));
        }
        if c == 't' && self.s[self.i..].starts_with("true") {
            self.i += 4;
            return Ok(Val::Bool(true));
        }
        if c == 'f' && self.s[self.i..].starts_with("false") {
            self.i += 5;
            return Ok(Val::Bool(false));
        }
        if c == 'n' && self.s[self.i..].starts_with("null") {
            self.i += 4;
            return Ok(Val::Null);
        }
        if c == '-' || c.is_ascii_digit() {
            return Ok(Val::Num(self.number()?));
        }
        Err("expected value".to_string())
    }

    fn string(&mut self) -> Result<String, String> {
        self.skip();
        if self.peek() != '"' {
            return Err("expected string".to_string());
        }
        self.i += 1;
        let mut out = String::new();
        let bytes = self.s.as_bytes();
        while self.i < bytes.len() {
            let c = self.s[self.i..].chars().next().unwrap();
            self.i += c.len_utf8();
            if c == '"' {
                return Ok(out);
            }
            if c == '\\' {
                if self.i >= bytes.len() {
                    return Err("bad escape".to_string());
                }
                let e = self.s[self.i..].chars().next().unwrap();
                self.i += e.len_utf8();
                match e {
                    '"' | '\\' | '/' => out.push(e),
                    'b' => out.push('\u{0008}'),
                    'f' => out.push('\u{000c}'),
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'u' => {
                        if self.i + 4 > self.s.len() {
                            return Err("bad unicode escape".to_string());
                        }
                        let hex = &self.s[self.i..self.i + 4];
                        self.i += 4;
                        let n = u32::from_str_radix(hex, 16)
                            .map_err(|_| format!("For input string: \"{hex}\""))?;
                        if let Some(ch) = char::from_u32(n) {
                            out.push(ch);
                        } else {
                            return Err(format!("For input string: \"{hex}\""));
                        }
                    }
                    _ => return Err("bad escape".to_string()),
                }
                continue;
            }
            out.push(c);
        }
        Err("unterminated string".to_string())
    }

    fn number(&mut self) -> Result<i64, String> {
        self.skip();
        let start = self.i;
        if self.peek() == '-' {
            self.i += 1;
        }
        let bytes = self.s.as_bytes();
        if self.i >= bytes.len() || !bytes[self.i].is_ascii_digit() {
            return Err("expected number".to_string());
        }
        while self.i < bytes.len() && bytes[self.i].is_ascii_digit() {
            self.i += 1;
        }
        self.s[start..self.i]
            .parse::<i64>()
            .map_err(|_| "expected number".to_string())
    }
}
