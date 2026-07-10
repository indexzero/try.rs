//! Script builders — ports of `script_cd` / `script_mkdir_cd` /
//! `script_clone` / `script_worktree` / `script_delete` / `script_ascend` /
//! `script_rename` (`try.rb:1411-1467`). Every byte matters: the emitted
//! strings are pinned by `test_05`/`07`/`12`/`16`/`31`/`37`.

use crate::emit::q;
use std::path::Path;

fn p(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// `script_cd` (`try.rb:1411-1413`). The leading `touch` bumps mtime, which
/// feeds `base_score` recency — selecting a try boosts its future ranking.
/// That feedback loop is a feature, not an accident.
#[must_use]
pub fn script_cd(path: &Path) -> Vec<String> {
    let qp = q(&p(path));
    vec![
        format!("touch {qp}"),
        format!("echo {qp}"),
        format!("cd {qp}"),
    ]
}

/// `script_mkdir_cd` (`try.rb:1415-1417`).
#[must_use]
pub fn script_mkdir_cd(path: &Path) -> Vec<String> {
    let mut cmds = vec![format!("mkdir -p {}", q(&p(path)))];
    cmds.extend(script_cd(path));
    cmds
}

/// `script_clone` (`try.rb:1419-1421`). Upstream hardcodes single quotes
/// around the URI (`git clone '#{uri}'`) instead of `q()` — preserved.
#[must_use]
pub fn script_clone(path: &Path, uri: &str) -> Vec<String> {
    let qp = q(&p(path));
    let mut cmds = vec![
        format!("mkdir -p {qp}"),
        format!(
            "echo {}",
            q(&format!("Using git clone to create this trial from {uri}."))
        ),
        format!("git clone '{uri}' {qp}"),
    ];
    cmds.extend(script_cd(path));
    cmds
}

/// `script_worktree` (`try.rb:1423-1432`). The inner `sh -c` guard adds the
/// worktree (detached) only when inside a work tree and **always exits 0**,
/// so a non-git source still mkdir+cds. `repo = None` is the
/// current-directory variant (no `-C`); `src` is what the echo names.
#[must_use]
pub fn script_worktree(path: &Path, repo: Option<&Path>, cwd: &Path) -> Vec<String> {
    let qp = q(&p(path));
    let worktree_cmd = match repo {
        Some(r) => {
            let qr = q(&p(r));
            format!(
                "/usr/bin/env sh -c 'if git -C {qr} rev-parse --is-inside-work-tree >/dev/null 2>&1; then repo=$(git -C {qr} rev-parse --show-toplevel); git -C \"$repo\" worktree add --detach {qp} >/dev/null 2>&1 || true; fi; exit 0'"
            )
        }
        None => format!(
            "/usr/bin/env sh -c 'if git rev-parse --is-inside-work-tree >/dev/null 2>&1; then repo=$(git rev-parse --show-toplevel); git -C \"$repo\" worktree add --detach {qp} >/dev/null 2>&1 || true; fi; exit 0'"
        ),
    };
    let src = repo.unwrap_or(cwd);
    let mut cmds = vec![
        format!("mkdir -p {qp}"),
        format!(
            "echo {}",
            q(&format!(
                "Using git worktree to create this trial from {}.",
                p(src)
            ))
        ),
        worktree_cmd,
    ];
    cmds.extend(script_cd(path));
    cmds
}

/// `script_delete` (`try.rb:1434-1439`): cd into the base, `test -d` guard
/// per basename, then restore the original PWD with a base-path fallback.
#[must_use]
pub fn script_delete(basenames: &[String], base_path: &Path, original_pwd: &Path) -> Vec<String> {
    let mut cmds = vec![format!("cd {}", q(&p(base_path)))];
    for name in basenames {
        let qn = q(name);
        cmds.push(format!("test -d {qn} && rm -rf {qn}"));
    }
    cmds.push(format!(
        "cd {} 2>/dev/null || cd {}",
        q(&p(original_pwd)),
        q(&p(base_path))
    ));
    cmds
}

/// `script_ascend` (`try.rb:1441-1457`): `git worktree move` when the source
/// has a `.git` **file** (worktree marker), plain `mv` otherwise; then a
/// symlink back into the tries dir, the `Graduated:` echo (with a literal
/// `→`), and a cd to the destination.
#[must_use]
pub fn script_ascend(source: &Path, dest: &Path, basename: &str, base_path: &Path) -> Vec<String> {
    let symlink_path = base_path.join(basename);
    let is_worktree = source.join(".git").is_file();

    let mut cmds = Vec::new();
    if is_worktree {
        cmds.push(format!(
            "git worktree move {} {}",
            q(&p(source)),
            q(&p(dest))
        ));
    } else {
        cmds.push(format!("mv {} {}", q(&p(source)), q(&p(dest))));
    }
    cmds.push(format!("ln -s {} {}", q(&p(dest)), q(&p(&symlink_path))));
    cmds.push(format!(
        "echo {}",
        q(&format!("Graduated: {basename} → {}", p(dest)))
    ));
    cmds.extend(script_cd(dest));
    cmds
}

/// `script_rename` (`try.rb:1459-1467`).
#[must_use]
pub fn script_rename(base_path: &Path, old_name: &str, new_name: &str) -> Vec<String> {
    let new_path = base_path.join(new_name);
    vec![
        format!("cd {}", q(&p(base_path))),
        format!("mv {} {}", q(old_name), q(new_name)),
        format!("echo {}", q(&p(&new_path))),
        format!("cd {}", q(&p(&new_path))),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn cd_script_touch_echo_cd() {
        assert_eq!(
            script_cd(&PathBuf::from("/t/2026-07-10-x")),
            vec![
                "touch '/t/2026-07-10-x'",
                "echo '/t/2026-07-10-x'",
                "cd '/t/2026-07-10-x'"
            ]
        );
    }

    #[test]
    fn clone_script_hardcoded_uri_quotes() {
        let cmds = script_clone(&PathBuf::from("/t/d"), "https://github.com/u/r");
        assert_eq!(cmds[2], "git clone 'https://github.com/u/r' '/t/d'");
        assert_eq!(
            cmds[1],
            "echo 'Using git clone to create this trial from https://github.com/u/r.'"
        );
    }

    #[test]
    fn worktree_detach_and_always_exit_zero() {
        let cmds = script_worktree(
            &PathBuf::from("/t/w"),
            Some(&PathBuf::from("/repo")),
            &PathBuf::from("/cwd"),
        );
        assert!(cmds[2].contains("worktree add --detach '/t/w'"));
        assert!(cmds[2].ends_with("fi; exit 0'"));
        assert!(cmds[2].contains("git -C '/repo' rev-parse"));
        // cwd variant drops -C
        let cmds = script_worktree(&PathBuf::from("/t/w"), None, &PathBuf::from("/cwd"));
        assert!(cmds[2].contains("if git rev-parse"));
        assert!(cmds[1].contains("from /cwd."));
    }

    #[test]
    fn delete_uses_basenames_and_pwd_fallback() {
        let cmds = script_delete(
            &["a".to_string(), "b".to_string()],
            &PathBuf::from("/t"),
            &PathBuf::from("/orig"),
        );
        assert_eq!(cmds[0], "cd '/t'");
        assert_eq!(cmds[1], "test -d 'a' && rm -rf 'a'");
        assert_eq!(cmds[3], "cd '/orig' 2>/dev/null || cd '/t'");
    }

    #[test]
    fn ascend_mv_symlink_echo_cd() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("2026-07-10-exp");
        std::fs::create_dir(&src).unwrap();
        let cmds = script_ascend(
            &src,
            &PathBuf::from("/proj/exp"),
            "2026-07-10-exp",
            tmp.path(),
        );
        assert!(cmds[0].starts_with("mv "));
        assert!(cmds[1].starts_with("ln -s '/proj/exp' "));
        assert!(cmds[2].contains("Graduated: 2026-07-10-exp → /proj/exp"));
        assert_eq!(cmds.len(), 6);
    }

    #[test]
    fn rename_cd_mv_echo_cd() {
        let cmds = script_rename(&PathBuf::from("/t"), "old", "new");
        assert_eq!(
            cmds,
            vec!["cd '/t'", "mv 'old' 'new'", "echo '/t/new'", "cd '/t/new'"]
        );
    }
}
