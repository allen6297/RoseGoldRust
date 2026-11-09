mod commands;
mod format;
mod project;
mod scaffold;
mod suite;

pub use commands::{execute, fmt_program, VERSION};
pub use format::{format_source, list_rg_files, FormatResult, Options};

use std::path::PathBuf;

pub fn run(args: &[String]) -> i32 {
    if args.first().map(String::as_str) == Some("ide") {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        rosegold_ide::serve(stdin.lock(), &mut stdout);
        return 0;
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let wants_stdin = args
        .first()
        .map(|c| c == "check" || c == "c" || c == "-c" || c == "--c")
        .unwrap_or(false)
        && args.iter().any(|a| a == "--stdin");
    let stdin = if wants_stdin {
        std::io::read_to_string(std::io::stdin()).unwrap_or_default()
    } else {
        String::new()
    };
    execute(args, &cwd, &stdin)
}
