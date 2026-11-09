use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn rg(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let exe = env!("CARGO_BIN_EXE_rg");
    let out = Command::new(exe)
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn rg");
    let code = out.status.code().unwrap_or(1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (code, stdout, stderr)
}

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn new_check_run_test_fmt() {
    let dir = temp("rg-new");
    let (code, stdout, stderr) = rg(&dir, &["new"]);
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert!(
        stdout.contains("created project.toml, main.rg, tests.rg"),
        "{stdout}"
    );
    assert!(dir.join("project.toml").is_file());
    assert!(dir.join("main.rg").is_file());
    assert!(dir.join("tests.rg").is_file());

    let (code, stdout, stderr) = rg(&dir, &["check", "main.rg"]);
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");

    let (code, stdout, stderr) = rg(&dir, &["run"]);
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");

    let (code, stdout, stderr) = rg(&dir, &["test"]);
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert!(stdout.contains("ok ok"), "{stdout}");
    assert!(stdout.contains("tests passed"), "{stdout}");

    let (code, stdout, stderr) = rg(&dir, &["fmt", "--check", "main.rg"]);
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");

    let (code, stdout, stderr) = rg(&dir, &["version"]);
    assert_eq!(code, 0, "{stderr}");
    assert!(stdout.contains("version"), "{stdout}");

    let (code, stdout, stderr) = rg(&dir, &["new", "module", "greet"]);
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert!(stdout.contains("created module greet"), "{stdout}");
    assert!(dir.join("greet").join("lib.rg").is_file());
    let toml = fs::read_to_string(dir.join("project.toml")).unwrap();
    assert!(toml.contains("greet = \"greet\""), "{toml}");

    let (code, stdout, stderr) = rg(&dir, &["help"]);
    assert_eq!(code, 0, "{stderr}");
    for word in ["check", "run", "test", "fmt", "new", "ide"] {
        assert!(stdout.contains(word), "{stdout}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn pass_fail_directory() {
    let dir = temp("rg-suite");
    fs::create_dir_all(dir.join("pass")).unwrap();
    fs::create_dir_all(dir.join("fail")).unwrap();
    fs::write(
        dir.join("pass").join("ok.rg"),
        "fn main(): Int { return 0; }\n",
    )
    .unwrap();
    fs::write(
        dir.join("fail").join("bad.rg"),
        "// expect: undefined variable\nfn main(): Int {\n    return missing;\n}\n",
    )
    .unwrap();
    let (code, stdout, stderr) = rg(&dir, &["test", "."]);
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert!(stdout.contains("file tests passed"), "{stdout}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn project_test_profile_entry() {
    let dir = temp("rg-profile");
    fs::write(
        dir.join("project.toml"),
        "[project]\nname = \"app\"\nentry = \"main.rg\"\n\n[profile.test]\nentry = \"suite.rg\"\n",
    )
    .unwrap();
    fs::write(dir.join("main.rg"), "fn main(): Int { return 0; }\n").unwrap();
    fs::write(
        dir.join("suite.rg"),
        "@test\nfn profile_case() {\n    assert(1 == 1);\n}\n",
    )
    .unwrap();
    let (code, stdout, stderr) = rg(&dir, &["test"]);
    assert_eq!(code, 0, "stdout={stdout} stderr={stderr}");
    assert!(stdout.contains("ok profile_case"), "{stdout}");
    assert!(stdout.contains("tests passed"), "{stdout}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ide_reads_stdin_one_line_at_a_time() {
    let dir = temp("rg-ide");
    let exe = env!("CARGO_BIN_EXE_rg");
    let mut child = Command::new(exe)
        .arg("ide")
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn rg ide");
    {
        let mut stdin = child.stdin.take().unwrap();
        write!(
            stdin,
            "{{\"id\":1,\"method\":\"nope\"}}\n\n{{\"id\":2,\"method\":\"ping\"}}\n"
        )
        .unwrap();
    }
    let out = child.wait_with_output().expect("wait rg ide");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout={stdout} stderr={stderr}"
    );
    let lines: Vec<_> = stdout.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 2, "{stdout}");
    assert!(lines[0].contains("\"ok\":false"), "{}", lines[0]);
    assert!(lines[0].contains("unknown method"), "{}", lines[0]);
    assert!(lines[1].contains("\"id\":2"), "{}", lines[1]);
    assert!(lines[1].contains("\"ok\":true"), "{}", lines[1]);

    let (code, stdout, stderr) = rg(&dir, &["i"]);
    assert_eq!(code, 2, "stdout={stdout} stderr={stderr}");
    assert!(stderr.contains("Usage: ir"), "{stderr}");
    let _ = fs::remove_dir_all(&dir);
}
