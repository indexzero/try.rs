//! Directory naming — port of `unique_dir_name`,
//! `resolve_unique_name_with_versioning`, and `worktree_path`
//! (`try.rb:1469-1523`). Pinned by `test_33_versioning.sh`.

use std::path::Path;

/// Port of `unique_dir_name` (`try.rb:1470-1478`): append `-2`, `-3`, …
/// until the candidate does not exist under `tries_path`.
#[must_use]
pub fn unique_dir_name(tries_path: &Path, dir_name: &str) -> String {
    let mut candidate = dir_name.to_string();
    let mut i = 2;
    while tries_path.join(&candidate).is_dir() {
        candidate = format!("{dir_name}-{i}");
        i += 1;
    }
    candidate
}

/// Port of `resolve_unique_name_with_versioning` (`try.rb:1483-1501`).
///
/// Returns the (possibly bumped) **base** — without the date prefix. If
/// `<date>-<base>` exists and `base` ends in digits, bump the trailing
/// number (`feature1` → `feature2`); otherwise fall back to `-2`-style
/// uniqueness on the full name, stripping the date prefix back off.
#[must_use]
pub fn resolve_unique_name_with_versioning(
    tries_path: &Path,
    date_prefix: &str,
    base: &str,
) -> String {
    let initial = format!("{date_prefix}-{base}");
    if !tries_path.join(&initial).is_dir() {
        return base.to_string();
    }

    // Ruby: base.match(/^(.*?)(\d+)$/) — non-greedy stem, maximal trailing
    // digit run.
    let digit_start = {
        let mut idx = base.len();
        for (i, c) in base.char_indices().rev() {
            if c.is_ascii_digit() {
                idx = i;
            } else {
                break;
            }
        }
        idx
    };

    if digit_start < base.len() {
        let stem = &base[..digit_start];
        let n: u64 = base[digit_start..].parse().unwrap_or(0);
        let mut candidate_num = n + 1;
        loop {
            let candidate_base = format!("{stem}{candidate_num}");
            if !tries_path
                .join(format!("{date_prefix}-{candidate_base}"))
                .is_dir()
            {
                return candidate_base;
            }
            candidate_num += 1;
        }
    } else {
        let full = unique_dir_name(tries_path, &initial);
        full.strip_prefix(&format!("{date_prefix}-"))
            .unwrap_or(&full)
            .to_string()
    }
}

/// Port of `worktree_path` (`try.rb:1514-1523`): base is the kebabed custom
/// name or the basename of the repo's realpath (plain basename on error);
/// then date-prefix + collision versioning.
#[must_use]
pub fn worktree_path(
    tries_path: &Path,
    repo_dir: &Path,
    custom_name: &str,
    date_prefix: &str,
) -> std::path::PathBuf {
    let base = if custom_name.trim().is_empty() {
        let resolved = repo_dir
            .canonicalize()
            .unwrap_or_else(|_| repo_dir.to_path_buf());
        resolved
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
    } else {
        squeeze_ws_to_hyphen(custom_name)
    };
    let base = resolve_unique_name_with_versioning(tries_path, date_prefix, &base);
    tries_path.join(format!("{date_prefix}-{base}"))
}

/// Ruby `gsub(/\s+/, '-')`: every whitespace RUN becomes one hyphen.
#[must_use]
pub fn squeeze_ws_to_hyphen(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !in_ws {
                out.push('-');
                in_ws = true;
            }
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn no_collision_returns_base_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            resolve_unique_name_with_versioning(tmp.path(), "2026-07-10", "feature"),
            "feature"
        );
    }

    #[test]
    fn trailing_digits_bump() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("2026-07-10-feature1")).unwrap();
        fs::create_dir(tmp.path().join("2026-07-10-feature2")).unwrap();
        assert_eq!(
            resolve_unique_name_with_versioning(tmp.path(), "2026-07-10", "feature1"),
            "feature3"
        );
    }

    #[test]
    fn no_digits_appends_dash_n() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("2026-07-10-feature")).unwrap();
        assert_eq!(
            resolve_unique_name_with_versioning(tmp.path(), "2026-07-10", "feature"),
            "feature-2"
        );
        fs::create_dir(tmp.path().join("2026-07-10-feature-2")).unwrap();
        assert_eq!(
            resolve_unique_name_with_versioning(tmp.path(), "2026-07-10", "feature"),
            "feature-3"
        );
    }

    #[test]
    fn whitespace_runs_become_single_hyphens() {
        assert_eq!(squeeze_ws_to_hyphen("a  b\tc"), "a-b-c");
    }
}
