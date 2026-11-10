use crate::project::Project;
use crate::types::is_host_module;
use rosegold_syntax::{tokenize, ImportDecl, Program, Tok};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

const CHILDREN: &[&str] = &[
    "math",
    "str",
    "collections",
    "helpers",
    "ModAPI",
    "Blocks",
    "Items",
    "Entities",
    "BlockEntities",
    "io",
    "vec",
    "time",
    "path",
    "json",
    "regex",
    "ui",
    "GUIAPI",
    "SettingsAPI",
];

#[derive(Clone, Debug)]
pub struct StdlibExport {
    pub crate_name: String,
    pub kind: String,
}

pub fn is_stdlib_child(name: &str) -> bool {
    CHILDREN.contains(&name)
}

pub fn is_crate_stdlib(name: &str) -> bool {
    name == "std" || is_stdlib_child(name) || (name.len() > 4 && name.starts_with("std."))
}

pub fn canonical_stdlib_name(name: &str) -> String {
    if is_stdlib_child(name) {
        format!("std.{name}")
    } else {
        name.to_string()
    }
}

pub fn generic(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

pub fn lexical_abs_string(p: &Path) -> String {
    lexical_normalize(&generic(&abs_path(p)))
}

fn abs_path(p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(p)
    }
}

pub fn lexical_normalize(path: &str) -> String {
    let unified = path.replace('\\', "/");
    let mut prefix = String::new();
    let mut body = unified.as_str();
    let mut leading_slash = false;
    if body.len() >= 2 && body.as_bytes().get(1) == Some(&b':') {
        prefix = body[..2].to_string();
        body = &body[2..];
    } else if body.starts_with('/') {
        leading_slash = true;
    }
    let mut stack: Vec<&str> = Vec::new();
    for part in body.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            stack.pop();
            continue;
        }
        stack.push(part);
    }
    let mut out = prefix;
    if leading_slash {
        out.push('/');
    }
    for part in stack {
        if !out.is_empty() && !out.ends_with('/') {
            out.push('/');
        }
        if out.ends_with(':') {
            out.push('/');
        }
        out.push_str(part);
    }
    if out.is_empty() {
        "/".to_string()
    } else {
        out
    }
}

pub fn weakly_canonical(p: &Path) -> PathBuf {
    PathBuf::from(lexical_abs_string(p))
}

pub fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}

pub fn same_rg_file(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let pa = Path::new(a);
    let pb = Path::new(b);
    if pa.is_file() && pb.is_file() {
        if same_file(pa, pb) {
            return true;
        }
    }
    lexical_abs_string(pa).to_lowercase() == lexical_abs_string(pb).to_lowercase()
}

pub fn find_stdlib_root(from_file: &str) -> Option<PathBuf> {
    let mut starts = Vec::new();
    if !from_file.is_empty() {
        let p = Path::new(from_file);
        starts.push(
            p.parent()
                .map(|d| d.to_path_buf())
                .unwrap_or_else(|| PathBuf::from(".")),
        );
    }
    starts.push(PathBuf::from("."));
    for start in starts {
        let mut p = abs_path(&start);
        loop {
            let cand = p.join("builtin").join("std");
            if cand.is_dir() {
                return Some(cand);
            }
            if !p.pop() {
                break;
            }
        }
    }
    // A Terralite mod is not inside this repo, so the walk above misses std.
    bundled_stdlib_root()
}

/// `builtin/std` shipped next to this compiler. Used when the script's
/// directory has no standard library of its own.
fn bundled_stdlib_root() -> Option<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../builtin/std");
    if root.is_dir() {
        Some(root)
    } else {
        None
    }
}

pub fn godot_stub_dir(from_file: &str) -> Option<PathBuf> {
    let std_root = find_stdlib_root(from_file)?;
    let parent = std_root.parent()?;
    let dir = parent.join("godot");
    if dir.is_dir() && dir.join("lib.rg").is_file() {
        Some(dir)
    } else {
        None
    }
}

