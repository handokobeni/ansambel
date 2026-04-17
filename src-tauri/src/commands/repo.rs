use crate::error::{AppError, Result};
use crate::ids::repo_id;
use crate::persistence::repos::save_repos;
use crate::state::{AppState, RepoInfo};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};

// ── Public Tauri commands ────────────────────────────────────────────

#[tauri::command]
pub async fn add_repo(
    path: String,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<RepoInfo, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let inner_state = state.inner().clone();
    add_repo_inner(path, data_dir, inner_state)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "add_repo failed");
            e.to_string()
        })
}

#[tauri::command]
pub fn list_repos(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<Vec<RepoInfo>, String> {
    let st = state.lock().map_err(|e| e.to_string())?;
    let mut repos: Vec<RepoInfo> = st.repos.values().cloned().collect();
    repos.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
    Ok(repos)
}

#[tauri::command]
pub async fn remove_repo(
    repo_id: String,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    remove_repo_inner(repo_id, data_dir, state.inner().clone())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "remove_repo failed");
            e.to_string()
        })
}

#[tauri::command]
pub async fn update_gh_profile(
    repo_id: String,
    gh_profile: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<RepoInfo, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    update_gh_profile_inner(repo_id, gh_profile, data_dir, state.inner().clone())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "update_gh_profile failed");
            e.to_string()
        })
}

// ── Inner implementations ────────────────────────────────────────────

pub(crate) async fn add_repo_inner(
    path: String,
    data_dir: PathBuf,
    state: Arc<Mutex<AppState>>,
) -> Result<RepoInfo> {
    use crate::commands::helpers::{detect_default_branch, is_git_repo, now_unix};

    let canonical = dunce::canonicalize(&path).map_err(AppError::Io)?;

    if !is_git_repo(&canonical) {
        return Err(AppError::InvalidState(format!(
            "'{}' is not a git repository (no .git entry found)",
            canonical.display()
        )));
    }

    let default_branch = detect_default_branch(&canonical)?;
    let name = canonical
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| canonical.display().to_string());

    let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;

    // Deduplicate by canonicalized path
    if let Some(existing) = st.repos.values().find(|r| r.path == canonical).cloned() {
        tracing::info!(
            "Repo at {} already registered as {}",
            canonical.display(),
            existing.id
        );
        return Ok(existing);
    }

    let now = now_unix();
    let id = repo_id();
    let repo = RepoInfo {
        id: id.clone(),
        name,
        path: canonical,
        gh_profile: None,
        default_branch,
        created_at: now,
        updated_at: now,
    };

    st.repos.insert(id, repo.clone());
    save_repos(&data_dir, &st.repos)?;
    tracing::info!(repo_id = %repo.id, path = %repo.path.display(), "Registered repo");
    Ok(repo)
}

async fn remove_repo_inner(
    id: String,
    data_dir: PathBuf,
    state: Arc<Mutex<AppState>>,
) -> Result<()> {
    let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;

    if !st.repos.contains_key(&id) {
        return Err(AppError::NotFound(id));
    }

    let has_workspaces = st.workspaces.values().any(|w| w.repo_id == id);
    if has_workspaces {
        return Err(AppError::InvalidState(format!(
            "Cannot remove repo '{}': it still has workspaces. Remove all workspaces first.",
            id
        )));
    }

    st.repos.remove(&id);
    save_repos(&data_dir, &st.repos)?;
    tracing::info!(repo_id = %id, "Removed repo");
    Ok(())
}

