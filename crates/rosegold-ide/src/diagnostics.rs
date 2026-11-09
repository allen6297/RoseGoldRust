use rosegold_check::export_index;
use rosegold_syntax::{tokenize, Diagnostic, Tok};

pub struct SpanHint {
    pub end_line: u32,
    pub end_col: u32,
    pub hint: String,
}

/// The token under a diagnostic, plus a close-name hint when the message is
/// a missing function or export.
pub fn span_hint(source: &str, path: &str, diag: &Diagnostic) -> SpanHint {
    let (end_line, end_col) = token_end(source, diag.line, diag.col);
    let hint = suggest(source, path, &diag.message).unwrap_or_default();
    SpanHint {
        end_line,
        end_col,
        hint,
    }
}

fn token_end(source: &str, line: u32, col: u32) -> (u32, u32) {
    let mut errors = Vec::new();
    let tokens = tokenize(source, "", &mut errors);
    let mut covering = None;
    for token in &tokens {
        if token.kind == Tok::Eof || token.text.is_empty() {
            continue;
        }
        let (end_line, end_col) = end_of(&token.text, token.span.line, token.span.col);
        if token.span.line == line && token.span.col == col {
            return (end_line, end_col);
        }
        if token.span.line == line && end_line == line && col >= token.span.col && col < end_col {
            covering = Some((end_line, end_col));
        }
    }
    covering.unwrap_or((line, col.saturating_add(1)))
}

fn end_of(text: &str, line: u32, col: u32) -> (u32, u32) {
    let mut end_line = line;
    let mut end_col = col;
    for ch in text.chars() {
        if ch == '\n' {
            end_line += 1;
            end_col = 1;
        } else {
            end_col += 1;
        }
    }
    if end_line == line && end_col == col {
        (line, col.saturating_add(1))
    } else {
        (end_line, end_col)
    }
}

fn suggest(source: &str, path: &str, message: &str) -> Option<String> {
    let (module, missing) = missing_name(message)?;
    if missing.is_empty() {
        return None;
    }
    let mut names = match &module {
        Some(module) => names_in_crate(path, module),
        None => {
            let mut names = local_fns(source);
            names.extend(names_in_crate(path, ""));
            names
        }
    };
    names.sort();
    names.dedup();
    let mut best: Option<(usize, String)> = None;
    let mut ties = 0usize;
    for name in names {
        if name == missing {
            continue;
        }
        let distance = edit_distance(&name, &missing);
        if distance == 0 || distance > 2 {
            continue;
        }
        if missing.chars().count() < 3 && distance > 1 {
            continue;
        }
        match &best {
            None => {
                best = Some((distance, name));
                ties = 1;
            }
            Some((best_distance, _)) if distance < *best_distance => {
                best = Some((distance, name));
                ties = 1;
            }
            Some((best_distance, _)) if distance == *best_distance => {
                ties += 1;
            }
            _ => {}
        }
    }
    if ties != 1 {
        return None;
    }
    best.map(|(_, name)| format!("did you mean '{name}'?"))
}

fn missing_name(message: &str) -> Option<(Option<String>, String)> {
    if let Some(name) = after_marker(message, "has no export '") {
        return Some((between(message, "module '", "'"), name));
    }
    if let Some(name) = after_marker(message, "has no method '") {
        return Some((None, name));
    }
    if let Some(name) = after_marker(message, "unknown function '") {
        return Some((None, name));
    }
    let rest = message.split("unknown function ").nth(1)?;
    let token = rest
        .split(|c: char| c.is_whitespace() || c == ':' || c == '(')
        .next()
        .unwrap_or("");
    let token = token.trim_matches('\'');
    if let Some((module, name)) = token.split_once('.') {
        if !module.is_empty() && !name.is_empty() {
            return Some((Some(module.to_string()), name.to_string()));
        }
    }
    if token.is_empty() {
        None
    } else {
        Some((None, token.to_string()))
    }
}

fn after_marker(message: &str, marker: &str) -> Option<String> {
    let rest = message.split(marker).nth(1)?;
    let end = rest.find('\'')?;
    let name = &rest[..end];
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn between(message: &str, open: &str, close: &str) -> Option<String> {
    let rest = message.split(open).nth(1)?;
    let end = rest.find(close)?;
    let name = &rest[..end];
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn names_in_crate(path: &str, module: &str) -> Vec<String> {
    let index = export_index(path);
    let mut names = Vec::new();
    for (name, exports) in index.iter() {
        if module.is_empty()
            || exports
                .iter()
                .any(|export| crate_matches(&export.crate_name, module))
        {
            names.push(name.to_string());
        }
    }
    names
}

fn crate_matches(crate_name: &str, module: &str) -> bool {
    crate_name == module
        || crate_name == format!("std.{module}")
        || crate_name.ends_with(&format!(".{module}"))
}

fn local_fns(source: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let tokens = tokenize(source, "", &mut errors);
    let mut names = Vec::new();
    for pair in tokens.windows(2) {
        if pair[0].kind == Tok::Function && pair[1].kind == Tok::Identifier {
            names.push(pair[1].text.clone());
        }
    }
    names
}

fn edit_distance(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }
    let mut prev: Vec<usize> = (0..=right.len()).collect();
    let mut next = vec![0; right.len() + 1];
    for i in 1..=left.len() {
        next[0] = i;
        for j in 1..=right.len() {
            let cost = if left[i - 1] == right[j - 1] { 0 } else { 1 };
            next[j] = (prev[j] + 1).min(next[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut next);
    }
    prev[right.len()]
}
