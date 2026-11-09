use std::fs;
use std::path::{Path, PathBuf};

use rosegold_syntax::{parse_source, tokenize, Tok};

fn java_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../RoseGoldJava")
        .canonicalize()
        .expect("RoseGoldJava checkout")
}

fn read_expect(source: &str) -> String {
    for raw in source.split('\n') {
        let line = raw.trim();
        for prefix in ["# expect:", "// expect:", "/// expect:"] {
            if let Some(rest) = line.strip_prefix(prefix) {
                return rest.trim().to_string();
            }
        }
    }
    String::new()
}

/// Fail-file expects that the Java parser or lexer emits. Everything else in
/// the suite must parse with no diagnostics; the checker owns those messages.
const PARSE_EXPECTS: &[&str] = &[
    "abstract cannot apply to data",
    "cannot be both abstract and final",
    "abstract cannot apply to function",
    "abstract cannot apply to struct",
    "data cannot declare signals",
    "duplicate field",
    "integer literal too large",
    "conflicts with field",
    "@optional cannot apply to function",
    "@optional cannot apply to method",
    "expected expression",
    "@ufcs cannot apply to method",
    "unexpected character",
    "protected cannot apply to function",
    "protected cannot apply to struct field",
];

fn is_parse_expect(expect: &str) -> bool {
    PARSE_EXPECTS.iter().any(|needle| expect.contains(needle))
}

fn walk_rg(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_rg(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rg") {
            out.push(path);
        }
    }
}

fn suite_files() -> Vec<PathBuf> {
    let root = java_root();
    let mut files = Vec::new();
    for rel in ["tests/pass", "tests/fail", "examples"] {
        walk_rg(&root.join(rel), &mut files);
    }
    files.sort();
    files
}

#[test]
fn java_suite_parses() {
    let root = java_root();
    let mut problems = Vec::new();
    let mut parsed = 0usize;
    let mut parse_errors = 0usize;
    for path in suite_files() {
        let source = fs::read_to_string(&path).unwrap_or_else(|err| {
            panic!("read {}: {err}", path.display());
        });
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let (_program, diags) = parse_source(&source, &rel);
        let expect = read_expect(&source);
        parsed += 1;
        if is_parse_expect(&expect) {
            parse_errors += 1;
            let hit = diags.iter().any(|d| d.message.contains(&expect));
            if !hit {
                let got = if diags.is_empty() {
                    "(no diagnostics)".to_string()
                } else {
                    diags
                        .iter()
                        .map(|d| format!("{}:{}: {}", d.line, d.col, d.message))
                        .collect::<Vec<_>>()
                        .join(" | ")
                };
                problems.push(format!("{rel}: expected '{expect}', got {got}"));
            }
        } else if !diags.is_empty() {
            let got = diags
                .iter()
                .map(|d| format!("{}:{}: {}", d.line, d.col, d.message))
                .collect::<Vec<_>>()
                .join(" | ");
            problems.push(format!("{rel}: unexpected diagnostic: {got}"));
        }
    }
    assert!(parsed > 200, "suite walk found only {parsed} files");
    assert!(parse_errors >= 15, "only {parse_errors} parse-error files");
    assert!(
        problems.is_empty(),
        "{} parse mismatches:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn leading_comment_stays_on_item() {
    let (program, diags) = parse_source("// keep me\nfn b(): Int {\n    return 2;\n}\n", "t.rg");
    assert!(diags.is_empty(), "{diags:?}");
    assert_eq!(program.items.len(), 1);
    assert!(program.items[0]
        .leading_comments
        .iter()
        .any(|c| c.contains("keep me")));
}

#[test]
fn hello_has_main() {
    let path = java_root().join("examples/hello.rg");
    let source = fs::read_to_string(&path).unwrap();
    let (program, diags) = parse_source(&source, "examples/hello.rg");
    assert!(diags.is_empty(), "{diags:?}");
    assert!(program.fns.iter().any(|f| f.name == "main"));
}

#[test]
fn lexer_keywords_and_symbols() {
    let mut errors = Vec::new();
    let tokens = tokenize("fn main(): Int { return 1 + 2; }", "t.rg", &mut errors);
    assert!(errors.is_empty());
    assert_eq!(tokens[0].kind, Tok::Function);
    assert_eq!(tokens[1].kind, Tok::Identifier);
    assert_eq!(tokens[1].text, "main");
    assert!(tokens.iter().any(|t| t.kind == Tok::Integer));
    assert_eq!(tokens.last().unwrap().kind, Tok::Eof);
}

#[test]
fn lexer_formatted_string_is_one_token() {
    let mut errors = Vec::new();
    let tokens = tokenize("f\"mods {n}\"", "t.rg", &mut errors);
    assert!(errors.is_empty());
    assert_eq!(tokens[0].kind, Tok::FString);
    assert_eq!(tokens[0].texts, vec!["mods ".to_string(), String::new()]);
    assert_eq!(tokens[0].holes, vec!["n".to_string()]);
    assert_eq!(tokens[1].kind, Tok::Eof);
}

#[test]
fn lexer_unexpected_character() {
    let mut errors = Vec::new();
    tokenize(
        "fn main(): Int { print(?); return 0; }",
        "t.rg",
        &mut errors,
    );
    assert!(errors
        .iter()
        .any(|d| d.message.contains("unexpected character")));
}

#[test]
fn lexer_integer_too_large() {
    let mut errors = Vec::new();
    tokenize("99999999999999999999", "t.rg", &mut errors);
    assert_eq!(errors[0].message, "integer literal too large");
}
