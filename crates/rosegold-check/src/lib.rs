mod checker;
mod project;
mod stdlib;
mod toml;
mod typecheck;
mod types;
mod unused;

pub use checker::{
    check_interpreted, check_owned, godot_project, Checker, EnumRc, FieldHit, FnRc, LoadedMod,
    ModRc, StructRc, TraitRc,
};
pub use project::Project;
pub use stdlib::{
    crate_dir, crate_from_imports, crate_name_of_file, crate_of, export_index, files_for_crate,
    from_bind_crate, from_bind_exported, generic, godot_stub_dir, java_abs, list_rg_files, lookup,
    same_rg_file, stdlib_children, weakly_canonical, ExportIndex, StdlibExport,
};
pub use types::{is_host_module, type_head};

pub fn check_source(source: &str, path: &str) -> Vec<rosegold_syntax::Diagnostic> {
    let (program, mut diags) = rosegold_syntax::parse_source(source, path);
    let godot = checker::godot_project(path);
    checker::check(program, source, path, &mut diags, godot);
    diags
}
