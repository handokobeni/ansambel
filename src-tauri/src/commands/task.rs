use crate::error::{AppError, Result};
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
pub async fn add_task(
    repo_id: String,
    title: String,
    description: String,
    column: Option<KanbanColumn>,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
    provider_handle: State<'_, crate::state::TaskProviderHandle>,
) -> std::result::Result<Task, String> {
    let _ = app; // data_dir no longer needed — provider owns persistence
    let provider = provider_handle.read().await.clone();
    add_task_inner(
        repo_id,
        title,
        description,
        column,
        provider,
        state.inner().clone(),
    )
    .await
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
pub async fn update_task(
    task_id: String,
    patch: TaskPatch,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
    provider_handle: State<'_, crate::state::TaskProviderHandle>,
) -> std::result::Result<Task, String> {
    let _ = app; // data_dir no longer needed — provider owns persistence
    let provider = provider_handle.read().await.clone();
    update_task_inner(task_id, patch, provider, state.inner().clone())
        .await
        .map_err(|e| {
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
    provider_handle: State<'_, crate::state::TaskProviderHandle>,
) -> std::result::Result<Task, String> {
    // data_dir is still needed because moving Todo → InProgress can
    // auto-create a workspace, and workspace creation owns its own
    // persistence path (worktree dir + workspaces.json). The task
    // mutation itself goes through the provider.
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let provider = provider_handle.read().await.clone();
    move_task_inner(
        task_id,
        column,
        order,
        data_dir,
        provider,
        state.inner().clone(),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "move_task failed");
        e.to_string()
    })
}

#[tauri::command]
pub async fn remove_task(
    task_id: String,
    force: bool,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
    provider_handle: State<'_, crate::state::TaskProviderHandle>,
) -> std::result::Result<(), String> {
    let _ = app; // data_dir no longer needed — provider owns persistence
    let provider = provider_handle.read().await.clone();
    remove_task_inner(task_id, Some(force), provider, state.inner().clone())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "remove_task failed");
            e.to_string()
        })
}

// ── Inner implementations ────────────────────────────────────────────

pub(crate) async fn add_task_inner(
    repo_id: String,
    title: String,
    description: String,
    column: Option<KanbanColumn>,
    provider: Arc<dyn crate::task_provider::TaskProvider>,
    state: Arc<Mutex<AppState>>,
) -> Result<Task> {
    // Verify repo exists before hitting the provider. Drop the lock
    // before any await per the project's mutex discipline rule.
    {
        let st = state
            .lock()
            .map_err(|e| AppError::InvalidState(format!("AppState lock poisoned: {e}")))?;
        if !st.repos.contains_key(&repo_id) {
            return Err(AppError::NotFound(format!("repo '{}' not found", repo_id)));
        }
    }

    let args = crate::task_provider::CreateTaskArgs {
        repo_id,
        title,
        description,
        column,
    };
    let task = provider.create_task(args).await?;
    {
        let mut st = state
            .lock()
            .map_err(|e| AppError::InvalidState(format!("AppState lock poisoned: {e}")))?;
        st.tasks.insert(task.id.clone(), task.clone());
    }
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

#[tauri::command]
pub async fn refresh_tasks(
    repo_id: Option<String>,
    state: State<'_, Arc<Mutex<AppState>>>,
    provider_handle: State<'_, crate::state::TaskProviderHandle>,
) -> std::result::Result<Vec<Task>, String> {
    refresh_tasks_inner(
        repo_id,
        state.inner().clone(),
        provider_handle.read().await.clone(),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "refresh_tasks failed");
        e.to_string()
    })
}

pub(crate) async fn refresh_tasks_inner(
    repo_id: Option<String>,
    state: Arc<Mutex<AppState>>,
    provider: Arc<dyn crate::task_provider::TaskProvider>,
) -> Result<Vec<Task>> {
    let tasks = provider.list_tasks(repo_id.as_deref()).await?;
    {
        let mut st = state
            .lock()
            .map_err(|e| AppError::InvalidState(format!("AppState lock poisoned: {e}")))?;
        if let Some(rid) = repo_id.as_deref() {
            st.tasks.retain(|_, t| t.repo_id != rid);
        } else {
            st.tasks.clear();
        }
        for t in &tasks {
            st.tasks.insert(t.id.clone(), t.clone());
        }
    }
    Ok(tasks)
}