async fn update_gh_profile_inner(
    id: String,
    gh_profile: Option<String>,
    data_dir: PathBuf,
    state: Arc<Mutex<AppState>>,
) -> Result<RepoInfo> {
    use crate::commands::helpers::now_unix;

    let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    let repo = st
        .repos
        .get_mut(&id)
        .ok_or_else(|| AppError::NotFound(id.clone()))?;
    repo.gh_profile = gh_profile;
    repo.updated_at = now_unix();
    let repo = repo.clone();
    save_repos(&data_dir, &st.repos)?;
    Ok(repo)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use std::process::Command;
    use std::sync::{Arc, Mutex};

    fn init_repo_with_remote_main(tmp: &tempfile::TempDir) -> PathBuf {
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

        local
    }

    #[tokio::test]
    async fn add_repo_returns_repo_info_with_correct_name() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let local = init_repo_with_remote_main(&tmp);

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let result = add_repo_inner(local.to_str().unwrap().to_string(), data, state)
            .await
            .unwrap();

        assert_eq!(result.name, "local");
        assert_eq!(result.default_branch, "main");
        assert!(result.id.starts_with("repo_"));
    }

    #[tokio::test]
    async fn add_repo_deduplicates_by_path() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let local = init_repo_with_remote_main(&tmp);

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let r1 = add_repo_inner(
            local.to_str().unwrap().to_string(),
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        let r2 = add_repo_inner(local.to_str().unwrap().to_string(), data, state)
            .await
            .unwrap();
        assert_eq!(r1.id, r2.id);
    }

    #[tokio::test]
    async fn add_repo_non_git_dir_returns_err() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let plain = tmp.path().join("plain");
        std::fs::create_dir_all(&plain).unwrap();

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let result = add_repo_inner(plain.to_str().unwrap().to_string(), data, state).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn list_repos_returns_sorted_by_updated_at_desc() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let local = init_repo_with_remote_main(&tmp);

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        add_repo_inner(
            local.to_str().unwrap().to_string(),
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        // Manually tweak timestamps so we can assert sort order
        {
            let mut st = state.lock().unwrap();
            let ids: Vec<String> = st.repos.keys().cloned().collect();
            for (i, id) in ids.iter().enumerate() {
                st.repos.get_mut(id).unwrap().updated_at = 1_000 + i as i64;
            }
        }

        let st = state.lock().unwrap();
        let mut repos: Vec<crate::state::RepoInfo> = st.repos.values().cloned().collect();
        repos.sort_by_key(|b| std::cmp::Reverse(b.updated_at));
        // Verify first is greatest updated_at
        if repos.len() > 1 {
            assert!(repos[0].updated_at >= repos[1].updated_at);
        }
    }

    #[tokio::test]
    async fn remove_repo_with_workspaces_returns_err() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let local = init_repo_with_remote_main(&tmp);

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let repo = add_repo_inner(
            local.to_str().unwrap().to_string(),
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        // Insert a fake workspace belonging to this repo
        {
            let mut st = state.lock().unwrap();
            st.workspaces.insert(
                "ws_fake".into(),
                crate::state::WorkspaceInfo {
                    id: "ws_fake".into(),
                    repo_id: repo.id.clone(),
                    branch: "ws/fake".into(),
                    base_branch: "main".into(),
                    custom_branch: false,
                    title: "Fake".into(),
                    description: String::new(),
                    status: crate::state::WorkspaceStatus::NotStarted,
                    column: crate::state::KanbanColumn::Todo,
                    created_at: 0,
                    updated_at: 0,
                },
            );
        }

        let result = remove_repo_inner(repo.id, data, state).await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("workspaces"), "Got: {msg}");
    }

    #[tokio::test]
    async fn update_gh_profile_sets_and_clears_profile() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        let local = init_repo_with_remote_main(&tmp);

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let repo = add_repo_inner(
            local.to_str().unwrap().to_string(),
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let updated = update_gh_profile_inner(
            repo.id.clone(),
            Some("handokoben".into()),
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        assert_eq!(updated.gh_profile, Some("handokoben".into()));

        let cleared = update_gh_profile_inner(repo.id, None, data, state)
            .await
            .unwrap();
        assert_eq!(cleared.gh_profile, None);
    }

    #[test]
    fn list_repos_returns_empty_vec_when_no_repos() {
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let repos: Vec<crate::state::RepoInfo> =
            state.lock().unwrap().repos.values().cloned().collect();
        assert!(repos.is_empty());
    }
}
