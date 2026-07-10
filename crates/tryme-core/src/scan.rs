//! Directory scan — port of `load_all_tries` (`try.rb:94-136`).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// One try directory as scanned from disk.
pub struct TryDir {
    /// Directory basename (also the fuzzy match text).
    pub basename: String,
    /// Full path — the symlink **target** (realpath) for symlinked entries.
    pub path: PathBuf,
    /// Whether the entry itself is a symlink (🔗 icon).
    pub is_symlink: bool,
    /// Modification time (drives recency scoring and the meta column).
    pub mtime: SystemTime,
    /// Recency score + date-prefix bonus (`try.rb:113-119`).
    pub base_score: f64,
}

/// `^\d{4}-\d{2}-\d{2}-` (`try.rb:119`).
#[must_use]
pub fn has_date_prefix(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() >= 11
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[4] == b'-'
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[7] == b'-'
        && b[8..10].iter().all(u8::is_ascii_digit)
        && b[10] == b'-'
}

/// Signed seconds between two times, full float precision (Ruby
/// `Time - Time`). No truncation in the live path — fixtures use
/// whole-second mtimes instead.
#[must_use]
pub fn seconds_between(now: SystemTime, then: SystemTime) -> f64 {
    match now.duration_since(then) {
        Ok(d) => d.as_secs_f64(),
        Err(e) => -e.duration().as_secs_f64(),
    }
}

/// Scan `base_path` once (`try.rb:94-136`): skip dotfiles, directories only,
/// race-safe (ENOENT/EACCES skipped), symlinks resolved to realpath.
#[must_use]
pub fn load_all_tries(base_path: &Path, now: SystemTime) -> Vec<TryDir> {
    let Ok(rd) = std::fs::read_dir(base_path) else {
        return Vec::new();
    };
    let mut tries = Vec::new();
    for dent in rd.flatten() {
        let name = dent.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = base_path.join(&name);
        // File.stat follows symlinks; errors (races, perms) skip the entry
        let Ok(stat) = std::fs::metadata(&path) else {
            continue;
        };
        if !stat.is_dir() {
            continue;
        }
        let mtime = stat.modified().unwrap_or(now);
        let hours_since = seconds_between(now, mtime) / 3600.0;
        let mut base_score = 3.0 / (hours_since + 1.0).sqrt();
        if has_date_prefix(&name) {
            base_score += 2.0;
        }
        let is_symlink = std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
        let path = if is_symlink {
            path.canonicalize().unwrap_or(path)
        } else {
            path
        };
        tries.push(TryDir {
            basename: name,
            path,
            is_symlink,
            mtime,
            base_score,
        });
    }
    tries
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn date_prefix_detection() {
        assert!(has_date_prefix("2026-07-10-x"));
        assert!(!has_date_prefix("2026-07-10")); // no trailing hyphen
        assert!(!has_date_prefix("no-date-prefix"));
        assert!(!has_date_prefix("20a6-07-10-x"));
    }

    #[test]
    fn scan_skips_hidden_and_files_and_scores_dates() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("2026-07-10-alpha")).unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();
        std::fs::write(tmp.path().join("a-file"), b"x").unwrap();
        std::fs::create_dir(tmp.path().join("plain")).unwrap();

        let now = SystemTime::now() + Duration::from_secs(1);
        let mut tries = load_all_tries(tmp.path(), now);
        tries.sort_by(|a, b| a.basename.cmp(&b.basename));
        let names: Vec<&str> = tries.iter().map(|t| t.basename.as_str()).collect();
        assert_eq!(names, vec!["2026-07-10-alpha", "plain"]);
        // Date-prefixed entry gets the +2.0 bonus
        assert!(tries[0].base_score > tries[1].base_score + 1.9);
    }
}
