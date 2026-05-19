//! Canonical identifier for a git repository, agreed across machines.
//! Used by the team-activity publisher so engineer A's `repo_abc` and
//! engineer B's `repo_xyz` for the same upstream resolve to the same row.

use crate::commands::helpers::exec_git;
use crate::error::Result;
use std::path::Path;

/// Returns the canonical remote URL for the repository at `repo_path`, or
/// an empty string when the repo has no `origin` remote (solo / not-yet-
/// pushed work).
///
/// Delegates to [`crate::commands::helpers::exec_git`] so it shares the
/// Windows `git.exe` resolution path (`which::which`) and the structured
/// `AppError::Command` error variant used elsewhere in the codebase. The
/// degenerate "no origin configured" case collapses any `Err` from
/// `exec_git` into `Ok(String::new())`, matching the documented design that
/// repos without an `origin` remote publish with `repo_remote_url = ""`.
pub fn read_origin_url(repo_path: &Path) -> Result<String> {
    match exec_git(&["remote", "get-url", "origin"], repo_path) {
        Ok(raw) => Ok(canonicalise_remote_url(&raw)),
        // No origin configured (or any other git failure) — surface as empty,
        // not an error. This is the documented degenerate case for
        // solo / not-yet-pushed work.
        Err(_) => Ok(String::new()),
    }
}

/// Pure normaliser: trim whitespace, strip trailing `.git`, lowercase host.
/// Host detection is best-effort — handles `https://`, `http://`, and SSH
/// `git@host:path` forms.
///
/// # Caveat
///
/// Case normalisation is intentionally limited and asymmetric:
///
/// - For `http(s)://` URLs, only the **host** is lowercased. The path
///   component is preserved verbatim, so
///   `https://github.com/Handoko/Repo` and `https://github.com/handoko/repo`
///   canonicalise to **different** strings even though GitHub treats them
///   as the same repository (paths are case-insensitive on GitHub).
/// - SSH-form URLs (`git@host:path`) pass through with **no case
///   normalisation at all** — neither host nor path is lowercased.
///
/// Consequently, two engineers who cloned the same upstream with different
/// casing on the path (HTTPS) or with any case difference (SSH) will be
/// treated by the publisher as working on *different* repos, breaking
/// cross-machine deduplication. The expectation is that a team agrees on
/// a single canonical clone URL form (one casing, one transport) and uses
/// it consistently. Loosening this behaviour (e.g. lowercasing paths on
/// known case-insensitive hosts) would silently change a security/identity
/// property and is deferred.
pub fn canonicalise_remote_url(raw: &str) -> String {
    let trimmed = raw.trim();
    // Strip `.git` suffix first, *then* trim trailing slashes — handles the
    // `https://host/x/y/.git` edge case which would otherwise leave a stray
    // trailing `/` because `.git` no longer matches after the slash trim.
    let without_git = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    let without_git = without_git.trim_end_matches('/');
    // https://Host/path → https://host/path
    if let Some(rest) = without_git.strip_prefix("https://") {
        if let Some((host, path)) = rest.split_once('/') {
            return format!("https://{}/{}", host.to_ascii_lowercase(), path);
        }
        return format!("https://{}", rest.to_ascii_lowercase());
    }
    if let Some(rest) = without_git.strip_prefix("http://") {
        if let Some((host, path)) = rest.split_once('/') {
            return format!("http://{}/{}", host.to_ascii_lowercase(), path);
        }
        return format!("http://{}", rest.to_ascii_lowercase());
    }
    // SSH form is case-sensitive on the path portion; leave alone.
    without_git.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn canonical_repo_url_strips_trailing_dot_git() {
        assert_eq!(
            canonicalise_remote_url("git@github.com:handokobeni/ansambel.git"),
            "git@github.com:handokobeni/ansambel"
        );
    }

    #[test]
    fn canonical_repo_url_lowercases_host_only() {
        assert_eq!(
            canonicalise_remote_url("https://GitHub.com/Handoko/Repo"),
            "https://github.com/Handoko/Repo"
        );
    }

    #[test]
    fn canonical_repo_url_passes_through_when_empty() {
        assert_eq!(canonicalise_remote_url(""), "");
    }

    #[test]
    fn canonical_repo_url_trims_whitespace() {
        assert_eq!(
            canonicalise_remote_url("  https://github.com/x/y.git\n"),
            "https://github.com/x/y"
        );
    }

    #[test]
    fn canonical_repo_url_handles_trailing_slash_before_dot_git() {
        assert_eq!(
            canonicalise_remote_url("https://github.com/x/y/.git"),
            "https://github.com/x/y"
        );
    }

    /// Initialise an empty git repo at `path` with a basic user identity
    /// configured so subsequent operations (had we needed any) wouldn't
    /// complain. Mirrors the fixture pattern in `commands::helpers::tests`.
    fn git_init_with_config(path: &Path) {
        Command::new("git")
            .args(["init"])
            .current_dir(path)
            .stderr(Stdio::piped())
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(path)
            .stderr(Stdio::piped())
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(path)
            .stderr(Stdio::piped())
            .output()
            .unwrap();
    }

    #[test]
    fn read_origin_url_returns_canonicalised_remote_when_origin_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git_init_with_config(&repo);
        Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/Test/Repo.git",
            ])
            .current_dir(&repo)
            .stderr(Stdio::piped())
            .output()
            .unwrap();

        let url = read_origin_url(&repo).unwrap();
        assert_eq!(url, "https://github.com/Test/Repo");
    }

    #[test]
    fn read_origin_url_returns_empty_when_no_origin() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git_init_with_config(&repo);
        // Deliberately no `git remote add origin …` call.

        let url = read_origin_url(&repo).unwrap();
        assert_eq!(url, "");
    }
}
