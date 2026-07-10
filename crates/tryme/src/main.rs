//! `tryme` — binary entry point for try-me-maybe, a Rust port of tobi/try.
//!
//! The binary is deliberately thin (`main.rs` stays under 200 lines); all
//! test-observable behavior lives in `tryme_core`. The user-facing `try`
//! command is the shell function emitted by `tryme init` (ADR-0001).

use std::io::Write as _;
use std::process::ExitCode;

fn main() -> ExitCode {
    // M0 skeleton: real dispatch (argv normalizer -> command routing -> script
    // emission) lands in M1. Upstream contract for bare invocation with no
    // args: help on stderr, exit 2 (try.rb:1526) — stubbed here so the CI
    // skeleton exercises the exit-code path end to end.
    let mut stderr = std::io::stderr();
    let _ = writeln!(
        stderr,
        "tryme {} (M0 skeleton — port of tobi/try v1.9.3 in progress)",
        env!("CARGO_PKG_VERSION")
    );
    ExitCode::from(2)
}
