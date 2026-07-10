//! Git URI parsing — port of `parse_git_uri` / `is_git_uri?` /
//! `generate_clone_directory_name` (`try.rb:1039-1078`).
//!
//! The four-pattern cascade order is load-bearing (github-https, github-ssh,
//! generic-https, generic-ssh) and pinned by `test_32_git_uri.sh`.

/// Parsed git URI components.
#[derive(Debug, PartialEq, Eq)]
pub struct GitUri {
    /// Repository owner segment.
    pub user: String,
    /// Repository name segment (`.git` suffix already stripped).
    pub repo: String,
    /// Host, e.g. `github.com`.
    pub host: String,
}

/// Take chars while they match `pred`, returning `(matched, rest)`;
/// `None` when nothing matched. Mirrors the Ruby regex `[^/]+` idiom.
fn take_while1(s: &str, pred: impl Fn(char) -> bool) -> Option<(&str, &str)> {
    let end = s.find(|c| !pred(c)).unwrap_or(s.len());
    (end > 0).then(|| s.split_at(end))
}

fn not_slash(c: char) -> bool {
    c != '/'
}

/// Port of `parse_git_uri` (`try.rb:1039-1063`). Strips a trailing `.git`,
/// then matches the four upstream patterns in order. Ruby's `[^/]+` capture
/// for `repo` is unanchored, so trailing path segments are simply not
/// captured — hence prefix matching, not full-string matching.
#[must_use]
pub fn parse_git_uri(uri: &str) -> Option<GitUri> {
    let uri = uri.strip_suffix(".git").unwrap_or(uri);

    // ^https?://github\.com/([^/]+)/([^/]+)
    for scheme in ["https://", "http://"] {
        if let Some(rest) = uri.strip_prefix(scheme) {
            if let Some(rest) = rest.strip_prefix("github.com/") {
                let (user, rest) = take_while1(rest, not_slash)?;
                let rest = rest.strip_prefix('/')?;
                let (repo, _) = take_while1(rest, not_slash)?;
                return Some(GitUri {
                    user: user.into(),
                    repo: repo.into(),
                    host: "github.com".into(),
                });
            }
        }
    }
    // ^git@github\.com:([^/]+)/([^/]+)
    if let Some(rest) = uri.strip_prefix("git@github.com:") {
        if let Some((user, rest)) = take_while1(rest, not_slash) {
            if let Some(rest) = rest.strip_prefix('/') {
                if let Some((repo, _)) = take_while1(rest, not_slash) {
                    return Some(GitUri {
                        user: user.into(),
                        repo: repo.into(),
                        host: "github.com".into(),
                    });
                }
            }
        }
    }
    // ^https?://([^/]+)/([^/]+)/([^/]+)
    for scheme in ["https://", "http://"] {
        if let Some(rest) = uri.strip_prefix(scheme) {
            if let Some((host, rest)) = take_while1(rest, not_slash) {
                if let Some(rest) = rest.strip_prefix('/') {
                    if let Some((user, rest)) = take_while1(rest, not_slash) {
                        if let Some(rest) = rest.strip_prefix('/') {
                            if let Some((repo, _)) = take_while1(rest, not_slash) {
                                return Some(GitUri {
                                    user: user.into(),
                                    repo: repo.into(),
                                    host: host.into(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    // ^git@([^:]+):([^/]+)/([^/]+)
    if let Some(rest) = uri.strip_prefix("git@") {
        if let Some((host, rest)) = take_while1(rest, |c| c != ':') {
            if let Some(rest) = rest.strip_prefix(':') {
                if let Some((user, rest)) = take_while1(rest, not_slash) {
                    if let Some(rest) = rest.strip_prefix('/') {
                        if let Some((repo, _)) = take_while1(rest, not_slash) {
                            return Some(GitUri {
                                user: user.into(),
                                repo: repo.into(),
                                host: host.into(),
                            });
                        }
                    }
                }
            }
        }
    }
    None
}

/// Port of `is_git_uri?` (`try.rb:1075-1078`): scheme prefix, or a known-host
/// substring anywhere, or a `.git` suffix.
#[must_use]
#[allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "Ruby's end_with?('.git') is case-sensitive; case-folding would diverge from upstream"
)]
pub fn is_git_uri(arg: &str) -> bool {
    arg.starts_with("https://")
        || arg.starts_with("http://")
        || arg.starts_with("git@")
        || arg.contains("github.com")
        || arg.contains("gitlab.com")
        || arg.ends_with(".git")
}

/// Port of `generate_clone_directory_name` (`try.rb:1065-1073`).
///
/// A non-empty custom name is used **verbatim — no date prefix** (upstream
/// asymmetry, pinned by `test_07_clone_naming.sh`). Otherwise
/// `YYYY-MM-DD-<user>-<repo>`; `None` when the URI does not parse.
#[must_use]
pub fn generate_clone_directory_name(
    git_uri: &str,
    custom_name: Option<&str>,
    date_prefix: &str,
) -> Option<String> {
    if let Some(name) = custom_name {
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    let parsed = parse_git_uri(git_uri)?;
    Some(format!("{date_prefix}-{}-{}", parsed.user, parsed.repo))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(uri: &str) -> (String, String, String) {
        let g = parse_git_uri(uri).unwrap();
        (g.user, g.repo, g.host)
    }

    #[test]
    fn github_https_and_ssh() {
        assert_eq!(
            parts("https://github.com/user/repo"),
            ("user".into(), "repo".into(), "github.com".into())
        );
        assert_eq!(
            parts("git@github.com:user/repo.git"),
            ("user".into(), "repo".into(), "github.com".into())
        );
    }

    #[test]
    fn generic_hosts() {
        assert_eq!(
            parts("https://gitlab.com/grp/proj"),
            ("grp".into(), "proj".into(), "gitlab.com".into())
        );
        assert_eq!(
            parts("git@my.host.dev:owner/thing.git"),
            ("owner".into(), "thing".into(), "my.host.dev".into())
        );
    }

    #[test]
    fn unparseable_returns_none() {
        assert_eq!(parse_git_uri("not-a-uri"), None);
        assert_eq!(parse_git_uri("https://host-only"), None);
    }

    #[test]
    fn trailing_path_segments_ignored_like_upstream_regex() {
        // Ruby's unanchored [^/]+ captures stop at the third segment.
        assert_eq!(
            parts("https://github.com/user/repo/tree/main"),
            ("user".into(), "repo".into(), "github.com".into())
        );
    }

    #[test]
    fn clone_name_custom_verbatim_no_date() {
        assert_eq!(
            generate_clone_directory_name("https://github.com/u/r", Some("my name"), "2026-07-10")
                .unwrap(),
            "my name"
        );
        assert_eq!(
            generate_clone_directory_name("https://github.com/u/r", None, "2026-07-10").unwrap(),
            "2026-07-10-u-r"
        );
        assert_eq!(
            generate_clone_directory_name("nope", None, "2026-07-10"),
            None
        );
    }

    #[test]
    fn is_git_uri_matches_upstream_predicate() {
        assert!(is_git_uri("https://github.com/u/r"));
        assert!(is_git_uri("git@host:u/r"));
        assert!(is_git_uri("something.github.com-ish")); // substring match, upstream quirk
        assert!(is_git_uri("local/path.git"));
        assert!(!is_git_uri("plain-query"));
    }
}
