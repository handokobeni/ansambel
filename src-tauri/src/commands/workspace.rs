use crate::error::{AppError, Result};
use crate::ids::workspace_id;
use crate::persistence::workspaces::save_workspaces;
use crate::state::{
    AppState, KanbanColumn, WorkspaceEvent, WorkspaceEventTx, WorkspaceInfo, WorkspaceStatus,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};

// ── Random workspace names ───────────────────────────────────────────
//
// Identical surface to korlap so worktree directory names stay
// human-readable on the filesystem. Branch names use the `ansambel/`
// prefix to make their origin obvious in `git branch` listings.

const ADJECTIVES: &[&str] = &[
    "swift", "calm", "bright", "gentle", "quiet", "bold", "keen", "warm", "cool", "wild", "deep",
    "soft", "sharp", "fresh", "still", "true", "pure", "rare", "wise", "fair", "clear", "proud",
    "quick", "neat", "slim", "vast", "vivid", "lucid", "amber", "misty",
];

const NOUNS: &[&str] = &[
    "oak", "elm", "pine", "fern", "moss", "reed", "sage", "mint", "jade", "onyx", "ruby", "opal",
    "hawk", "dove", "wolf", "bear", "fox", "lynx", "hare", "wren", "lark", "crow", "orca", "puma",
    "coral", "pearl", "ember", "dusk", "dawn", "vale",
];

const BRANCH_PREFIX: &str = "ansambel";
const MAX_NAME_RETRIES: u32 = 10;

/// Hash-seeded pick of an adjective-noun pair. Cheap, sync, and good
/// enough for workspace identifiers — collisions are caught and retried
/// by the caller via [`generate_unique_workspace_name`].
fn random_workspace_name() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};

    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    let h = hasher.finish();

    let adj = ADJECTIVES[(h as usize) % ADJECTIVES.len()];
    let noun = NOUNS[((h >> 16) as usize) % NOUNS.len()];
    format!("{adj}-{noun}")
}

/// Try up to [`MAX_NAME_RETRIES`] random names; on conflict (branch
/// already exists *or* worktree folder exists) append a short
/// disambiguator and try again.
///
/// Returns `(dir_name, branch)` where `branch = ansambel/<dir_name>`.
fn generate_unique_workspace_name(
    repo_path: &Path,
    workspace_root: &Path,
) -> Result<(String, String)> {
    let mut name = random_workspace_name();
    for attempt in 0..MAX_NAME_RETRIES {
        let branch_candidate = format!("{BRANCH_PREFIX}/{name}");
        let branch_exists = git_branch_exists(repo_path, &branch_candidate)?;
        let folder_exists = workspace_root.join(&name).exists();

        if !branch_exists && !folder_exists {
            return Ok((name, branch_candidate));
        }

        if attempt + 1 == MAX_NAME_RETRIES {
            return Err(AppError::Other(format!(
                "Could not generate a unique workspace name after {MAX_NAME_RETRIES} attempts"
            )));
        }

        // Disambiguate by appending the first 4 chars of a fresh ID. We
        // pull from `workspace_id()` rather than minting another
        // `random_workspace_name` so the suffix is guaranteed-distinct
        // even if the hash clock granularity collides.
        let suffix = workspace_id();
        let suffix = suffix.split('_').next_back().unwrap_or(&suffix);
        let suffix = &suffix[..suffix.len().min(4)];
        name = format!("{}-{suffix}", random_workspace_name());
    }
    // Loop is guaranteed to either return Ok or Err above.
    unreachable!("retry loop must terminate within MAX_NAME_RETRIES");
}

/// Returns true if `branch` resolves via `git rev-parse --verify`. Used
/// both to gate custom branch names and to drive the auto-name retry
/// loop. Surfaces I/O errors rather than swallowing them so the caller
/// can produce a descriptive error.
fn git_branch_exists(repo_path: &Path, branch: &str) -> Result<bool> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--verify", branch])
        .current_dir(repo_path)
        .output()
        .map_err(|e| AppError::Command {
            cmd: "git rev-parse".into(),
            msg: e.to_string(),
        })?;
    Ok(output.status.success())
}

// ── Public Tauri commands ────────────────────────────────────────────