pub fn stdlib_child_dir(root: &Path, child: &str) -> PathBuf {
    let mut dir = root.to_path_buf();
    let mut part = String::new();
    for c in child.chars() {
        if c == '.' {
            if !part.is_empty() {
                dir = dir.join(&part);
                part.clear();
            }
        } else {
            part.push(c);
        }
    }
    if !part.is_empty() {
        dir = dir.join(&part);
    }
    dir
}

pub fn crate_dir(name: &str, from_file: &str) -> Option<PathBuf> {
    let root = find_stdlib_root(from_file)?;
    if name == "std" {
        return Some(root);
    }
    let child = if name.len() > 4 && name.starts_with("std.") {
        &name[4..]
    } else if is_stdlib_child(name) {
        name
    } else {
        return None;
    };
    // `ui` ships with this compiler. A script inside the Java checkout must
    // not pick up `RoseGoldJava/builtin/std/ui`.
    if child == "ui" {
        if let Some(dir) = bundled_ui_dir() {
            return Some(dir);
        }
    }
    if let Some(file) = terralite_api_file(&root, child) {
        return Some(file);
    }
    Some(stdlib_child_dir(&root, child))
}

/// Single-file TerraLite APIs under `builtin/std/TerraLite/`.
fn terralite_api_file(root: &Path, child: &str) -> Option<PathBuf> {
    let name = match child {
        "ModAPI" | "Blocks" | "Items" | "Entities" | "BlockEntities" | "GUIAPI" | "SettingsAPI" => {
            child
        }
        _ => return None,
    };
    let file = root.join("TerraLite").join(format!("{name}.rg"));
    if file.is_file() {
        Some(file)
    } else {
        None
    }
}

fn bundled_ui_dir() -> Option<PathBuf> {
    let dir = bundled_stdlib_root()?.join("ui");
    if dir.join("lib.rg").is_file() {
        Some(dir)
    } else {
        None
    }
}

pub fn project_module(name: &str, from_file: &str) -> Option<PathBuf> {
    let start = if from_file.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(from_file)
    };
    let project = Project::find(&start).ok()??;
    project.module_path(name)
}

pub fn list_rg_files(dir: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if !dir.is_dir() {
        return paths;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return paths;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("rg") {
            paths.push(path);
        }
    }
    paths.sort_by(|a, b| generic(a).cmp(&generic(b)));
    paths
}

pub fn list_child_dirs(dir: &Path) -> Vec<String> {
    let mut kids = Vec::new();
    if !dir.is_dir() {
        return kids;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return kids;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let kid = entry.file_name().to_string_lossy().to_string();
        if kid.is_empty() || kid.starts_with('.') {
            continue;
        }
        kids.push(kid);
    }
    kids.sort();
    kids
}

pub fn lookup(name: &str, from_file: &str) -> Option<StdlibExport> {
    if let Some(hits) = export_index(from_file).get(name) {
        if let Some(hit) = hits.first() {
            return Some(hit.clone());
        }
    }
    project_export_index(from_file)
        .get(name)
        .and_then(|h| h.first().cloned())
}

pub fn import_hint(name: &str, from_file: &str) -> String {
    let mut hits = export_index(from_file)
        .get(name)
        .cloned()
        .unwrap_or_default();
    if hits.is_empty() {
        hits = project_export_index(from_file)
            .get(name)
            .cloned()
            .unwrap_or_default();
    }
    if hits.is_empty() {
        if crate_dir(name, from_file).is_none() {
            if project_module(name, from_file).is_some() {
                return format!(" (in crate {name}; try 'import {name}')");
            }
        }
        return String::new();
    }
    let mut crates = String::new();
    for (i, h) in hits.iter().enumerate() {
        if i > 0 {
            crates.push_str(" or ");
        }
        crates.push_str(&h.crate_name);
    }
    let try_msg = if hits.len() == 1 {
        let h = &hits[0];
        if h.kind == "fn" {
            if h.crate_name == "std" {
                format!("; try 'import std' then 'std.{name}'")
            } else {
                format!(
                    "; try 'import {}' then '{}.{}'",
                    h.crate_name, h.crate_name, name
                )
            }
        } else if h.crate_name == "std" {
            "; try 'import std'".to_string()
        } else {
            format!(
                "; try 'from {} import {name}' or 'import {}'",
                h.crate_name, h.crate_name
            )
        }
    } else {
        format!("; try importing {crates}")
    };
    format!(" (in crate {crates}{try_msg})")
}

