use crate::error::{AppError, Result};
use crate::ids::task_id;
use crate::persistence::tasks::save_tasks;
use crate::state::{AppState, KanbanColumn, Task};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};

// ── Structs ──────────────────────────────────────────────────────────

#[derive(serde::Deserialize, Debug)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub order: Option<i32>,
}

// ── Public Tauri commands ────────────────────────────────────────────

#[tauri::command]
pub fn add_task(
    repo_id: String,
    title: String,
    description: String,
    column: Option<KanbanColumn>,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<Task, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    add_task_inner(
        repo_id,
        title,
        description,
        column,
        data_dir,
        state.inner().clone(),
    )
    .map_err(|e| {
        tracing::error!(error = %e, "add_task failed");
        e.to_string()
    })
}

#[tauri::command]
pub fn list_tasks(
    repo_id: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<Vec<Task>, String> {
    list_tasks_inner(repo_id, state.inner().clone()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_task(
    task_id: String,
    patch: TaskPatch,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<Task, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    update_task_inner(task_id, patch, data_dir, state.inner().clone()).map_err(|e| {
        tracing::error!(error = %e, "update_task failed");
        e.to_string()
    })
}

#[tauri::command]
pub async fn move_task(
    task_id: String,
    column: KanbanColumn,
    order: i32,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<Task, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    move_task_inner(task_id, column, order, data_dir, state.inner().clone())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "move_task failed");
            e.to_string()
        })
}

#[tauri::command]
pub fn remove_task(
    task_id: String,
    force: bool,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    remove_task_inner(task_id, force, data_dir, state.inner().clone()).map_err(|e| {
        tracing::error!(error = %e, "remove_task failed");
        e.to_string()
    })
}

// ── Inner implementations ────────────────────────────────────────────

pub(crate) fn add_task_inner(
    repo_id: String,
    title: String,
    description: String,
    column: Option<KanbanColumn>,
    data_dir: PathBuf,
    state: Arc<Mutex<AppState>>,
) -> Result<Task> {
    use crate::commands::helpers::now_unix;

    let col = column.unwrap_or_default();

    let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;

    // Verify repo exists
    if !st.repos.contains_key(&repo_id) {
        return Err(AppError::NotFound(format!("repo '{}' not found", repo_id)));
    }

    // Compute order: max existing order in column + 1024
    let max_order = st
        .tasks
        .values()
        .filter(|t| t.repo_id == repo_id && t.column == col)
        .map(|t| t.order)
        .max()
        .unwrap_or(0);

    let now = now_unix();
    let id = task_id();
    let task = Task {
        id: id.clone(),
        repo_id,
        workspace_id: None,
        title,
        description,
        column: col,
        order: max_order + 1024,
        created_at: now,
        updated_at: now,
    };

    st.tasks.insert(id, task.clone());
    save_tasks(&data_dir, &st.tasks)?;
    tracing::info!(task_id = %task.id, column = ?task.column, "Created task");
    Ok(task)
}

pub(crate) fn list_tasks_inner(repo_id: String, state: Arc<Mutex<AppState>>) -> Result<Vec<Task>> {
    let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;

    let mut tasks: Vec<Task> = st
        .tasks
        .values()
        .filter(|t| t.repo_id == repo_id)
        .cloned()
        .collect();

    // Sort by column (enum ordinal order: Todo < InProgress < Review < Done)
    // then by order descending (higher order = top of column)
    tasks.sort_by(|a, b| {
        let col_ord = |c: &KanbanColumn| match c {
            KanbanColumn::Todo => 0u8,
            KanbanColumn::InProgress => 1,
            KanbanColumn::Review => 2,
            KanbanColumn::Done => 3,
        };
        col_ord(&a.column)
            .cmp(&col_ord(&b.column))
            .then_with(|| b.order.cmp(&a.order))
    });

    Ok(tasks)
}

