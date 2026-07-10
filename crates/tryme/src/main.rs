//! `tryme` — binary entry point for try-me-maybe, a Rust port of tobi/try.
//!
//! The binary is deliberately thin (`main.rs` stays under 200 lines); all
//! test-observable behavior lives in `tryme_core`. The user-facing `try`
//! command is the shell function emitted by `tryme init` (ADR-0001).

mod cli;

use std::io::Write as _;
use std::process::ExitCode;
use tryme_core::{Ctx, Env, ScriptOut};

fn main() -> ExitCode {
    let mut os_iter = std::env::args_os();
    // argv[0] is the wrapper-visible self path: expand_path'd downstream,
    // never canonicalized (symlink resolution would break the wrapper-path
    // contract the conformance runner greps for).
    let arg0 = os_iter
        .next()
        .map_or_else(|| "tryme".to_string(), |a| a.to_string_lossy().into_owned());
    let cli_args: Vec<String> = os_iter.map(|a| a.to_string_lossy().into_owned()).collect();

    // Build-time escape hatch: emit the usage KDL spec (committed as
    // try.usage.kdl; completions/manpage/docs generate from it).
    if cli_args.iter().any(|a| a == "--usage-spec") {
        let mut out = ScriptOut::new(std::io::stdout().lock());
        let _ = out.raw(&cli::usage_spec());
        return ExitCode::SUCCESS;
    }

    let ctx = Ctx {
        env: Env::from_process(),
        cwd: std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from(".")),
        arg0,
        version: env!("CARGO_PKG_VERSION").to_string(),
        today: chrono::Local::now().format("%Y-%m-%d").to_string(),
    };

    let mut err = std::io::stderr().lock();
    let code = {
        let mut out = ScriptOut::new(std::io::stdout().lock());
        tryme_core::run(cli_args, &ctx, &mut err, &mut out)
    };
    let _ = err.flush();
    ExitCode::from(code)
}
