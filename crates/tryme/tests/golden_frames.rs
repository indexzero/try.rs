//! Byte-exact frame parity against upstream Ruby.
//!
//! `testdata/golden/*.bin` holds stderr captured from upstream at the pinned
//! tag (see `scripts/gen-golden-frames.sh`), 80x24, fixture mtimes relative
//! to now. This test recreates the same fixtures and compares our binary's
//! stderr byte-for-byte — stricter than the conformance suite's greps.

use std::process::Command;
use std::time::{Duration, SystemTime};

fn setup_fixtures(fx: &std::path::Path) {
    // Must mirror scripts/golden-fixtures.sh exactly.
    let mk = |name: &str, secs_ago: u64| {
        let dir = fx.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let mtime = SystemTime::now() - Duration::from_secs(secs_ago);
        let f = std::fs::File::open(&dir).unwrap();
        f.set_times(
            std::fs::FileTimes::new()
                .set_modified(mtime)
                .set_accessed(mtime),
        )
        .unwrap();
    };
    mk("2025-11-01-alpha", 7200);
    mk("2025-11-15-beta", 172_800);
    mk("2025-11-20-gamma", 1_209_600);
    mk("no-date-prefix", 700_000);
}

fn run_case(fx: &std::path::Path, args: &[&str]) -> Vec<u8> {
    let out = Command::new(env!("CARGO_BIN_EXE_tryme"))
        .args(args)
        .arg("--path")
        .arg(fx)
        .env("TRY_WIDTH", "80")
        .env("TRY_HEIGHT", "24")
        .env("NO_COLOR", "")
        .env("NO_COLORS", "")
        .env_remove("TRY_PATH")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("binary runs");
    out.stderr
}

fn golden(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../testdata/golden/{name}.bin",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("golden fixture missing — run scripts/gen-golden-frames.sh")
}

fn diff_hint(ours: &[u8], theirs: &[u8]) -> String {
    let n = ours.iter().zip(theirs).take_while(|(a, b)| a == b).count();
    let ctx = 40;
    let start = n.saturating_sub(ctx);
    format!(
        "first divergence at byte {n}\n ours: {:?}\ntheirs: {:?}",
        String::from_utf8_lossy(&ours[start..(n + ctx).min(ours.len())]),
        String::from_utf8_lossy(&theirs[start..(n + ctx).min(theirs.len())]),
    )
}

fn check(name: &str, args: &[&str]) {
    let tmp = tempfile::tempdir().unwrap();
    setup_fixtures(tmp.path());
    let raw = run_case(tmp.path(), args);
    // Goldens template generation-day dates as {TODAY} (create-new row);
    // apply the same substitution to our bytes before comparing.
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let ours: Vec<u8> = String::from_utf8_lossy(&raw)
        .replace(&today, "{TODAY}")
        .into_bytes();
    let theirs = golden(name);
    assert!(
        ours == theirs,
        "golden frame mismatch for {name}: {}",
        diff_hint(&ours, &theirs)
    );
}

#[test]
fn empty_query_frame() {
    check("empty-query", &["--and-exit", "exec"]);
}

#[test]
fn filtered_beta_frame() {
    check(
        "filtered-beta",
        &["--and-type", "beta", "--and-exit", "exec"],
    );
}

#[test]
fn no_colors_frame() {
    check("no-colors", &["--no-colors", "--and-exit", "exec"]);
}

#[test]
fn nav_keys_frames() {
    check("nav-keys", &["--and-keys", "DOWN, DOWN, ESC", "exec"]);
}
