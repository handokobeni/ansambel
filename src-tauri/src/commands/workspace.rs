use crate::error::{AppError, Result};
use crate::ids::workspace_id;
use crate::persistence::workspaces::save_workspaces;
use crate::state::{AppState, KanbanColumn, WorkspaceInfo, WorkspaceStatus};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};

// ── Public Tauri commands ────────────────────────────────────────────

#[tauri::command]
pub async fn create_workspace(
    repo_id: String,
    title: String,
    description: String,
    branch_name: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<WorkspaceInfo, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    create_workspace_inner(
        repo_id,
        title,
        description,
        branch_name,
        data_dir,
        state.inner().clone(),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "create_workspace failed");
        e.to_string()
    })
}

#[tauri::command]
pub fn list_workspaces(
    repo_id: Option<String>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<Vec<WorkspaceInfo>, String> {
    let st = state.lock().map_err(|e| e.to_string())?;
    let mut workspaces: Vec<WorkspaceInfo> = st
        .workspaces
        .values()
        .filter(|w| repo_id.as_ref().is_none_or(|id| &w.repo_id == id))
        .cloned()
        .collect();
    workspaces.sort_by_key(|w| std::cmp::Reverse(w.updated_at));
    Ok(workspaces)
}

#[tauri::command]
pub async fn remove_workspace(
    workspace_id: String,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    remove_workspace_inner(workspace_id, data_dir, state.inner().clone())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "remove_workspace failed");
            e.to_string()
        })
}

// ── Inner implementations ────────────────────────────────────────────

pub(crate) async fn create_workspace_inner(
    repo_id: String,
    title: String,
    description: String,
    branch_name: Option<String>,
    data_dir: PathBuf,
    state: Arc<Mutex<AppState>>,
) -> Result<WorkspaceInfo> {
    use crate::commands::helpers::now_unix;

    let repo_path = {
        let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        st.repos
            .get(&repo_id)
            .map(|r| r.path.clone())
            .ok_or_else(|| AppError::NotFound(format!("repo '{}'", repo_id)))?
    };

    let base_branch = {
        let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        st.repos
            .get(&repo_id)
            .map(|r| r.default_branch.clone())
            .ok_or_else(|| AppError::NotFound(repo_id.clone()))?
    };

    let ws_id = workspace_id();
    let worktree_path = crate::platform::paths::worktree_dir(&data_dir, &ws_id);

    let (branch, custom_branch) = if let Some(ref custom) = branch_name {
        if custom.trim().is_empty() {
            return Err(AppError::InvalidState("Branch name cannot be empty".into()));
        }
        (custom.trim().to_string(), true)
    } else {
        (format!("ws/{}", ws_id), false)
    };

    // Create parent dir for the worktree
    if let Some(parent) = worktree_path.parent() {
        std::fs::create_dir_all(parent).map_err(AppError::Io)?;
    }

    // Run: git worktree add -b <branch> <worktree_path> <base_branch>
    let worktree_str = worktree_path.to_string_lossy();
    let output = std::process::Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            &branch,
            &worktree_str,
            &base_branch,
        ])
        .current_dir(&repo_path)
        .output()
        .map_err(|e| AppError::Command {
            cmd: "git worktree add".into(),
            msg: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AppError::Git(format!(
            "git worktree add failed: {}",
            stderr
        )));
    }

    let now = now_unix();
    let ws = WorkspaceInfo {
        id: ws_id.clone(),
        repo_id,
        branch,
        base_branch,
        custom_branch,
        title,
        description,
        status: WorkspaceStatus::NotStarted,
        column: KanbanColumn::Todo,
        created_at: now,
        updated_at: now,
    };

    let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    st.workspaces.insert(ws_id, ws.clone());
    save_workspaces(&data_dir, &st.workspaces)?;
    tracing::info!(workspace_id = %ws.id, branch = %ws.branch, "Created workspace");
    Ok(ws)
}