pub(crate) fn update_task_inner(
    task_id: String,
    patch: TaskPatch,
    data_dir: PathBuf,
    state: Arc<Mutex<AppState>>,
) -> Result<Task> {
    use crate::commands::helpers::now_unix;

    let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    let task = st
        .tasks
        .get_mut(&task_id)
        .ok_or_else(|| AppError::NotFound(format!("task '{}' not found", task_id)))?;

    if let Some(title) = patch.title {
        task.title = title;
    }
    if let Some(description) = patch.description {
        task.description = description;
    }
    if let Some(order) = patch.order {
        task.order = order;
    }
    task.updated_at = now_unix();

    let updated = task.clone();
    save_tasks(&data_dir, &st.tasks)?;
    tracing::info!(task_id = %updated.id, "Updated task");
    Ok(updated)
}

pub(crate) async fn move_task_inner(
    task_id: String,
    column: KanbanColumn,
    order: i32,
    data_dir: PathBuf,
    state: Arc<Mutex<AppState>>,
) -> Result<Task> {
    use crate::commands::helpers::now_unix;

    // Extract task and check for auto-workspace conditions before any async work.
    let (repo_id, task_title, task_desc, needs_workspace) = {
        let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        let task = st
            .tasks
            .get(&task_id)
            .ok_or_else(|| AppError::NotFound(format!("task '{}' not found", task_id)))?;
        let needs = column == KanbanColumn::InProgress && task.workspace_id.is_none();
        (
            task.repo_id.clone(),
            task.title.clone(),
            task.description.clone(),
            needs,
        )
    };
    // Lock is dropped here.

    // Auto-create workspace if moving into InProgress with no linked workspace.
    let maybe_ws_id: Option<String> = if needs_workspace {
        let ws = crate::commands::workspace::create_workspace_inner(
            repo_id,
            task_title,
            task_desc,
            None, // auto-branch
            data_dir.clone(),
            Arc::clone(&state),
        )
        .await?;
        tracing::info!(task_id = %task_id, workspace_id = %ws.id, "Auto-created workspace for task");
        Some(ws.id)
    } else {
        None
    };

    // Now update the task.
    let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    let task = st
        .tasks
        .get_mut(&task_id)
        .ok_or_else(|| AppError::NotFound(format!("task '{}' not found", task_id)))?;

    task.column = column;
    task.order = order;
    if let Some(ws_id) = maybe_ws_id {
        task.workspace_id = Some(ws_id);
    }
    task.updated_at = now_unix();

    let updated = task.clone();
    save_tasks(&data_dir, &st.tasks)?;
    Ok(updated)
}

