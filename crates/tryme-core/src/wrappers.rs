//! Shell integration — ports of `init_snippet`, `detect_shell`,
//! `shell_rc_file`, `fish?` (`try.rb:1230-1313, 1503-1510`) and Ruby's
//! `File.expand_path` semantics.
//!
//! Wrapper templates are byte-identical to upstream with exactly one
//! sanctioned divergence: the invocation token. Upstream emits
//! `/usr/bin/env ruby '<script>' exec…`; we emit `'<binary>' exec…`.
//! `test_14` greps for the binary path as the runner computed it — absolute
//! but NOT symlink-resolved — which is why this module never canonicalizes.

use crate::env::Env;
use std::path::{Component, Path, PathBuf};

/// Port of Ruby `File.expand_path(path, base)`: `~` expansion from `home`,
/// CWD-join for relative paths, then **lexical** `.`/`..` normalization.
/// Never resolves symlinks (Ruby's `expand_path` does not either — resolving
/// would break `test_14`'s path grep under symlinked build dirs).
#[must_use]
pub fn expand_path(path: &str, base: &Path, home: Option<&str>) -> PathBuf {
    let joined: PathBuf = if path == "~" {
        home.map_or_else(|| base.join(path), PathBuf::from)
    } else if let Some(rest) = path.strip_prefix("~/") {
        home.map_or_else(|| base.join(path), |h| Path::new(h).join(rest))
    } else if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        base.join(path)
    };

    let mut out = PathBuf::new();
    for comp in joined.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                // ".." at the root stays at the root (Ruby behavior).
                if !matches!(
                    out.components().next_back(),
                    None | Some(Component::RootDir)
                ) {
                    out.pop();
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// Shells the wrapper templates distinguish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    /// bash and zsh share one template (`try.rb:1299`).
    Bash,
    /// fish template with `string collect` / `$pipestatus` (`try.rb:1267`).
    Fish,
    /// PowerShell — reachable only via `install` (`cmd_init!` emits
    /// fish/bash only; `try.rb:1180`).
    Pwsh,
    /// zsh — same wrapper as bash, distinct rc file for `install`.
    Zsh,
}

/// Port of `init_snippet` (`try.rb:1265-1313`). `script_path` is this
/// binary's `expand_path`'d argv\[0\]; `explicit_path` switches to the
/// quoted-literal `--path` form (`test_14` asserts `--path '<path>'`).
#[must_use]
pub fn init_snippet(
    shell: Shell,
    script_path: &Path,
    explicit_path: Option<&Path>,
    default_path: &Path,
) -> String {
    let sp = script_path.to_string_lossy();
    let dp = default_path.to_string_lossy();
    match shell {
        Shell::Fish => {
            let fish_path_arg = match explicit_path {
                Some(p) => format!(" --path '{}'", p.to_string_lossy()),
                None => format!(
                    " --path (if set -q TRY_PATH; echo \"$TRY_PATH\"; else; echo '{dp}'; end)"
                ),
            };
            format!(
                "function try\n  set -l out ('{sp}' exec{fish_path_arg} $argv 2>/dev/tty | string collect)\n  if test $pipestatus[1] -eq 0\n    eval $out\n  else\n    echo $out\n  end\nend\n"
            )
        }
        Shell::Pwsh => {
            let ps_path_expr = match explicit_path {
                Some(p) => format!("'{}'", p.to_string_lossy()),
                None => format!("$(if ($env:TRY_PATH) {{ $env:TRY_PATH }} else {{ '{dp}' }})"),
            };
            format!(
                "function try {{\n  $tryPath = {ps_path_expr}\n  $tempErr = [System.IO.Path]::GetTempFileName()\n  $out = & '{sp}' exec --path $tryPath @args 2>$tempErr\n  if ($LASTEXITCODE -eq 0) {{\n    $out | Invoke-Expression\n  }} else {{\n    Get-Content $tempErr | Write-Host\n    $out | Write-Output\n  }}\n  Remove-Item $tempErr -ErrorAction SilentlyContinue\n}}\n"
            )
        }
        Shell::Bash | Shell::Zsh => {
            let path_arg = match explicit_path {
                Some(p) => format!(" --path '{}'", p.to_string_lossy()),
                None => format!(" --path \"${{TRY_PATH:-{dp}}}\""),
            };
            format!(
                "try() {{\n  local out\n  out=$('{sp}' exec{path_arg} \"$@\" 2>/dev/tty)\n  if [ $? -eq 0 ]; then\n    eval \"$out\"\n  else\n    echo \"$out\"\n  fi\n}}\n"
            )
        }
    }
}

/// Port of `fish?` (`try.rb:1505-1510`): `$SHELL` contains "fish", falling
/// back to the parent-process name when `$SHELL` is empty.
#[must_use]
pub fn is_fish(env: &Env) -> bool {
    let shell = env.shell.clone().unwrap_or_default();
    let shell = if shell.is_empty() {
        parent_process_name().unwrap_or_default()
    } else {
        shell
    };
    shell.contains("fish")
}

