//! Command dispatch — port of the `__FILE__ == $0` driver
//! (`try.rb:1008-1587`): flag extraction, the command case tree, and the
//! exit-code contract (0 = script emitted, 1 = cancel/error, 2 = bare help).

use crate::argv::{normalize, Normalized};
use crate::emit::ScriptOut;
use crate::env::Env;
use crate::giturl::{generate_clone_directory_name, is_git_uri};
use crate::help::global_help;
use crate::naming::{resolve_unique_name_with_versioning, squeeze_ws_to_hyphen, worktree_path};
use crate::scripts;
use crate::testkeys::parse_test_keys;
use crate::wrappers::{detect_shell, expand_path, init_snippet, is_fish, shell_rc_file, Shell};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Everything an invocation needs besides argv: injected so tests and the
/// conformance suite drive the same code as production.
pub struct Ctx {
    /// Environment snapshot.
    pub env: Env,
    /// Process working directory.
    pub cwd: PathBuf,
    /// Raw `argv[0]` — `expand_path`'d (never canonicalized) for wrapper
    /// emission.
    pub arg0: String,
    /// This package's version (rendered into help/version output).
    pub version: String,
    /// Local date as `YYYY-MM-DD` (upstream calls `Time.now.strftime`).
    pub today: String,
}

/// Run a full invocation. Writes TUI/help/errors to `err` (stderr) and the
/// emitted script to `out` (stdout); returns the process exit code.
pub fn run<W: Write>(
    args: Vec<String>,
    ctx: &Ctx,
    err: &mut dyn Write,
    out: &mut ScriptOut<W>,
) -> u8 {
    let n = normalize(args);

    // Color gate: NO_COLORS at "module load" (tui.rb:25), then the CLI
    // aliases, then NO_COLOR (try.rb:1009-1013)
    crate::tui::set_colors_enabled(ctx.env.no_colors.as_deref().unwrap_or("").is_empty());
    if n.colors_disabled || ctx.env.no_color.as_deref().is_some_and(|v| !v.is_empty()) {
        crate::tui::set_colors_enabled(false);
    }

    // --help / -h anywhere → help on stderr, exit 0 (try.rb:1016-1019)
    if n.help {
        let _ = write!(err, "{}", global_help(&ctx.version));
        return 0;
    }
    // --version / -v anywhere → stderr, exit 0 (try.rb:1022-1025)
    if n.version {
        let _ = writeln!(err, "try {}", ctx.version);
        return 0;
    }

    // --path > TRY_PATH env > ~/src/tries, then expand (try.rb:12,1081-1082)
    let tries_raw = n
        .path
        .clone()
        .or_else(|| ctx.env.try_path.clone())
        .unwrap_or_else(|| "~/src/tries".to_string());
    let tries_path = expand_path(&tries_raw, &ctx.cwd, ctx.env.home.as_deref());

    let mut rest = n.rest.clone();
    if rest.is_empty() {
        // Bare `try`: help + exit 2 (try.rb:1526-1528)
        let _ = write!(err, "{}", global_help(&ctx.version));
        return 2;
    }
    let command = rest.remove(0);

    match command.as_str() {
        "clone" => match cmd_clone(&rest, &tries_path, &ctx.today, err) {
            Ok(cmds) => {
                let _ = out.emit_script(&cmds);
                0
            }
            Err(code) => code,
        },
        "init" => {
            let snippet = build_init_snippet(&rest, ctx, &tries_path);
            let _ = out.raw(&snippet);
            0
        }
        "install" => cmd_install(&rest, ctx, &tries_path, err),
        "exec" => {
            let sub = rest.first().cloned();
            match sub.as_deref() {
                Some("clone") => match cmd_clone(&rest[1..], &tries_path, &ctx.today, err) {
                    Ok(cmds) => {
                        let _ = out.emit_script(&cmds);
                        0
                    }
                    Err(code) => code,
                },
                Some("worktree") => {
                    let cmds = cmd_worktree(&rest[1..], ctx, &tries_path);
                    let _ = out.emit_script(&cmds);
                    0
                }
                other => {
                    // `exec cd` shifts the sub; anything else keeps args
                    // intact (try.rb:1550-1568)
                    let args = if other == Some("cd") {
                        &rest[1..]
                    } else {
                        &rest[..]
                    };
                    finish_cd(args, ctx, &tries_path, &n, err, out)
                }
            }
        }
        "worktree" => {
            let cmds = cmd_worktree(&rest, ctx, &tries_path);
            let _ = out.emit_script(&cmds);
            0
        }
        _ => {
            // Default: try [query] — command becomes part of the query
            // (try.rb:1577-1586)
            let mut args = vec![command];
            args.extend(rest);
            finish_cd(&args, ctx, &tries_path, &n, err, out)
        }
    }
}

