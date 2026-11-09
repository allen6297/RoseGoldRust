use std::fs;
use std::path::{Path, PathBuf};

use rosegold_cli::{format_source, Options};
use rosegold_syntax::parse_source;

#[test]
fn suite_format_is_idempotent() {
    let root = Path::new(r"C:\CODE\RoseGoldJava");
    let mut files = Vec::new();
    for rel in ["tests/pass", "tests/fail", "examples"] {
        collect(root.join(rel).as_path(), &mut files);
    }
    files.sort();
    assert!(
        !files.is_empty(),
        "expected RoseGoldJava .rg files under {}",
        root.display()
    );
    let mut bad = Vec::new();
    for path in &files {
        let source = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(err) => {
                bad.push(format!("{}: read {}", path.display(), err));
                continue;
            }
        };
        let (_program, diags) = parse_source(&source, &path.display().to_string());
        if diags.iter().any(|d| d.severity == "error") {
            continue;
        }
        let once = format_source(&source, &path.display().to_string(), &Options::default());
        if !once.ok {
            bad.push(format!(
                "{}: format failed: {}",
                path.display(),
                once.message
            ));
            continue;
        }
        let twice = format_source(&once.out, &path.display().to_string(), &Options::default());
        if !twice.ok {
            bad.push(format!(
                "{}: second format failed: {}",
                path.display(),
                twice.message
            ));
            continue;
        }
        if once.out != twice.out {
            bad.push(format!(
                "{}: not idempotent\n{}",
                path.display(),
                hint(&once.out, &twice.out)
            ));
        }
    }
    assert!(
        bad.is_empty(),
        "{} file(s) failed to round-trip:\n{}",
        bad.len(),
        bad.join("\n")
    );
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for ent in rd.flatten() {
        let path = ent.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("rg"))
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
}

fn hint(a: &str, b: &str) -> String {
    let bytes = a
        .bytes()
        .zip(b.bytes())
        .position(|(x, y)| x != y)
        .unwrap_or(a.len().min(b.len()));
    let start = bytes.saturating_sub(60);
    let end_a = (bytes + 80).min(a.len());
    let end_b = (bytes + 80).min(b.len());
    format!(
        "at {bytes}\n first: {:?}\nsecond: {:?}",
        &a[start..end_a],
        &b[start..end_b]
    )
}