#[derive(Clone, Debug, Default)]
pub struct ExportIndex {
    keys: Vec<String>,
    map: HashMap<String, Vec<StdlibExport>>,
}

impl ExportIndex {
    pub fn get(&self, name: &str) -> Option<&Vec<StdlibExport>> {
        self.map.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &[StdlibExport])> {
        self.keys
            .iter()
            .filter_map(|k| self.map.get(k).map(|v| (k.as_str(), v.as_slice())))
    }

    fn list_mut(&mut self, name: String) -> &mut Vec<StdlibExport> {
        if !self.map.contains_key(&name) {
            self.keys.push(name.clone());
            self.map.insert(name.clone(), Vec::new());
        }
        self.map.get_mut(&name).unwrap()
    }
}

pub fn export_index(from_file: &str) -> ExportIndex {
    let root = find_stdlib_root(from_file);
    let key = root
        .as_ref()
        .map(|r| generic(&weakly_canonical(r)))
        .unwrap_or_default();
    let mut guard = INDEX.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((cached, idx)) = guard.as_ref() {
        if cached == &key {
            return idx.clone();
        }
    }
    let mut idx = ExportIndex::default();
    if let Some(root) = &root {
        index_dir(root, "std", &mut idx);
        for child in CHILDREN {
            if *child == "ui" {
                if let Some(ui) = bundled_ui_dir() {
                    index_dir(&ui, "ui", &mut idx);
                    continue;
                }
            }
            if let Some(file) = terralite_api_file(root, child) {
                if let Ok(src) = std::fs::read_to_string(&file) {
                    index_source(&src, child, &mut idx);
                }
                continue;
            }
            index_dir(&root.join(child), child, &mut idx);
        }
    }
    *guard = Some((key, idx.clone()));
    idx
}

fn project_export_index(from_file: &str) -> ExportIndex {
    let start = if from_file.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(from_file)
    };
    let Ok(Some(project)) = Project::find(&start) else {
        return ExportIndex::default();
    };
    let key = generic(&weakly_canonical(&project.file));
    let mut guard = PROJECT_INDEX.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((cached, idx)) = guard.as_ref() {
        if cached == &key {
            return idx.clone();
        }
    }
    let mut idx = ExportIndex::default();
    for (crate_name, mapped) in &project.modules {
        if mapped.is_file() {
            if let Ok(src) = std::fs::read_to_string(mapped) {
                index_source(&src, crate_name, &mut idx);
            }
        } else {
            index_dir(mapped, crate_name, &mut idx);
        }
    }
    *guard = Some((key, idx.clone()));
    idx
}

static INDEX: Mutex<Option<(String, ExportIndex)>> = Mutex::new(None);
static PROJECT_INDEX: Mutex<Option<(String, ExportIndex)>> = Mutex::new(None);

fn index_dir(dir: &Path, crate_name: &str, out: &mut ExportIndex) {
    for path in list_rg_files(dir) {
        if let Ok(src) = std::fs::read_to_string(&path) {
            index_source(&src, crate_name, out);
        }
    }
}

fn index_source(source: &str, crate_name: &str, out: &mut ExportIndex) {
    let mut junk = Vec::new();
    let tokens = tokenize(source, "", &mut junk);
    let mut depth = 0i32;
    for i in 0..tokens.len().saturating_sub(1) {
        match tokens[i].kind {
            Tok::LBrace => {
                depth += 1;
                continue;
            }
            Tok::RBrace => {
                if depth > 0 {
                    depth -= 1;
                }
                continue;
            }
            _ => {}
        }
        if depth != 0 {
            continue;
        }
        let kind = match tokens[i].kind {
            Tok::Function => "fn",
            Tok::Struct => "struct",
            Tok::Data => "data",
            Tok::Class => "class",
            Tok::Trait => "trait",
            Tok::Enum => "enum",
            _ => continue,
        };
        if tokens[i + 1].kind != Tok::Identifier {
            continue;
        }
        let name = tokens[i + 1].text.clone();
        let list = out.list_mut(name);
        if list.iter().any(|e| e.crate_name == crate_name) {
            continue;
        }
        list.push(StdlibExport {
            crate_name: crate_name.to_string(),
            kind: kind.to_string(),
        });
    }
}

