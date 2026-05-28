use crate::error::{AppError, Result};
use crate::state::{AppState, KanbanColumn, Task};
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};

// ── Per-repo provider lookup ─────────────────────────────────────────

/// Return the provider registered for `repo_id`, or fall back to a
/// fresh LocalProvider rooted at `data_dir` when the map has no entry.
/// The read guard is dropped before returning so callers can immediately
/// call async methods on the returned provider without holding the lock.
async fn provider_for_repo(
    handle: &crate::state::TaskProviderHandle,
    data_dir: &std::path::Path,
    repo_id: &str,
) -> Arc<dyn crate::task_provider::TaskProvider> {
    let guard = handle.read().await;
    if let Some(p) = guard.get(repo_id) {
        return p.clone();
    }
    drop(guard);
    crate::state::make_default_local_provider(data_dir)
}

// ── Structs ──────────────────────────────────────────────────────────

#[derive(serde::Deserialize, Debug)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub order: Option<i32>,
}

/// Result of `unlink_task_from_workspace`.
///
/// - `Unlinked`: the card was detached (or was never linked); the workspace
///   remains.
/// - `Removed`: the unlink dropped the last live ref AND the workspace was
///   empty (no commits ahead, clean worktree, no chat, no live agent), so it
///   was cleaned up.
/// - `WouldRemove { workspace_title }`: returned in preview mode
///   (`force = false`) when the unlink would trigger `Removed`. No state was
///   mutated — the UI uses this to drive a confirm modal, then re-calls with
///   `force = true`.
#[derive(serde::Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UnlinkResult {
    Unlinked,
    Removed,
    WouldRemove { workspace_title: String },
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
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let provider = provider_for_repo(provider_handle.inner(), &data_dir, &repo_id).await;
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
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    // Resolve repo_id from the in-memory mirror so we can pick the right provider.
    let repo_id = {
        let st = state
            .lock()
            .map_err(|e| format!("AppState lock poisoned: {e}"))?;
        st.tasks
            .get(&task_id)
            .map(|t| t.repo_id.clone())
            .ok_or_else(|| format!("Task '{task_id}' not found in mirror"))?
    };
    let provider = provider_for_repo(provider_handle.inner(), &data_dir, &repo_id).await;
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
    // Resolve repo_id from the in-memory mirror so we can pick the right provider.
    let repo_id = {
        let st = state
            .lock()
            .map_err(|e| format!("AppState lock poisoned: {e}"))?;
        st.tasks
            .get(&task_id)
            .map(|t| t.repo_id.clone())
            .ok_or_else(|| format!("Task '{task_id}' not found in mirror"))?
    };
    let provider = provider_for_repo(provider_handle.inner(), &data_dir, &repo_id).await;
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
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    // Resolve repo_id from the in-memory mirror so we can pick the right provider.
    let repo_id = {
        let st = state
            .lock()
            .map_err(|e| format!("AppState lock poisoned: {e}"))?;
        st.tasks
            .get(&task_id)
            .map(|t| t.repo_id.clone())
            .unwrap_or_default()
    };
    let provider = provider_for_repo(provider_handle.inner(), &data_dir, &repo_id).await;
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
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
    provider_handle: State<'_, crate::state::TaskProviderHandle>,
) -> std::result::Result<Vec<Task>, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    // Resolve which repo to look up: use the provided repo_id, or fall
    // back to the selected/first repo from AppState when None.
    let lookup_repo_id: String = match &repo_id {
        Some(r) => r.clone(),
        None => {
            let st = state
                .lock()
                .map_err(|e| format!("AppState lock poisoned: {e}"))?;
            resolve_default_repo(&st).unwrap_or_default()
        }
    };
    let provider = provider_for_repo(provider_handle.inner(), &data_dir, &lookup_repo_id).await;
    refresh_tasks_inner(repo_id, state.inner().clone(), provider)
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
        // Snapshot local-only fields the provider can't know (LarkProvider
        // blanks repo_id/workspace_id). Re-stamp them onto the fresh tasks
        // so a refocus refresh never severs a card<->workspace link — the
        // root cause of duplicate auto-created workspaces.
        let preserved: std::collections::HashMap<String, (String, Option<String>)> = st
            .tasks
            .iter()
            .map(|(id, t)| (id.clone(), (t.repo_id.clone(), t.workspace_id.clone())))
            .collect();
        if let Some(rid) = repo_id.as_deref() {
            st.tasks.retain(|_, t| t.repo_id != rid);
        } else {
            st.tasks.clear();
        }
        for t in &tasks {
            let mut t = t.clone();
            if let Some((repo, ws)) = preserved.get(&t.id) {
                if t.repo_id.is_empty() {
                    t.repo_id = repo.clone();
                }
                if t.workspace_id.is_none() {
                    t.workspace_id = ws.clone();
                }
            }
            st.tasks.insert(t.id.clone(), t);
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
    // Snapshot local-only fields before provider call. See `move_task_inner`
    // for the rationale — LarkProvider can't know repo_id/workspace_id so
    // it returns them blank; we stamp them back to keep the mirror entry
    // intact across mutations.
    let (existing_repo_id, existing_workspace_id) = {
        let st = state
            .lock()
            .map_err(|e| AppError::InvalidState(format!("AppState lock poisoned: {e}")))?;
        let task = st
            .tasks
            .get(&task_id)
            .ok_or_else(|| AppError::NotFound(format!("task '{}' not found", task_id)))?;
        (task.repo_id.clone(), task.workspace_id.clone())
    };

    let provider_patch = crate::task_provider::TaskPatch {
        title: patch.title,
        description: patch.description,
        order: patch.order,
    };
    let mut updated = provider.update_task(&task_id, provider_patch).await?;
    updated.repo_id = existing_repo_id;
    updated.workspace_id = existing_workspace_id;
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
    // Snapshot under one lock, then drop it before git / async work.
    // reattach_ws_id: the workspace this card should link to when entering
    //   In Progress — a still-valid existing link, else a workspace already
    //   tagged with this task_id (link may have been wiped by a Lark
    //   refresh). None → create a fresh workspace.
    // todo_cleanup: (workspace, agent_live) when the linked workspace still
    //   exists — used by the empty check on the way to Todo.
    let (repo_id, task_title, task_desc, reattach_ws_id, todo_cleanup, needs_create) = {
        let st = state
            .lock()
            .map_err(|e| AppError::InvalidState(format!("AppState lock poisoned: {e}")))?;
        let task = st
            .tasks
            .get(&task_id)
            .ok_or_else(|| AppError::NotFound(format!("task '{}' not found", task_id)))?;
        let repo_id = task.repo_id.clone();
        let linked = task.workspace_id.clone();

        let reattach_ws_id = match linked.as_ref() {
            Some(id) if st.workspaces.contains_key(id) => Some(id.clone()),
            _ => st
                .workspaces
                .values()
                .find(|w| w.repo_id == repo_id && w.task_ids.contains(&task_id))
                .map(|w| w.id.clone()),
        };

        let todo_cleanup = linked
            .as_ref()
            .and_then(|id| st.workspaces.get(id))
            .map(|w| (w.clone(), st.agents.contains_key(&w.id)));

        // Only create a fresh workspace if moving to InProgress AND there is
        // no reattach target AND the task carries no workspace_id at all.
        // A stale link (workspace_id set but not present in state.workspaces)
        // is left as-is — we don't know why it's missing, so we leave it
        // untouched rather than creating a duplicate.
        let needs_create =
            column == KanbanColumn::InProgress && reattach_ws_id.is_none() && linked.is_none();
        (
            repo_id,
            task.title.clone(),
            task.description.clone(),
            reattach_ws_id,
            todo_cleanup,
            needs_create,
        )
    };

    // (A) Auto-create when entering In Progress with no reattach target.
    let created_ws_id: Option<String> = if needs_create {
        let ws = crate::commands::workspace::create_workspace_inner(
            repo_id.clone(),
            task_title,
            task_desc,
            None,
            data_dir.clone(),
            Arc::clone(&state),
        )
        .await?;
        {
            let mut st = state
                .lock()
                .map_err(|e| AppError::InvalidState(format!("AppState lock poisoned: {e}")))?;
            if let Some(w) = st.workspaces.get_mut(&ws.id) {
                if !w.task_ids.contains(&task_id) {
                    w.task_ids.push(task_id.clone());
                }
            }
            crate::persistence::workspaces::save_workspaces(&data_dir, &st.workspaces)?;
        }
        tracing::info!(task_id = %task_id, workspace_id = %ws.id, "Auto-created workspace for task");
        Some(ws.id)
    } else {
        None
    };

    // (B) Auto-remove an EMPTY linked workspace when returning to Todo.
    //
    // Multi-card refcount rule (spec §4):
    //   refcount > 1  → sticky; no cleanup, link stays, only column changes.
    //   refcount == 1 + empty → PR #32 behaviour (unlink + remove).
    //   refcount == 1 + not-empty → keep (existing behaviour, no-op).
    let removed_ws = if column == KanbanColumn::Todo {
        match todo_cleanup.as_ref() {
            Some((ws, agent_live))
                if ws.task_ids.len() <= 1
                    && crate::commands::workspace::is_workspace_empty(
                        &data_dir,
                        ws,
                        *agent_live,
                    ) =>
            {
                crate::commands::workspace::remove_workspace_inner(
                    ws.id.clone(),
                    data_dir.clone(),
                    Arc::clone(&state),
                )
                .await?;
                tracing::info!(task_id = %task_id, workspace_id = %ws.id, "Removed empty workspace on return to Todo");
                Some(ws.id.clone())
            }
            _ => None,
        }
    } else {
        None
    };

    // Route the column/order change through the provider.
    let mut updated = provider.move_task(&task_id, column, order).await?;
    updated.repo_id = repo_id.clone();

    // Final link: removal wins; else created/reattached id; else preserve.
    updated.workspace_id = if removed_ws.is_some() {
        None
    } else if let Some(id) = created_ws_id.or(reattach_ws_id) {
        Some(id)
    } else {
        let st = state
            .lock()
            .map_err(|e| AppError::InvalidState(format!("AppState lock poisoned: {e}")))?;
        st.tasks.get(&task_id).and_then(|t| t.workspace_id.clone())
    };

    // Persist updated task (mirror + tasks.json so the link is durable).
    {
        let mut st = state
            .lock()
            .map_err(|e| AppError::InvalidState(format!("AppState lock poisoned: {e}")))?;
        st.tasks.insert(updated.id.clone(), updated.clone());
        let map = st.tasks.clone();
        crate::persistence::tasks::save_tasks(&data_dir, &map)?;
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

/// Picks a fallback `repo_id` to assign to tasks hydrated from a provider
/// that returned rows with no `repo_id` set. Preference order: the user's
/// currently selected repo, then the first repo registered in AppState.
/// Returns `None` only when Ansambel has no repos at all — in that case
/// the task stays with an empty `repo_id` and is unreachable until the
/// user adds a repo.
pub(crate) fn resolve_default_repo(state: &AppState) -> Option<String> {
    state
        .settings
        .selected_repo_id
        .clone()
        .or_else(|| state.repos.keys().next().cloned())
}

// ── unlink_task_from_workspace ───────────────────────────────────────
//
// Detaches a card from its workspace, optionally previewing whether the
// unlink would trigger refcount-cleanup of the workspace itself.
//
// Behaviour (spec §3 + §4):
// - If the card is not currently linked, returns `Unlinked` (no-op).
// - If the workspace forward-link is dangling (workspace id refers to a
//   workspace that no longer exists), the forward link is cleared and
//   `Unlinked` is returned — defensive fail-safe.
// - Otherwise the post-unlink refcount is computed (== ws.task_ids.len() - 1).
//   If `refcount_after == 0` AND the workspace would be considered empty
//   by `is_workspace_empty` (no commits ahead, clean tree, no chat, no
//   live agent), the unlink will trigger cleanup:
//   - In preview mode (`force = false`) the function returns
//     `WouldRemove { workspace_title }` WITHOUT mutating any state —
//     the UI uses this to show a confirm modal before re-calling with
//     `force = true`.
//   - With `force = true`, the unlink is performed AND
//     `remove_workspace_inner` is called; `Removed` is returned.
// - Any other case (refcount_after > 0, OR refcount_after == 0 but the
//   workspace is non-empty) performs the unlink and returns `Unlinked`.
//
// Mutex discipline:
// - State is read once under the lock to capture (workspace_id, title,
//   worktree_dir, base_branch, refcount_after, agent_live), then the
//   lock is dropped before calling `is_workspace_empty` (which does git
//   I/O) or `save_*` (disk I/O) or `remove_workspace_inner` (locks
//   internally).
#[tauri::command]
pub async fn unlink_task_from_workspace(
    task_id: String,
    force: bool,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<UnlinkResult, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    unlink_task_from_workspace_inner(&task_id, force, data_dir, state.inner().clone())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "unlink_task_from_workspace failed");
            e.to_string()
        })
}

