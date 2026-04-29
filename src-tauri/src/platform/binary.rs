use std::path::{Path, PathBuf};

/// Locate a CLI binary with a 3-step fallback chain:
/// 1. `override_path` (user-set absolute path), if it exists
/// 2. `PATH` lookup via `which`
/// 3. Any of the `fallback_paths` that exists
///
/// Returns `None` when none of the sources locate an existing file.
pub fn detect_binary(
    override_path: Option<&Path>,
    name: &str,
    fallback_paths: &[&PathBuf],
) -> Option<PathBuf> {
    if let Some(p) = override_path {
        if p.exists() {
            return Some(p.to_path_buf());
        }
        return None;
    }
    if let Ok(p) = which::which(name) {
        return Some(p);
    }
    for candidate in fallback_paths {
        if candidate.exists() {
            return Some((*candidate).clone());
        }
    }
    None
}

pub fn claude_binary(override_path: Option<&Path>) -> Option<PathBuf> {
    let fallbacks = default_claude_fallbacks();
    let borrowed: Vec<&PathBuf> = fallbacks.iter().collect();
    detect_binary(override_path, "claude", &borrowed)
}

pub fn gh_binary(override_path: Option<&Path>) -> Option<PathBuf> {
    let fallbacks = default_gh_fallbacks();
    let borrowed: Vec<&PathBuf> = fallbacks.iter().collect();
    detect_binary(override_path, "gh", &borrowed)
}

pub fn git_binary(override_path: Option<&Path>) -> Option<PathBuf> {
    detect_binary(override_path, "git", &[])
}

fn default_claude_fallbacks() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        let mut v = Vec::new();
        if let Ok(appdata) = std::env::var("APPDATA") {
            v.push(PathBuf::from(&appdata).join("npm").join("claude.cmd"));
        }
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            v.push(
                PathBuf::from(&local)
                    .join("Programs")
                    .join("claude")
                    .join("claude.exe"),
            );
        }
        v
    }
    #[cfg(target_os = "macos")]
    {
        let home = dirs_home();
        vec![
            PathBuf::from("/opt/homebrew/bin/claude"),
            PathBuf::from("/usr/local/bin/claude"),
            home.join(".local/bin/claude"),
        ]
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let home = dirs_home();
        vec![
            home.join(".local/bin/claude"),
            PathBuf::from("/usr/local/bin/claude"),
            PathBuf::from("/usr/bin/claude"),
        ]
    }
}

fn default_gh_fallbacks() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        vec![
            PathBuf::from(r"C:\Program Files\GitHub CLI\gh.exe"),
            PathBuf::from(r"C:\Program Files (x86)\GitHub CLI\gh.exe"),
        ]
    }
    #[cfg(target_os = "macos")]
    {
        vec![
            PathBuf::from("/opt/homebrew/bin/gh"),
            PathBuf::from("/usr/local/bin/gh"),
        ]
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        vec![
            PathBuf::from("/usr/bin/gh"),
            PathBuf::from("/usr/local/bin/gh"),
        ]
    }
}

#[cfg(unix)]
fn dirs_home() -> PathBuf {
    directories::UserDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detect_binary_returns_override_when_present_and_executable() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let p = tmp.path().to_path_buf();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&p).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&p, perms).unwrap();
        }
        let result = detect_binary(Some(&p), "any-name", &[]);
        assert_eq!(result, Some(p));
    }

    #[test]
    fn detect_binary_returns_none_when_override_does_not_exist() {
        let missing = PathBuf::from("/nonexistent/binary-xyz");
        let result = detect_binary(Some(&missing), "any-name", &[]);
        assert_eq!(result, None);
    }

    #[test]
    fn detect_binary_finds_real_system_binary() {
        let name = if cfg!(windows) { "cmd" } else { "sh" };
        let result = detect_binary(None, name, &[]);
        assert!(result.is_some(), "should find {} on PATH", name);
    }

    #[test]
    fn detect_binary_falls_back_to_provided_paths() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let p = tmp.path().to_path_buf();
        let result = detect_binary(None, "ansambel-surely-missing-binary", &[&p]);
        assert_eq!(result, Some(p));
    }

    #[test]
    fn detect_binary_returns_none_when_all_sources_fail() {
        let result = detect_binary(None, "ansambel-no-such-binary-exists", &[]);
        assert!(result.is_none());
    }

    #[test]
    fn claude_binary_returns_none_when_not_installed() {
        // On CI or clean environments, claude may not be installed.
        // We only assert it doesn't panic and returns an Option.
        let _result: Option<PathBuf> = claude_binary(None);
    }

    #[test]
    fn claude_binary_uses_override_path_when_provided() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let p = tmp.path().to_path_buf();
        let result = claude_binary(Some(&p));
        assert_eq!(result, Some(p));
    }

    #[test]
    fn gh_binary_returns_option_without_panicking() {
        let _result: Option<PathBuf> = gh_binary(None);
    }

    #[test]
    fn gh_binary_uses_override_path_when_provided() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let p = tmp.path().to_path_buf();
        let result = gh_binary(Some(&p));
        assert_eq!(result, Some(p));
    }

    #[test]
    fn git_binary_finds_git_on_path() {
        // git is virtually always available in dev/CI environments.
        let result = git_binary(None);
        assert!(result.is_some(), "git should be on PATH");
    }

    #[test]
    fn git_binary_uses_override_when_provided() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let p = tmp.path().to_path_buf();
        let result = git_binary(Some(&p));
        assert_eq!(result, Some(p));
    }
}