#[tauri::command]
pub async fn create_workspace(
    repo_id: String,
    title: String,
    description: String,
    branch_name: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
    event_tx: State<'_, WorkspaceEventTx>,
) -> std::result::Result<WorkspaceInfo, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let publisher_tx: WorkspaceEventTx = event_tx.inner().clone();
    create_workspace_inner_with_publisher(
        repo_id,
        title,
        description,
        branch_name,
        data_dir,
        state.inner().clone(),
        Some(&publisher_tx),
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

/// Task 18 — flip the per-workspace privacy flag.
///
/// On `is_private = true`, the team-activity publisher's `private_lock`
/// kicks in (see `commands::team_activity::AggregatedState`) and the
/// sensitive columns are cleared on the next flush. On `false`, the
/// publisher resumes normal field population.
///
/// The new value is persisted to `workspaces.json` so the toggle survives
/// app restart, and `WorkspaceEvent::PrivacyChanged` is broadcast to the
/// publisher AFTER the state lock is released — per the Mutex-discipline
/// rule, we never hold the AppState lock across a broadcast send.
#[tauri::command]
pub async fn set_workspace_team_activity_private(
    workspace_id: String,
    is_private: bool,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
    event_tx: State<'_, WorkspaceEventTx>,
) -> std::result::Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let publisher_tx: WorkspaceEventTx = event_tx.inner().clone();
    set_workspace_team_activity_private_inner(
        workspace_id,
        is_private,
        data_dir,
        state.inner().clone(),
        Some(&publisher_tx),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "set_workspace_team_activity_private failed");
        e.to_string()
    })
}

pub(crate) async fn set_workspace_team_activity_private_inner(
    ws_id: String,
    is_private: bool,
    data_dir: PathBuf,
    state: Arc<Mutex<AppState>>,
    publisher_tx: Option<&WorkspaceEventTx>,
) -> Result<()> {
    {
        let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        let ws = st
            .workspaces
            .get_mut(&ws_id)
            .ok_or_else(|| AppError::NotFound(format!("workspace '{}'", ws_id)))?;
        if ws.team_activity_private == is_private {
            // Idempotent — still persist + emit so the publisher gets a
            // refresh event (cheap, and avoids subtle bugs where a UI
            // double-click yields no broadcast).
        }
        ws.team_activity_private = is_private;
        save_workspaces(&data_dir, &st.workspaces)?;
        tracing::info!(
            workspace_id = %ws_id,
            is_private,
            "Updated workspace team_activity_private"
        );
    } // lock dropped before broadcasting
    if let Some(tx) = publisher_tx {
        let _ = tx.send(WorkspaceEvent::PrivacyChanged {
            workspace_id: ws_id,
            is_private,
        });
    }
    Ok(())
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
    create_workspace_inner_with_publisher(
        repo_id,
        title,
        description,
        branch_name,
        data_dir,
        state,
        None,
    )
    .await
}

/// Variant of [`create_workspace_inner`] that broadcasts
/// `WorkspaceEvent::BranchChanged` to the team-activity publisher after
/// the new branch is materialised via `git worktree add -b`. In this
/// codebase, branch creation is the only branch-flipping event we ever
/// emit (there is no in-app `git checkout` command yet) — see Task 12 of
/// the Phase 3a-3 plan.
pub(crate) async fn create_workspace_inner_with_publisher(
    repo_id: String,
    title: String,
    description: String,
    branch_name: Option<String>,
    data_dir: PathBuf,
    state: Arc<Mutex<AppState>>,
    publisher_tx: Option<&WorkspaceEventTx>,
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
    let workspace_root = data_dir.join("workspaces");

    // Resolve `(dir_name, branch, custom)` triple. Custom-branch users
    // get an explicit duplicate check up front; auto-branch generation
    // picks a fresh adjective-noun and retries on conflict.
    let (dir_name, branch, custom_branch) = if let Some(ref custom) = branch_name {
        let custom = custom.trim();
        if custom.is_empty() {
            return Err(AppError::InvalidState("Branch name cannot be empty".into()));
        }
        if git_branch_exists(&repo_path, custom)? {
            return Err(AppError::Git(format!("Branch '{custom}' already exists")));
        }
        // Decouple the on-disk folder from the branch slug — branches
        // commonly contain `/` which would create a nested directory.
        let dir = random_workspace_name();
        (dir, custom.to_string(), true)
    } else {
        let (name, branch) = generate_unique_workspace_name(&repo_path, &workspace_root)?;
        (name, branch, false)
    };

    let worktree_path = workspace_root.join(&dir_name);

    // Create parent dir for the worktree.
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
        return Err(AppError::Git(format!("git worktree add failed: {stderr}")));
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
        worktree_dir: worktree_path.clone(),
        team_activity_private: false,
        task_ids: Vec::new(),
    };

    {
        let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        st.workspaces.insert(ws_id, ws.clone());
        save_workspaces(&data_dir, &st.workspaces)?;
        tracing::info!(workspace_id = %ws.id, branch = %ws.branch, dir = %dir_name, "Created workspace");
    } // lock dropped before broadcasting
      // Task 12: surface the new branch on the team-activity row. Emitted
      // here because `git worktree add -b <branch>` is the only branch-
      // creation path in the app today — there is no follow-up `git
      // checkout` command yet.
    if let Some(tx) = publisher_tx {
        let _ = tx.send(WorkspaceEvent::BranchChanged {
            workspace_id: ws.id.clone(),
            branch_name: ws.branch.clone(),
        });
    }
    Ok(ws)
}

