use crate::error::{AppError, Result};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Current time as Unix timestamp (seconds).
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Resolve the `git` binary. Uses PATH on all platforms.
/// On Windows, `which::which` will find `git.exe` correctly.
fn git_binary() -> std::path::PathBuf {
    which::which("git").unwrap_or_else(|_| std::path::PathBuf::from("git"))
}

/// Run `git <args>` in `cwd`, return trimmed stdout on success,
/// or `AppError::Command` carrying stderr on nonzero exit.
pub fn exec_git(args: &[&str], cwd: &Path) -> Result<String> {
    let git = git_binary();
    let output = std::process::Command::new(&git)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| AppError::Command {
            cmd: git.display().to_string(),
            msg: e.to_string(),
        })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(AppError::Command {
            cmd: format!("git {}", args.join(" ")),
            msg: stderr,
        })
    }
}

/// Detect the default branch from origin remote tracking refs.
///
/// Tier 1: `git symbolic-ref --short refs/remotes/origin/HEAD`
/// Tier 2: probe `git ls-remote --heads origin main` then `master`
///
/// Never falls back to local branches — workspaces must always branch from origin.
pub fn detect_default_branch(repo_path: &Path) -> Result<String> {
    // Tier 1: origin HEAD symref
    let tier1 = exec_git(
        &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"],
        repo_path,
    );
    if let Ok(ref_short) = tier1 {
        // ref_short looks like "origin/main"
        if let Some(branch) = ref_short.strip_prefix("origin/") {
            return Ok(branch.to_string());
        }
    }

    // Tier 2: probe ls-remote for known default names
    for candidate in ["main", "master"] {
        let ls = exec_git(&["ls-remote", "--heads", "origin", candidate], repo_path);
        if let Ok(out) = ls {
            if !out.is_empty() {
                return Ok(candidate.to_string());
            }
        }
    }

    Err(AppError::InvalidState(
        "Could not detect default branch from origin remote. \
         No origin/HEAD, origin/main, or origin/master found. \
         Run `git remote set-head origin --auto` or check your remote configuration."
            .into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn make_repo_with_origin_main(
        tmp: &tempfile::TempDir,
    ) -> (std::path::PathBuf, std::path::PathBuf) {
        // Create a bare "remote" repo
        let remote = tmp.path().join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        Command::new("git")
            .args(["init", "--bare"])
            .current_dir(&remote)
            .output()
            .unwrap();

        // Clone it as local repo
        let local = tmp.path().join("local");
        Command::new("git")
            .args(["clone", remote.to_str().unwrap(), local.to_str().unwrap()])
            .output()
            .unwrap();

        // Configure identity for commits
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(&local)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&local)
            .output()
            .unwrap();

        // Make an initial commit so the branch exists
        let readme = local.join("README.md");
        std::fs::write(&readme, b"init").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(&local)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&local)
            .output()
            .unwrap();
        Command::new("git")
            .args(["push", "origin", "HEAD:main"])
            .current_dir(&local)
            .output()
            .unwrap();

        // Set origin HEAD to main
        Command::new("git")
            .args(["remote", "set-head", "origin", "main"])
            .current_dir(&local)
            .output()
            .unwrap();

        (local, remote)
    }

    #[test]
    fn detect_default_branch_finds_main_via_symbolic_ref() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, _remote) = make_repo_with_origin_main(&tmp);
        let branch = detect_default_branch(&local).unwrap();
        assert_eq!(branch, "main");
    }

    #[test]
    fn detect_default_branch_falls_back_to_ls_remote() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, _remote) = make_repo_with_origin_main(&tmp);

        // Remove origin/HEAD symref to force tier-2 fallback
        let _ = Command::new("git")
            .args(["remote", "set-head", "origin", "--delete"])
            .current_dir(&local)
            .output();

        let branch = detect_default_branch(&local).unwrap();
        assert_eq!(branch, "main");
    }

    #[test]
    fn detect_default_branch_no_origin_returns_err() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("noop");
        std::fs::create_dir_all(&repo).unwrap();
        Command::new("git")
            .args(["init"])
            .current_dir(&repo)
            .output()
            .unwrap();
        // No remote added
        let result = detect_default_branch(&repo);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("Could not detect") || msg.contains("origin"),
            "Got: {msg}"
        );
    }

    #[test]
    fn now_unix_is_recent() {
        let t = now_unix();
        // Should be after 2026-01-01 (unix 1_767_225_600) and before 2100
        assert!(t > 1_767_225_600, "now_unix returned {t}, expected > 2026");
        assert!(t < 4_102_444_800, "now_unix returned {t}, expected < 2100");
    }

    #[test]
    fn exec_git_version_returns_non_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let out = exec_git(&["--version"], tmp.path()).unwrap();
        assert!(!out.is_empty());
        assert!(out.starts_with("git version"), "Got: {out}");
    }

    #[test]
    fn exec_git_invalid_subcommand_returns_err() {
        let tmp = tempfile::tempdir().unwrap();
        let result = exec_git(&["__no_such_subcommand__"], tmp.path());
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("git") || msg.contains("External command"),
            "Got: {msg}"
        );
    }
}