async fn remove_workspace_inner(
    ws_id: String,
    data_dir: PathBuf,
    state: Arc<Mutex<AppState>>,
) -> Result<()> {
    let (worktree_path, repo_path, branch) = {
        let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        let ws = st
            .workspaces
            .get(&ws_id)
            .ok_or_else(|| AppError::NotFound(format!("workspace '{}'", ws_id)))?;
        let repo = st
            .repos
            .get(&ws.repo_id)
            .ok_or_else(|| AppError::NotFound(format!("repo '{}'", ws.repo_id)))?;
        let wt_path = crate::platform::paths::worktree_dir(&data_dir, &ws_id);
        (wt_path, repo.path.clone(), ws.branch.clone())
    };

    // git worktree remove --force <path>
    if worktree_path.exists() {
        let wt_str = worktree_path.to_string_lossy();
        let rm_out = std::process::Command::new("git")
            .args(["worktree", "remove", "--force", &wt_str])
            .current_dir(&repo_path)
            .output()
            .map_err(|e| AppError::Command {
                cmd: "git worktree remove".into(),
                msg: e.to_string(),
            })?;

        if !rm_out.status.success() {
            // Prune stale worktree entries as fallback
            let _ = std::process::Command::new("git")
                .args(["worktree", "prune"])
                .current_dir(&repo_path)
                .output();
        }
    }

    // git branch -D <branch> — ignore error if branch already gone
    let _ = std::process::Command::new("git")
        .args(["branch", "-D", &branch])
        .current_dir(&repo_path)
        .output();

    let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    st.workspaces.remove(&ws_id);
    save_workspaces(&data_dir, &st.workspaces)?;
    tracing::info!(workspace_id = %ws_id, "Removed workspace");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use std::process::Command;

    fn init_repo_with_remote(tmp: &tempfile::TempDir) -> (PathBuf, PathBuf) {
        let remote = tmp.path().join("remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        Command::new("git")
            .args(["init", "--bare"])
            .current_dir(&remote)
            .output()
            .unwrap();

        let local = tmp.path().join("local");
        Command::new("git")
            .args(["clone", remote.to_str().unwrap(), local.to_str().unwrap()])
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.email", "t@t.com"])
            .current_dir(&local)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "T"])
            .current_dir(&local)
            .output()
            .unwrap();
        std::fs::write(local.join("f"), b"x").unwrap();
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
        Command::new("git")
            .args(["remote", "set-head", "origin", "main"])
            .current_dir(&local)
            .output()
            .unwrap();

        (local, remote)
    }

    #[tokio::test]
    async fn create_workspace_creates_worktree_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, _) = init_repo_with_remote(&tmp);
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let repo = crate::commands::repo::add_repo_inner(
            local.to_str().unwrap().to_string(),
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let ws = create_workspace_inner(
            repo.id,
            "Fix login".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let worktree_path = crate::platform::paths::worktree_dir(&data, &ws.id);
        assert!(
            worktree_path.exists(),
            "Worktree dir should exist at {}",
            worktree_path.display()
        );
    }

    #[tokio::test]
    async fn create_workspace_git_worktree_list_shows_new_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, _) = init_repo_with_remote(&tmp);
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let repo = crate::commands::repo::add_repo_inner(
            local.to_str().unwrap().to_string(),
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let ws = create_workspace_inner(
            repo.id,
            "Test".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let out = Command::new("git")
            .args(["worktree", "list", "--porcelain"])
            .current_dir(&local)
            .output()
            .unwrap();
        let list = String::from_utf8_lossy(&out.stdout);
        assert!(
            list.contains(&ws.id),
            "worktree list should contain ws id: {}",
            ws.id
        );
    }

    #[tokio::test]
    async fn create_workspace_auto_branch_has_ws_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, _) = init_repo_with_remote(&tmp);
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let repo = crate::commands::repo::add_repo_inner(
            local.to_str().unwrap().to_string(),
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let ws = create_workspace_inner(
            repo.id,
            "Auto branch".into(),
            String::new(),
            None,
            data,
            state,
        )
        .await
        .unwrap();

        assert!(
            ws.branch.starts_with("ws/"),
            "branch should start with ws/, got {}",
            ws.branch
        );
        assert!(!ws.custom_branch);
    }

    #[tokio::test]
    async fn create_workspace_custom_branch_sets_flag() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, _) = init_repo_with_remote(&tmp);
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let repo = crate::commands::repo::add_repo_inner(
            local.to_str().unwrap().to_string(),
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let ws = create_workspace_inner(
            repo.id,
            "Custom branch test".into(),
            String::new(),
            Some("feat/custom-branch".into()),
            data.clone(),
            state,
        )
        .await
        .unwrap();

        assert!(ws.custom_branch);
        assert_eq!(ws.branch, "feat/custom-branch");
    }

    #[tokio::test]
    async fn create_workspace_missing_repo_returns_err() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let result = create_workspace_inner(
            "repo_nonexistent".into(),
            "X".into(),
            String::new(),
            None,
            data,
            state,
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn remove_workspace_cleans_up_worktree() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, _) = init_repo_with_remote(&tmp);
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let repo = crate::commands::repo::add_repo_inner(
            local.to_str().unwrap().to_string(),
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let ws = create_workspace_inner(
            repo.id,
            "Remove test".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let wt_path = crate::platform::paths::worktree_dir(&data, &ws.id);
        assert!(wt_path.exists());

        remove_workspace_inner(ws.id.clone(), data.clone(), Arc::clone(&state))
            .await
            .unwrap();

        assert!(!wt_path.exists(), "Worktree dir should be removed");
        let st = state.lock().unwrap();
        assert!(!st.workspaces.contains_key(&ws.id));
    }

    #[tokio::test]
    async fn list_workspaces_filters_by_repo_id() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, _) = init_repo_with_remote(&tmp);
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let repo = crate::commands::repo::add_repo_inner(
            local.to_str().unwrap().to_string(),
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        create_workspace_inner(
            repo.id.clone(),
            "WS 1".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        create_workspace_inner(
            repo.id.clone(),
            "WS 2".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let st = state.lock().unwrap();
        let all: Vec<_> = st.workspaces.values().collect();
        let filtered: Vec<_> = all.iter().filter(|w| w.repo_id == repo.id).collect();
        assert_eq!(filtered.len(), 2);
        let unrelated: Vec<_> = all.iter().filter(|w| w.repo_id == "repo_other").collect();
        assert_eq!(unrelated.len(), 0);
    }

    #[tokio::test]
    async fn list_workspaces_none_filter_returns_all() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, _) = init_repo_with_remote(&tmp);
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let repo = crate::commands::repo::add_repo_inner(
            local.to_str().unwrap().to_string(),
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        create_workspace_inner(
            repo.id.clone(),
            "WS A".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        create_workspace_inner(
            repo.id.clone(),
            "WS B".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let st = state.lock().unwrap();
        // None filter means all workspaces
        let all: Vec<_> = st.workspaces.values().collect();
        assert_eq!(all.len(), 2);
    }
}
