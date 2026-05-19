//! Canonical identifier for a git repository, agreed across machines.
//! Used by the team-activity publisher so engineer A's `repo_abc` and
//! engineer B's `repo_xyz` for the same upstream resolve to the same row.

use crate::error::{AppError, Result};
use std::path::Path;
use std::process::Command;

/// Returns the canonical remote URL for the repository at `repo_path`, or
/// an empty string when the repo has no `origin` remote (solo / not-yet-
/// pushed work).
pub fn read_origin_url(repo_path: &Path) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["remote", "get-url", "origin"])
        .output()
        .map_err(|e| AppError::Git(format!("git remote get-url origin: {e}")))?;
    if !output.status.success() {
        // No origin configured — surface as empty, not an error.
        return Ok(String::new());
    }
    let raw = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(canonicalise_remote_url(&raw))
}

/// Pure normaliser: trim whitespace, strip trailing `.git`, lowercase host.
/// Host detection is best-effort — handles `https://`, `http://`, and SSH
/// `git@host:path` forms.
pub fn canonicalise_remote_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    let without_git = trimmed.strip_suffix(".git").unwrap_or(trimmed);
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
}
