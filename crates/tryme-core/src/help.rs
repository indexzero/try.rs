//! Global help text — byte-for-byte port of `print_global_help`
//! (`try.rb:960-1006`), with one sanctioned substitution: the version token
//! renders this package's version (ADR-0002). Printed to STDERR.

/// Render the help text with the given semver (upstream: `try v1.9.3 - …`).
#[must_use]
pub fn global_help(version: &str) -> String {
    format!(
        r#"try v{version} - ephemeral workspace manager

To use try, add to your shell config:

  # bash/zsh (~/.bashrc or ~/.zshrc)
  eval "$(try init ~/src/tries)"

  # fish (~/.config/fish/config.fish)
  eval (try init ~/src/tries | string collect)

Usage:
  try [query]           Interactive directory selector
  try clone <url>       Clone repo into dated directory
  try worktree <name>   Create worktree from current git repo
  try --help            Show this help

Commands:
  init [path]           Output shell function definition
  clone <url> [name]    Clone git repo into date-prefixed directory
  worktree <name>       Create worktree in dated directory

Examples:
  try                   Open interactive selector
  try project           Selector with initial filter
  try clone https://github.com/user/repo
  try worktree feature-branch

Manual mode (without alias):
  try exec [query]      Output shell script to eval

Environment:
  TRY_PATH          Tries directory (default: ~/src/tries)
  TRY_PROJECTS      Graduate destination (default: parent of TRY_PATH)

Keyboard:
  ↑/↓, Ctrl-P/N     Navigate
  Enter              Select / Create new
  Ctrl-R             Rename
  Ctrl-G             Graduate (promote try to project)
  Ctrl-D             Mark for deletion
  Ctrl-T             Create new try
  Esc                Cancel
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_first_line_and_key_sections() {
        let h = global_help("0.0.0");
        assert!(h.starts_with("try v0.0.0 - ephemeral workspace manager\n"));
        assert!(h.contains("\nUsage:\n  try [query]           Interactive directory selector\n"));
        assert!(h.contains("TRY_PROJECTS      Graduate destination (default: parent of TRY_PATH)"));
        assert!(h.ends_with("Esc                Cancel\n"));
    }
}
