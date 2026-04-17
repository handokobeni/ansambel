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

#[cfg(test)]
mod tests {
    use super::*;

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