/// Shared tail of the `exec`/default branches: emit script + 0, or
/// `Cancelled.` on STDOUT + 1 (try.rb:1552-1568, 1579-1586).
fn finish_cd<W: Write>(
    args: &[String],
    ctx: &Ctx,
    tries_path: &Path,
    n: &Normalized,
    err: &mut dyn Write,
    out: &mut ScriptOut<W>,
) -> u8 {
    match cmd_cd(args, ctx, tries_path, n, err) {
        Ok(Some(cmds)) => {
            let _ = out.emit_script(&cmds);
            0
        }
        Ok(None) => {
            let _ = out.cancelled();
            1
        }
        Err(code) => code,
    }
}

/// Port of `cmd_clone!` (`try.rb:1153-1170`).
fn cmd_clone(
    args: &[String],
    tries_path: &Path,
    today: &str,
    err: &mut dyn Write,
) -> Result<Vec<String>, u8> {
    let Some(git_uri) = args.first() else {
        let _ = writeln!(err, "Error: git URI required for clone command");
        let _ = writeln!(err, "Usage: try clone <git-uri> [name]");
        return Err(1);
    };
    let custom_name = args.get(1).map(String::as_str);
    let Some(dir_name) = generate_clone_directory_name(git_uri, custom_name, today) else {
        let _ = writeln!(err, "Error: Unable to parse git URI: {git_uri}");
        return Err(1);
    };
    Ok(scripts::script_clone(&tries_path.join(dir_name), git_uri))
}

/// Port of the `worktree` command body (`try.rb:1545-1549, 1570-1576`),
/// including the literal `dir` token quirk: `try worktree dir <name>`
/// treats `dir` as "use the cwd", not a repo path.
fn cmd_worktree(args: &[String], ctx: &Ctx, tries_path: &Path) -> Vec<String> {
    let repo = args.first();
    let repo_dir = match repo {
        Some(r) if r != "dir" => expand_path(r, &ctx.cwd, ctx.env.home.as_deref()),
        _ => ctx.cwd.clone(),
    };
    let custom = args.get(1..).unwrap_or(&[]).join(" ");
    let full_path = worktree_path(tries_path, &repo_dir, &custom, &ctx.today);
    let repo_arg = if repo_dir == ctx.cwd {
        None
    } else {
        Some(repo_dir.as_path())
    };
    scripts::script_worktree(&full_path, repo_arg, &ctx.cwd)
}

/// Port of `cmd_cd!` (`try.rb:1315-1386`): clone passthrough, the
/// dot-shorthand, the git-URL shorthand, then the interactive selector.
fn cmd_cd(
    args: &[String],
    ctx: &Ctx,
    tries_path: &Path,
    n: &Normalized,
    err: &mut dyn Write,
) -> Result<Option<Vec<String>>, u8> {
    use crate::selector::Selection;

    if args.first().map(String::as_str) == Some("clone") {
        return cmd_clone(&args[1..], tries_path, &ctx.today, err).map(Some);
    }

    // try . [name] / try ./path [name] (try.rb:1321-1345)
    if let Some(path_arg) = args.first().filter(|a| a.starts_with('.')) {
        let custom = args[1..].join(" ");
        let repo_dir = expand_path(path_arg, &ctx.cwd, ctx.env.home.as_deref());
        if path_arg == "." && custom.trim().is_empty() {
            let _ = writeln!(err, "Error: 'try .' requires a name argument");
            let _ = writeln!(err, "Usage: try . <name>");
            return Err(1);
        }
        let base = if custom.trim().is_empty() {
            repo_dir
                .file_name()
                .map_or_else(String::new, |f| f.to_string_lossy().into_owned())
        } else {
            squeeze_ws_to_hyphen(&custom)
        };
        let base = resolve_unique_name_with_versioning(tries_path, &ctx.today, &base);
        let full_path = tries_path.join(format!("{}-{base}", ctx.today));
        // Worktree when .git exists — file (worktrees) OR directory (repos)
        return Ok(Some(if repo_dir.join(".git").exists() {
            scripts::script_worktree(&full_path, Some(&repo_dir), &ctx.cwd)
        } else {
            scripts::script_mkdir_cd(&full_path)
        }));
    }

    let search_term = args.join(" ");

    // Git URL shorthand → clone workflow (try.rb:1350-1359)
    if is_git_uri(search_term.split_whitespace().next().unwrap_or("")) {
        let mut parts = search_term.splitn(2, char::is_whitespace);
        let git_uri = parts.next().unwrap_or("").to_string();
        let custom_name = parts.next().map(str::trim_start).filter(|s| !s.is_empty());
        let Some(dir_name) = generate_clone_directory_name(&git_uri, custom_name, &ctx.today)
        else {
            let _ = writeln!(err, "Error: Unable to parse git URI: {git_uri}");
            return Err(1);
        };
        return Ok(Some(scripts::script_clone(
            &tries_path.join(dir_name),
            &git_uri,
        )));
    }

    // Regular interactive selector (try.rb:1361-1385)
    let test_keys = n.and_keys_raw.as_deref().and_then(parse_test_keys);
    let selector = crate::selector::Selector::new(
        &search_term,
        tries_path.to_path_buf(),
        &ctx.env,
        n.and_type.as_deref(),
        n.and_exit,
        test_keys,
        n.and_confirm.clone(),
    );
    let Some(selection) = selector.run() else {
        return Ok(None);
    };
    Ok(Some(match selection {
        Selection::Cd { path } => scripts::script_cd(&path),
        Selection::Mkdir { path } => scripts::script_mkdir_cd(&path),
        Selection::Rename {
            base_path,
            old,
            new,
        } => scripts::script_rename(&base_path, &old, &new),
        Selection::Ascend {
            source,
            dest,
            basename,
            base_path,
        } => scripts::script_ascend(&source, &dest, &basename, &base_path),
        Selection::Delete {
            basenames,
            base_path,
        } => scripts::script_delete(&basenames, &base_path, &ctx.cwd),
    }))
}