pub(crate) async fn unlink_task_from_workspace_inner(
    task_id: &str,
    force: bool,
    data_dir: std::path::PathBuf,
    state: Arc<Mutex<AppState>>,
) -> Result<UnlinkResult> {
    use crate::state::{KanbanColumn, WorkspaceStatus};

    // Phase 1: read current state under the lock — find the task and
    // its workspace, capture everything needed for the
    // `is_workspace_empty` decision (which must run OUTSIDE the lock).
    let (workspace_id, workspace_title, refcount_after, worktree_dir, base_branch, agent_live) = {
        let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        let task = st
            .tasks
            .get(task_id)
            .ok_or_else(|| AppError::NotFound(format!("task '{task_id}'")))?;
        let Some(ws_id) = task.workspace_id.clone() else {
            return Ok(UnlinkResult::Unlinked);
        };
        let Some(ws) = st.workspaces.get(&ws_id) else {
            // Defensive: the task's forward link points to a workspace
            // that no longer exists. Clear the dangling link so future
            // reads see a clean state. Drop the read guard before
            // re-acquiring write access.
            drop(st);
            let snapshot = {
                let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
                if let Some(t) = st.tasks.get_mut(task_id) {
                    t.workspace_id = None;
                    t.updated_at = crate::commands::helpers::now_unix();
                }
                st.tasks.clone()
            };
            crate::persistence::tasks::save_tasks(&data_dir, &snapshot)?;
            return Ok(UnlinkResult::Unlinked);
        };
        let refcount_after = ws
            .task_ids
            .iter()
            .filter(|id| id.as_str() != task_id)
            .count();
        let agent_live = st.agents.contains_key(&ws_id);
        (
            ws_id,
            ws.title.clone(),
            refcount_after,
            ws.worktree_dir.clone(),
            ws.base_branch.clone(),
            agent_live,
        )
    };

    // Phase 2: cleanup-check decision. Treat `refcount_after == 0 +
    // empty` as "would remove". This call shells out to git, so it
    // MUST happen with no state lock held.
    let would_remove = if refcount_after == 0 {
        // Construct a transient WorkspaceInfo proxy carrying only the
        // fields `is_workspace_empty` reads (worktree_dir, base_branch,
        // id for the messages-log lookup). Other fields are stubbed.
        let ws_proxy = crate::state::WorkspaceInfo {
            id: workspace_id.clone(),
            repo_id: String::new(),
            branch: String::new(),
            base_branch: base_branch.clone(),
            custom_branch: false,
            title: workspace_title.clone(),
            description: String::new(),
            status: WorkspaceStatus::Waiting,
            column: KanbanColumn::InProgress,
            created_at: 0,
            updated_at: 0,
            worktree_dir: worktree_dir.clone(),
            team_activity_private: false,
            task_ids: Vec::new(),
        };
        crate::commands::workspace::is_workspace_empty(&data_dir, &ws_proxy, agent_live)
    } else {
        false
    };

    if !force && would_remove {
        return Ok(UnlinkResult::WouldRemove { workspace_title });
    }

    // Phase 3: perform the unlink under the lock; clone snapshots;
    // release the lock before disk / cleanup I/O.
    let (tasks_snapshot, workspaces_snapshot) = {
        let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        let now = crate::commands::helpers::now_unix();
        if let Some(task) = st.tasks.get_mut(task_id) {
            task.workspace_id = None;
            task.updated_at = now;
        }
        if let Some(ws) = st.workspaces.get_mut(&workspace_id) {
            ws.task_ids.retain(|id| id != task_id);
            ws.updated_at = now;
        }
        (st.tasks.clone(), st.workspaces.clone())
    };
    crate::persistence::tasks::save_tasks(&data_dir, &tasks_snapshot)?;
    crate::persistence::workspaces::save_workspaces(&data_dir, &workspaces_snapshot)?;

    // Phase 4: if cleanup is now warranted, remove the workspace.
    // `remove_workspace_inner` locks internally — call OUTSIDE any
    // currently-held lock (already dropped above).
    if would_remove {
        crate::commands::workspace::remove_workspace_inner(
            workspace_id,
            data_dir.clone(),
            Arc::clone(&state),
        )
        .await?;
        return Ok(UnlinkResult::Removed);
    }

    Ok(UnlinkResult::Unlinked)
}

