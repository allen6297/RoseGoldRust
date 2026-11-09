use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use rosegold_syntax::Diagnostic;

#[derive(Clone, Debug)]
pub struct Project {
    pub file: PathBuf,
    pub dir: PathBuf,
    pub entry: String,
    pub entry_file: PathBuf,
    pub test_entry: String,
    pub modules: BTreeMap<String, String>,
}

impl Project {
    pub fn test_target(&self) -> PathBuf {
        if !self.test_entry.is_empty() {
            return normalize(&self.dir.join(&self.test_entry));
        }
        let unit = self.dir.join("tests.rg");
        if unit.is_file() {
            return unit;
        }
        let suite = self.dir.join("tests");
        if suite.is_dir() {
            return suite;
        }
        self.entry_file.clone()
    }
}

pub fn is_project_file(path: &Path) -> bool {
    path.file_name().and_then(|n| n.to_str()) == Some("project.toml")
}

pub fn locate(start: &Path) -> Option<PathBuf> {
    let mut p = if start.as_os_str().is_empty() {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        normalize(start)
    };
    if p.is_file() {
        if is_project_file(&p) {
            return Some(p);
        }
        if !p.pop() {
            return None;
        }
    }
    loop {
        let cand = p.join("project.toml");
        if cand.is_file() {
            return Some(normalize(&cand));
        }
        if !p.pop() {
            return None;
        }
    }
}

pub fn load(path: &Path) -> Result<Project, Diagnostic> {
    let file = normalize(path);
    let source = fs::read_to_string(&file).map_err(|_| {
        Diagnostic::new(
            file.display().to_string(),
            1,
            1,
            "error",
            "cannot read project.toml",
            "error",
        )
    })?;
    parse(&file, &source)
}

pub fn parse(file: &Path, source: &str) -> Result<Project, Diagnostic> {
    let file = normalize(file);
    let rows = scan(source);
    let mut name = String::new();
    let mut entry = String::new();
    let mut saw_project = false;
    let mut test_entry = String::new();
    let mut modules = BTreeMap::new();
    for row in &rows {
        if row.section == "project" {
            saw_project = true;
            if row.key == "name" {
                name = row.value.clone();
            } else if row.key == "entry" {
                entry = row.value.clone();
            }
        } else if row.section == "modules" {
            modules.insert(row.key.clone(), row.value.clone());
        } else if row.section == "profile.test" && row.key == "entry" {
            test_entry = row.value.clone();
        }
    }
    if !saw_project {
        return Err(Diagnostic::new(
            file.display().to_string(),
            1,
            1,
            "error",
            "missing [project] table",
            "error",
        ));
    }
    if entry.is_empty() {
        return Err(Diagnostic::new(
            file.display().to_string(),
            1,
            1,
            "error",
            "missing project.entry",
            "error",
        ));
    }
    let dir = file
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let entry_file = normalize(&dir.join(&entry));
    let _ = name;
    Ok(Project {
        file,
        dir,
        entry,
        entry_file,
        test_entry,
        modules,
    })
}

struct Row {
    section: String,
    key: String,
    value: String,
}

fn scan(source: &str) -> Vec<Row> {
    let mut section = String::new();
    let mut rows = Vec::new();
    for raw in source.replace("\r\n", "\n").split('\n') {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') && !line.starts_with("[[") {
            section = line[1..line.len() - 1].trim().to_string();
            continue;
        }
        let Some((key, rest)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        let Some(value) = parse_value(rest.trim()) else {
            continue;
        };
        rows.push(Row {
            section: section.clone(),
            key: key.to_string(),
            value,
        });
    }
    rows
}

fn parse_value(s: &str) -> Option<String> {
    let mut chars = s.chars().peekable();
    let quote = chars.peek().copied()?;
    if quote != '"' && quote != '\'' {
        let bare = s.split('#').next().unwrap_or(s).trim();
        return if bare.is_empty() {
            None
        } else {
            Some(bare.to_string())
        };
    }
    chars.next();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        if c == '\\' && quote == '"' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => break,
            }
            continue;
        }
        if c == quote {
            return Some(out);
        }
        out.push(c);
    }
    None
}

pub fn normalize(path: &Path) -> PathBuf {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut out = PathBuf::new();
    for c in abs.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn resolve(cwd: &Path, path: &Path) -> PathBuf {
    if path.as_os_str().is_empty() {
        normalize(cwd)
    } else if path.is_absolute() {
        normalize(path)
    } else {
        normalize(&cwd.join(path))
    }
}