pub(crate) async fn update_task_inner(
    task_id: String,
    patch: TaskPatch,
    provider: Arc<dyn crate::task_provider::TaskProvider>,
    state: Arc<Mutex<AppState>>,
) -> Result<Task> {
    let provider_patch = crate::task_provider::TaskPatch {
        title: patch.title,
        description: patch.description,
        order: patch.order,
    };
    let updated = provider.update_task(&task_id, provider_patch).await?;
    {
        let mut st = state
            .lock()
            .map_err(|e| AppError::InvalidState(format!("AppState lock poisoned: {e}")))?;
        st.tasks.insert(updated.id.clone(), updated.clone());
    }
    tracing::info!(task_id = %updated.id, "Updated task");
    Ok(updated)
}

pub(crate) async fn move_task_inner(
    task_id: String,
    column: KanbanColumn,
    order: i32,
    data_dir: std::path::PathBuf,
    provider: Arc<dyn crate::task_provider::TaskProvider>,
    state: Arc<Mutex<AppState>>,
) -> Result<Task> {
    // Extract auto-workspace conditions before any async work. The
    // auto-workspace path predates the TaskProvider abstraction; it
    // still lives in the command layer because it crosses subsystems
    // (workspace creation + task mutation) — providers only own task
    // persistence.
    let (repo_id, task_title, task_desc, needs_workspace) = {
        let st = state
            .lock()
            .map_err(|e| AppError::InvalidState(format!("AppState lock poisoned: {e}")))?;
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

    // Route the column/order update through the provider so persistence
    // is owned by it (tasks.json for LocalProvider, Bitable for Lark).
    let mut updated = provider.move_task(&task_id, column, order).await?;

    // If we auto-created a workspace, stamp the workspace_id onto the
    // task. The TaskProvider trait doesn't model workspace_id (it's a
    // local-only concept for the orchestrator), so we patch the
    // in-memory mirror here and re-save tasks.json directly so
    // workspace_id is durable on disk. LarkProvider ignores
    // workspace_id on its remote anyway, so this stays a local-only
    // concern.
    if let Some(ws_id) = maybe_ws_id {
        updated.workspace_id = Some(ws_id);
        let map = {
            let mut st = state
                .lock()
                .map_err(|e| AppError::InvalidState(format!("AppState lock poisoned: {e}")))?;
            st.tasks.insert(updated.id.clone(), updated.clone());
            st.tasks.clone()
        };
        crate::persistence::tasks::save_tasks(&data_dir, &map)?;
    } else {
        let mut st = state
            .lock()
            .map_err(|e| AppError::InvalidState(format!("AppState lock poisoned: {e}")))?;
        st.tasks.insert(updated.id.clone(), updated.clone());
    }
    Ok(updated)
}

pub(crate) async fn remove_task_inner(
    task_id: String,
    force: Option<bool>,
    provider: Arc<dyn crate::task_provider::TaskProvider>,
    state: Arc<Mutex<AppState>>,
) -> Result<()> {
    let force = force.unwrap_or(false);
    // Refuse delete when the task has an active workspace unless
    // force=true. The check reads the AppState mirror so it works for
    // both providers — LarkProvider returns workspace_id=None on
    // hydrate, so this guard only fires for the local store.
    {
        let st = state
            .lock()
            .map_err(|e| AppError::InvalidState(format!("AppState lock poisoned: {e}")))?;
        if let Some(t) = st.tasks.get(&task_id) {
            if t.workspace_id.is_some() && !force {
                return Err(AppError::InvalidState(format!(
                    "task '{}' has a linked workspace '{}'. Use force=true to remove anyway, \
                     or remove the workspace via the sidebar first.",
                    task_id,
                    t.workspace_id.as_deref().unwrap_or("")
                )));
            }
        } else {
            return Err(AppError::NotFound(format!("task '{}' not found", task_id)));
        }
    }
    provider.delete_task(&task_id).await?;
    {
        let mut st = state
            .lock()
            .map_err(|e| AppError::InvalidState(format!("AppState lock poisoned: {e}")))?;
        st.tasks.remove(&task_id);
    }
    tracing::info!(task_id = %task_id, "Removed task");
    Ok(())
}

#[tauri::command]
pub async fn get_task_source(
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<crate::state::TaskSource, String> {
    let st = state
        .lock()
        .map_err(|e| format!("AppState lock poisoned: {e}"))?;
    Ok(st.settings.task_source)
}

#[tauri::command]
pub async fn set_task_source(
    source: crate::state::TaskSource,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
    provider_handle: State<'_, crate::state::TaskProviderHandle>,
) -> std::result::Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    set_task_source_inner(
        source,
        data_dir,
        state.inner().clone(),
        provider_handle.inner().clone(),
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "set_task_source failed");
        e.to_string()
    })?;
    // Emit a Tauri event so the frontend store reloads tasks.
    use tauri::Emitter;
    let _ = app.emit("tasks-rehydrated", ());
    Ok(())
}

