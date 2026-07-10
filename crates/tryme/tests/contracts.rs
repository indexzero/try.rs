//! Integration coverage for contracts the upstream conformance suite never
//! exercises (suite gaps identified during plan review): bare-invocation
//! exit 2, the non-tty error path, `install`, and `--and-confirm` value
//! semantics (only literal `YES` confirms).

use std::process::{Command, Stdio};

fn bin() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_tryme"));
    c.stdin(Stdio::null())
        .env_remove("TRY_PATH")
        .env("TRY_WIDTH", "80")
        .env("TRY_HEIGHT", "24");
    c
}

#[test]
fn bare_invocation_help_on_stderr_exit_2() {
    let out = bin().output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty(), "stdout must stay script-only");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.starts_with("try v"));
    assert!(err.contains("ephemeral workspace manager"));
}

#[test]
fn non_tty_without_keys_errors_then_cancels_on_stdout() {
    // try.rb:57-61 + 1557: error to stderr, selector returns nil, dispatcher
    // prints "Cancelled." on STDOUT, exit 1.
    let tmp = tempfile::tempdir().unwrap();
    let out = bin()
        .args(["--path"])
        .arg(tmp.path())
        .arg("exec")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "Cancelled.\n");
    assert!(String::from_utf8_lossy(&out.stderr)
        .contains("Error: try requires an interactive terminal"));
}

#[test]
fn install_appends_idempotently_to_zshrc() {
    let home = tempfile::tempdir().unwrap();
    let tries = tempfile::tempdir().unwrap();
    let run = || {
        bin()
            .args(["install"])
            .arg(tries.path())
            .env("HOME", home.path())
            .env("SHELL", "/bin/zsh")
            .output()
            .unwrap()
    };
    let out = run();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.is_empty(), "install talks on stderr only");
    let rc = std::fs::read_to_string(home.path().join(".zshrc")).unwrap();
    assert!(rc.contains("# try shell integration"));
    assert!(rc.contains("try() {"));

    // Second run: idempotent, no duplicate block, still exit 0
    let out2 = run();
    assert_eq!(out2.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out2.stderr).contains("already installed"));
    let rc2 = std::fs::read_to_string(home.path().join(".zshrc")).unwrap();
    assert_eq!(rc2.matches("# try shell integration").count(), 1);
}

/// Drive a delete: mark the top entry (Ctrl-D), Enter to reach the
/// confirmation, with `--and-confirm VALUE` supplying the typed answer.
fn delete_with_confirm(confirm: &str) -> (Option<i32>, String) {
    let tries = tempfile::tempdir().unwrap();
    std::fs::create_dir(tries.path().join("2025-11-01-doomed")).unwrap();
    let out = bin()
        .args([
            "--and-keys",
            "CTRL-D, ENTER",
            "--and-confirm",
            confirm,
            "exec",
        ])
        .arg("--path")
        .arg(tries.path())
        .output()
        .unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn and_confirm_yes_emits_delete_script() {
    let (code, stdout) = delete_with_confirm("YES");
    assert_eq!(code, Some(0));
    assert!(stdout.contains("test -d '2025-11-01-doomed' && rm -rf '2025-11-01-doomed'"));
}

#[test]
fn and_confirm_is_a_value_not_a_boolean() {
    // Anything but literal "YES" cancels — including "yes" (try.rb:920).
    for wrong in ["no", "yes", "Y"] {
        let (code, stdout) = delete_with_confirm(wrong);
        assert_eq!(code, Some(1), "--and-confirm {wrong} must cancel");
        assert_eq!(stdout, "Cancelled.\n");
    }
}

#[test]
fn init_from_bare_path_invocation_embeds_real_binary_path() {
    // Regression: `tryme install` invoked as a bare PATH command embedded
    // <cwd>/tryme (nonexistent) in the wrapper because argv[0] has no
    // separator. Ruby never hits this (shebang $0 is always absolute).
    let bin_dir = tempfile::tempdir().unwrap();
    let real = std::path::Path::new(env!("CARGO_BIN_EXE_tryme"));
    let linked = bin_dir.path().join("tryme");
    std::os::unix::fs::symlink(real, &linked).unwrap();

    let cwd = tempfile::tempdir().unwrap();
    let out = std::process::Command::new("tryme") // bare argv[0]
        .args(["init"])
        .current_dir(cwd.path())
        .env("PATH", bin_dir.path())
        .env("SHELL", "/bin/zsh")
        .env("TRY_WIDTH", "80")
        .env("TRY_HEIGHT", "24")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let wrapper = String::from_utf8_lossy(&out.stdout);
    // Must embed the PATH-resolved location (symlink UNresolved), never cwd/tryme
    let embedded = format!("'{}'", linked.display());
    assert!(
        wrapper.contains(&embedded),
        "wrapper should embed {embedded}, got:\n{wrapper}"
    );
    assert!(!wrapper.contains(&format!("{}/tryme", cwd.path().display())));
}