pub(crate) fn remove_task_inner(
    task_id: String,
    force: bool,
    data_dir: PathBuf,
    state: Arc<Mutex<AppState>>,
) -> Result<()> {
    let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    let task = st
        .tasks
        .get(&task_id)
        .ok_or_else(|| AppError::NotFound(format!("task '{}' not found", task_id)))?;

    if task.workspace_id.is_some() && !force {
        return Err(AppError::InvalidState(format!(
            "task '{}' has a linked workspace '{}'. Use force=true to remove anyway, \
             or remove the workspace via the sidebar first.",
            task_id,
            task.workspace_id.as_deref().unwrap_or("")
        )));
    }

    st.tasks.remove(&task_id);
    save_tasks(&data_dir, &st.tasks)?;
    tracing::info!(task_id = %task_id, "Removed task");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, KanbanColumn, RepoInfo};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;

    fn make_state_with_repo(data_dir: &std::path::Path) -> Arc<Mutex<AppState>> {
        let _ = data_dir; // used by caller for data_dir path
        let mut state = AppState::default();
        state.repos.insert(
            "repo_r1".into(),
            RepoInfo {
                id: "repo_r1".into(),
                name: "my-repo".into(),
                path: PathBuf::from("/tmp/my-repo"),
                gh_profile: None,
                default_branch: "main".into(),
                created_at: 1_776_000_000,
                updated_at: 1_776_000_000,
            },
        );
        Arc::new(Mutex::new(state))
    }

    // ── Task 5: add_task tests ───────────────────────────────────────

    #[test]
    fn add_task_creates_task_with_correct_fields() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());

        let task = add_task_inner(
            "repo_r1".into(),
            "Fix login".into(),
            "Auth fails".into(),
            None,
            tmp.path().to_path_buf(),
            Arc::clone(&state),
        )
        .unwrap();

        assert!(task.id.starts_with("tk_"));
        assert_eq!(task.repo_id, "repo_r1");
        assert_eq!(task.title, "Fix login");
        assert_eq!(task.description, "Auth fails");
        assert_eq!(task.column, KanbanColumn::Todo);
        assert!(task.workspace_id.is_none());
        assert_eq!(task.order, 1024); // first task in column → 0 + 1024
    }

    #[test]
    fn add_task_uses_specified_column() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());

        let task = add_task_inner(
            "repo_r1".into(),
            "Review task".into(),
            String::new(),
            Some(KanbanColumn::Review),
            tmp.path().to_path_buf(),
            Arc::clone(&state),
        )
        .unwrap();

        assert_eq!(task.column, KanbanColumn::Review);
    }

    #[test]
    fn add_task_order_increments_by_1024() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());

        let t1 = add_task_inner(
            "repo_r1".into(),
            "Task 1".into(),
            String::new(),
            None,
            tmp.path().to_path_buf(),
            Arc::clone(&state),
        )
        .unwrap();
        assert_eq!(t1.order, 1024);

        let t2 = add_task_inner(
            "repo_r1".into(),
            "Task 2".into(),
            String::new(),
            None,
            tmp.path().to_path_buf(),
            Arc::clone(&state),
        )
        .unwrap();
        assert_eq!(t2.order, 2048);

        let t3 = add_task_inner(
            "repo_r1".into(),
            "Task 3".into(),
            String::new(),
            None,
            tmp.path().to_path_buf(),
            Arc::clone(&state),
        )
        .unwrap();
        assert_eq!(t3.order, 3072);
    }

    #[test]
    fn add_task_persists_to_tasks_json() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());

        add_task_inner(
            "repo_r1".into(),
            "Persisted task".into(),
            String::new(),
            None,
            tmp.path().to_path_buf(),
            Arc::clone(&state),
        )
        .unwrap();

        let tasks_path = crate::platform::paths::tasks_file(tmp.path());
        assert!(tasks_path.exists(), "tasks.json should be written");
        let loaded = crate::persistence::tasks::load_tasks(tmp.path()).unwrap();
        assert_eq!(loaded.len(), 1);
    }

    #[test]
    fn add_task_unknown_repo_returns_err() {
        let tmp = tempdir().unwrap();
        let state = Arc::new(Mutex::new(AppState::default()));
        let result = add_task_inner(
            "repo_nonexistent".into(),
            "X".into(),
            String::new(),
            None,
            tmp.path().to_path_buf(),
            state,
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("repo") || msg.contains("not found"),
            "Got: {msg}"
        );
    }

    // ── Task 6: list_tasks tests ─────────────────────────────────────

    #[test]
    fn list_tasks_filters_by_repo_id() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());

        add_task_inner(
            "repo_r1".into(),
            "Task A".into(),
            String::new(),
            None,
            tmp.path().to_path_buf(),
            Arc::clone(&state),
        )
        .unwrap();
        add_task_inner(
            "repo_r1".into(),
            "Task B".into(),
            String::new(),
            None,
            tmp.path().to_path_buf(),
            Arc::clone(&state),
        )
        .unwrap();

        // Insert a task for a different repo directly
        {
            let mut st = state.lock().unwrap();
            let other = Task {
                id: "tk_other1".into(),
                repo_id: "repo_other".into(),
                workspace_id: None,
                title: "Other repo task".into(),
                description: String::new(),
                column: KanbanColumn::Todo,
                order: 1024,
                created_at: 0,
                updated_at: 0,
            };
            st.tasks.insert("tk_other1".into(), other);
        }

        let tasks = list_tasks_inner("repo_r1".into(), Arc::clone(&state)).unwrap();
        assert_eq!(tasks.len(), 2);
        assert!(tasks.iter().all(|t| t.repo_id == "repo_r1"));
    }

    #[test]
    fn list_tasks_sorted_by_column_then_order_desc() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());

        // Seed tasks directly for deterministic order control
        {
            let mut st = state.lock().unwrap();
            let tasks = vec![
                Task {
                    id: "tk_t1".into(),
                    repo_id: "repo_r1".into(),
                    workspace_id: None,
                    title: "T1".into(),
                    description: String::new(),
                    column: KanbanColumn::Todo,
                    order: 1024,
                    created_at: 0,
                    updated_at: 0,
                },
                Task {
                    id: "tk_t2".into(),
                    repo_id: "repo_r1".into(),
                    workspace_id: None,
                    title: "T2".into(),
                    description: String::new(),
                    column: KanbanColumn::Todo,
                    order: 2048,
                    created_at: 0,
                    updated_at: 0,
                },
                Task {
                    id: "tk_ip1".into(),
                    repo_id: "repo_r1".into(),
                    workspace_id: None,
                    title: "IP1".into(),
                    description: String::new(),
                    column: KanbanColumn::InProgress,
                    order: 1024,
                    created_at: 0,
                    updated_at: 0,
                },
            ];
            for t in tasks {
                st.tasks.insert(t.id.clone(), t);
            }
        }

        let listed = list_tasks_inner("repo_r1".into(), Arc::clone(&state)).unwrap();
        assert_eq!(listed.len(), 3);

        // Verify column ordering: Todo first, then InProgress
        // Within Todo: order desc (2048 before 1024)
        let todo_tasks: Vec<_> = listed
            .iter()
            .filter(|t| t.column == KanbanColumn::Todo)
            .collect();
        assert_eq!(todo_tasks[0].order, 2048);
        assert_eq!(todo_tasks[1].order, 1024);
    }

    #[test]
    fn list_tasks_empty_repo_returns_empty_vec() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());
        let tasks = list_tasks_inner("repo_r1".into(), Arc::clone(&state)).unwrap();
        assert!(tasks.is_empty());
    }

    // ── Task 7: update_task tests ────────────────────────────────────

    #[test]
    fn update_task_title_and_description() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());

        let task = add_task_inner(
            "repo_r1".into(),
            "Original title".into(),
            "Original desc".into(),
            None,
            tmp.path().to_path_buf(),
            Arc::clone(&state),
        )
        .unwrap();

        let patch = TaskPatch {
            title: Some("Updated title".into()),
            description: Some("Updated desc".into()),
            order: None,
        };
        let updated = update_task_inner(
            task.id.clone(),
            patch,
            tmp.path().to_path_buf(),
            Arc::clone(&state),
        )
        .unwrap();

        assert_eq!(updated.title, "Updated title");
        assert_eq!(updated.description, "Updated desc");
        assert_eq!(updated.column, KanbanColumn::Todo); // column unchanged
        assert_eq!(updated.id, task.id);
    }

    #[test]
    fn update_task_partial_patch_leaves_other_fields_unchanged() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());

        let task = add_task_inner(
            "repo_r1".into(),
            "Keep me".into(),
            "Keep desc".into(),
            None,
            tmp.path().to_path_buf(),
            Arc::clone(&state),
        )
        .unwrap();

        let patch = TaskPatch {
            title: Some("New title only".into()),
            description: None,
            order: None,
        };
        let updated = update_task_inner(
            task.id.clone(),
            patch,
            tmp.path().to_path_buf(),
            Arc::clone(&state),
        )
        .unwrap();

        assert_eq!(updated.title, "New title only");
        assert_eq!(updated.description, "Keep desc"); // unchanged
    }

    #[test]
    fn update_task_order_change() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());

        let task = add_task_inner(
            "repo_r1".into(),
            "T".into(),
            String::new(),
            None,
            tmp.path().to_path_buf(),
            Arc::clone(&state),
        )
        .unwrap();
        assert_eq!(task.order, 1024);

        let patch = TaskPatch {
            title: None,
            description: None,
            order: Some(512),
        };
        let updated = update_task_inner(
            task.id.clone(),
            patch,
            tmp.path().to_path_buf(),
            Arc::clone(&state),
        )
        .unwrap();

        assert_eq!(updated.order, 512);
    }

    #[test]
    fn update_task_not_found_returns_err() {
        let tmp = tempdir().unwrap();
        let state = Arc::new(Mutex::new(AppState::default()));
        let patch = TaskPatch {
            title: Some("X".into()),
            description: None,
            order: None,
        };
        let result = update_task_inner(
            "tk_nonexistent".into(),
            patch,
            tmp.path().to_path_buf(),
            state,
        );
        assert!(result.is_err());
    }

    // ── Task 8: move_task tests ──────────────────────────────────────

    fn init_repo_with_remote_for_move(tmp: &tempfile::TempDir) -> (PathBuf, PathBuf) {
        use std::process::Command;
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
    async fn move_task_todo_to_in_progress_creates_workspace() {
        let tmp = tempdir().unwrap();
        let (local, _) = init_repo_with_remote_for_move(&tmp);
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

        let task = add_task_inner(
            repo.id.clone(),
            "Auto WS task".into(),
            "Description".into(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .unwrap();
        assert!(task.workspace_id.is_none());

        let moved = move_task_inner(
            task.id.clone(),
            KanbanColumn::InProgress,
            task.order,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        assert_eq!(moved.column, KanbanColumn::InProgress);
        assert!(
            moved.workspace_id.is_some(),
            "workspace_id should be populated"
        );

        // Verify workspace exists in state
        let ws_id = moved.workspace_id.as_ref().unwrap();
        let st = state.lock().unwrap();
        assert!(
            st.workspaces.contains_key(ws_id),
            "workspace should be in AppState"
        );
    }

    #[tokio::test]
    async fn move_task_in_progress_to_review_keeps_workspace() {
        let tmp = tempdir().unwrap();
        let (local, _) = init_repo_with_remote_for_move(&tmp);
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

        let task = add_task_inner(
            repo.id.clone(),
            "Review task".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .unwrap();

        // Move to InProgress first (creates workspace)
        let in_progress = move_task_inner(
            task.id.clone(),
            KanbanColumn::InProgress,
            task.order,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        let ws_id = in_progress.workspace_id.clone().unwrap();

        // Move to Review
        let review = move_task_inner(
            task.id.clone(),
            KanbanColumn::Review,
            in_progress.order,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        assert_eq!(review.column, KanbanColumn::Review);
        assert_eq!(
            review.workspace_id.as_deref(),
            Some(ws_id.as_str()),
            "workspace_id should remain after moving to Review"
        );
    }

    #[tokio::test]
    async fn move_task_review_to_done_keeps_workspace() {
        let tmp = tempdir().unwrap();
        let (local, _) = init_repo_with_remote_for_move(&tmp);
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

        let task = add_task_inner(
            repo.id.clone(),
            "Done task".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .unwrap();

        let in_progress = move_task_inner(
            task.id.clone(),
            KanbanColumn::InProgress,
            task.order,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        let ws_id = in_progress.workspace_id.clone().unwrap();

        let review = move_task_inner(
            task.id.clone(),
            KanbanColumn::Review,
            in_progress.order,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let done = move_task_inner(
            task.id.clone(),
            KanbanColumn::Done,
            review.order,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        assert_eq!(done.column, KanbanColumn::Done);
        assert_eq!(done.workspace_id.as_deref(), Some(ws_id.as_str()));
    }

    #[test]
    fn move_task_todo_to_review_does_not_create_workspace() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());

        let task = add_task_inner(
            "repo_r1".into(),
            "Skip InProgress".into(),
            String::new(),
            None,
            tmp.path().to_path_buf(),
            Arc::clone(&state),
        )
        .unwrap();

        // move_task_inner is async; use tokio::runtime for this sync-ish test
        let rt = tokio::runtime::Runtime::new().unwrap();
        let moved = rt
            .block_on(move_task_inner(
                task.id.clone(),
                KanbanColumn::Review,
                task.order,
                tmp.path().to_path_buf(),
                Arc::clone(&state),
            ))
            .unwrap();

        assert_eq!(moved.column, KanbanColumn::Review);
        assert!(
            moved.workspace_id.is_none(),
            "Moving Todo→Review should NOT create a workspace"
        );
        let st = state.lock().unwrap();
        assert!(st.workspaces.is_empty());
    }

    // ── Task 9: remove_task tests ────────────────────────────────────

    #[test]
    fn remove_task_without_workspace_succeeds() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());

        let task = add_task_inner(
            "repo_r1".into(),
            "To remove".into(),
            String::new(),
            None,
            tmp.path().to_path_buf(),
            Arc::clone(&state),
        )
        .unwrap();

        remove_task_inner(
            task.id.clone(),
            false,
            tmp.path().to_path_buf(),
            Arc::clone(&state),
        )
        .unwrap();

        let st = state.lock().unwrap();
        assert!(!st.tasks.contains_key(&task.id));
    }

    #[test]
    fn remove_task_with_workspace_and_no_force_returns_err() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());

        // Insert a task with workspace_id already set
        let task_id = "tk_linked".to_string();
        {
            let mut st = state.lock().unwrap();
            st.tasks.insert(
                task_id.clone(),
                Task {
                    id: task_id.clone(),
                    repo_id: "repo_r1".into(),
                    workspace_id: Some("ws_exists".into()),
                    title: "Has workspace".into(),
                    description: String::new(),
                    column: KanbanColumn::InProgress,
                    order: 1024,
                    created_at: 0,
                    updated_at: 0,
                },
            );
        }

        let result = remove_task_inner(
            task_id.clone(),
            false,
            tmp.path().to_path_buf(),
            Arc::clone(&state),
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("workspace") || msg.contains("force"),
            "Error should mention workspace/force. Got: {msg}"
        );

        // Task should still exist
        let st = state.lock().unwrap();
        assert!(st.tasks.contains_key(&task_id));
    }

    #[test]
    fn remove_task_with_workspace_and_force_true_succeeds() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());

        let task_id = "tk_forced".to_string();
        {
            let mut st = state.lock().unwrap();
            st.tasks.insert(
                task_id.clone(),
                Task {
                    id: task_id.clone(),
                    repo_id: "repo_r1".into(),
                    workspace_id: Some("ws_exists".into()),
                    title: "Force remove".into(),
                    description: String::new(),
                    column: KanbanColumn::InProgress,
                    order: 1024,
                    created_at: 0,
                    updated_at: 0,
                },
            );
        }

        remove_task_inner(
            task_id.clone(),
            true,
            tmp.path().to_path_buf(),
            Arc::clone(&state),
        )
        .unwrap();

        let st = state.lock().unwrap();
        assert!(!st.tasks.contains_key(&task_id));
    }

    #[test]
    fn remove_task_not_found_returns_err() {
        let tmp = tempdir().unwrap();
        let state = Arc::new(Mutex::new(AppState::default()));
        let result = remove_task_inner("tk_ghost".into(), false, tmp.path().to_path_buf(), state);
        assert!(result.is_err());
    }
}