pub(crate) async fn set_task_source_inner(
    source: crate::state::TaskSource,
    data_dir: PathBuf,
    state: Arc<Mutex<AppState>>,
    provider_handle: crate::state::TaskProviderHandle,
) -> Result<()> {
    // Build the new provider before we acquire the write lock, so a
    // failure (e.g. missing Lark credentials) doesn't leave us
    // partially swapped.
    let new_provider: Arc<dyn crate::task_provider::TaskProvider> = match source {
        crate::state::TaskSource::Local => Arc::new(
            crate::task_provider::local::LocalProvider::new(data_dir.clone()),
        ),
        crate::state::TaskSource::Lark => {
            let store = crate::commands::lark_auth::KeyringStore;
            let cfg = crate::commands::lark_auth::load_lark_config_inner(&data_dir, &store)
                .map_err(|e| {
                    AppError::InvalidState(format!(
                        "Cannot switch to Lark: {e}. Configure Lark credentials first."
                    ))
                })?;
            let app_token = cfg.app_token.clone();
            let table_id = cfg.table_id.clone();
            let client = Arc::new(crate::platform::lark_client::LarkClient::new(cfg));
            Arc::new(crate::task_provider::lark::LarkProvider::new(
                client, app_token, table_id,
            ))
        }
    };

    // Persist setting.
    {
        let mut st = state
            .lock()
            .map_err(|e| AppError::InvalidState(format!("AppState lock poisoned: {e}")))?;
        st.settings.task_source = source;
        crate::persistence::settings::save_settings(&data_dir, &st.settings)?;
    }

    // Swap provider.
    {
        let mut guard = provider_handle.write().await;
        *guard = new_provider.clone();
    }

    // Re-hydrate AppState.tasks.
    let tasks = new_provider.list_tasks(None).await?;
    {
        let mut st = state
            .lock()
            .map_err(|e| AppError::InvalidState(format!("AppState lock poisoned: {e}")))?;
        st.tasks.clear();
        for t in tasks {
            st.tasks.insert(t.id.clone(), t);
        }
    }
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
                scripts: Vec::new(),
            },
        );
        Arc::new(Mutex::new(state))
    }

    fn make_provider(data_dir: &std::path::Path) -> Arc<dyn crate::task_provider::TaskProvider> {
        Arc::new(crate::task_provider::local::LocalProvider::new(
            data_dir.to_path_buf(),
        ))
    }

    // ── Task 5: add_task tests ───────────────────────────────────────

    #[tokio::test]
    async fn add_task_creates_task_with_correct_fields() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());

        let task = add_task_inner(
            "repo_r1".into(),
            "Fix login".into(),
            "Auth fails".into(),
            None,
            make_provider(tmp.path()),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        assert!(task.id.starts_with("tk_"));
        assert_eq!(task.repo_id, "repo_r1");
        assert_eq!(task.title, "Fix login");
        assert_eq!(task.description, "Auth fails");
        assert_eq!(task.column, KanbanColumn::Todo);
        assert!(task.workspace_id.is_none());
        assert_eq!(task.order, 1024); // first task in column → 0 + 1024
    }

    #[tokio::test]
    async fn add_task_uses_specified_column() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());

        let task = add_task_inner(
            "repo_r1".into(),
            "Review task".into(),
            String::new(),
            Some(KanbanColumn::Review),
            make_provider(tmp.path()),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        assert_eq!(task.column, KanbanColumn::Review);
    }

    #[tokio::test]
    async fn add_task_order_increments_by_1024() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());
        let provider = make_provider(tmp.path());

        let t1 = add_task_inner(
            "repo_r1".into(),
            "Task 1".into(),
            String::new(),
            None,
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        assert_eq!(t1.order, 1024);

        let t2 = add_task_inner(
            "repo_r1".into(),
            "Task 2".into(),
            String::new(),
            None,
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        assert_eq!(t2.order, 2048);

        let t3 = add_task_inner(
            "repo_r1".into(),
            "Task 3".into(),
            String::new(),
            None,
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        assert_eq!(t3.order, 3072);
    }

    #[tokio::test]
    async fn add_task_persists_to_tasks_json() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());

        add_task_inner(
            "repo_r1".into(),
            "Persisted task".into(),
            String::new(),
            None,
            make_provider(tmp.path()),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let tasks_path = crate::platform::paths::tasks_file(tmp.path());
        assert!(tasks_path.exists(), "tasks.json should be written");
        let loaded = crate::persistence::tasks::load_tasks(tmp.path()).unwrap();
        assert_eq!(loaded.len(), 1);
    }

    #[tokio::test]
    async fn add_task_unknown_repo_returns_err() {
        let tmp = tempdir().unwrap();
        let state = Arc::new(Mutex::new(AppState::default()));
        let result = add_task_inner(
            "repo_nonexistent".into(),
            "X".into(),
            String::new(),
            None,
            make_provider(tmp.path()),
            state,
        )
        .await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("repo") || msg.contains("not found"),
            "Got: {msg}"
        );
    }

    // ── Task 6: list_tasks tests ─────────────────────────────────────

    #[tokio::test]
    async fn list_tasks_filters_by_repo_id() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());
        let provider = make_provider(tmp.path());

        add_task_inner(
            "repo_r1".into(),
            "Task A".into(),
            String::new(),
            None,
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        add_task_inner(
            "repo_r1".into(),
            "Task B".into(),
            String::new(),
            None,
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
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

    #[tokio::test]
    async fn update_task_title_and_description() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());
        let provider = make_provider(tmp.path());

        let task = add_task_inner(
            "repo_r1".into(),
            "Original title".into(),
            "Original desc".into(),
            None,
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let patch = TaskPatch {
            title: Some("Updated title".into()),
            description: Some("Updated desc".into()),
            order: None,
        };
        let updated = update_task_inner(
            task.id.clone(),
            patch,
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        assert_eq!(updated.title, "Updated title");
        assert_eq!(updated.description, "Updated desc");
        assert_eq!(updated.column, KanbanColumn::Todo); // column unchanged
        assert_eq!(updated.id, task.id);
    }

    #[tokio::test]
    async fn update_task_partial_patch_leaves_other_fields_unchanged() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());
        let provider = make_provider(tmp.path());

        let task = add_task_inner(
            "repo_r1".into(),
            "Keep me".into(),
            "Keep desc".into(),
            None,
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let patch = TaskPatch {
            title: Some("New title only".into()),
            description: None,
            order: None,
        };
        let updated = update_task_inner(
            task.id.clone(),
            patch,
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        assert_eq!(updated.title, "New title only");
        assert_eq!(updated.description, "Keep desc"); // unchanged
    }

    #[tokio::test]
    async fn update_task_order_change() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());
        let provider = make_provider(tmp.path());

        let task = add_task_inner(
            "repo_r1".into(),
            "T".into(),
            String::new(),
            None,
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
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
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        assert_eq!(updated.order, 512);
    }

    #[tokio::test]
    async fn update_task_not_found_returns_err() {
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
            make_provider(tmp.path()),
            state,
        )
        .await;
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
        let provider = make_provider(&data);

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
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        assert!(task.workspace_id.is_none());

        let moved = move_task_inner(
            task.id.clone(),
            KanbanColumn::InProgress,
            task.order,
            data.clone(),
            Arc::clone(&provider),
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
        let provider = make_provider(&data);

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
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        // Move to InProgress first (creates workspace)
        let in_progress = move_task_inner(
            task.id.clone(),
            KanbanColumn::InProgress,
            task.order,
            data.clone(),
            Arc::clone(&provider),
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
            Arc::clone(&provider),
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
        let provider = make_provider(&data);

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
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let in_progress = move_task_inner(
            task.id.clone(),
            KanbanColumn::InProgress,
            task.order,
            data.clone(),
            Arc::clone(&provider),
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
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let done = move_task_inner(
            task.id.clone(),
            KanbanColumn::Done,
            review.order,
            data.clone(),
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        assert_eq!(done.column, KanbanColumn::Done);
        assert_eq!(done.workspace_id.as_deref(), Some(ws_id.as_str()));
    }

    #[tokio::test]
    async fn move_task_todo_to_review_does_not_create_workspace() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());
        let provider = make_provider(tmp.path());

        let task = add_task_inner(
            "repo_r1".into(),
            "Skip InProgress".into(),
            String::new(),
            None,
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let moved = move_task_inner(
            task.id.clone(),
            KanbanColumn::Review,
            task.order,
            tmp.path().to_path_buf(),
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
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

    #[tokio::test]
    async fn remove_task_without_workspace_succeeds() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());
        let provider = make_provider(tmp.path());

        let task = add_task_inner(
            "repo_r1".into(),
            "To remove".into(),
            String::new(),
            None,
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        remove_task_inner(
            task.id.clone(),
            Some(false),
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let st = state.lock().unwrap();
        assert!(!st.tasks.contains_key(&task.id));
    }

    #[tokio::test]
    async fn remove_task_with_workspace_and_no_force_returns_err() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());
        let provider = make_provider(tmp.path());

        // Insert a task with workspace_id already set. We seed both the
        // in-memory mirror and the provider's tasks.json so the guard
        // (which reads AppState) and the eventual provider.delete_task
        // (which reads tasks.json) both see it.
        let task_id = "tk_linked".to_string();
        let seeded_task = Task {
            id: task_id.clone(),
            repo_id: "repo_r1".into(),
            workspace_id: Some("ws_exists".into()),
            title: "Has workspace".into(),
            description: String::new(),
            column: KanbanColumn::InProgress,
            order: 1024,
            created_at: 0,
            updated_at: 0,
        };
        {
            let mut st = state.lock().unwrap();
            st.tasks.insert(task_id.clone(), seeded_task.clone());
        }
        {
            let mut map = std::collections::HashMap::new();
            map.insert(task_id.clone(), seeded_task);
            crate::persistence::tasks::save_tasks(tmp.path(), &map).unwrap();
        }

        let result = remove_task_inner(
            task_id.clone(),
            Some(false),
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await;
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

    #[tokio::test]
    async fn remove_task_with_workspace_and_force_true_succeeds() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());
        let provider = make_provider(tmp.path());

        let task_id = "tk_forced".to_string();
        let seeded_task = Task {
            id: task_id.clone(),
            repo_id: "repo_r1".into(),
            workspace_id: Some("ws_exists".into()),
            title: "Force remove".into(),
            description: String::new(),
            column: KanbanColumn::InProgress,
            order: 1024,
            created_at: 0,
            updated_at: 0,
        };
        {
            let mut st = state.lock().unwrap();
            st.tasks.insert(task_id.clone(), seeded_task.clone());
        }
        {
            let mut map = std::collections::HashMap::new();
            map.insert(task_id.clone(), seeded_task);
            crate::persistence::tasks::save_tasks(tmp.path(), &map).unwrap();
        }

        remove_task_inner(
            task_id.clone(),
            Some(true),
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        let st = state.lock().unwrap();
        assert!(!st.tasks.contains_key(&task_id));
    }

    #[tokio::test]
    async fn remove_task_not_found_returns_err() {
        let tmp = tempdir().unwrap();
        let state = Arc::new(Mutex::new(AppState::default()));
        let result = remove_task_inner(
            "tk_ghost".into(),
            Some(false),
            make_provider(tmp.path()),
            state,
        )
        .await;
        assert!(result.is_err());
    }

    fn make_state() -> Arc<Mutex<AppState>> {
        Arc::new(Mutex::new(AppState::default()))
    }

    #[tokio::test]
    async fn refresh_tasks_replaces_mirror_subset_for_repo() {
        let tmp = tempdir().unwrap();
        let provider = make_provider(tmp.path());
        // Pre-populate via the provider directly.
        for repo in ["repo_a", "repo_a", "repo_b"] {
            provider
                .create_task(crate::task_provider::CreateTaskArgs {
                    repo_id: repo.into(),
                    title: "t".into(),
                    description: String::new(),
                    column: None,
                })
                .await
                .unwrap();
        }
        let state = make_state();
        // Refresh only repo_a — pulls 2 tasks into mirror.
        let tasks = refresh_tasks_inner(Some("repo_a".into()), state.clone(), provider.clone())
            .await
            .unwrap();
        assert_eq!(tasks.len(), 2);
        let st = state.lock().unwrap();
        assert_eq!(
            st.tasks.values().filter(|t| t.repo_id == "repo_a").count(),
            2
        );
    }

    #[tokio::test]
    async fn set_task_source_lark_rejects_when_credentials_missing() {
        let tmp = tempdir().unwrap();
        let state = make_state();
        let provider_handle = std::sync::Arc::new(tokio::sync::RwLock::new(std::sync::Arc::new(
            crate::task_provider::local::LocalProvider::new(tmp.path().to_path_buf()),
        )
            as std::sync::Arc<dyn crate::task_provider::TaskProvider>));
        let err = set_task_source_inner(
            crate::state::TaskSource::Lark,
            tmp.path().to_path_buf(),
            state,
            provider_handle,
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string().contains("Configure Lark credentials"),
            "{err}"
        );
    }
}