// ── link_task_to_workspace ──────────────────────────────────────────
//
// Attaches a card to a workspace. Idempotent when the card is already
// linked to the target workspace; performs an atomic switch (via the
// unlink-inner cleanup path) when the card was previously linked to a
// different workspace.
//
// Validation (under lock, then dropped before I/O):
// - task exists,
// - workspace exists,
// - `task.repo_id == workspace.repo_id` — else `InvalidState("repo
//   mismatch: ...")`.
//
// Mutex discipline:
// - The unlink-inner call (for the switch case) and the `save_*` disk
//   writes both happen with NO lock held — locks are reacquired only to
//   read or mutate the in-memory state and then immediately released.
#[tauri::command]
pub async fn link_task_to_workspace(
    task_id: String,
    workspace_id: String,
    app: tauri::AppHandle,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    link_task_to_workspace_inner(&task_id, &workspace_id, data_dir, state.inner().clone())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "link_task_to_workspace failed");
            e.to_string()
        })
}

pub(crate) async fn link_task_to_workspace_inner(
    task_id: &str,
    workspace_id: &str,
    data_dir: std::path::PathBuf,
    state: Arc<Mutex<AppState>>,
) -> Result<()> {
    // Phase 1: validate + plan the mutation while holding the lock briefly.
    let (already_linked_here, switch_from) = {
        let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        let task = st
            .tasks
            .get(task_id)
            .ok_or_else(|| AppError::NotFound(format!("task '{task_id}'")))?;
        let ws = st
            .workspaces
            .get(workspace_id)
            .ok_or_else(|| AppError::NotFound(format!("workspace '{workspace_id}'")))?;
        if task.repo_id != ws.repo_id {
            return Err(AppError::InvalidState(format!(
                "repo mismatch: task '{task_id}' is in repo '{}', workspace '{workspace_id}' is in repo '{}'",
                task.repo_id, ws.repo_id
            )));
        }
        let already = task.workspace_id.as_deref() == Some(workspace_id);
        let switch_from = task
            .workspace_id
            .clone()
            .filter(|w| w.as_str() != workspace_id);
        (already, switch_from)
    };

    if already_linked_here {
        return Ok(());
    }

    // Phase 2: if switching, unlink from the previous workspace (may
    // trigger cleanup). This calls the unlink inner so the cleanup
    // logic stays in one place. MUST happen with no lock held — the
    // inner re-acquires the lock and may invoke `remove_workspace_inner`.
    if switch_from.is_some() {
        let _ = unlink_task_from_workspace_inner(
            task_id,
            /* force = */ true,
            data_dir.clone(),
            Arc::clone(&state),
        )
        .await?;
    }

    // Phase 3: link — bump both timestamps, idempotent guard on
    // `ws.task_ids`. Clone snapshots; drop lock before disk I/O.
    let now = crate::commands::helpers::now_unix();
    let (tasks_snapshot, workspaces_snapshot) = {
        let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        if let Some(task) = st.tasks.get_mut(task_id) {
            task.workspace_id = Some(workspace_id.to_string());
            task.updated_at = now;
        }
        if let Some(ws) = st.workspaces.get_mut(workspace_id) {
            if !ws.task_ids.iter().any(|id| id == task_id) {
                ws.task_ids.push(task_id.to_string());
            }
            ws.updated_at = now;
        }
        (st.tasks.clone(), st.workspaces.clone())
    };

    crate::persistence::tasks::save_tasks(&data_dir, &tasks_snapshot)?;
    crate::persistence::workspaces::save_workspaces(&data_dir, &workspaces_snapshot)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, KanbanColumn, RepoInfo};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;

    #[test]
    fn resolve_default_repo_prefers_selected_repo_id() {
        let mut state = AppState::default();
        state.repos.insert(
            "repo_first".into(),
            RepoInfo {
                id: "repo_first".into(),
                name: "first".into(),
                path: PathBuf::from("/tmp/first"),
                gh_profile: None,
                default_branch: "main".into(),
                created_at: 0,
                updated_at: 0,
                scripts: Vec::new(),
            },
        );
        state.repos.insert(
            "repo_selected".into(),
            RepoInfo {
                id: "repo_selected".into(),
                name: "selected".into(),
                path: PathBuf::from("/tmp/sel"),
                gh_profile: None,
                default_branch: "main".into(),
                created_at: 0,
                updated_at: 0,
                scripts: Vec::new(),
            },
        );
        state.settings.selected_repo_id = Some("repo_selected".into());
        assert_eq!(resolve_default_repo(&state), Some("repo_selected".into()));
    }

    #[test]
    fn resolve_default_repo_falls_back_to_first_repo_when_no_selection() {
        let mut state = AppState::default();
        state.repos.insert(
            "repo_only".into(),
            RepoInfo {
                id: "repo_only".into(),
                name: "only".into(),
                path: PathBuf::from("/tmp/only"),
                gh_profile: None,
                default_branch: "main".into(),
                created_at: 0,
                updated_at: 0,
                scripts: Vec::new(),
            },
        );
        state.settings.selected_repo_id = None;
        assert_eq!(resolve_default_repo(&state), Some("repo_only".into()));
    }

    #[test]
    fn resolve_default_repo_returns_none_when_no_repos() {
        let state = AppState::default();
        assert_eq!(resolve_default_repo(&state), None);
    }

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
                pic_names: Vec::new(),
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
                    pic_names: Vec::new(),
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
                    pic_names: Vec::new(),
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
                    pic_names: Vec::new(),
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
            pic_names: Vec::new(),
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
            pic_names: Vec::new(),
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

    // ── provider_for_repo tests ──────────────────────────────────────

    #[tokio::test]
    async fn provider_for_repo_returns_default_when_repo_not_in_handle() {
        use std::collections::HashMap;
        let tmp = tempdir().unwrap();
        let handle: crate::state::TaskProviderHandle =
            Arc::new(tokio::sync::RwLock::new(HashMap::new()));
        let provider = provider_for_repo(&handle, tmp.path(), "unknown_repo").await;
        // The default fallback is a LocalProvider. Verify it doesn't crash
        // and produces an empty task list for an unknown repo.
        let tasks = provider.list_tasks(Some("unknown_repo")).await.unwrap();
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn provider_for_repo_returns_existing_provider_when_repo_present() {
        use std::collections::HashMap;
        let tmp = tempdir().unwrap();
        let local: Arc<dyn crate::task_provider::TaskProvider> = Arc::new(
            crate::task_provider::local::LocalProvider::new(tmp.path().to_path_buf()),
        );
        let mut map = HashMap::new();
        map.insert("repo_x".to_string(), local.clone());
        let handle: crate::state::TaskProviderHandle = Arc::new(tokio::sync::RwLock::new(map));
        let p = provider_for_repo(&handle, tmp.path(), "repo_x").await;
        // Same Arc — Arc::ptr_eq verifies it's the original instance, not a fresh fallback.
        assert!(Arc::ptr_eq(&p, &local));
    }

    // ── Regression: LarkProvider-shaped responses must not regress mirror ──

    /// Mock provider that simulates LarkProvider's behavior of returning
    /// Task with `repo_id = ""` and `workspace_id = None` because Lark
    /// can't know those local-only fields. Used to verify the command
    /// layer stamps them back from the mirror.
    #[derive(Debug)]
    struct BlankRepoIdProvider;

    #[async_trait::async_trait]
    impl crate::task_provider::TaskProvider for BlankRepoIdProvider {
        async fn list_tasks(
            &self,
            _: Option<&str>,
        ) -> crate::error::Result<Vec<crate::state::Task>> {
            Ok(Vec::new())
        }
        async fn create_task(
            &self,
            args: crate::task_provider::CreateTaskArgs,
        ) -> crate::error::Result<crate::state::Task> {
            Ok(crate::state::Task {
                id: "rec_x".into(),
                repo_id: String::new(), // ← Lark-style blank
                workspace_id: None,
                title: args.title,
                description: args.description,
                column: args.column.unwrap_or_default(),
                order: 0,
                created_at: 0,
                updated_at: 0,
                pic_names: Vec::new(),
            })
        }
        async fn update_task(
            &self,
            id: &str,
            _patch: crate::task_provider::TaskPatch,
        ) -> crate::error::Result<crate::state::Task> {
            Ok(crate::state::Task {
                id: id.into(),
                repo_id: String::new(),
                workspace_id: None,
                title: "t".into(),
                description: String::new(),
                column: KanbanColumn::Todo,
                order: 0,
                created_at: 0,
                updated_at: 0,
                pic_names: Vec::new(),
            })
        }
        async fn move_task(
            &self,
            id: &str,
            column: KanbanColumn,
            order: i32,
        ) -> crate::error::Result<crate::state::Task> {
            Ok(crate::state::Task {
                id: id.into(),
                repo_id: String::new(), // ← Lark-style blank
                workspace_id: None,     // ← Lark-style blank
                title: "t".into(),
                description: String::new(),
                column,
                order,
                created_at: 0,
                updated_at: 0,
                pic_names: Vec::new(),
            })
        }
        async fn delete_task(&self, _id: &str) -> crate::error::Result<()> {
            Ok(())
        }
    }

    /// Regression: moving a task whose backing provider blanks
    /// repo_id/workspace_id (e.g., LarkProvider) must NOT cause the
    /// mirror entry to regress. Without the fix, the second move on the
    /// same card finds `repo_id = ""` in the mirror and fails with
    /// `Not found: repo ''`.
    #[tokio::test]
    async fn move_task_preserves_repo_id_across_repeated_moves() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());
        // Seed mirror with a task that has both repo_id + workspace_id.
        {
            let mut st = state.lock().unwrap();
            st.tasks.insert(
                "tk_a".into(),
                crate::state::Task {
                    id: "tk_a".into(),
                    repo_id: "repo_r1".into(),
                    workspace_id: Some("ws_existing".into()),
                    title: "Card A".into(),
                    description: String::new(),
                    column: KanbanColumn::InProgress,
                    order: 0,
                    created_at: 0,
                    updated_at: 0,
                    pic_names: Vec::new(),
                },
            );
        }
        let provider: Arc<dyn crate::task_provider::TaskProvider> = Arc::new(BlankRepoIdProvider);

        // First move: In Progress → Todo
        let after1 = move_task_inner(
            "tk_a".into(),
            KanbanColumn::Todo,
            0,
            tmp.path().to_path_buf(),
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        assert_eq!(after1.repo_id, "repo_r1", "repo_id preserved after move #1");
        assert_eq!(
            after1.workspace_id.as_deref(),
            Some("ws_existing"),
            "workspace_id preserved after move #1"
        );

        // Second move: Todo → In Progress. The mirror must still carry
        // repo_id = "repo_r1" — otherwise this call fails with
        // `Not found: repo ''` (the bug we're regressing against).
        let after2 = move_task_inner(
            "tk_a".into(),
            KanbanColumn::InProgress,
            0,
            tmp.path().to_path_buf(),
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        assert_eq!(after2.repo_id, "repo_r1", "repo_id survives repeated moves");
        assert_eq!(
            after2.workspace_id.as_deref(),
            Some("ws_existing"),
            "workspace_id survives repeated moves"
        );
    }

    // ── Task 4: move_task lifecycle tests ───────────────────────────

    fn init_bare_repo_for_task_tests(tmp: &tempfile::TempDir) -> PathBuf {
        use std::process::Command as Cmd;
        let remote = tmp.path().join("task_remote.git");
        std::fs::create_dir_all(&remote).unwrap();
        Cmd::new("git")
            .args(["init", "--bare"])
            .current_dir(&remote)
            .output()
            .unwrap();
        let local = tmp.path().join("task_local");
        Cmd::new("git")
            .args(["clone", remote.to_str().unwrap(), local.to_str().unwrap()])
            .output()
            .unwrap();
        Cmd::new("git")
            .args(["config", "user.email", "t@t.com"])
            .current_dir(&local)
            .output()
            .unwrap();
        Cmd::new("git")
            .args(["config", "user.name", "T"])
            .current_dir(&local)
            .output()
            .unwrap();
        std::fs::write(local.join("f"), b"x").unwrap();
        Cmd::new("git")
            .args(["add", "."])
            .current_dir(&local)
            .output()
            .unwrap();
        Cmd::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&local)
            .output()
            .unwrap();
        Cmd::new("git")
            .args(["push", "origin", "HEAD:main"])
            .current_dir(&local)
            .output()
            .unwrap();
        Cmd::new("git")
            .args(["remote", "set-head", "origin", "main"])
            .current_dir(&local)
            .output()
            .unwrap();
        local
    }

    fn make_state_with_real_repo(
        tmp: &tempfile::TempDir,
        local_path: &std::path::Path,
    ) -> Arc<Mutex<AppState>> {
        let mut state = AppState::default();
        state.repos.insert(
            "repo_r1".into(),
            RepoInfo {
                id: "repo_r1".into(),
                name: "my-repo".into(),
                path: local_path.to_path_buf(),
                gh_profile: None,
                default_branch: "main".into(),
                created_at: 1_776_000_000,
                updated_at: 1_776_000_000,
                scripts: Vec::new(),
            },
        );
        let _ = tmp;
        Arc::new(Mutex::new(state))
    }

    #[tokio::test]
    async fn move_to_in_progress_reattaches_instead_of_duplicating() {
        let tmp = tempdir().unwrap();
        let local = init_bare_repo_for_task_tests(&tmp);
        let data = tmp.path().join("data4a");
        std::fs::create_dir_all(&data).unwrap();
        let state = make_state_with_real_repo(&tmp, &local);
        let repo_id = { state.lock().unwrap().repos.keys().next().unwrap().clone() };
        let ws = crate::commands::workspace::create_workspace_inner(
            repo_id.clone(),
            "Card A".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        {
            let mut st = state.lock().unwrap();
            st.workspaces.get_mut(&ws.id).unwrap().task_ids = vec!["tk_a".into()];
            st.tasks.insert(
                "tk_a".into(),
                crate::state::Task {
                    id: "tk_a".into(),
                    repo_id: repo_id.clone(),
                    workspace_id: None,
                    title: "Card A".into(),
                    description: String::new(),
                    column: KanbanColumn::Todo,
                    order: 0,
                    created_at: 0,
                    updated_at: 0,
                    pic_names: Vec::new(),
                },
            );
        }
        let provider: Arc<dyn crate::task_provider::TaskProvider> = Arc::new(BlankRepoIdProvider);
        let updated = move_task_inner(
            "tk_a".into(),
            KanbanColumn::InProgress,
            0,
            data.clone(),
            provider,
            Arc::clone(&state),
        )
        .await
        .unwrap();
        assert_eq!(
            updated.workspace_id.as_deref(),
            Some(ws.id.as_str()),
            "reattached to existing ws"
        );
        let st = state.lock().unwrap();
        assert_eq!(st.workspaces.len(), 1, "must NOT create a second workspace");
    }

    #[tokio::test]
    async fn move_to_todo_removes_empty_workspace_and_unlinks() {
        let tmp = tempdir().unwrap();
        let local = init_bare_repo_for_task_tests(&tmp);
        let data = tmp.path().join("data4b");
        std::fs::create_dir_all(&data).unwrap();
        let state = make_state_with_real_repo(&tmp, &local);
        let repo_id = { state.lock().unwrap().repos.keys().next().unwrap().clone() };
        let ws = crate::commands::workspace::create_workspace_inner(
            repo_id.clone(),
            "Card A".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        {
            let mut st = state.lock().unwrap();
            st.workspaces.get_mut(&ws.id).unwrap().task_ids = vec!["tk_a".into()];
            st.tasks.insert(
                "tk_a".into(),
                crate::state::Task {
                    id: "tk_a".into(),
                    repo_id: repo_id.clone(),
                    workspace_id: Some(ws.id.clone()),
                    title: "Card A".into(),
                    description: String::new(),
                    column: KanbanColumn::InProgress,
                    order: 0,
                    created_at: 0,
                    updated_at: 0,
                    pic_names: Vec::new(),
                },
            );
        }
        let provider: Arc<dyn crate::task_provider::TaskProvider> = Arc::new(BlankRepoIdProvider);
        let updated = move_task_inner(
            "tk_a".into(),
            KanbanColumn::Todo,
            0,
            data.clone(),
            provider,
            Arc::clone(&state),
        )
        .await
        .unwrap();
        assert_eq!(updated.workspace_id, None, "empty workspace unlinked");
        let st = state.lock().unwrap();
        assert!(st.workspaces.is_empty(), "empty workspace removed");
        assert!(!ws.worktree_dir.exists(), "worktree deleted from disk");
    }

    #[tokio::test]
    async fn move_to_todo_keeps_non_empty_workspace() {
        let tmp = tempdir().unwrap();
        let local = init_bare_repo_for_task_tests(&tmp);
        let data = tmp.path().join("data4c");
        std::fs::create_dir_all(&data).unwrap();
        let state = make_state_with_real_repo(&tmp, &local);
        let repo_id = { state.lock().unwrap().repos.keys().next().unwrap().clone() };
        let ws = crate::commands::workspace::create_workspace_inner(
            repo_id.clone(),
            "Card A".into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        let wt = ws.worktree_dir.to_string_lossy().to_string();
        std::fs::write(ws.worktree_dir.join("f.txt"), "x").unwrap();
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
            std::process::Command::new("git")
                .args(&args)
                .output()
                .unwrap();
        }
        {
            let mut st = state.lock().unwrap();
            st.workspaces.get_mut(&ws.id).unwrap().task_ids = vec!["tk_a".into()];
            st.tasks.insert(
                "tk_a".into(),
                crate::state::Task {
                    id: "tk_a".into(),
                    repo_id: repo_id.clone(),
                    workspace_id: Some(ws.id.clone()),
                    title: "Card A".into(),
                    description: String::new(),
                    column: KanbanColumn::InProgress,
                    order: 0,
                    created_at: 0,
                    updated_at: 0,
                    pic_names: Vec::new(),
                },
            );
        }
        let provider: Arc<dyn crate::task_provider::TaskProvider> = Arc::new(BlankRepoIdProvider);
        let updated = move_task_inner(
            "tk_a".into(),
            KanbanColumn::Todo,
            0,
            data.clone(),
            provider,
            Arc::clone(&state),
        )
        .await
        .unwrap();
        assert_eq!(
            updated.workspace_id.as_deref(),
            Some(ws.id.as_str()),
            "non-empty ws kept + linked"
        );
        let st = state.lock().unwrap();
        assert_eq!(st.workspaces.len(), 1, "non-empty workspace not removed");
    }

    // ── Task 3: refresh_tasks preservation tests ────────────────────

    /// Provider that returns a single task with workspace_id=None on
    /// list_tasks — mimics LarkProvider re-hydrating a card whose local-only
    /// link it cannot know.
    #[derive(Debug)]
    struct OneBlankTaskProvider;

    #[async_trait::async_trait]
    impl crate::task_provider::TaskProvider for OneBlankTaskProvider {
        async fn list_tasks(
            &self,
            _: Option<&str>,
        ) -> crate::error::Result<Vec<crate::state::Task>> {
            Ok(vec![crate::state::Task {
                id: "tk_a".into(),
                repo_id: String::new(),
                workspace_id: None,
                title: "Card A".into(),
                description: String::new(),
                column: KanbanColumn::InProgress,
                order: 0,
                created_at: 0,
                updated_at: 0,
                pic_names: Vec::new(),
            }])
        }
        async fn create_task(
            &self,
            a: crate::task_provider::CreateTaskArgs,
        ) -> crate::error::Result<crate::state::Task> {
            Ok(crate::state::Task {
                id: "x".into(),
                repo_id: String::new(),
                workspace_id: None,
                title: a.title,
                description: a.description,
                column: a.column.unwrap_or_default(),
                order: 0,
                created_at: 0,
                updated_at: 0,
                pic_names: Vec::new(),
            })
        }
        async fn update_task(
            &self,
            id: &str,
            _p: crate::task_provider::TaskPatch,
        ) -> crate::error::Result<crate::state::Task> {
            Ok(crate::state::Task {
                id: id.into(),
                repo_id: String::new(),
                workspace_id: None,
                title: "t".into(),
                description: String::new(),
                column: KanbanColumn::Todo,
                order: 0,
                created_at: 0,
                updated_at: 0,
                pic_names: Vec::new(),
            })
        }
        async fn move_task(
            &self,
            id: &str,
            column: KanbanColumn,
            order: i32,
        ) -> crate::error::Result<crate::state::Task> {
            Ok(crate::state::Task {
                id: id.into(),
                repo_id: String::new(),
                workspace_id: None,
                title: "t".into(),
                description: String::new(),
                column,
                order,
                created_at: 0,
                updated_at: 0,
                pic_names: Vec::new(),
            })
        }
        async fn delete_task(&self, _id: &str) -> crate::error::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn refresh_tasks_preserves_workspace_id_link() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());
        let repo_id = { state.lock().unwrap().repos.keys().next().unwrap().clone() };
        {
            let mut st = state.lock().unwrap();
            st.tasks.insert(
                "tk_a".into(),
                crate::state::Task {
                    id: "tk_a".into(),
                    repo_id: repo_id.clone(),
                    workspace_id: Some("ws_keep".into()),
                    title: "Card A".into(),
                    description: String::new(),
                    column: KanbanColumn::InProgress,
                    order: 0,
                    created_at: 0,
                    updated_at: 0,
                    pic_names: Vec::new(),
                },
            );
        }
        let provider: Arc<dyn crate::task_provider::TaskProvider> = Arc::new(OneBlankTaskProvider);
        refresh_tasks_inner(Some(repo_id.clone()), Arc::clone(&state), provider)
            .await
            .unwrap();
        let st = state.lock().unwrap();
        let t = st.tasks.get("tk_a").unwrap();
        assert_eq!(
            t.workspace_id.as_deref(),
            Some("ws_keep"),
            "workspace_id must survive refresh"
        );
        assert_eq!(t.repo_id, repo_id, "repo_id must survive refresh");
    }

    #[tokio::test]
    async fn update_task_preserves_repo_id_and_workspace_id() {
        let tmp = tempdir().unwrap();
        let state = make_state_with_repo(tmp.path());
        {
            let mut st = state.lock().unwrap();
            st.tasks.insert(
                "tk_a".into(),
                crate::state::Task {
                    id: "tk_a".into(),
                    repo_id: "repo_r1".into(),
                    workspace_id: Some("ws_existing".into()),
                    title: "Card A".into(),
                    description: String::new(),
                    column: KanbanColumn::Todo,
                    order: 0,
                    created_at: 0,
                    updated_at: 0,
                    pic_names: Vec::new(),
                },
            );
        }
        let provider: Arc<dyn crate::task_provider::TaskProvider> = Arc::new(BlankRepoIdProvider);
        let updated = update_task_inner(
            "tk_a".into(),
            TaskPatch {
                title: Some("New title".into()),
                description: None,
                order: None,
            },
            Arc::clone(&provider),
            Arc::clone(&state),
        )
        .await
        .unwrap();
        assert_eq!(updated.repo_id, "repo_r1");
        assert_eq!(updated.workspace_id.as_deref(), Some("ws_existing"));
    }

    // ── Task 4 (multi-card workspace): unlink_task_from_workspace ───

    /// Test fixture used by the multi-card workspace tests
    /// (`unlink_*`, `link_*`, `move_to_todo_with_refcount_*`,
    /// `remove_task_*`). Builds:
    ///
    /// 1. A real on-disk bare git repo + local clone (so
    ///    `is_workspace_empty` can shell out to git).
    /// 2. An `AppState` with one repo registered under id `"repo_a"` whose
    ///    `path` is the cloned local working dir.
    /// 3. A real workspace (via `create_workspace_inner` — runs
    ///    `git worktree add`) renamed in-state to `ws_id` so test
    ///    assertions can address it by the stable identifier (e.g.
    ///    `"ws_a"`). The disk worktree path is preserved on the in-state
    ///    entry, so `is_workspace_empty` / `remove_workspace_inner` both
    ///    operate on the real on-disk worktree.
    /// 4. One linked card with id `initial_task_id` (e.g. `"tk_a"`) in
    ///    the kanban `InProgress` column with `workspace_id = ws_id`,
    ///    appearing in `ws.task_ids`.
    ///
    /// Returned tuple: `(data_dir, state)`. The `TempDir` containing both
    /// the repo and the data_dir is leaked (via `into_path`) so the test
    /// can hold paths to a still-existing on-disk fixture for the full
    /// test lifetime without juggling a `TempDir` guard.
    ///
    /// Adapted from the existing PR #32 helpers
    /// `init_bare_repo_for_task_tests` + `make_state_with_real_repo` in
    /// this same tests module, with an added `create_workspace_inner`
    /// call (mirroring the `move_to_todo_*` lifecycle tests in this
    /// file) and an in-state rename so the workspace key matches the
    /// caller-supplied `ws_id`.
    async fn setup_state_with_repo_and_workspace(
        ws_id: &str,
        initial_task_id: &str,
    ) -> (PathBuf, Arc<Mutex<AppState>>) {
        let tmp = tempdir().unwrap();
        let local = init_bare_repo_for_task_tests(&tmp);
        let data = tmp.path().join("data_mcw");
        std::fs::create_dir_all(&data).unwrap();

        // Build the AppState with repo registered under id "repo_a"
        // (matches the plan test fixtures).
        let mut state = AppState::default();
        state.repos.insert(
            "repo_a".into(),
            RepoInfo {
                id: "repo_a".into(),
                name: "my-repo".into(),
                path: local,
                gh_profile: None,
                default_branch: "main".into(),
                created_at: 0,
                updated_at: 0,
                scripts: Vec::new(),
            },
        );
        let state = Arc::new(Mutex::new(state));

        let created = crate::commands::workspace::create_workspace_inner(
            "repo_a".into(),
            ws_id.into(),
            String::new(),
            None,
            data.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap();

        // Rename the in-state entry so it's addressable by the
        // caller-supplied stable id (the on-disk worktree path is
        // preserved — only the HashMap key and `id` field move).
        {
            let mut st = state.lock().unwrap();
            let mut ws = st.workspaces.remove(&created.id).unwrap();
            ws.id = ws_id.into();
            ws.task_ids = vec![initial_task_id.into()];
            st.workspaces.insert(ws_id.into(), ws);
            st.tasks.insert(
                initial_task_id.into(),
                Task {
                    id: initial_task_id.into(),
                    repo_id: "repo_a".into(),
                    workspace_id: Some(ws_id.into()),
                    title: initial_task_id.into(),
                    description: String::new(),
                    column: KanbanColumn::InProgress,
                    order: 0,
                    created_at: 0,
                    updated_at: 0,
                    pic_names: Vec::new(),
                },
            );
        }

        // Leak the TempDir guard — the test owns the fixture for its
        // full lifetime and we don't want premature cleanup yanking the
        // worktree out from under `is_workspace_empty`.
        let _kept = tmp.keep();
        (data, state)
    }

    fn seed_task(
        state: &Arc<Mutex<AppState>>,
        task_id: &str,
        repo_id: &str,
        workspace_id: Option<String>,
    ) {
        let mut st = state.lock().unwrap();
        st.tasks.insert(
            task_id.into(),
            Task {
                id: task_id.into(),
                repo_id: repo_id.into(),
                workspace_id,
                title: task_id.into(),
                description: String::new(),
                column: KanbanColumn::Todo,
                order: 0,
                created_at: 0,
                updated_at: 0,
                pic_names: Vec::new(),
            },
        );
    }

    #[tokio::test]
    async fn unlink_force_with_refcount_gt_1_keeps_workspace_and_returns_unlinked() {
        let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
        // Seed a second card linked to the same workspace.
        seed_task(&state, "tk_b", "repo_a", Some("ws_a".into()));
        state
            .lock()
            .unwrap()
            .workspaces
            .get_mut("ws_a")
            .unwrap()
            .task_ids = vec!["tk_a".into(), "tk_b".into()];

        let r = unlink_task_from_workspace_inner("tk_b", true, data_dir, Arc::clone(&state))
            .await
            .unwrap();
        assert_eq!(r, UnlinkResult::Unlinked);

        let st = state.lock().unwrap();
        let ws = st.workspaces.get("ws_a").unwrap();
        assert_eq!(ws.task_ids, vec!["tk_a".to_string()]);
        let tk = st.tasks.get("tk_b").unwrap();
        assert!(tk.workspace_id.is_none());
    }

    #[tokio::test]
    async fn unlink_force_with_refcount_1_and_empty_workspace_removes_workspace() {
        // Setup helper creates ws_a with one linked card and a fresh,
        // empty real worktree (no commits ahead, clean tree, no chat,
        // no agent).
        let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
        let r = unlink_task_from_workspace_inner("tk_a", true, data_dir, Arc::clone(&state))
            .await
            .unwrap();
        assert_eq!(r, UnlinkResult::Removed);
        let st = state.lock().unwrap();
        assert!(!st.workspaces.contains_key("ws_a"));
    }

    #[tokio::test]
    async fn unlink_force_with_refcount_1_and_non_empty_workspace_keeps_workspace() {
        let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
        // Dirty the worktree so is_workspace_empty returns false.
        let wt = state
            .lock()
            .unwrap()
            .workspaces
            .get("ws_a")
            .unwrap()
            .worktree_dir
            .clone();
        std::fs::write(wt.join("DIRTY.md"), b"x").unwrap();

        let r = unlink_task_from_workspace_inner("tk_a", true, data_dir, Arc::clone(&state))
            .await
            .unwrap();
        assert_eq!(r, UnlinkResult::Unlinked);

        let st = state.lock().unwrap();
        assert!(st.workspaces.contains_key("ws_a"));
        let ws = st.workspaces.get("ws_a").unwrap();
        assert!(ws.task_ids.is_empty());
        let tk = st.tasks.get("tk_a").unwrap();
        assert!(tk.workspace_id.is_none());
    }

    #[tokio::test]
    async fn unlink_preview_returns_would_remove_for_last_empty_link() {
        let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
        let r = unlink_task_from_workspace_inner(
            "tk_a",
            /* force = */ false,
            data_dir,
            Arc::clone(&state),
        )
        .await
        .unwrap();
        match r {
            UnlinkResult::WouldRemove { workspace_title } => {
                assert_eq!(workspace_title, "ws_a");
            }
            other => panic!("expected WouldRemove, got {other:?}"),
        }
        // Preview MUST NOT mutate.
        let st = state.lock().unwrap();
        let ws = st.workspaces.get("ws_a").unwrap();
        assert_eq!(ws.task_ids, vec!["tk_a".to_string()]);
        let tk = st.tasks.get("tk_a").unwrap();
        assert_eq!(tk.workspace_id.as_deref(), Some("ws_a"));
    }

    #[tokio::test]
    async fn unlink_preview_returns_unlinked_when_no_cleanup_would_fire() {
        let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
        seed_task(&state, "tk_b", "repo_a", Some("ws_a".into()));
        state
            .lock()
            .unwrap()
            .workspaces
            .get_mut("ws_a")
            .unwrap()
            .task_ids = vec!["tk_a".into(), "tk_b".into()];

        let r = unlink_task_from_workspace_inner("tk_b", false, data_dir, Arc::clone(&state))
            .await
            .unwrap();
        // refcount > 1 → no cleanup would fire; preview returns Unlinked
        // (the UI uses this as "safe to execute immediately, no modal").
        // Per the plan, when `would_remove` is false the inner falls
        // through to mutate even with `force = false` — the UI only
        // skips the call entirely when it doesn't want a mutation.
        assert_eq!(r, UnlinkResult::Unlinked);
    }

    #[tokio::test]
    async fn unlink_unlinked_task_is_noop_unlinked() {
        let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
        seed_task(&state, "tk_loose", "repo_a", None);
        let r = unlink_task_from_workspace_inner("tk_loose", true, data_dir, state)
            .await
            .unwrap();
        assert_eq!(r, UnlinkResult::Unlinked);
    }

    // ── Task 3 (multi-card workspace): link_task_to_workspace ────────

    /// Creates a real on-disk workspace (via `create_workspace_inner`,
    /// which runs `git worktree add`) in the existing `repo_a` registered
    /// by `setup_state_with_repo_and_workspace`, then renames the in-state
    /// entry so it is addressable by the stable id `ws_id`. Mirrors the
    /// rename trick used by `setup_state_with_repo_and_workspace` so the
    /// on-disk worktree (used by `is_workspace_empty` for the atomic
    /// switch cleanup) is preserved.
    async fn seed_workspace(
        state: &Arc<Mutex<AppState>>,
        data_dir: &std::path::Path,
        ws_id: &str,
        repo_id: &str,
    ) {
        let created = crate::commands::workspace::create_workspace_inner(
            repo_id.into(),
            ws_id.into(),
            String::new(),
            None,
            data_dir.to_path_buf(),
            Arc::clone(state),
        )
        .await
        .unwrap();
        let mut st = state.lock().unwrap();
        let mut ws = st.workspaces.remove(&created.id).unwrap();
        ws.id = ws_id.into();
        st.workspaces.insert(ws_id.into(), ws);
    }

    #[tokio::test]
    async fn link_task_to_workspace_attaches_card_and_appends_task_id() {
        let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
        // Seed a second card in the same repo, currently unlinked.
        seed_task(&state, "tk_b", "repo_a", None);
        link_task_to_workspace_inner("tk_b", "ws_a", data_dir.clone(), Arc::clone(&state))
            .await
            .unwrap();
        let st = state.lock().unwrap();
        let ws = st.workspaces.get("ws_a").unwrap();
        assert!(ws.task_ids.contains(&"tk_a".to_string()));
        assert!(ws.task_ids.contains(&"tk_b".to_string()));
        let tk = st.tasks.get("tk_b").unwrap();
        assert_eq!(tk.workspace_id.as_deref(), Some("ws_a"));
    }

    #[tokio::test]
    async fn link_task_to_workspace_is_idempotent_for_already_linked_card() {
        let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
        // tk_a is already in ws_a from setup.
        let before = state
            .lock()
            .unwrap()
            .workspaces
            .get("ws_a")
            .unwrap()
            .task_ids
            .clone();
        link_task_to_workspace_inner("tk_a", "ws_a", data_dir, Arc::clone(&state))
            .await
            .unwrap();
        let after = state
            .lock()
            .unwrap()
            .workspaces
            .get("ws_a")
            .unwrap()
            .task_ids
            .clone();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn link_task_to_workspace_atomically_switches_from_other_workspace() {
        let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
        // Seed a second workspace with a real on-disk worktree + link a
        // card to it. The real worktree is required so the unlink-driven
        // cleanup (refcount=0 + empty) actually fires on ws_b.
        seed_workspace(&state, &data_dir, "ws_b", "repo_a").await;
        seed_task(&state, "tk_b", "repo_a", Some("ws_b".into()));
        state
            .lock()
            .unwrap()
            .workspaces
            .get_mut("ws_b")
            .unwrap()
            .task_ids = vec!["tk_b".into()];

        link_task_to_workspace_inner("tk_b", "ws_a", data_dir, Arc::clone(&state))
            .await
            .unwrap();

        let st = state.lock().unwrap();
        assert!(st
            .workspaces
            .get("ws_a")
            .unwrap()
            .task_ids
            .contains(&"tk_b".to_string()));
        // ws_b lost the link AND was removed (refcount=0 + empty, per
        // cleanup rule).
        assert!(!st.workspaces.contains_key("ws_b"));
        let tk = st.tasks.get("tk_b").unwrap();
        assert_eq!(tk.workspace_id.as_deref(), Some("ws_a"));
    }

    #[tokio::test]
    async fn link_task_to_workspace_rejects_cross_repo() {
        let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
        seed_task(&state, "tk_x", "repo_other", None);
        let err = link_task_to_workspace_inner("tk_x", "ws_a", data_dir, state)
            .await
            .unwrap_err();
        assert!(err.to_string().to_lowercase().contains("repo"));
    }

    #[tokio::test]
    async fn link_task_to_workspace_rejects_missing_task_or_workspace() {
        let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
        let err = link_task_to_workspace_inner(
            "tk_missing",
            "ws_a",
            data_dir.clone(),
            Arc::clone(&state),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("tk_missing"));

        let err = link_task_to_workspace_inner("tk_a", "ws_missing", data_dir, state)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("ws_missing"));
    }

    // ── Task 5 (multi-card workspace): move_task_inner sticky refcount ───

    /// When two cards share a workspace (refcount=2), moving one to Todo
    /// must NOT remove the workspace or break any link. Only the column
    /// of the moved card changes. (spec §4: refcount > 1 → sticky)
    #[tokio::test]
    async fn move_to_todo_with_refcount_gt_1_keeps_workspace_and_link() {
        // Setup: ws_a linked to [tk_a, tk_b]; both in InProgress.
        let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
        // Seed a second card in the same repo linked to ws_a.
        {
            let mut st = state.lock().unwrap();
            st.tasks.insert(
                "tk_b".into(),
                Task {
                    id: "tk_b".into(),
                    repo_id: "repo_a".into(),
                    workspace_id: Some("ws_a".into()),
                    title: "tk_b".into(),
                    description: String::new(),
                    column: KanbanColumn::InProgress,
                    order: 1,
                    created_at: 0,
                    updated_at: 0,
                    pic_names: Vec::new(),
                },
            );
            st.workspaces
                .get_mut("ws_a")
                .unwrap()
                .task_ids
                .push("tk_b".into());
        }

        let provider: Arc<dyn crate::task_provider::TaskProvider> = Arc::new(BlankRepoIdProvider);
        let updated = move_task_inner(
            "tk_a".into(),
            KanbanColumn::Todo,
            0,
            data_dir.clone(),
            provider,
            Arc::clone(&state),
        )
        .await
        .unwrap();

        // Column changed.
        assert_eq!(
            updated.column,
            KanbanColumn::Todo,
            "column must change to Todo"
        );

        // Link preserved.
        assert_eq!(
            updated.workspace_id.as_deref(),
            Some("ws_a"),
            "tk_a.workspace_id must remain ws_a (sticky, refcount=2)"
        );

        let st = state.lock().unwrap();
        // Workspace still exists.
        assert!(
            st.workspaces.contains_key("ws_a"),
            "workspace ws_a must NOT be removed when refcount=2"
        );
        // Both task_ids still in ws.task_ids.
        let task_ids = &st.workspaces.get("ws_a").unwrap().task_ids;
        assert!(
            task_ids.contains(&"tk_a".to_string()),
            "tk_a must remain in ws_a.task_ids"
        );
        assert!(
            task_ids.contains(&"tk_b".to_string()),
            "tk_b must remain in ws_a.task_ids"
        );
    }

    /// Regression guard for PR #32: when the sole linked card moves to
    /// Todo and the workspace is empty, the workspace must still be removed
    /// and the link cleared. (spec §4: refcount=1 + empty → unlink + remove)
    #[tokio::test]
    async fn move_to_todo_with_refcount_1_and_empty_still_removes_workspace() {
        let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;

        let provider: Arc<dyn crate::task_provider::TaskProvider> = Arc::new(BlankRepoIdProvider);
        let updated = move_task_inner(
            "tk_a".into(),
            KanbanColumn::Todo,
            0,
            data_dir.clone(),
            provider,
            Arc::clone(&state),
        )
        .await
        .unwrap();

        // Link cleared.
        assert_eq!(
            updated.workspace_id, None,
            "workspace_id must be None after removing empty solo workspace"
        );
        // Workspace removed from state.
        let st = state.lock().unwrap();
        assert!(
            st.workspaces.is_empty(),
            "empty solo workspace must be removed from state"
        );
    }
}