/// Port of `detect_shell` (`try.rb:1230-1248`): `$SHELL` substring checks,
/// then `PSModulePath`, then the parent-process name.
#[must_use]
pub fn detect_shell(env: &Env) -> Option<Shell> {
    let shell_env = env.shell.clone().unwrap_or_default();
    if shell_env.contains("fish") {
        return Some(Shell::Fish);
    }
    if shell_env.contains("zsh") {
        return Some(Shell::Zsh);
    }
    if shell_env.contains("bash") {
        return Some(Shell::Bash);
    }
    if env.psmodulepath.as_deref().is_some_and(|v| !v.is_empty()) {
        return Some(Shell::Pwsh);
    }
    let parent = parent_process_name().unwrap_or_default();
    if parent.contains("fish") {
        return Some(Shell::Fish);
    }
    if parent.contains("zsh") {
        return Some(Shell::Zsh);
    }
    if parent.contains("bash") {
        return Some(Shell::Bash);
    }
    let lower = parent.to_lowercase();
    if lower.contains("pwsh") || lower.contains("powershell") {
        return Some(Shell::Pwsh);
    }
    None
}

/// Port of `shell_rc_file` (`try.rb:1250-1263`). Returns the (tilde-form or
/// absolute) rc path for `install`.
#[must_use]
pub fn shell_rc_file(shell: Shell, env: &Env) -> Option<String> {
    match shell {
        Shell::Fish => Some("~/.config/fish/config.fish".into()),
        Shell::Zsh => Some("~/.zshrc".into()),
        Shell::Bash => {
            let home = env.home.clone().unwrap_or_default();
            let bashrc = Path::new(&home).join(".bashrc");
            Some(if bashrc.exists() {
                "~/.bashrc".into()
            } else {
                "~/.bash_profile".into()
            })
        }
        Shell::Pwsh => {
            if let Some(profile) = env.profile.clone() {
                return Some(profile);
            }
            let base = if cfg!(windows) {
                let up = env
                    .userprofile
                    .clone()
                    .or_else(|| env.home.clone())
                    .unwrap_or_default();
                Path::new(&up).join("Documents").join("PowerShell")
            } else {
                let home = env.home.clone().unwrap_or_default();
                Path::new(&home).join(".config").join("powershell")
            };
            Some(
                base.join("Microsoft.PowerShell_profile.ps1")
                    .to_string_lossy()
                    .into_owned(),
            )
        }
    }
}

/// Parent-process name via `ps c -p <ppid> -o ucomm=` (`try.rb:1241,1507`).
#[cfg(unix)]
fn parent_process_name() -> Option<String> {
    let ppid = std::os::unix::process::parent_id();
    let out = std::process::Command::new("ps")
        .args(["c", "-p", &ppid.to_string(), "-o", "ucomm="])
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(not(unix))]
fn parent_process_name() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_with_shell(shell: &str) -> Env {
        Env {
            shell: Some(shell.to_string()),
            ..Env::default()
        }
    }

    #[test]
    fn expand_path_tilde_relative_and_dotdot() {
        let base = Path::new("/base/dir");
        assert_eq!(
            expand_path("~/x", base, Some("/home/u")),
            PathBuf::from("/home/u/x")
        );
        assert_eq!(
            expand_path("sub", base, None),
            PathBuf::from("/base/dir/sub")
        );
        assert_eq!(
            expand_path("../up/./x", base, None),
            PathBuf::from("/base/up/x")
        );
        assert_eq!(expand_path("/../x", base, None), PathBuf::from("/x"));
    }

    #[test]
    fn bash_wrapper_bytes() {
        let s = init_snippet(Shell::Bash, Path::new("/bin/tryme"), None, Path::new("/t"));
        assert!(s.starts_with("try() {\n"));
        assert!(s.contains("out=$('/bin/tryme' exec --path \"${TRY_PATH:-/t}\" \"$@\" 2>/dev/tty)"));
        assert!(s.contains("eval \"$out\""));
    }

    #[test]
    fn bash_wrapper_explicit_path_quoted_literal() {
        // test_14 asserts: --path '<path>'
        let s = init_snippet(
            Shell::Bash,
            Path::new("/bin/tryme"),
            Some(Path::new("/tries")),
            Path::new("/t"),
        );
        assert!(s.contains(" --path '/tries' "));
        assert!(!s.contains("TRY_PATH:-"));
    }

    #[test]
    fn fish_wrapper_has_no_bashisms() {
        // test_36 asserts the fish function contains no `$(` or `$?`.
        let s = init_snippet(Shell::Fish, Path::new("/bin/tryme"), None, Path::new("/t"));
        assert!(s.starts_with("function try\n"));
        assert!(!s.contains("$("));
        assert!(!s.contains("$?"));
        assert!(s.contains("| string collect)"));
        assert!(s.contains("$pipestatus[1]"));
    }

    #[test]
    fn shell_detection_from_env() {
        assert_eq!(detect_shell(&env_with_shell("/bin/zsh")), Some(Shell::Zsh));
        assert_eq!(
            detect_shell(&env_with_shell("/usr/bin/fish")),
            Some(Shell::Fish)
        );
        assert!(is_fish(&env_with_shell("/usr/local/bin/fish")));
        assert!(!is_fish(&env_with_shell("/bin/bash")));
    }
}
