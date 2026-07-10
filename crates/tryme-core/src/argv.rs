//! Argv normalization — a direct port of upstream's ARGV surgery
//! (`try.rb:1008-1092`). clap cannot model this (flags anywhere, last-wins
//! `--path`, unknown tokens becoming the query), so the declaration in the
//! binary's `cli` module is interface-only and this module is the runtime
//! parser. An inventory-diff unit test chains the two.

/// Result of the extraction pass, in upstream's processing order.
#[derive(Debug, Default, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "1:1 record of upstream's independent boolean flags — not a state machine"
)]
pub struct Normalized {
    /// `--no-colors` / `--no-expand-tokens` seen (all occurrences deleted).
    pub colors_disabled: bool,
    /// `--help` / `-h` anywhere.
    pub help: bool,
    /// `--version` / `-v` anywhere.
    pub version: bool,
    /// Last-wins `--path VALUE` / `--path=VALUE`, if any.
    pub path: Option<String>,
    /// `--and-type VALUE` (test-only).
    pub and_type: Option<String>,
    /// `--and-exit` (test-only).
    pub and_exit: bool,
    /// `--and-keys SPEC` raw value (test-only).
    pub and_keys_raw: Option<String>,
    /// `--and-confirm VALUE` (test-only): substitutes for typed
    /// delete-confirmation input; only the literal string `YES` confirms.
    pub and_confirm: Option<String>,
    /// Everything left, in order: `rest[0]` is the command, the remainder
    /// its args.
    pub rest: Vec<String>,
}

/// Port of `extract_option_with_value!` (`try.rb:1028-1037`): find the LAST
/// `--name` or `--name=VALUE` (rindex), remove it, and take its value —
/// which for the space form is the following argument (None if the flag was
/// final).
fn extract_option_with_value(args: &mut Vec<String>, opt: &str) -> Option<String> {
    let prefix = format!("{opt}=");
    let i = args
        .iter()
        .rposition(|a| a == opt || a.starts_with(&prefix))?;
    let arg = args.remove(i);
    if let Some(eq) = arg.find('=') {
        Some(arg[eq + 1..].to_string())
    } else if i < args.len() {
        Some(args.remove(i))
    } else {
        None
    }
}

/// Ruby `ARGV.delete(x)`: remove ALL occurrences, return whether any existed.
fn delete_all(args: &mut Vec<String>, value: &str) -> bool {
    let before = args.len();
    args.retain(|a| a != value);
    args.len() != before
}

/// Run the full extraction pass in upstream's exact order
/// (`try.rb:1008-1092`): colors → help → version → `--path` → test flags →
/// remainder.
#[must_use]
pub fn normalize(mut args: Vec<String>) -> Normalized {
    // Field order below is upstream's processing order — it matters because
    // each step mutates args before the next looks at them.
    let colors_disabled =
        delete_all(&mut args, "--no-colors") | delete_all(&mut args, "--no-expand-tokens");
    let help = args.iter().any(|a| a == "--help" || a == "-h");
    let version = args.iter().any(|a| a == "--version" || a == "-v");
    let path = extract_option_with_value(&mut args, "--path");
    let and_type = extract_option_with_value(&mut args, "--and-type");
    let and_exit = delete_all(&mut args, "--and-exit");
    let and_keys_raw = extract_option_with_value(&mut args, "--and-keys");
    let and_confirm = extract_option_with_value(&mut args, "--and-confirm");

    Normalized {
        colors_disabled,
        help,
        version,
        path,
        and_type,
        and_exit,
        and_keys_raw,
        and_confirm,
        rest: args,
    }
}

/// The flag inventory this normalizer understands — chained against the clap
/// declaration by the binary's inventory-diff test so the two sources of
/// truth cannot drift.
pub const FLAG_INVENTORY: &[&str] = &[
    "--no-colors",
    "--no-expand-tokens",
    "--help",
    "-h",
    "--version",
    "-v",
    "--path",
    "--and-type",
    "--and-exit",
    "--and-keys",
    "--and-confirm",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn v(args: &[&str]) -> Vec<String> {
        args.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn path_last_wins_and_earlier_occurrences_stay_in_argv() {
        // Ruby's extract_option_with_value! removes only the rindex match:
        // earlier --path occurrences REMAIN and become command/query tokens.
        // Quirky, but it is the shipped behavior — port it, don't fix it.
        let n = normalize(v(&["--path", "/a", "clone", "--path=/b", "url"]));
        assert_eq!(n.path.as_deref(), Some("/b"));
        assert_eq!(n.rest, v(&["--path", "/a", "clone", "url"]));

        let n = normalize(v(&["--path=/a", "x", "--path", "/b"]));
        assert_eq!(n.path.as_deref(), Some("/b"));
        assert_eq!(n.rest, v(&["--path=/a", "x"]));
    }

    #[test]
    fn trailing_flag_without_value_yields_none() {
        let n = normalize(v(&["exec", "--path"]));
        assert_eq!(n.path, None);
        assert_eq!(n.rest, v(&["exec"]));
    }

    #[test]
    fn flags_anywhere_and_unknowns_become_query() {
        // Ruby checks ARGV.include? and exits — help/version flags are never
        // deleted, so they remain in rest (harmless: dispatch exits first).
        let n = normalize(v(&["some", "-h", "query"]));
        assert!(n.help);
        assert_eq!(n.rest, v(&["some", "-h", "query"]));

        let n = normalize(v(&["--weird", "query"]));
        assert!(!n.help);
        assert_eq!(n.rest, v(&["--weird", "query"])); // unknown flags are query text
    }

    #[test]
    fn color_aliases_delete_all_occurrences() {
        let n = normalize(v(&[
            "--no-colors",
            "a",
            "--no-colors",
            "--no-expand-tokens",
        ]));
        assert!(n.colors_disabled);
        assert_eq!(n.rest, v(&["a"]));
    }

    #[test]
    fn test_flags_extracted_before_command_shift() {
        let n = normalize(v(&["--and-keys", "ENTER", "exec", "--and-exit"]));
        assert_eq!(n.and_keys_raw.as_deref(), Some("ENTER"));
        assert!(n.and_exit);
        assert_eq!(n.rest, v(&["exec"]));
    }
}
