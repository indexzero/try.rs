//! clap **declaration** of the CLI surface — interface spec, not the runtime
//! parser.
//!
//! Upstream's argv handling (flags anywhere, last-wins `--path`, unknown
//! tokens becoming the query) cannot be modeled by clap's runtime, so
//! `tryme_core::argv` parses at runtime and this declaration exists to:
//! 1. feed `clap_usage` → the committed `try.usage.kdl` (completions,
//!    manpage, and docs all generate from that one spec), and
//! 2. hold the help prose for generated docs.
//!
//! An inventory-diff test below chains this declaration to the runtime
//! parser's flag table so the two sources of truth cannot drift.

use clap::{Arg, ArgAction, Command};

/// Build the declarative `clap::Command` for the whole surface.
pub fn command() -> Command {
    Command::new("tryme")
        .about("Ephemeral workspace manager — fuzzy-searchable, dated experiment directories")
        .long_about(
            "A Rust port of tobi/try. The TUI renders on stderr and the chosen \
             action is emitted as a shell script on stdout; the `try` shell \
             function (from `tryme init`) evals it. Conformance target: \
             tobi/try v1.9.3.",
        )
        .disable_help_flag(true)
        .disable_version_flag(true)
        .arg(
            Arg::new("help")
                .long("help")
                .short('h')
                .action(ArgAction::SetTrue)
                .global(true)
                .help("Show help (printed to stderr)"),
        )
        .arg(
            Arg::new("version")
                .long("version")
                .short('v')
                .action(ArgAction::SetTrue)
                .help("Show version (printed to stderr)"),
        )
        .arg(
            Arg::new("path")
                .long("path")
                .value_name("PATH")
                .env("TRY_PATH")
                .help("Tries directory (default: ~/src/tries); last occurrence wins"),
        )
        .arg(
            Arg::new("no-colors")
                .long("no-colors")
                .action(ArgAction::SetTrue)
                .help("Disable ANSI styling (NO_COLOR env is also honored)"),
        )
        .arg(
            Arg::new("no-expand-tokens")
                .long("no-expand-tokens")
                .action(ArgAction::SetTrue)
                .help("Alias of --no-colors (historical name)"),
        )
        .arg(hidden_flag(
            "and-exit",
            "Render one frame and exit (test harness)",
        ))
        .arg(hidden_value(
            "and-type",
            "Pre-seed the input buffer (test harness)",
        ))
        .arg(hidden_value(
            "and-keys",
            "Inject a key sequence (test harness)",
        ))
        .arg(hidden_value(
            "and-confirm",
            "Substitute for delete-confirmation input; only literal YES confirms (test harness)",
        ))
        .arg(
            Arg::new("usage-spec")
                .long("usage-spec")
                .action(ArgAction::SetTrue)
                .hide(true)
                .help("Emit the usage KDL spec on stdout and exit"),
        )
        .subcommand(
            Command::new("init")
                .about("Output the `try` shell wrapper function")
                .arg(
                    Arg::new("path")
                        .value_name("ABS_PATH")
                        .help("Absolute tries path baked into the wrapper (must start with /)"),
                ),
        )
        .subcommand(
            Command::new("install")
                .about("Append the wrapper to your shell rc file (idempotent)")
                .arg(Arg::new("path").value_name("ABS_PATH")),
        )
        .subcommand(
            Command::new("clone")
                .about("Clone a git repo into a date-prefixed try directory")
                .arg(Arg::new("uri").value_name("GIT_URI").required(true))
                .arg(Arg::new("name").value_name("NAME")),
        )
        .subcommand(
            Command::new("worktree")
                .about("Create a detached worktree try from a git repo (or `dir` for the cwd)")
                .arg(Arg::new("repo").value_name("REPO|dir"))
                .arg(Arg::new("name").value_name("NAME")),
        )
        .subcommand(
            Command::new("exec")
                .about("Manual mode: emit the shell script on stdout (what the wrapper calls)")
                .arg(Arg::new("args").num_args(0..).trailing_var_arg(true)),
        )
}

fn hidden_flag(name: &'static str, help: &'static str) -> Arg {
    Arg::new(name)
        .long(name)
        .action(ArgAction::SetTrue)
        .hide(true)
        .help(help)
}

fn hidden_value(name: &'static str, help: &'static str) -> Arg {
    Arg::new(name)
        .long(name)
        .value_name("VALUE")
        .hide(true)
        .help(help)
}

/// Generate the usage KDL spec from the declaration
/// (`clap_usage::generate`, verified against docs.rs 2026-07-10).
pub fn usage_spec() -> String {
    let mut buf = Vec::new();
    clap_usage::generate(&mut command(), "tryme", &mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// The inventory-diff test: every long/short flag the runtime normalizer
    /// understands must be declared here, and every non-subcommand flag
    /// declared here must be in the normalizer's table (or be the spec-only
    /// `--usage-spec`). Add a flag to one side and this fails.
    #[test]
    fn clap_declaration_matches_runtime_normalizer_inventory() {
        let cmd = command();
        let mut declared = BTreeSet::new();
        for arg in cmd.get_arguments() {
            if let Some(l) = arg.get_long() {
                declared.insert(format!("--{l}"));
            }
            if let Some(s) = arg.get_short() {
                declared.insert(format!("-{s}"));
            }
        }

        let runtime: BTreeSet<String> = tryme_core::argv::FLAG_INVENTORY
            .iter()
            .map(ToString::to_string)
            .collect();

        let missing_in_clap: Vec<_> = runtime.difference(&declared).collect();
        assert!(
            missing_in_clap.is_empty(),
            "flags known to the runtime parser but undeclared in clap: {missing_in_clap:?}"
        );

        let spec_only: BTreeSet<String> = ["--usage-spec".to_string()].into();
        let missing_in_runtime: Vec<_> = declared
            .difference(&runtime)
            .filter(|f| !spec_only.contains(*f))
            .collect();
        assert!(
            missing_in_runtime.is_empty(),
            "flags declared in clap but unknown to the runtime parser: {missing_in_runtime:?}"
        );
    }

    #[test]
    fn usage_spec_generates_and_names_the_binary() {
        let spec = usage_spec();
        assert!(spec.contains("tryme"), "spec should reference the bin name");
        assert!(!spec.is_empty());
    }
}

#[cfg(test)]
mod freshness {
    /// The committed try.usage.kdl must match what the clap declaration
    /// generates — regenerate with `tryme --usage-spec > try.usage.kdl`
    /// after any surface change. (Interface changes become reviewable
    /// diffs of the committed spec.)
    #[test]
    fn committed_usage_spec_is_fresh() {
        let committed =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../try.usage.kdl"))
                .expect("try.usage.kdl must exist at the repo root");
        assert_eq!(
            committed,
            super::usage_spec(),
            "try.usage.kdl is stale — regenerate with `tryme --usage-spec > try.usage.kdl`"
        );
    }
}
