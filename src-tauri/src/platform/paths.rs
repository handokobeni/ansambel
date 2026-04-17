use crate::error::Result;
use std::path::{Path, PathBuf};

pub fn worktree_dir(data_dir: &Path, workspace_id: &str) -> PathBuf {
    data_dir.join("workspaces").join(workspace_id)
}

pub fn messages_file(data_dir: &Path, workspace_id: &str) -> PathBuf {
    data_dir
        .join("messages")
        .join(format!("{}.json", workspace_id))
}

pub fn todos_file(data_dir: &Path, workspace_id: &str) -> PathBuf {
    data_dir
        .join("todos")
        .join(format!("{}.json", workspace_id))
}

pub fn autopilot_log_file(data_dir: &Path, workspace_id: &str) -> PathBuf {
    data_dir
        .join("autopilot_log")
        .join(format!("{}.json", workspace_id))
}

pub fn context_dir(data_dir: &Path, repo_id: &str) -> PathBuf {
    data_dir.join("contexts").join(repo_id)
}

pub fn images_dir(data_dir: &Path, workspace_id: &str) -> PathBuf {
    data_dir.join("images").join(workspace_id)
}

pub fn repos_file(data_dir: &Path) -> PathBuf {
    data_dir.join("repos.json")
}
pub fn workspaces_file(data_dir: &Path) -> PathBuf {
    data_dir.join("workspaces.json")
}
pub fn sessions_file(data_dir: &Path) -> PathBuf {
    data_dir.join("sessions.json")
}
pub fn app_settings_file(data_dir: &Path) -> PathBuf {
    data_dir.join("app_settings.json")
}
pub fn context_meta_file(data_dir: &Path) -> PathBuf {
    data_dir.join("context_meta.json")
}

pub fn lock_file(data_dir: &Path) -> PathBuf {
    data_dir.join(".ansambel.lock")
}
pub fn logs_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("logs")
}
pub fn crash_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("logs").join("crashes")
}

pub fn ensure_data_dirs(data_dir: &Path) -> Result<()> {
    for sub in [
        "workspaces",
        "messages",
        "contexts",
        "todos",
        "autopilot_log",
        "images",
        "logs",
        "logs/crashes",
    ] {
        std::fs::create_dir_all(data_dir.join(sub))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn worktree_dir_is_subdir_of_data_dir() {
        let data = PathBuf::from("/tmp/ansambel");
        let wt = worktree_dir(&data, "ws_abc123");
        assert!(wt.starts_with(&data));
        assert!(wt.ends_with("workspaces/ws_abc123"));
    }

    #[test]
    fn messages_file_path_uses_workspace_id() {
        let data = PathBuf::from("/tmp/ansambel");
        let p = messages_file(&data, "ws_abc123");
        assert_eq!(p, PathBuf::from("/tmp/ansambel/messages/ws_abc123.json"));
    }

    #[test]
    fn context_dir_is_under_contexts() {
        let data = PathBuf::from("/tmp/ansambel");
        let p = context_dir(&data, "repo_xyz");
        assert_eq!(p, PathBuf::from("/tmp/ansambel/contexts/repo_xyz"));
    }

    #[test]
    fn repos_json_path_is_at_data_dir_root() {
        let data = PathBuf::from("/tmp/ansambel");
        let p = repos_file(&data);
        assert_eq!(p, PathBuf::from("/tmp/ansambel/repos.json"));
    }

    #[test]
    fn ensure_data_dirs_creates_all_subdirs() {
        let tmp = tempfile::tempdir().unwrap();
        ensure_data_dirs(tmp.path()).unwrap();
        assert!(tmp.path().join("workspaces").is_dir());
        assert!(tmp.path().join("messages").is_dir());
        assert!(tmp.path().join("contexts").is_dir());
        assert!(tmp.path().join("todos").is_dir());
        assert!(tmp.path().join("autopilot_log").is_dir());
        assert!(tmp.path().join("images").is_dir());
        assert!(tmp.path().join("logs").is_dir());
        assert!(tmp.path().join("logs/crashes").is_dir());
    }
}
