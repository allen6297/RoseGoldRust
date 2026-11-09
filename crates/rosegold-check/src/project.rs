use crate::toml::{self, Value};
use rosegold_syntax::Diagnostic;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Project {
    pub file: PathBuf,
    pub dir: PathBuf,
    pub modules: HashMap<String, PathBuf>,
}

impl Project {
    pub fn find(start: &Path) -> Result<Option<Project>, Diagnostic> {
        let Some(file) = locate(start) else {
            return Ok(None);
        };
        let source = std::fs::read_to_string(&file).map_err(|_| {
            Diagnostic::new(
                file.display().to_string(),
                1,
                1,
                "error",
                "cannot read project.toml",
                "error",
            )
        })?;
        Ok(Some(from_toml(&file, &source)?))
    }

    pub fn module_path(&self, name: &str) -> Option<PathBuf> {
        if name.is_empty() {
            return None;
        }
        if let Some(hit) = self.modules.get(name) {
            return Some(hit.clone());
        }
        if let Some(dot) = name.rfind('.') {
            if dot > 0 {
                return self.modules.get(&name[..dot]).cloned();
            }
        }
        None
    }
}

pub fn locate(start: &Path) -> Option<PathBuf> {
    let mut p = if start.as_os_str().is_empty() {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(start)
    };
    if p.is_file() {
        if p.file_name().and_then(|n| n.to_str()) == Some("project.toml") {
            return Some(lexical_abs(&p));
        }
        p.pop();
    }
    loop {
        let cand = p.join("project.toml");
        if cand.is_file() {
            return Some(lexical_abs(&cand));
        }
        if !p.pop() {
            return None;
        }
    }
}

fn from_toml(file: &Path, source: &str) -> Result<Project, Diagnostic> {
    let abs = lexical_abs(file);
    let file_s = abs.display().to_string();
    let root = toml::parse(source, &file_s)?;
    if !root.is_table() {
        return fail(&abs, 1, 1, "project.toml root must be a table");
    }
    for key in root.keys() {
        if key != "project" && key != "modules" && key != "profile" {
            let v = root.get(key).unwrap();
            return fail(&abs, v.line, v.col, &format!("unknown table '{key}'"));
        }
    }
    let project = match root.get("project") {
        None => return fail(&abs, 1, 1, "missing [project] table"),
        Some(p) if !p.is_table() => return fail(&abs, p.line, p.col, "[project] must be a table"),
        Some(p) => p,
    };
    for key in project.keys() {
        if !matches!(
            key.as_str(),
            "name" | "author" | "version" | "rosegold" | "entry"
        ) {
            let v = project.get(key).unwrap();
            return fail(&abs, v.line, v.col, &format!("unknown project key '{key}'"));
        }
    }
    let _name = require_string(&abs, project, "name")?;
    let _entry = require_string(&abs, project, "entry")?;
    let _author = optional_string(&abs, project, "author", "project.author")?;
    let _version = optional_string(&abs, project, "version", "project.version")?;
    let _rosegold = optional_string(&abs, project, "rosegold", "project.rosegold")?;
    let dir = abs
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| lexical_abs(Path::new(".")));
    let mut modules = HashMap::new();
    if let Some(mods) = root.get("modules") {
        if !mods.is_table() {
            return fail(&abs, mods.line, mods.col, "[modules] must be a table");
        }
        for key in mods.keys() {
            let v = mods.get(key).unwrap();
            if !v.is_string() {
                return fail(
                    &abs,
                    v.line,
                    v.col,
                    &format!("module '{key}' must be a string path"),
                );
            }
            modules.insert(key.clone(), lexical_norm(&dir.join(&v.s)));
        }
    }
    if let Some(profile) = root.get("profile") {
        if !profile.is_table() {
            return fail(&abs, profile.line, profile.col, "[profile] must be a table");
        }
        for key in profile.keys() {
            let v = profile.get(key).unwrap();
            if key != "debug" && key != "release" && key != "test" {
                return fail(&abs, v.line, v.col, &format!("unknown profile '{key}'"));
            }
            if !v.is_table() {
                return fail(
                    &abs,
                    v.line,
                    v.col,
                    &format!("[profile.{key}] must be a table"),
                );
            }
            for inner_key in v.keys() {
                let inner = v.get(inner_key).unwrap();
                let allowed = key == "test" && inner_key == "entry";
                if !allowed {
                    return fail(
                        &abs,
                        inner.line,
                        inner.col,
                        &format!("unknown profile.{key} key '{inner_key}'"),
                    );
                }
            }
        }
        if let Some(test) = profile.get("test") {
            let _ = optional_string(&abs, test, "entry", "profile.test.entry")?;
        }
    }
    Ok(Project {
        file: abs,
        dir,
        modules,
    })
}

fn require_string(file: &Path, table: &Value, key: &str) -> Result<String, Diagnostic> {
    let Some(v) = table.get(key) else {
        return fail(
            file,
            table.line,
            table.col,
            &format!("missing project.{key}"),
        );
    };
    if !v.is_string() {
        return fail(
            file,
            v.line,
            v.col,
            &format!("project.{key} must be a string"),
        );
    }
    if v.s.is_empty() {
        return fail(
            file,
            v.line,
            v.col,
            &format!("project.{key} must not be empty"),
        );
    }
    Ok(v.s.clone())
}

fn optional_string(
    file: &Path,
    table: &Value,
    key: &str,
    label: &str,
) -> Result<String, Diagnostic> {
    let Some(v) = table.get(key) else {
        return Ok(String::new());
    };
    if !v.is_string() {
        return fail(file, v.line, v.col, &format!("{label} must be a string"));
    }
    if v.s.is_empty() {
        return fail(file, v.line, v.col, &format!("{label} must not be empty"));
    }
    Ok(v.s.clone())
}

fn fail<T>(file: &Path, line: u32, col: u32, message: &str) -> Result<T, Diagnostic> {
    Err(Diagnostic::new(
        file.display().to_string(),
        line,
        col,
        "error",
        message,
        "error",
    ))
}

fn lexical_abs(p: &Path) -> PathBuf {
    PathBuf::from(crate::stdlib::lexical_abs_string(p))
}

fn lexical_norm(p: &Path) -> PathBuf {
    lexical_abs(p)
}