pub fn host(name: &str) -> bool {
    is_host_module(name)
}

pub fn stdlib_children() -> &'static [&'static str] {
    CHILDREN
}

/// Java `Path.toAbsolutePath().normalize()` without resolving symlinks.
pub fn java_abs(p: &Path) -> PathBuf {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(p)
    };
    let mut out = PathBuf::new();
    for comp in abs.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

pub fn files_for_crate(crate_name: &str, from_file: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let dir = match crate_dir(crate_name, from_file) {
        Some(dir) => dir,
        None => match project_module(crate_name, from_file) {
            Some(dir) => dir,
            None => return files,
        },
    };
    if dir.is_file() {
        files.push(java_abs(&dir));
        return files;
    }
    for file in list_rg_files(&dir) {
        files.push(java_abs(&file));
    }
    files
}

pub fn crate_from_imports(program: &Program, local: &str) -> String {
    if local.is_empty() {
        return String::new();
    }
    for im in &program.imports {
        if im.is_from || im.path.is_empty() {
            continue;
        }
        let bind = if im.alias.is_empty() {
            im.path.last().map(|s| s.as_str()).unwrap_or("")
        } else {
            im.alias.as_str()
        };
        if local == bind {
            return im.path.join(".");
        }
    }
    String::new()
}

pub fn crate_of(local: &str, program: &Program, from_file: &str) -> String {
    if local.is_empty() {
        return String::new();
    }
    if !files_for_crate(local, from_file).is_empty() {
        return local.to_string();
    }
    let aliased = crate_from_imports(program, local);
    if !aliased.is_empty() && !files_for_crate(&aliased, from_file).is_empty() {
        return aliased;
    }
    String::new()
}

fn from_bind<'a>(program: &'a Program, local: &str) -> Option<&'a ImportDecl> {
    if local.is_empty() {
        return None;
    }
    for im in &program.imports {
        if !im.is_from || im.path.len() < 2 {
            continue;
        }
        let exported = im.path[1].as_str();
        let bind = if im.alias.is_empty() {
            exported
        } else {
            im.alias.as_str()
        };
        if local == bind {
            return Some(im);
        }
    }
    None
}

pub fn from_bind_crate(program: &Program, local: &str) -> String {
    from_bind(program, local)
        .map(|im| im.path[0].clone())
        .unwrap_or_default()
}

pub fn from_bind_exported(program: &Program, local: &str) -> String {
    from_bind(program, local)
        .map(|im| im.path[1].clone())
        .unwrap_or_default()
}

