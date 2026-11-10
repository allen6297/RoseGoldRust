use rosegold_syntax::TypeParam;
use std::collections::HashMap;

pub fn type_head(ty: &str) -> String {
    match ty.find('[') {
        Some(p) => ty[..p].to_string(),
        None => ty.to_string(),
    }
}

pub fn type_arg_list(ty: &str) -> Vec<String> {
    let Some(p) = ty.find('[') else {
        return Vec::new();
    };
    if !ty.ends_with(']') {
        return Vec::new();
    }
    let inner = &ty[p + 1..ty.len() - 1];
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let flush = |cur: &mut String, out: &mut Vec<String>| {
        let s = cur.as_str();
        let a = s.find(|c: char| c != ' ');
        let b = s.rfind(|c: char| c != ' ');
        if let (Some(a), Some(b)) = (a, b) {
            out.push(s[a..=b].to_string());
        }
        cur.clear();
    };
    for c in inner.chars() {
        if c == '[' {
            depth += 1;
        } else if c == ']' {
            depth -= 1;
        }
        if c == ',' && depth == 0 {
            flush(&mut cur, &mut out);
            continue;
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        flush(&mut cur, &mut out);
    }
    out
}

pub fn type_apply(name: &str, args: &[String]) -> String {
    if args.is_empty() {
        return name.to_string();
    }
    let mut out = String::from(name);
    out.push('[');
    for (i, arg) in args.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(arg);
    }
    out.push(']');
    out
}

pub fn subst_type(ty: &str, env: &HashMap<String, String>) -> String {
    if ty.is_empty() {
        return String::new();
    }
    let args = type_arg_list(ty);
    let mut head = type_head(ty);
    if args.is_empty() {
        return env.get(&head).cloned().unwrap_or(head);
    }
    let out: Vec<String> = args.iter().map(|a| subst_type(a, env)).collect();
    if let Some(mapped) = env.get(&head) {
        head = mapped.clone();
    }
    type_apply(&head, &out)
}

pub fn type_env_from(params: &[TypeParam], args: &[String]) -> HashMap<String, String> {
    let mut env = HashMap::new();
    if args.len() == params.len() {
        for (p, a) in params.iter().zip(args.iter()) {
            env.insert(p.name.clone(), a.clone());
        }
    } else {
        for p in params {
            env.insert(p.name.clone(), p.name.clone());
        }
    }
    env
}

pub fn self_applied(name: &str, params: &[TypeParam]) -> String {
    if params.is_empty() {
        return name.to_string();
    }
    let args: Vec<String> = params.iter().map(|p| p.name.clone()).collect();
    type_apply(name, &args)
}

pub fn is_host_module(name: &str) -> bool {
    matches!(
        name,
        "checks"
            | "process"
            | "__math"
            | "__str"
            | "__io"
            | "__uuid"
            | "__time"
            | "__path"
            | "__json"
            | "__regex"
            | "__ui"
            | "hooks"
            | "events"
            | "world"
            | "player"
            | "item"
            | "block"
            | "recipe"
            | "loot"
            | "entity"
            | "block_entity"
            | "modapi"
            | "gui"
            | "chat"
            | "settings"
    )
}

pub fn type_of_field(fields: &[String], field_types: &[String], name: &str) -> String {
    for (i, f) in fields.iter().enumerate() {
        if f == name {
            return field_types.get(i).cloned().unwrap_or_default();
        }
    }
    String::new()
}

pub fn is_optional_field(fields: &[String], optional: &[bool], name: &str) -> bool {
    for (i, f) in fields.iter().enumerate() {
        if f == name {
            return optional.get(i).copied().unwrap_or(false);
        }
    }
    false
}

pub fn min_arity(
    params: &[String],
    defaults: &[Option<rosegold_syntax::Expr>],
    method: bool,
) -> i32 {
    let off = if method { 1 } else { 0 };
    for i in off..params.len() {
        if defaults.get(i).and_then(|d| d.as_ref()).is_some() {
            return i as i32 - off as i32;
        }
    }
    (params.len() as i32 - off as i32).max(0)
}

pub fn max_arity(params: &[String], method: bool) -> i32 {
    let off = if method { 1 } else { 0 };
    (params.len() as i32 - off).max(0)
}