/// True only when a workspace holds no work and is safe to auto-delete:
/// no chat messages, no commits ahead of its base branch, a clean
/// worktree, and no live agent. `agent_live` is computed by the caller
/// under the AppState lock (this fn shells out to git and must not hold
/// it). Fail-safe: any git error or unreadable message log is treated as
/// "not empty" so work is never destroyed on uncertainty.
pub(crate) fn is_workspace_empty(data_dir: &Path, ws: &WorkspaceInfo, agent_live: bool) -> bool {
    if agent_live {
        return false;
    }
    let messages_empty = crate::persistence::messages::load_messages(data_dir, &ws.id)
        .map(|m| m.is_empty())
        .unwrap_or(false);
    if !messages_empty {
        return false;
    }
    let wt = ws.worktree_dir.to_string_lossy().to_string();
    // Compare against the remote tracking ref (origin/<base>) so the check is
    // correct whether the worktree branch is a new ansambel/* branch or a local
    // tracking branch that git auto-created from origin during `worktree add`.
    let range = format!("origin/{}..HEAD", ws.base_branch);
    let no_commits = match std::process::Command::new("git")
        .args(["-C", &wt, "rev-list", "--count", &range])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim() == "0",
        _ => false,
    };
    if !no_commits {
        return false;
    }
    match std::process::Command::new("git")
        .args(["-C", &wt, "status", "--porcelain"])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().is_empty(),
        _ => false,
    }
}