pub fn crate_name_of_file(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    let Some(root) = find_stdlib_root(path) else {
        return String::new();
    };
    let file_s = lexical_abs_string(Path::new(path));
    let root_s = lexical_abs_string(&root);
    let rel = if let Some(rest) = file_s.strip_prefix(&root_s) {
        rest
    } else if file_s.to_lowercase().starts_with(&root_s.to_lowercase())
        && file_s.len() >= root_s.len()
    {
        &file_s[root_s.len()..]
    } else {
        return String::new();
    };
    let rel = rel.trim_start_matches('/');
    if rel.is_empty() || rel == "." || rel.starts_with("..") {
        return String::new();
    }
    let parent = match Path::new(rel).parent() {
        Some(p) => generic(p),
        None => String::new(),
    };
    if parent.is_empty() || parent == "." {
        return "std".to_string();
    }
    let mut crate_name = String::from("std");
    for part in parent.split('/') {
        if part.is_empty() {
            continue;
        }
        crate_name.push('.');
        crate_name.push_str(part);
    }
    crate_name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_outside_this_repo_still_finds_std_math() {
        let root = find_stdlib_root(r"C:\__rg_no_stdlib__\mods\probe.rg").expect("bundled std");
        let bundled = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../builtin/std");
        assert!(same_file(&root, &bundled));
        assert!(root.join("math").join("lib.rg").is_file());
        assert!(root.join("str").join("lib.rg").is_file());
        assert!(root.join("collections").join("lib.rg").is_file());
        assert!(root.join("helpers").join("lib.rg").is_file());
        assert!(crate_name_of_file(r"C:\__rg_no_stdlib__\mods\probe.rg").is_empty());
    }

    #[test]
    fn ui_loads_from_the_rust_tree_even_inside_the_java_checkout() {
        let from = r"C:\CODE\RoseGoldJava\tests\pass\stdlib_ui.rg";
        let dir = crate_dir("ui", from).expect("ui");
        let bundled = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../builtin/std/ui");
        assert!(same_file(&dir, &bundled), "{}", dir.display());
        assert!(dir.join("lib.rg").is_file());
        assert!(dir.join("color.rg").is_file());
        assert!(dir.join("widgets.rg").is_file());
        let shown = generic(&weakly_canonical(&dir)).to_lowercase();
        assert!(shown.contains("rosegoldrust/builtin/std/ui"));
        assert!(!shown.contains("rosegoldjava"));
        let std_ui = crate_dir("std.ui", from).expect("std.ui");
        assert!(same_file(&std_ui, &bundled));
    }

    #[test]
    fn mod_api_loads_from_terralite_not_the_old_tlmodapi_folder() {
        let from = r"C:\CODE\TerraLite\mods\k's-guide\src\mod.rg";
        let file = crate_dir("ModAPI", from).expect("ModAPI");
        let bundled = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../builtin/std/TerraLite/ModAPI.rg");
        assert!(same_file(&file, &bundled), "{}", file.display());
        assert!(file.is_file());
        let shown = generic(&weakly_canonical(&file)).to_lowercase();
        assert!(shown.contains("rosegoldrust/builtin/std/terralite/modapi.rg"));
        assert!(!shown.contains("/tlmodapi"));
        let std_mod = crate_dir("std.ModAPI", from).expect("std.ModAPI");
        assert!(same_file(&std_mod, &bundled));
        assert!(crate_dir("TLModAPI", from).is_none());
    }

    #[test]
    fn terralite_api_files_accept_host_calls() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../builtin/std/TerraLite");
        for name in [
            "ModAPI.rg",
            "Blocks.rg",
            "Items.rg",
            "Entities.rg",
            "BlockEntities.rg",
            "GUIAPI.rg",
            "SettingsAPI.rg",
        ] {
            let path = root.join(name);
            let source = std::fs::read_to_string(&path).expect(name);
            let diags = crate::check_source(&source, &path.display().to_string());
            let errors: Vec<_> = diags
                .iter()
                .filter(|d| d.severity == "error")
                .map(|d| format!("{name}:{} {}", d.line, d.message))
                .collect();
            assert!(errors.is_empty(), "{}", errors.join("\n"));
        }
    }

    #[test]
    fn ember_mod_typechecks() {
        let path = PathBuf::from(r"C:\CODE\TerraLite\mods\ember.rg");
        let source = std::fs::read_to_string(&path).expect("ember.rg");
        let diags = crate::check_source(&source, &path.display().to_string());
        let errors: Vec<_> = diags
            .iter()
            .filter(|d| d.severity == "error")
            .map(|d| format!("{}:{} {}", d.line, d.col, d.message))
            .collect();
        assert!(errors.is_empty(), "{}", errors.join("\n"));
    }

    #[test]
    fn blocks_and_items_load_from_terralite() {
        let from = r"C:\CODE\TerraLite\mods\ember.rg";
        for name in ["Blocks", "Items", "Entities", "BlockEntities"] {
            let file = crate_dir(name, from).unwrap_or_else(|| panic!("{name}"));
            let bundled = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../builtin/std/TerraLite")
                .join(format!("{name}.rg"));
            assert!(same_file(&file, &bundled), "{}", file.display());
            assert!(file.is_file());
        }
    }
}