/// Port of `cmd_init!` (`try.rb:1172-1183`): positional path only when it
/// starts with `/`; fish vs bash selection via `fish?`.
fn build_init_snippet(args: &[String], ctx: &Ctx, tries_path: &Path) -> String {
    let script_path = expand_path(&ctx.arg0, &ctx.cwd, ctx.env.home.as_deref());
    let explicit_path = args
        .first()
        .filter(|a| a.starts_with('/'))
        .map(|a| expand_path(a, &ctx.cwd, ctx.env.home.as_deref()));
    let shell = if is_fish(&ctx.env) {
        Shell::Fish
    } else {
        Shell::Bash
    };
    init_snippet(shell, &script_path, explicit_path.as_deref(), tries_path)
}

/// Port of `cmd_install!` (`try.rb:1185-1228`): detect shell, locate the rc
/// file, append the wrapper idempotently. All messages to stderr.
fn cmd_install(args: &[String], ctx: &Ctx, tries_path: &Path, err: &mut dyn Write) -> u8 {
    let script_path = expand_path(&ctx.arg0, &ctx.cwd, ctx.env.home.as_deref());
    let explicit_path = args
        .first()
        .filter(|a| a.starts_with('/'))
        .map(|a| expand_path(a, &ctx.cwd, ctx.env.home.as_deref()));

    let shell = detect_shell(&ctx.env);
    let rc_file = shell.and_then(|s| shell_rc_file(s, &ctx.env));

    let Some(rc_file) = rc_file else {
        let _ = writeln!(err, "Error: could not determine shell config file");
        let _ = writeln!(
            err,
            "Your shell was detected as: {}",
            shell.map_or("unknown".to_string(), |s| format!("{s:?}").to_lowercase())
        );
        let _ = writeln!(
            err,
            "Run 'try init' and manually add the output to your shell config."
        );
        return 1;
    };
    let shell = shell.expect("rc_file implies shell");
    let snippet = init_snippet(shell, &script_path, explicit_path.as_deref(), tries_path);
    let rc_path = expand_path(&rc_file, &ctx.cwd, ctx.env.home.as_deref());

    if rc_path.exists() {
        let contents = std::fs::read_to_string(&rc_path).unwrap_or_default();
        if contents.contains("# try shell integration") {
            let _ = writeln!(err, "try is already installed in {}", rc_path.display());
            let _ = writeln!(
                err,
                "To reinstall, remove the '# try shell integration' block first."
            );
            return 0;
        }
        let readonly = std::fs::metadata(&rc_path).is_ok_and(|m| m.permissions().readonly());
        if readonly {
            let _ = writeln!(
                err,
                "Warning: {} is read-only, skipping.",
                rc_path.display()
            );
            let _ = writeln!(
                err,
                "Run 'try init' and manually add the output to your shell config."
            );
            return 1;
        }
    }

    if let Some(parent) = rc_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let block = format!("\n# try shell integration\n{snippet}");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&rc_path)
    {
        let _ = f.write_all(block.as_bytes());
    }
    let _ = writeln!(err, "Added try shell integration to {}", rc_path.display());
    if shell == Shell::Pwsh {
        let _ = writeln!(err, "Restart your shell or run: . $PROFILE");
    } else {
        let _ = writeln!(
            err,
            "Restart your shell or run: source {}",
            rc_path.display()
        );
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(cwd: &Path) -> Ctx {
        Ctx {
            env: Env {
                home: Some("/home/u".into()),
                shell: Some("/bin/bash".into()),
                ..Env::default()
            },
            cwd: cwd.to_path_buf(),
            arg0: "/bin/tryme".into(),
            version: "0.0.0".into(),
            today: "2026-07-10".into(),
        }
    }

    fn run_capture(args: &[&str], c: &Ctx) -> (u8, String, String) {
        let mut errb = Vec::new();
        let mut outb = Vec::new();
        let code = {
            let mut out = ScriptOut::new(&mut outb);
            run(
                args.iter().map(ToString::to_string).collect(),
                c,
                &mut errb,
                &mut out,
            )
        };
        (
            code,
            String::from_utf8(outb).unwrap(),
            String::from_utf8(errb).unwrap(),
        )
    }

    #[test]
    fn bare_invocation_help_exit_2_stdout_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let (code, out, err) = run_capture(&[], &ctx(tmp.path()));
        assert_eq!(code, 2);
        assert!(out.is_empty());
        assert!(err.starts_with("try v0.0.0 - ephemeral workspace manager"));
    }

    #[test]
    fn version_and_help_exit_0_on_stderr() {
        let tmp = tempfile::tempdir().unwrap();
        let (code, out, err) = run_capture(&["--version"], &ctx(tmp.path()));
        assert_eq!((code, out.as_str(), err.as_str()), (0, "", "try 0.0.0\n"));
        let (code, out, err) = run_capture(&["clone", "-h"], &ctx(tmp.path()));
        assert_eq!(code, 0);
        assert!(out.is_empty());
        assert!(err.contains("ephemeral workspace manager"));
    }

    #[test]
    fn clone_emits_script_with_dated_name() {
        let tmp = tempfile::tempdir().unwrap();
        let c = ctx(tmp.path());
        let (code, out, _) = run_capture(&["--path", "/t", "clone", "https://github.com/u/r"], &c);
        assert_eq!(code, 0);
        assert!(out.contains("git clone 'https://github.com/u/r' '/t/2026-07-10-u-r'"));
        assert!(out.starts_with("# if you can read this"));
    }

    #[test]
    fn clone_missing_and_bad_uri_exit_1() {
        let tmp = tempfile::tempdir().unwrap();
        let c = ctx(tmp.path());
        let (code, out, err) = run_capture(&["clone"], &c);
        assert_eq!(code, 1);
        assert!(out.is_empty());
        assert!(err.contains("git URI required"));
        let (code, _, err) = run_capture(&["clone", "not a uri"], &c);
        assert_eq!(code, 1);
        assert!(err.contains("Unable to parse git URI"));
    }

    #[test]
    fn url_shorthand_routes_to_clone() {
        let tmp = tempfile::tempdir().unwrap();
        let (code, out, _) = run_capture(
            &["--path", "/t", "https://github.com/u/r"],
            &ctx(tmp.path()),
        );
        assert_eq!(code, 0);
        assert!(out.contains("git clone 'https://github.com/u/r'"));
    }

    #[test]
    fn selector_path_cancels_with_stdout_cancelled() {
        let tmp = tempfile::tempdir().unwrap();
        let (code, out, _) = run_capture(&["--and-exit", "exec"], &ctx(tmp.path()));
        assert_eq!(code, 1);
        assert_eq!(out, "Cancelled.\n");
    }

    #[test]
    fn bare_dot_requires_name() {
        let tmp = tempfile::tempdir().unwrap();
        let (code, _, err) = run_capture(&["exec", "cd", "."], &ctx(tmp.path()));
        assert_eq!(code, 1);
        assert!(err.contains("'try .' requires a name argument"));
    }

    #[test]
    fn worktree_dir_token_means_cwd() {
        let tmp = tempfile::tempdir().unwrap();
        let c = ctx(tmp.path());
        let (code, out, _) = run_capture(&["--path", "/t", "worktree", "dir", "feat"], &c);
        assert_eq!(code, 0);
        // cwd variant: no -C in the guard
        assert!(out.contains("if git rev-parse --is-inside-work-tree"));
        assert!(out.contains("'/t/2026-07-10-feat'"));
    }

    #[test]
    fn init_emits_wrapper_on_stdout() {
        let tmp = tempfile::tempdir().unwrap();
        let (code, out, _) = run_capture(&["--path", "/t", "init"], &ctx(tmp.path()));
        assert_eq!(code, 0);
        assert!(out.starts_with("try() {\n"));
        assert!(out.contains("'/bin/tryme' exec --path \"${TRY_PATH:-/t}\""));
    }
}