pub(crate) async fn remove_workspace_inner(
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
        // Use the path stored on WorkspaceInfo: with random dir naming
        // we can't reconstruct it from `ws_id` alone any more.
        (
            ws.worktree_dir.clone(),
            repo.path.clone(),
            ws.branch.clone(),
        )
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

    // Kill any terminals the workspace owned so their PTYs don't leak.
    // Called OUTSIDE any held AppState lock — kill_workspace_terminals_inner
    // locks internally, so calling it inside a lock scope would deadlock.
    let _ = crate::commands::terminal::kill_workspace_terminals_inner(&ws_id, Arc::clone(&state));

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

    // ── Random naming helpers ────────────────────────────────────────

    #[test]
    fn random_workspace_name_uses_adjective_noun_format() {
        let name = random_workspace_name();
        let (adj, noun) = name.split_once('-').expect("name should have a '-'");
        assert!(
            ADJECTIVES.contains(&adj),
            "adjective '{adj}' not in ADJECTIVES"
        );
        assert!(NOUNS.contains(&noun), "noun '{noun}' not in NOUNS");
    }

    #[test]
    fn generate_unique_workspace_name_returns_branch_with_ansambel_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, _) = init_repo_with_remote(&tmp);
        let workspace_root = tmp.path().join("workspaces");
        let (_, branch) = generate_unique_workspace_name(&local, &workspace_root).unwrap();
        assert!(
            branch.starts_with("ansambel/"),
            "branch should start with ansambel/, got {branch}"
        );
    }

    #[test]
    fn generate_unique_workspace_name_skips_pre_existing_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, _) = init_repo_with_remote(&tmp);
        let workspace_root = tmp.path().join("workspaces");

        // Pre-create one of the candidate branches by hand. The
        // generator should sidestep it (suffix on second attempt). We
        // can't predict which adjective-noun the RNG will pick, so we
        // pre-create *all* of them — making the first attempt always
        // collide and forcing the suffix branch.
        for adj in ADJECTIVES {
            for noun in NOUNS {
                let _ = Command::new("git")
                    .args(["branch", &format!("{BRANCH_PREFIX}/{adj}-{noun}")])
                    .current_dir(&local)
                    .output();
            }
        }

        let (name, branch) = generate_unique_workspace_name(&local, &workspace_root).unwrap();
        // The first attempt's `<adj>-<noun>` always collides, so the
        // resolver must have appended a 4-char suffix.
        let parts: Vec<&str> = name.split('-').collect();
        assert!(
            parts.len() >= 3,
            "expected suffix-disambiguated name, got '{name}'"
        );
        assert!(branch.starts_with("ansambel/"));
        assert_eq!(branch, format!("ansambel/{name}"));
    }

    #[test]
    fn git_branch_exists_returns_false_for_unknown_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, _) = init_repo_with_remote(&tmp);
        assert!(!git_branch_exists(&local, "no/such/branch").unwrap());
    }

    #[test]
    fn git_branch_exists_returns_true_for_existing_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, _) = init_repo_with_remote(&tmp);
        Command::new("git")
            .args(["branch", "feature/preexisting"])
            .current_dir(&local)
            .output()
            .unwrap();
        assert!(git_branch_exists(&local, "feature/preexisting").unwrap());
    }

    // ── create_workspace_inner ───────────────────────────────────────

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

        assert!(
            ws.worktree_dir.exists(),
            "Worktree dir should exist at {}",
            ws.worktree_dir.display()
        );
        // Folder name is human-readable, not the workspace id.
        assert!(ws.worktree_dir.starts_with(data.join("workspaces")));
        assert!(!ws.worktree_dir.ends_with(&ws.id));
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
        // Compare against the basename only. `git worktree list` reports
        // paths with forward slashes on every platform AND can resolve
        // through 8.3 short names on Windows runners (e.g. RUNNER~1 vs
        // runneradmin), so a literal-string match against
        // `ws.worktree_dir.to_string_lossy()` is fragile cross-platform.
        // The random adjective-noun slug is unique enough to identify
        // the worktree without that fragility.
        let dir_name = ws
            .worktree_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        assert!(
            !dir_name.is_empty() && list.contains(dir_name),
            "worktree list should contain workspace dir name '{dir_name}', got: {list}"
        );
    }

    #[tokio::test]
    async fn create_workspace_auto_branch_uses_ansambel_prefix() {
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
            ws.branch.starts_with("ansambel/"),
            "branch should start with ansambel/, got {}",
            ws.branch
        );
        assert!(!ws.custom_branch);
        // Branch slug after the prefix should be parseable as
        // <adjective>-<noun> so it stays human-readable.
        let slug = ws.branch.strip_prefix("ansambel/").unwrap();
        assert!(
            slug.contains('-'),
            "expected adjective-noun slug, got {slug}"
        );
    }

    #[tokio::test]
    async fn create_workspace_custom_branch_sets_flag_and_uses_random_dir_name() {
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
        // Folder must NOT contain the slash from the branch name —
        // otherwise we'd create a nested `feat/custom-branch/` tree on
        // disk which `git worktree remove` later struggles with.
        let last = ws
            .worktree_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        assert!(!last.contains('/'));
        assert!(
            last.contains('-'),
            "expected adjective-noun dir, got {last}"
        );
    }

    #[tokio::test]
    async fn create_workspace_custom_branch_rejects_existing_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, _) = init_repo_with_remote(&tmp);
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        // Pre-create the branch so the pre-check trips.
        Command::new("git")
            .args(["branch", "feature/already-here"])
            .current_dir(&local)
            .output()
            .unwrap();

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        let repo = crate::commands::repo::add_repo_inner(
            local.to_str().unwrap().to_string(),
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let err = create_workspace_inner(
            repo.id,
            "Conflict".into(),
            String::new(),
            Some("feature/already-here".into()),
            data,
            state,
        )
        .await
        .unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.contains("already exists"),
            "expected 'already exists' in error, got: {msg}"
        );
    }

    #[tokio::test]
    async fn create_workspace_rejects_empty_custom_branch() {
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

        let err = create_workspace_inner(
            repo.id,
            "Empty branch".into(),
            String::new(),
            Some("   ".into()),
            data,
            state,
        )
        .await
        .unwrap_err();

        let msg = err.to_string();
        assert!(
            msg.to_lowercase().contains("empty"),
            "expected 'empty' in error, got: {msg}"
        );
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

        assert!(ws.worktree_dir.exists());

        remove_workspace_inner(ws.id.clone(), data.clone(), Arc::clone(&state))
            .await
            .unwrap();

        assert!(
            !ws.worktree_dir.exists(),
            "Worktree dir should be removed at {}",
            ws.worktree_dir.display()
        );
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

    // ── Task 12 — WorkspaceEvent::BranchChanged emission ─────────────────────

    #[tokio::test]
    async fn create_workspace_with_publisher_emits_branch_changed() {
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

        let (tx, mut rx) = tokio::sync::broadcast::channel::<crate::state::WorkspaceEvent>(8);
        let publisher_tx: crate::state::WorkspaceEventTx = std::sync::Arc::new(tx);

        let ws = create_workspace_inner_with_publisher(
            repo.id,
            "Branch-emit test".into(),
            String::new(),
            None,
            data,
            state,
            Some(&publisher_tx),
        )
        .await
        .unwrap();

        let ev = rx.try_recv().expect("BranchChanged should be emitted");
        match ev {
            crate::state::WorkspaceEvent::BranchChanged {
                workspace_id,
                branch_name,
            } => {
                assert_eq!(workspace_id, ws.id);
                assert_eq!(branch_name, ws.branch);
                assert!(
                    branch_name.starts_with("ansambel/"),
                    "auto-named branches must carry the ansambel/ prefix; got {branch_name}"
                );
            }
            other => panic!("expected BranchChanged, got {other:?}"),
        }
    }

    // ── Task 18 — per-workspace privacy toggle ───────────────────────────────

    #[tokio::test]
    async fn set_team_activity_private_toggles_flag_and_emits_event() {
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
            "Privacy toggle".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        assert!(!ws.team_activity_private);

        let (tx, mut rx) = tokio::sync::broadcast::channel::<crate::state::WorkspaceEvent>(8);
        let publisher_tx: crate::state::WorkspaceEventTx = std::sync::Arc::new(tx);

        set_workspace_team_activity_private_inner(
            ws.id.clone(),
            true,
            data.clone(),
            Arc::clone(&state),
            Some(&publisher_tx),
        )
        .await
        .unwrap();

        {
            let st = state.lock().unwrap();
            assert!(
                st.workspaces.get(&ws.id).unwrap().team_activity_private,
                "flag should be true after toggle"
            );
        }

        let ev = rx.try_recv().expect("PrivacyChanged should be emitted");
        match ev {
            crate::state::WorkspaceEvent::PrivacyChanged {
                workspace_id,
                is_private,
            } => {
                assert_eq!(workspace_id, ws.id);
                assert!(is_private);
            }
            other => panic!("expected PrivacyChanged, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn set_team_activity_private_returns_err_when_workspace_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));

        let err = set_workspace_team_activity_private_inner(
            "ws_does_not_exist".into(),
            true,
            data,
            state,
            None,
        )
        .await
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.to_lowercase().contains("not found") || msg.contains("ws_does_not_exist"),
            "expected not-found error, got: {msg}"
        );
    }

    #[tokio::test]
    async fn set_team_activity_private_persists_across_load() {
        use crate::persistence::workspaces::load_workspaces;

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
            "Persist privacy".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        set_workspace_team_activity_private_inner(
            ws.id.clone(),
            true,
            data.clone(),
            Arc::clone(&state),
            None,
        )
        .await
        .unwrap();

        // Reload from disk into a fresh map (no in-memory state) — the
        // persisted flag must round-trip.
        let reloaded = load_workspaces(&data).unwrap();
        let persisted_ws = reloaded
            .get(&ws.id)
            .expect("workspace should be on disk after toggle");
        assert!(
            persisted_ws.team_activity_private,
            "team_activity_private should persist across load"
        );
    }

    #[tokio::test]
    async fn set_team_activity_private_to_false_emits_false_event() {
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
            "Toggle off".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        // Pre-set to true (no publisher) so we can verify the false-edge event.
        set_workspace_team_activity_private_inner(
            ws.id.clone(),
            true,
            data.clone(),
            Arc::clone(&state),
            None,
        )
        .await
        .unwrap();

        let (tx, mut rx) = tokio::sync::broadcast::channel::<crate::state::WorkspaceEvent>(8);
        let publisher_tx: crate::state::WorkspaceEventTx = std::sync::Arc::new(tx);
        set_workspace_team_activity_private_inner(
            ws.id.clone(),
            false,
            data,
            Arc::clone(&state),
            Some(&publisher_tx),
        )
        .await
        .unwrap();

        let ev = rx.try_recv().expect("PrivacyChanged should be emitted");
        match ev {
            crate::state::WorkspaceEvent::PrivacyChanged {
                workspace_id,
                is_private,
            } => {
                assert_eq!(workspace_id, ws.id);
                assert!(!is_private, "expected is_private=false on toggle-off");
            }
            other => panic!("expected PrivacyChanged, got {other:?}"),
        }
    }

    // ── Task 2 — is_workspace_empty ─────────────────────────────────────────

    #[tokio::test]
    async fn is_workspace_empty_true_for_fresh_workspace() {
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
            "T".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        assert!(is_workspace_empty(&data, &ws, false));
    }

    #[tokio::test]
    async fn is_workspace_empty_false_when_agent_live() {
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
            "T".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        assert!(!is_workspace_empty(&data, &ws, true));
    }

    #[tokio::test]
    async fn is_workspace_empty_false_with_commit_ahead() {
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
            "T".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        let wt = ws.worktree_dir.to_string_lossy().to_string();
        std::fs::write(ws.worktree_dir.join("new.txt"), "x").unwrap();
        for args in [
            vec!["-C", &wt, "add", "."],
            vec![
                "-C",
                &wt,
                "-c",
                "user.email=t@t",
                "-c",
                "user.name=t",
                "commit",
                "-m",
                "wip",
            ],
        ] {
            Command::new("git").args(&args).output().unwrap();
        }
        assert!(!is_workspace_empty(&data, &ws, false));
    }

    #[tokio::test]
    async fn is_workspace_empty_false_with_dirty_worktree() {
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
            "T".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        std::fs::write(ws.worktree_dir.join("untracked.txt"), "x").unwrap();
        assert!(!is_workspace_empty(&data, &ws, false));
    }

    #[tokio::test]
    async fn is_workspace_empty_false_with_messages() {
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
            "T".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        let msg = crate::state::Message {
            id: "m1".into(),
            workspace_id: ws.id.clone(),
            role: crate::state::MessageRole::User,
            text: "hi".into(),
            is_partial: false,
            tool_use: None,
            tool_result: None,
            created_at: 0,
            attachments: Vec::new(),
        };
        crate::persistence::messages::append_message(&data, &ws.id, &msg).unwrap();
        assert!(!is_workspace_empty(&data, &ws, false));
    }

    #[tokio::test]
    async fn remove_workspace_kills_its_terminals() {
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
            "T".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        let (mock, _h) = crate::platform::pty::MockPty::new(7);
        crate::commands::terminal::spawn_terminal_inner_with_pty(
            &ws.id,
            "term_x",
            Box::new(mock),
            80,
            24,
            Arc::clone(&state),
        )
        .unwrap();
        assert_eq!(state.lock().unwrap().terminals.len(), 1);
        remove_workspace_inner(ws.id.clone(), data.clone(), Arc::clone(&state))
            .await
            .unwrap();
        assert_eq!(
            state.lock().unwrap().terminals.len(),
            0,
            "workspace removal killed its terminal"
        );
    }

    #[tokio::test]
    async fn create_workspace_with_publisher_does_not_emit_when_worktree_add_fails() {
        // If `git worktree add -b` fails (repo path invalid), we must NOT
        // emit a stale BranchChanged for a branch that was never created.
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path().join("data");
        std::fs::create_dir_all(&data).unwrap();

        let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
        // Insert a fake repo whose path points at a non-existent directory
        // so `git worktree add` fails.
        let bad_path = tmp.path().join("not-a-real-repo");
        state.lock().unwrap().repos.insert(
            "repo_bad".into(),
            crate::state::RepoInfo {
                id: "repo_bad".into(),
                name: "bad".into(),
                path: bad_path,
                gh_profile: None,
                default_branch: "main".into(),
                created_at: 0,
                updated_at: 0,
                scripts: Vec::new(),
            },
        );

        let (tx, mut rx) = tokio::sync::broadcast::channel::<crate::state::WorkspaceEvent>(8);
        let publisher_tx: crate::state::WorkspaceEventTx = std::sync::Arc::new(tx);

        let res = create_workspace_inner_with_publisher(
            "repo_bad".into(),
            "Will fail".into(),
            String::new(),
            None,
            data,
            state,
            Some(&publisher_tx),
        )
        .await;

        assert!(res.is_err(), "expected git worktree add failure");
        assert!(
            rx.try_recv().is_err(),
            "no BranchChanged should be emitted when create fails"
        );
    }
}
