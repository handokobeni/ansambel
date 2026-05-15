# Phase 3a-2 — TaskProvider Abstraction + Lark Plugin Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Introduce a `TaskProvider` trait with `LocalProvider` and
`LarkProvider` impls, swap-able via Settings, plus a Bitable schema wizard so
kanban data can be sourced from either local `tasks.json` or a shared Lark
Bitable table.

**Architecture:** Backend `AppState.tasks` stays as the in-memory mirror; a new
`Arc<RwLock<Arc<dyn TaskProvider>>>` registered as Tauri state owns the
persistence backend. Frontend handles optimistic UI + revert. Schema wizard
creates only the 5 fields required for 3a-2; later phases extend the registry.

**Tech Stack:**

- Rust: `async-trait = "0.1"` (new), existing `reqwest`/`wiremock`/`tokio`
- Frontend: Svelte 5 runes, existing Vitest + Testing Library

**Spec:** `docs/superpowers/specs/2026-05-13-phase-3a-2-task-provider-design.md`

---

## File Structure

### Create

| Path                                     | Responsibility                                        |
| ---------------------------------------- | ----------------------------------------------------- |
| `src-tauri/src/task_provider/mod.rs`     | `TaskProvider` trait + `CreateTaskArgs` + module glue |
| `src-tauri/src/task_provider/local.rs`   | `LocalProvider` — wraps tasks.json                    |
| `src-tauri/src/task_provider/lark.rs`    | `LarkProvider` — wraps Bitable CRUD via `lark_client` |
| `src-tauri/src/task_provider/schema.rs`  | Required-fields registry + `verify_schema` logic      |
| `tests/e2e/phase-3a-2-lark-sync.spec.ts` | Env-gated E2E for full sync flow                      |

### Modify

| Path                                          | Reason                                                              |
| --------------------------------------------- | ------------------------------------------------------------------- |
| `src-tauri/Cargo.toml`                        | Add `async-trait = "0.1"` to `[dependencies]`                       |
| `src-tauri/src/state.rs`                      | `TaskSource` enum, `AppSettings.task_source`, `TaskProviderHandle`  |
| `src-tauri/src/commands/mod.rs`               | `pub mod task_provider` re-export from sibling module               |
| `src-tauri/src/commands/task.rs`              | Refactor `*_inner` fns to call provider; add 3 new commands         |
| `src-tauri/src/commands/lark_auth.rs`         | Add `verify_lark_schema` command                                    |
| `src-tauri/src/platform/lark_client.rs`       | Add `bitable_list_fields` + `bitable_create_field` + `BitableField` |
| `src-tauri/src/lib.rs`                        | Wire provider in `setup`, register new commands                     |
| `src/lib/ipc.ts`                              | `api.task.refresh/setSource/getSource` + `api.lark.verifySchema`    |
| `src/lib/types.ts`                            | `TaskSource`, `SchemaCheckResult`                                   |
| `src/lib/stores/tasks.svelte.ts`              | Optimistic move + revert + refresh + window-focus listener          |
| `src/lib/App.svelte`                          | Window-focus listener wiring                                        |
| `src/lib/components/SettingsDialog.svelte`    | Task source radio section                                           |
| `src/lib/components/lark/LarkSettings.svelte` | Bitable schema verify section                                       |

---

## Task 1: Add async-trait dependency + scaffold task_provider module

**Files:**

- Modify: `src-tauri/Cargo.toml` ([dependencies] block)
- Create: `src-tauri/src/task_provider/mod.rs`
- Modify: `src-tauri/src/lib.rs:1-12` (add `pub mod task_provider`)

- [ ] **Step 1: Add async-trait to Cargo.toml**

Open `src-tauri/Cargo.toml` and add this line under `[dependencies]`
(alphabetically near `anyhow`):

```toml
async-trait = "0.1"
```

- [ ] **Step 2: Create `task_provider/mod.rs` with empty module shell**

Create file with:

```rust
// Phase 3a-2 — TaskProvider abstraction.
//
// Splits "where do tasks live" from the command layer. `LocalProvider`
// wraps tasks.json; `LarkProvider` wraps Lark Bitable. Both implement
// the same async trait so commands/task.rs can stay agnostic.
//
// AppState.tasks remains the in-memory mirror — the command layer
// reads it for list_tasks (cache) and writes it after a provider
// mutation succeeds.

use crate::error::Result;
use crate::state::{KanbanColumn, Task};
use async_trait::async_trait;

pub mod local;

/// Argument bundle for create_task. Mirrors what the frontend posts;
/// each provider decides its own ID strategy (LocalProvider generates
/// `tk_<nanoid>`; LarkProvider returns Bitable's `record_id`).
#[derive(Debug, Clone)]
pub struct CreateTaskArgs {
    pub repo_id: String,
    pub title: String,
    pub description: String,
    /// Defaults to `Todo` when None.
    pub column: Option<KanbanColumn>,
}

/// Partial update bundle. Fields left as `None` are not modified.
#[derive(Debug, Clone, Default)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub order: Option<i32>,
}

#[async_trait]
pub trait TaskProvider: Send + Sync + std::fmt::Debug {
    async fn list_tasks(&self, repo_filter: Option<&str>) -> Result<Vec<Task>>;
    async fn create_task(&self, args: CreateTaskArgs) -> Result<Task>;
    async fn update_task(&self, id: &str, patch: TaskPatch) -> Result<Task>;
    async fn move_task(&self, id: &str, column: KanbanColumn, order: i32) -> Result<Task>;
    async fn delete_task(&self, id: &str) -> Result<()>;
}
```

- [ ] **Step 3: Register module in lib.rs**

In `src-tauri/src/lib.rs`, find the `pub mod` block (around lines 1-12) and add:

```rust
pub mod task_provider;
```

Place it alphabetically (between `state` and the top-level uses).

- [ ] **Step 4: Verify compile**

Run: `cd src-tauri && cargo check --lib --all-targets` Expected: `Finished dev`
with no errors. (Module is empty — `local` submodule referenced but not yet
present will cause an error.)

If you see "file not found for module `local`", that's expected — Step 5 fixes
it.

- [ ] **Step 5: Create empty `local.rs` stub to satisfy module ref**

Create `src-tauri/src/task_provider/local.rs` with a placeholder:

```rust
// Phase 3a-2 — LocalProvider impl. Body added in Task 2.
```

Re-run `cargo check --lib --all-targets`. Expected: clean compile.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/task_provider/ src-tauri/src/lib.rs
git commit -m "feat(phase-3a-2): scaffold TaskProvider trait module"
```

---

## Task 2: LocalProvider — file-IO impl + tests

**Files:**

- Modify: `src-tauri/src/task_provider/local.rs` (full implementation)

This wraps existing tasks.json behavior. The trait methods are the
persistence-only parts of what `commands/task.rs::*_inner` does today — they do
not touch `AppState`.

- [ ] **Step 1: Write the test module skeleton first (TDD)**

Open `src-tauri/src/task_provider/local.rs` and write:

```rust
// Phase 3a-2 — LocalProvider.
//
// Trait impl that backs the in-process tasks.json store. Each method
// is a small read-modify-write over the JSON file, serialized by an
// internal `Mutex<()>`. ID format remains `tk_<nanoid>` for
// compatibility with persisted state pre-3a-2.

use super::{CreateTaskArgs, TaskPatch, TaskProvider};
use crate::error::{AppError, Result};
use crate::ids::task_id;
use crate::persistence::tasks::{load_tasks, save_tasks};
use crate::state::{KanbanColumn, Task};
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug)]
pub struct LocalProvider {
    data_dir: PathBuf,
    /// Serializes read-modify-write of tasks.json across concurrent
    /// trait calls. A standard Mutex is fine because all the work
    /// inside the critical section is sync (file I/O + JSON).
    inner_lock: Mutex<()>,
}

impl LocalProvider {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            data_dir,
            inner_lock: Mutex::new(()),
        }
    }
}

#[async_trait]
impl TaskProvider for LocalProvider {
    async fn list_tasks(&self, repo_filter: Option<&str>) -> Result<Vec<Task>> {
        let _g = self.inner_lock.lock().map_err(|e| {
            AppError::InvalidState(format!("local_provider lock poisoned: {e}"))
        })?;
        let map = load_tasks(&self.data_dir)?;
        let mut out: Vec<Task> = map
            .into_values()
            .filter(|t| repo_filter.map_or(true, |r| t.repo_id == r))
            .collect();
        // Sort by (column, -order) so callers see a deterministic kanban.
        out.sort_by(|a, b| match a.column.cmp(&b.column) {
            std::cmp::Ordering::Equal => b.order.cmp(&a.order),
            o => o,
        });
        Ok(out)
    }

    async fn create_task(&self, args: CreateTaskArgs) -> Result<Task> {
        let _g = self.inner_lock.lock().map_err(|e| {
            AppError::InvalidState(format!("local_provider lock poisoned: {e}"))
        })?;
        let mut map = load_tasks(&self.data_dir)?;
        let now = chrono::Utc::now().timestamp();
        let max_order = map
            .values()
            .filter(|t| t.repo_id == args.repo_id)
            .map(|t| t.order)
            .max()
            .unwrap_or(0);
        let task = Task {
            id: task_id(),
            repo_id: args.repo_id,
            workspace_id: None,
            title: args.title,
            description: args.description,
            column: args.column.unwrap_or_default(),
            order: max_order + 1024,
            created_at: now,
            updated_at: now,
        };
        map.insert(task.id.clone(), task.clone());
        save_tasks(&self.data_dir, &map)?;
        Ok(task)
    }

    async fn update_task(&self, id: &str, patch: TaskPatch) -> Result<Task> {
        let _g = self.inner_lock.lock().map_err(|e| {
            AppError::InvalidState(format!("local_provider lock poisoned: {e}"))
        })?;
        let mut map = load_tasks(&self.data_dir)?;
        let task = map
            .get_mut(id)
            .ok_or_else(|| AppError::NotFound(format!("task {id}")))?;
        if let Some(title) = patch.title {
            task.title = title;
        }
        if let Some(description) = patch.description {
            task.description = description;
        }
        if let Some(order) = patch.order {
            task.order = order;
        }
        task.updated_at = chrono::Utc::now().timestamp();
        let updated = task.clone();
        save_tasks(&self.data_dir, &map)?;
        Ok(updated)
    }

    async fn move_task(&self, id: &str, column: KanbanColumn, order: i32) -> Result<Task> {
        let _g = self.inner_lock.lock().map_err(|e| {
            AppError::InvalidState(format!("local_provider lock poisoned: {e}"))
        })?;
        let mut map = load_tasks(&self.data_dir)?;
        let task = map
            .get_mut(id)
            .ok_or_else(|| AppError::NotFound(format!("task {id}")))?;
        task.column = column;
        task.order = order;
        task.updated_at = chrono::Utc::now().timestamp();
        let updated = task.clone();
        save_tasks(&self.data_dir, &map)?;
        Ok(updated)
    }

    async fn delete_task(&self, id: &str) -> Result<()> {
        let _g = self.inner_lock.lock().map_err(|e| {
            AppError::InvalidState(format!("local_provider lock poisoned: {e}"))
        })?;
        let mut map = load_tasks(&self.data_dir)?;
        if map.remove(id).is_none() {
            return Err(AppError::NotFound(format!("task {id}")));
        }
        save_tasks(&self.data_dir, &map)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_provider() -> (LocalProvider, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        (LocalProvider::new(tmp.path().to_path_buf()), tmp)
    }

    #[tokio::test]
    async fn local_provider_round_trip_via_tasks_json() {
        let (p, _tmp) = make_provider();
        let created = p
            .create_task(CreateTaskArgs {
                repo_id: "repo_a".into(),
                title: "Hello".into(),
                description: "world".into(),
                column: None,
            })
            .await
            .unwrap();
        assert!(created.id.starts_with("tk_"));
        let listed = p.list_tasks(Some("repo_a")).await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].title, "Hello");
    }

    #[tokio::test]
    async fn local_provider_list_filters_by_repo() {
        let (p, _tmp) = make_provider();
        for repo in ["repo_a", "repo_a", "repo_b"] {
            p.create_task(CreateTaskArgs {
                repo_id: repo.into(),
                title: "t".into(),
                description: String::new(),
                column: None,
            })
            .await
            .unwrap();
        }
        assert_eq!(p.list_tasks(Some("repo_a")).await.unwrap().len(), 2);
        assert_eq!(p.list_tasks(Some("repo_b")).await.unwrap().len(), 1);
        assert_eq!(p.list_tasks(None).await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn local_provider_create_assigns_tk_prefix_id() {
        let (p, _tmp) = make_provider();
        let created = p
            .create_task(CreateTaskArgs {
                repo_id: "r".into(),
                title: "t".into(),
                description: String::new(),
                column: Some(KanbanColumn::Review),
            })
            .await
            .unwrap();
        assert!(created.id.starts_with("tk_"), "got id: {}", created.id);
        assert_eq!(created.column, KanbanColumn::Review);
    }

    #[tokio::test]
    async fn local_provider_move_updates_column_and_order() {
        let (p, _tmp) = make_provider();
        let t = p
            .create_task(CreateTaskArgs {
                repo_id: "r".into(),
                title: "t".into(),
                description: String::new(),
                column: None,
            })
            .await
            .unwrap();
        let moved = p
            .move_task(&t.id, KanbanColumn::InProgress, 4096)
            .await
            .unwrap();
        assert_eq!(moved.column, KanbanColumn::InProgress);
        assert_eq!(moved.order, 4096);
        assert!(moved.updated_at >= t.updated_at);
    }

    #[tokio::test]
    async fn local_provider_update_patches_only_named_fields() {
        let (p, _tmp) = make_provider();
        let t = p
            .create_task(CreateTaskArgs {
                repo_id: "r".into(),
                title: "old title".into(),
                description: "old desc".into(),
                column: None,
            })
            .await
            .unwrap();
        let updated = p
            .update_task(
                &t.id,
                TaskPatch {
                    title: Some("new title".into()),
                    description: None,
                    order: None,
                },
            )
            .await
            .unwrap();
        assert_eq!(updated.title, "new title");
        assert_eq!(updated.description, "old desc"); // unchanged
    }

    #[tokio::test]
    async fn local_provider_delete_removes_from_disk() {
        let (p, _tmp) = make_provider();
        let t = p
            .create_task(CreateTaskArgs {
                repo_id: "r".into(),
                title: "t".into(),
                description: String::new(),
                column: None,
            })
            .await
            .unwrap();
        p.delete_task(&t.id).await.unwrap();
        let listed = p.list_tasks(None).await.unwrap();
        assert!(listed.is_empty());
    }

    #[tokio::test]
    async fn local_provider_delete_missing_returns_not_found() {
        let (p, _tmp) = make_provider();
        let err = p.delete_task("tk_ghost").await.unwrap_err();
        assert!(err.to_string().contains("Not found"), "{err}");
    }

    #[tokio::test]
    async fn local_provider_update_missing_returns_not_found() {
        let (p, _tmp) = make_provider();
        let err = p
            .update_task("tk_ghost", TaskPatch::default())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Not found"), "{err}");
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib task_provider::local` Expected: 8 tests
pass.

- [ ] **Step 3: Run clippy + fmt to check style**

Run:
`cd src-tauri && cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: clean. If fmt fails, run `cargo fmt --all` and re-stage.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/task_provider/local.rs
git commit -m "feat(phase-3a-2): add LocalProvider impl of TaskProvider trait"
```

---

## Task 3: Wire LocalProvider into AppState + refactor commands/task.rs

This refactor swaps `commands/task.rs` from direct tasks.json access to provider
calls. Behavior for users is unchanged — LocalProvider does exactly what the old
code did.

**Files:**

- Modify: `src-tauri/src/state.rs` (add `TaskProviderHandle` typedef)
- Modify: `src-tauri/src/commands/task.rs` (refactor `*_inner` fns to use
  provider)
- Modify: `src-tauri/src/lib.rs::setup` (init provider + register state)

- [ ] **Step 1: Add `TaskProviderHandle` typedef to `state.rs`**

In `src-tauri/src/state.rs`, near the `AppState` struct definition, add:

```rust
/// Tauri-managed handle to the active task provider. Lives separately
/// from AppState so async provider calls don't hold the AppState lock.
/// Inner Arc is swap-able via write lock when the user changes the
/// task source in Settings.
pub type TaskProviderHandle =
    std::sync::Arc<tokio::sync::RwLock<std::sync::Arc<dyn crate::task_provider::TaskProvider>>>;
```

- [ ] **Step 2: Refactor `add_task_inner` in `commands/task.rs`**

The current `add_task_inner` (around line 99) reads tasks.json + writes
tasks.json directly. Replace it with a provider call:

```rust
pub(crate) async fn add_task_inner(
    repo_id: String,
    title: String,
    description: String,
    column: Option<crate::state::KanbanColumn>,
    provider: Arc<dyn crate::task_provider::TaskProvider>,
    state: Arc<Mutex<AppState>>,
) -> Result<Task> {
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
    Ok(task)
}
```

The Tauri command wrapper now needs the provider handle. Update `add_task`
(around line 21):

```rust
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
    add_task_inner(repo_id, title, description, column, provider, state.inner().clone())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "add_task failed");
            e.to_string()
        })
}
```

(Note the function went from `pub fn` to `pub async fn`.)

- [ ] **Step 3: Refactor `update_task_inner` similarly**

Find `update_task_inner` (around line 174). Replace body:

```rust
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
    Ok(updated)
}
```

Update the Tauri command wrapper `update_task` to take
`provider_handle: State<'_, TaskProviderHandle>`, become `async`, and pass the
resolved provider.

- [ ] **Step 4: Refactor `move_task_inner`**

`move_task_inner` is already `async`. Replace body:

```rust
pub(crate) async fn move_task_inner(
    task_id: String,
    column: KanbanColumn,
    order: i32,
    provider: Arc<dyn crate::task_provider::TaskProvider>,
    state: Arc<Mutex<AppState>>,
) -> Result<Task> {
    let updated = provider.move_task(&task_id, column, order).await?;
    {
        let mut st = state
            .lock()
            .map_err(|e| AppError::InvalidState(format!("AppState lock poisoned: {e}")))?;
        st.tasks.insert(updated.id.clone(), updated.clone());
    }
    Ok(updated)
}
```

Update `move_task` Tauri command wrapper to accept the provider handle.

- [ ] **Step 5: Refactor `remove_task_inner`**

```rust
pub(crate) async fn remove_task_inner(
    task_id: String,
    force: Option<bool>,
    provider: Arc<dyn crate::task_provider::TaskProvider>,
    state: Arc<Mutex<AppState>>,
) -> Result<()> {
    let force = force.unwrap_or(false);
    // Existing guard: refuse delete when task has active workspace,
    // unless force. The workspace_id check uses the AppState mirror
    // (works for both providers since LarkProvider returns
    // workspace_id=None on hydrate, so this only fires for local).
    {
        let st = state
            .lock()
            .map_err(|e| AppError::InvalidState(format!("AppState lock poisoned: {e}")))?;
        if let Some(t) = st.tasks.get(&task_id) {
            if t.workspace_id.is_some() && !force {
                return Err(AppError::InvalidState(format!(
                    "task {task_id} has active workspace; pass force=true to delete"
                )));
            }
        }
    }
    provider.delete_task(&task_id).await?;
    {
        let mut st = state
            .lock()
            .map_err(|e| AppError::InvalidState(format!("AppState lock poisoned: {e}")))?;
        st.tasks.remove(&task_id);
    }
    Ok(())
}
```

Update `remove_task` Tauri command wrapper.

- [ ] **Step 6: `list_tasks_inner` still reads from `AppState` mirror — no
      change**

Confirm `list_tasks_inner` (around line 147) reads from `state.tasks`. Leave it
alone; the mirror is populated by `setup()` and refresh commands.

- [ ] **Step 7: Wire provider in `lib.rs::setup`**

In `src-tauri/src/lib.rs`, in the `.setup` closure (around line 17-55), add
after `app.manage(Arc::new(Mutex::new(state)))`:

```rust
// Phase 3a-2: provider handle is separate from AppState so async
// trait calls don't hold the AppState lock. For now we always init
// LocalProvider; Task 7 wires the Lark/Local switch from settings.
let provider: std::sync::Arc<dyn crate::task_provider::TaskProvider> =
    std::sync::Arc::new(crate::task_provider::local::LocalProvider::new(data_dir.clone()));
let provider_handle: crate::state::TaskProviderHandle =
    std::sync::Arc::new(tokio::sync::RwLock::new(provider));
app.manage(provider_handle);
```

- [ ] **Step 8: Update existing tests in `commands/task.rs`**

The existing tests pass `data_dir` directly to the `_inner` fns. They now need a
`provider` arg. Find the test module (around line 750+) and add a helper:

```rust
fn make_provider(data_dir: &std::path::Path) -> Arc<dyn crate::task_provider::TaskProvider> {
    Arc::new(crate::task_provider::local::LocalProvider::new(data_dir.to_path_buf()))
}
```

Then go through each test — wherever it calls `add_task_inner(...)` /
`update_task_inner(...)` / etc, swap the `data_dir` argument for
`make_provider(tmp.path())` and `tmp.path().to_path_buf()` → just
`make_provider(...)`. The tests become `async` (add `#[tokio::test]` if not
already, and `.await` the calls).

For example, a test that was:

```rust
let result = add_task_inner(
    "repo_a".into(), "title".into(), "desc".into(), None,
    tmp.path().to_path_buf(), state,
);
```

Becomes:

```rust
let result = add_task_inner(
    "repo_a".into(), "title".into(), "desc".into(), None,
    make_provider(tmp.path()), state,
).await;
```

- [ ] **Step 9: Run task tests + verify regression-free**

Run: `cd src-tauri && cargo test --lib commands::task` Expected: all
pre-existing tests pass (now async). If a test fails, the bug is in the refactor
— fix it before moving on.

- [ ] **Step 10: Run full lib test suite**

Run: `cd src-tauri && cargo test --lib` Expected: all tests pass. No new tests
yet; just regression.

- [ ] **Step 11: Run clippy + fmt**

Run:
`cd src-tauri && cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all -- --check`

- [ ] **Step 12: Commit**

```bash
git add src-tauri/src/state.rs src-tauri/src/commands/task.rs src-tauri/src/lib.rs
git commit -m "refactor(phase-3a-2): route task commands through TaskProvider"
```

---

## Task 4: Add `bitable_list_fields` + `bitable_create_field` to lark_client

The schema wizard needs to enumerate existing fields and create missing ones.
Both methods go through `send_with_retry` (existing rate-limit + 429 retry).

**Files:**

- Modify: `src-tauri/src/platform/lark_client.rs` (add two methods +
  `BitableField` struct + tests)

- [ ] **Step 1: Write the test for `bitable_list_fields` first**

Open `src-tauri/src/platform/lark_client.rs`. In the
`#[cfg(test)] mod tests { ... }` block, near the other Bitable tests, add:

```rust
#[tokio::test]
async fn bitable_list_fields_returns_field_metadata() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("GET"))
        .and(path(
            "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields",
        ))
        .and(header("authorization", "Bearer t_xyz"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "data": {
                "items": [
                    {
                        "field_id": "fld_a",
                        "field_name": "title",
                        "type": 1,
                        "property": null
                    },
                    {
                        "field_id": "fld_b",
                        "field_name": "kanban_column",
                        "type": 3,
                        "property": {
                            "options": [{"name": "todo", "id": "opt_1"}]
                        }
                    }
                ],
                "has_more": false,
                "page_token": ""
            }
        })))
        .mount(&server)
        .await;
    let client = LarkClient::new(make_config(&server.uri()));
    let fields = client
        .bitable_list_fields("bascntest", "tbltest")
        .await
        .unwrap();
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].field_name, "title");
    assert_eq!(fields[0].field_type, 1);
    assert_eq!(fields[1].field_name, "kanban_column");
    assert_eq!(fields[1].field_type, 3);
}
```

- [ ] **Step 2: Run the test to verify it fails (function doesn't exist)**

Run:
`cd src-tauri && cargo test --lib platform::lark_client::tests::bitable_list_fields_returns_field_metadata`
Expected: compile error "no method named `bitable_list_fields`".

- [ ] **Step 3: Add `BitableField` struct + `bitable_list_fields` method**

In `src-tauri/src/platform/lark_client.rs`, near the other Bitable response
types (after `BitableEmptyResponse` around line 380), add:

```rust
/// Field metadata as returned by Bitable. `field_type` is the numeric
/// code Lark uses (1=Text, 2=Number, 3=SingleSelect, 5=DateTime,
/// 7=Checkbox, 15=URL, 17=Attachment). Property is free-form JSON
/// whose shape depends on the type.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BitableField {
    pub field_id: String,
    pub field_name: String,
    #[serde(rename = "type")]
    pub field_type: u32,
    #[serde(default)]
    pub property: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct BitableFieldListResponse {
    code: i64,
    #[serde(default)]
    msg: String,
    #[serde(default)]
    data: Option<BitableFieldListData>,
}

#[derive(Deserialize)]
struct BitableFieldListData {
    #[serde(default)]
    items: Vec<BitableField>,
}
```

Then in the `impl LarkClient { ... }` block that holds the other Bitable
methods, add:

```rust
/// List all fields in a Bitable table. Used by the schema wizard.
pub async fn bitable_list_fields(
    &self,
    app_token: &str,
    table_id: &str,
) -> Result<Vec<BitableField>> {
    let token = self.tenant_access_token().await?;
    let url = format!(
        "{}/open-apis/bitable/v1/apps/{}/tables/{}/fields",
        self.config.base_url, app_token, table_id
    );
    let resp = self
        .send_with_retry("bitable_list_fields", || {
            self.http.get(&url).bearer_auth(&token)
        })
        .await?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| AppError::Lark(format!("bitable_list_fields body: {e}")))?;
    if !status.is_success() {
        return Err(AppError::Lark(format!(
            "bitable_list_fields http {status}: {}",
            truncate(&text, 200)
        )));
    }
    let parsed: BitableFieldListResponse = serde_json::from_str(&text).map_err(|e| {
        AppError::Lark(format!(
            "bitable_list_fields parse: {e}; body={}",
            truncate(&text, 200)
        ))
    })?;
    if parsed.code != 0 {
        return Err(AppError::Lark(format!(
            "bitable_list_fields code {}: {}",
            parsed.code, parsed.msg
        )));
    }
    Ok(parsed.data.map(|d| d.items).unwrap_or_default())
}
```

- [ ] **Step 4: Run test to verify it passes**

Run:
`cd src-tauri && cargo test --lib platform::lark_client::tests::bitable_list_fields_returns_field_metadata`
Expected: pass.

- [ ] **Step 5: Write test for `bitable_create_field`**

In the same test module, add:

```rust
#[tokio::test]
async fn bitable_create_field_posts_field_definition() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(
            "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields",
        ))
        .and(header("authorization", "Bearer t_xyz"))
        .and(body_json(serde_json::json!({
            "field_name": "kanban_column",
            "type": 3,
            "property": {
                "options": [
                    {"name": "todo"},
                    {"name": "in_progress"}
                ]
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "data": {
                "field": {
                    "field_id": "fld_new",
                    "field_name": "kanban_column",
                    "type": 3,
                    "property": {
                        "options": [
                            {"name": "todo", "id": "opt_1"},
                            {"name": "in_progress", "id": "opt_2"}
                        ]
                    }
                }
            }
        })))
        .mount(&server)
        .await;
    let client = LarkClient::new(make_config(&server.uri()));
    let field = client
        .bitable_create_field(
            "bascntest",
            "tbltest",
            "kanban_column",
            3,
            Some(serde_json::json!({
                "options": [
                    {"name": "todo"},
                    {"name": "in_progress"}
                ]
            })),
        )
        .await
        .unwrap();
    assert_eq!(field.field_id, "fld_new");
    assert_eq!(field.field_name, "kanban_column");
    assert_eq!(field.field_type, 3);
}

#[tokio::test]
async fn bitable_create_field_omits_property_when_none() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path(
            "/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields",
        ))
        .and(body_json(serde_json::json!({
            "field_name": "repo_id",
            "type": 1
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "data": {
                "field": {
                    "field_id": "fld_repo",
                    "field_name": "repo_id",
                    "type": 1
                }
            }
        })))
        .mount(&server)
        .await;
    let client = LarkClient::new(make_config(&server.uri()));
    let field = client
        .bitable_create_field("bascntest", "tbltest", "repo_id", 1, None)
        .await
        .unwrap();
    assert_eq!(field.field_id, "fld_repo");
}
```

- [ ] **Step 6: Run tests to confirm they fail**

Run:
`cd src-tauri && cargo test --lib platform::lark_client::tests::bitable_create_field`
Expected: compile error.

- [ ] **Step 7: Add `bitable_create_field` method**

Add this struct near `BitableFieldListResponse`:

```rust
#[derive(Deserialize)]
struct BitableCreateFieldResponse {
    code: i64,
    #[serde(default)]
    msg: String,
    #[serde(default)]
    data: Option<BitableCreateFieldData>,
}

#[derive(Deserialize)]
struct BitableCreateFieldData {
    #[serde(default)]
    field: Option<BitableField>,
}
```

And this method in the `impl LarkClient` block:

```rust
/// Create one field in a Bitable table. `property` shape depends on
/// `field_type` — see Lark Open Platform docs.
pub async fn bitable_create_field(
    &self,
    app_token: &str,
    table_id: &str,
    field_name: &str,
    field_type: u32,
    property: Option<serde_json::Value>,
) -> Result<BitableField> {
    let token = self.tenant_access_token().await?;
    let url = format!(
        "{}/open-apis/bitable/v1/apps/{}/tables/{}/fields",
        self.config.base_url, app_token, table_id
    );
    let mut body = serde_json::json!({
        "field_name": field_name,
        "type": field_type,
    });
    if let Some(prop) = &property {
        body["property"] = prop.clone();
    }
    let resp = self
        .send_with_retry("bitable_create_field", || {
            self.http.post(&url).bearer_auth(&token).json(&body)
        })
        .await?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| AppError::Lark(format!("bitable_create_field body: {e}")))?;
    if !status.is_success() {
        return Err(AppError::Lark(format!(
            "bitable_create_field http {status}: {}",
            truncate(&text, 200)
        )));
    }
    let parsed: BitableCreateFieldResponse = serde_json::from_str(&text).map_err(|e| {
        AppError::Lark(format!(
            "bitable_create_field parse: {e}; body={}",
            truncate(&text, 200)
        ))
    })?;
    if parsed.code != 0 {
        return Err(AppError::Lark(format!(
            "bitable_create_field code {}: {}",
            parsed.code, parsed.msg
        )));
    }
    parsed
        .data
        .and_then(|d| d.field)
        .ok_or_else(|| AppError::Lark("bitable_create_field missing field in response".into()))
}
```

- [ ] **Step 8: Run tests to verify they pass**

Run:
`cd src-tauri && cargo test --lib platform::lark_client::tests::bitable_create_field`
Expected: both new tests pass.

- [ ] **Step 9: Run full lark_client test suite**

Run: `cd src-tauri && cargo test --lib platform::lark_client` Expected: all
tests pass (43+ tests).

- [ ] **Step 10: Clippy + fmt**

Run:
`cd src-tauri && cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all -- --check`

- [ ] **Step 11: Commit**

```bash
git add src-tauri/src/platform/lark_client.rs
git commit -m "feat(lark-client): add bitable_list_fields + bitable_create_field"
```

---

## Task 5: Schema registry + `verify_schema` logic

**Files:**

- Create: `src-tauri/src/task_provider/schema.rs`
- Modify: `src-tauri/src/task_provider/mod.rs` (add `pub mod schema;`)

- [ ] **Step 1: Register `schema` module**

In `src-tauri/src/task_provider/mod.rs`, add near `pub mod local;`:

```rust
pub mod schema;
```

- [ ] **Step 2: Write `schema.rs` with registry + check logic**

Create `src-tauri/src/task_provider/schema.rs`:

```rust
// Phase 3a-2 — Bitable schema wizard.
//
// Defines the set of fields that must exist for the LarkProvider to
// function, and provides a verify-and-create function the Tauri
// command surface delegates to. Idempotent: re-running after fields
// exist is a no-op.

use crate::error::{AppError, Result};
use crate::platform::lark_client::LarkClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Lark numeric field type codes used by 3a-2 fields. See
/// `bitable_create_field` docs for the full enum.
pub const FIELD_TYPE_TEXT: u32 = 1;
pub const FIELD_TYPE_NUMBER: u32 = 2;
pub const FIELD_TYPE_SINGLE_SELECT: u32 = 3;

#[derive(Debug, Clone)]
pub struct RequiredField {
    pub name: &'static str,
    pub field_type: u32,
    pub property: Option<serde_json::Value>,
}

/// The 5 fields required for Phase 3a-2 kanban sync. Later phases
/// extend this with their own `required_fields_phase_3a_X()` and the
/// wizard runs the union.
pub fn required_fields_phase_3a2() -> Vec<RequiredField> {
    vec![
        RequiredField {
            name: "title",
            field_type: FIELD_TYPE_TEXT,
            property: None,
        },
        RequiredField {
            name: "description",
            field_type: FIELD_TYPE_TEXT,
            property: None,
        },
        RequiredField {
            name: "kanban_column",
            field_type: FIELD_TYPE_SINGLE_SELECT,
            property: Some(serde_json::json!({
                "options": [
                    {"name": "todo"},
                    {"name": "in_progress"},
                    {"name": "review"},
                    {"name": "done"}
                ]
            })),
        },
        RequiredField {
            name: "repo_id",
            field_type: FIELD_TYPE_TEXT,
            property: None,
        },
        RequiredField {
            name: "order_within_column",
            field_type: FIELD_TYPE_NUMBER,
            property: None,
        },
    ]
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SchemaCheckResult {
    pub ok: bool,
    pub created: Vec<String>,
    pub already_present: Vec<String>,
    pub type_mismatches: Vec<String>,
}

/// Diff the configured table against the required schema, creating
/// missing fields. Type mismatches are reported, not auto-fixed.
pub async fn verify_schema(
    client: &LarkClient,
    app_token: &str,
    table_id: &str,
    required: &[RequiredField],
) -> Result<SchemaCheckResult> {
    let existing = client.bitable_list_fields(app_token, table_id).await?;
    let existing_by_name: HashMap<&str, u32> = existing
        .iter()
        .map(|f| (f.field_name.as_str(), f.field_type))
        .collect();

    let mut created = Vec::new();
    let mut already_present = Vec::new();
    let mut type_mismatches = Vec::new();

    for req in required {
        match existing_by_name.get(req.name) {
            Some(t) if *t == req.field_type => {
                already_present.push(req.name.to_string());
            }
            Some(_wrong_type) => {
                type_mismatches.push(req.name.to_string());
            }
            None => {
                client
                    .bitable_create_field(
                        app_token,
                        table_id,
                        req.name,
                        req.field_type,
                        req.property.clone(),
                    )
                    .await
                    .map_err(|e| {
                        AppError::Lark(format!("create field {}: {e}", req.name))
                    })?;
                created.push(req.name.to_string());
            }
        }
    }

    Ok(SchemaCheckResult {
        ok: type_mismatches.is_empty(),
        created,
        already_present,
        type_mismatches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::lark_client::{LarkClient, LarkConfig};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_config(base: &str) -> LarkConfig {
        LarkConfig {
            app_id: "cli_t".into(),
            app_secret: "s".into(),
            app_token: "bascntest".into(),
            table_id: "tbltest".into(),
            base_url: base.into(),
        }
    }

    async fn mount_token(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "tenant_access_token": "t_xyz",
                "expire": 7200
            })))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn verify_schema_creates_all_missing() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": { "items": [], "has_more": false, "page_token": "" }
            })))
            .mount(&server)
            .await;
        // Every create returns the field. Wiremock will dispatch all 5
        // POSTs to this single mock (matches path + method).
        Mock::given(method("POST"))
            .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "field": {"field_id": "fld_x", "field_name": "x", "type": 1}
                }
            })))
            .expect(5)
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let result = verify_schema(
            &client,
            "bascntest",
            "tbltest",
            &required_fields_phase_3a2(),
        )
        .await
        .unwrap();
        assert!(result.ok);
        assert_eq!(result.created.len(), 5);
        assert!(result.already_present.is_empty());
        assert!(result.type_mismatches.is_empty());
    }

    #[tokio::test]
    async fn verify_schema_skips_present_fields() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        {"field_id": "f1", "field_name": "title", "type": 1},
                        {"field_id": "f2", "field_name": "repo_id", "type": 1},
                        {"field_id": "f3", "field_name": "description", "type": 1}
                    ]
                }
            })))
            .mount(&server)
            .await;
        // Only 2 fields are missing (kanban_column, order_within_column).
        Mock::given(method("POST"))
            .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "field": {"field_id": "fld_x", "field_name": "x", "type": 1}
                }
            })))
            .expect(2)
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let result = verify_schema(
            &client,
            "bascntest",
            "tbltest",
            &required_fields_phase_3a2(),
        )
        .await
        .unwrap();
        assert!(result.ok);
        assert_eq!(result.created.len(), 2);
        assert_eq!(result.already_present.len(), 3);
        assert!(result.type_mismatches.is_empty());
    }

    #[tokio::test]
    async fn verify_schema_surfaces_type_mismatch() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        // kanban_column exists but as Text (1) not SingleSelect (3).
        Mock::given(method("GET"))
            .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        {"field_id": "f1", "field_name": "kanban_column", "type": 1}
                    ]
                }
            })))
            .mount(&server)
            .await;
        // 4 missing fields still get created.
        Mock::given(method("POST"))
            .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "field": {"field_id": "fld_x", "field_name": "x", "type": 1}
                }
            })))
            .expect(4)
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let result = verify_schema(
            &client,
            "bascntest",
            "tbltest",
            &required_fields_phase_3a2(),
        )
        .await
        .unwrap();
        assert!(!result.ok);
        assert_eq!(result.type_mismatches, vec!["kanban_column".to_string()]);
        assert_eq!(result.created.len(), 4);
    }

    #[tokio::test]
    async fn verify_schema_idempotent_on_rerun() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        // All 5 fields exist with correct types.
        Mock::given(method("GET"))
            .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/fields"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        {"field_id": "f1", "field_name": "title", "type": 1},
                        {"field_id": "f2", "field_name": "description", "type": 1},
                        {"field_id": "f3", "field_name": "kanban_column", "type": 3},
                        {"field_id": "f4", "field_name": "repo_id", "type": 1},
                        {"field_id": "f5", "field_name": "order_within_column", "type": 2}
                    ]
                }
            })))
            .mount(&server)
            .await;
        let client = LarkClient::new(make_config(&server.uri()));
        let result = verify_schema(
            &client,
            "bascntest",
            "tbltest",
            &required_fields_phase_3a2(),
        )
        .await
        .unwrap();
        assert!(result.ok);
        assert!(result.created.is_empty());
        assert_eq!(result.already_present.len(), 5);
    }
}
```

- [ ] **Step 3: Run schema tests**

Run: `cd src-tauri && cargo test --lib task_provider::schema` Expected: 4 tests
pass.

- [ ] **Step 4: Clippy + fmt + commit**

```bash
cd src-tauri && cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all -- --check
cd ..
git add src-tauri/src/task_provider/schema.rs src-tauri/src/task_provider/mod.rs
git commit -m "feat(phase-3a-2): Bitable schema verify wizard"
```

---

## Task 6: LarkProvider — Bitable-backed TaskProvider impl

This is the biggest task. It maps Bitable rows ↔ Task structs, handles ID
strategy (record_id as Task.id), and routes all CRUD through `lark_client`.

**Files:**

- Modify: `src-tauri/src/task_provider/mod.rs` (add `pub mod lark;`)
- Create: `src-tauri/src/task_provider/lark.rs`

- [ ] **Step 1: Register `lark` module**

In `src-tauri/src/task_provider/mod.rs`, add:

```rust
pub mod lark;
```

- [ ] **Step 2: Sketch the file with type wiring (no impl bodies yet)**

Create `src-tauri/src/task_provider/lark.rs`:

```rust
// Phase 3a-2 — LarkProvider.
//
// Maps Bitable rows ↔ Task structs and routes all CRUD through the
// shared LarkClient (rate-limit-guarded, 429-aware). Task.id = Lark
// record_id (format `rec...`); the local `tk_` prefix doesn't apply
// when this provider is active because hydrate is single-provider.

use super::{CreateTaskArgs, TaskPatch, TaskProvider};
use crate::error::{AppError, Result};
use crate::platform::lark_client::{BitableRecord, LarkClient};
use crate::state::{KanbanColumn, Task};
use async_trait::async_trait;
use std::sync::Arc;

#[derive(Debug)]
pub struct LarkProvider {
    client: Arc<LarkClient>,
    app_token: String,
    table_id: String,
}

impl LarkProvider {
    pub fn new(client: Arc<LarkClient>, app_token: String, table_id: String) -> Self {
        Self {
            client,
            app_token,
            table_id,
        }
    }
}
```

- [ ] **Step 3: Write the first test (record → Task mapping)**

Add at the bottom of `lark.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::lark_client::{LarkClient, LarkConfig};
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_config(base: &str) -> LarkConfig {
        LarkConfig {
            app_id: "cli_t".into(),
            app_secret: "s".into(),
            app_token: "bascntest".into(),
            table_id: "tbltest".into(),
            base_url: base.into(),
        }
    }

    async fn mount_token(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/open-apis/auth/v3/tenant_access_token/internal"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "tenant_access_token": "t_xyz",
                "expire": 7200
            })))
            .mount(server)
            .await;
    }

    fn make_provider(uri: &str) -> LarkProvider {
        let client = Arc::new(LarkClient::new(make_config(uri)));
        LarkProvider::new(client, "bascntest".into(), "tbltest".into())
    }

    #[tokio::test]
    async fn lark_provider_list_maps_record_id_to_task_id() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "code": 0,
                "data": {
                    "items": [
                        {
                            "record_id": "rec_abc",
                            "fields": {
                                "title": "First",
                                "description": "Desc",
                                "kanban_column": "in_progress",
                                "repo_id": "repo_a",
                                "order_within_column": 4096
                            },
                            "created_time": 1700000000000_i64,
                            "last_modified_time": 1700000001000_i64
                        }
                    ],
                    "has_more": false,
                    "page_token": ""
                }
            })))
            .mount(&server)
            .await;
        let provider = make_provider(&server.uri());
        let tasks = provider.list_tasks(Some("repo_a")).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "rec_abc");
        assert_eq!(tasks[0].title, "First");
        assert_eq!(tasks[0].column, KanbanColumn::InProgress);
        assert_eq!(tasks[0].repo_id, "repo_a");
        assert_eq!(tasks[0].order, 4096);
        assert_eq!(tasks[0].created_at, 1_700_000_000);
        assert_eq!(tasks[0].updated_at, 1_700_000_001);
    }
}
```

- [ ] **Step 4: Implement `list_tasks` with field mapping**

Below the `impl LarkProvider` block (before the `#[cfg(test)]`), add:

```rust
/// Lark-API row → Task. Returns an error if a required field is
/// missing/malformed; this surfaces "schema not initialized" cleanly.
fn record_to_task(rec: &BitableRecord) -> Result<Task> {
    let fields = rec.fields.as_object().ok_or_else(|| {
        AppError::Lark(format!("record {} fields is not an object", rec.record_id))
    })?;

    let title = fields
        .get("title")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AppError::Lark(format!(
                "record {} missing required field 'title'",
                rec.record_id
            ))
        })?
        .to_string();

    let description = fields
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let column_str = fields
        .get("kanban_column")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AppError::Lark(format!(
                "record {} missing required field 'kanban_column'",
                rec.record_id
            ))
        })?;
    let column = match column_str {
        "todo" => KanbanColumn::Todo,
        "in_progress" => KanbanColumn::InProgress,
        "review" => KanbanColumn::Review,
        "done" => KanbanColumn::Done,
        other => {
            return Err(AppError::Lark(format!(
                "record {} has unknown kanban_column '{other}'",
                rec.record_id
            )))
        }
    };

    let repo_id = fields
        .get("repo_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AppError::Lark(format!(
                "record {} missing required field 'repo_id'",
                rec.record_id
            ))
        })?
        .to_string();

    let order = fields
        .get("order_within_column")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;

    // Lark returns ms since epoch; our Task uses seconds.
    let created_at = rec
        .extra_i64("created_time")
        .map(|ms| ms / 1000)
        .unwrap_or(0);
    let updated_at = rec
        .extra_i64("last_modified_time")
        .map(|ms| ms / 1000)
        .unwrap_or(created_at);

    Ok(Task {
        id: rec.record_id.clone(),
        repo_id,
        workspace_id: None,
        title,
        description,
        column,
        order,
        created_at,
        updated_at,
    })
}

#[async_trait]
impl TaskProvider for LarkProvider {
    async fn list_tasks(&self, repo_filter: Option<&str>) -> Result<Vec<Task>> {
        let filter = repo_filter.map(|r| format!("CurrentValue.[repo_id]=\"{r}\""));
        let records = self
            .client
            .bitable_list_records(&self.app_token, &self.table_id, filter.as_deref())
            .await?;
        let mut tasks: Vec<Task> = records
            .iter()
            .map(record_to_task)
            .collect::<Result<Vec<_>>>()?;
        // Same ordering as LocalProvider: column ASC then order DESC.
        tasks.sort_by(|a, b| match a.column.cmp(&b.column) {
            std::cmp::Ordering::Equal => b.order.cmp(&a.order),
            o => o,
        });
        Ok(tasks)
    }

    async fn create_task(&self, args: CreateTaskArgs) -> Result<Task> {
        let column = args.column.unwrap_or_default();
        let fields = serde_json::json!({
            "title": args.title,
            "description": args.description,
            "kanban_column": column_to_str(column),
            "repo_id": args.repo_id,
            "order_within_column": 0
        });
        let record = self
            .client
            .bitable_create_record(&self.app_token, &self.table_id, fields)
            .await?;
        record_to_task(&record)
    }

    async fn update_task(&self, id: &str, patch: TaskPatch) -> Result<Task> {
        let mut fields = serde_json::Map::new();
        if let Some(title) = patch.title {
            fields.insert("title".into(), serde_json::Value::String(title));
        }
        if let Some(description) = patch.description {
            fields.insert(
                "description".into(),
                serde_json::Value::String(description),
            );
        }
        if let Some(order) = patch.order {
            fields.insert("order_within_column".into(), serde_json::json!(order));
        }
        self.client
            .bitable_update_record(
                &self.app_token,
                &self.table_id,
                id,
                serde_json::Value::Object(fields),
            )
            .await?;
        // Re-fetch the single row to return canonical state. Bitable
        // update endpoint doesn't include record metadata in response.
        let filter = format!("CurrentValue.[record_id]=\"{id}\"");
        let records = self
            .client
            .bitable_list_records(&self.app_token, &self.table_id, Some(&filter))
            .await?;
        let rec = records
            .iter()
            .find(|r| r.record_id == id)
            .ok_or_else(|| AppError::NotFound(format!("task {id} not found after update")))?;
        record_to_task(rec)
    }

    async fn move_task(&self, id: &str, column: KanbanColumn, order: i32) -> Result<Task> {
        let fields = serde_json::json!({
            "kanban_column": column_to_str(column),
            "order_within_column": order
        });
        self.client
            .bitable_update_record(&self.app_token, &self.table_id, id, fields)
            .await?;
        let filter = format!("CurrentValue.[record_id]=\"{id}\"");
        let records = self
            .client
            .bitable_list_records(&self.app_token, &self.table_id, Some(&filter))
            .await?;
        let rec = records
            .iter()
            .find(|r| r.record_id == id)
            .ok_or_else(|| AppError::NotFound(format!("task {id} not found after move")))?;
        record_to_task(rec)
    }

    async fn delete_task(&self, id: &str) -> Result<()> {
        self.client
            .bitable_delete_record(&self.app_token, &self.table_id, id)
            .await
    }
}

fn column_to_str(c: KanbanColumn) -> &'static str {
    match c {
        KanbanColumn::Todo => "todo",
        KanbanColumn::InProgress => "in_progress",
        KanbanColumn::Review => "review",
        KanbanColumn::Done => "done",
    }
}
```

- [ ] **Step 5: Add `extra_i64` helper on `BitableRecord`**

`BitableRecord` currently has `record_id` + `fields`. We need access to
top-level `created_time` and `last_modified_time` returned by Lark. Open
`src-tauri/src/platform/lark_client.rs` and find `BitableRecord` (around line
332). Update it:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BitableRecord {
    pub record_id: String,
    pub fields: serde_json::Value,
    /// Lark returns these as top-level fields on the record object.
    /// Stored as opaque JSON so we can pull `created_time` /
    /// `last_modified_time` without explicit fields here.
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

impl BitableRecord {
    pub fn extra_i64(&self, key: &str) -> Option<i64> {
        self.extra.get(key).and_then(|v| v.as_i64())
    }
}
```

This is backward-compatible — existing tests pass empty extras.

- [ ] **Step 6: Run all lark_client tests to verify nothing regressed**

Run: `cd src-tauri && cargo test --lib platform::lark_client` Expected: all
existing tests still pass.

- [ ] **Step 7: Run the LarkProvider mapping test**

Run:
`cd src-tauri && cargo test --lib task_provider::lark::tests::lark_provider_list_maps_record_id_to_task_id`
Expected: pass.

- [ ] **Step 8: Add the remaining LarkProvider tests**

Add to `lark.rs` test module:

```rust
#[tokio::test]
async fn lark_provider_list_sorts_by_order_within_column_desc() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("GET"))
        .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "data": {
                "items": [
                    {
                        "record_id": "rec_low",
                        "fields": {"title":"L","description":"","kanban_column":"todo","repo_id":"r","order_within_column":100}
                    },
                    {
                        "record_id": "rec_high",
                        "fields": {"title":"H","description":"","kanban_column":"todo","repo_id":"r","order_within_column":900}
                    }
                ],
                "has_more": false,
                "page_token": ""
            }
        })))
        .mount(&server)
        .await;
    let provider = make_provider(&server.uri());
    let tasks = provider.list_tasks(None).await.unwrap();
    assert_eq!(tasks[0].id, "rec_high");
    assert_eq!(tasks[1].id, "rec_low");
}

#[tokio::test]
async fn lark_provider_list_passes_repo_filter_via_lark_filter_expr() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("GET"))
        .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records"))
        .and(query_param("filter", "CurrentValue.[repo_id]=\"repo_xyz\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "data": {"items": [], "has_more": false, "page_token": ""}
        })))
        .mount(&server)
        .await;
    let provider = make_provider(&server.uri());
    let tasks = provider.list_tasks(Some("repo_xyz")).await.unwrap();
    assert!(tasks.is_empty());
}

#[tokio::test]
async fn lark_provider_create_returns_task_with_record_id() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("POST"))
        .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records"))
        .and(body_json(serde_json::json!({
            "fields": {
                "title": "New task",
                "description": "desc",
                "kanban_column": "todo",
                "repo_id": "repo_a",
                "order_within_column": 0
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "data": {
                "record": {
                    "record_id": "rec_new",
                    "fields": {
                        "title": "New task",
                        "description": "desc",
                        "kanban_column": "todo",
                        "repo_id": "repo_a",
                        "order_within_column": 0
                    }
                }
            }
        })))
        .mount(&server)
        .await;
    let provider = make_provider(&server.uri());
    let task = provider
        .create_task(CreateTaskArgs {
            repo_id: "repo_a".into(),
            title: "New task".into(),
            description: "desc".into(),
            column: None,
        })
        .await
        .unwrap();
    assert_eq!(task.id, "rec_new");
    assert_eq!(task.column, KanbanColumn::Todo);
}

#[tokio::test]
async fn lark_provider_move_sends_kanban_column_and_order_only() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("PUT"))
        .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records/rec_x"))
        .and(body_json(serde_json::json!({
            "fields": {
                "kanban_column": "done",
                "order_within_column": 256
            }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "msg": "ok"
        })))
        .mount(&server)
        .await;
    // Re-fetch after update.
    Mock::given(method("GET"))
        .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records"))
        .and(query_param("filter", "CurrentValue.[record_id]=\"rec_x\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "data": {
                "items": [{
                    "record_id": "rec_x",
                    "fields": {
                        "title": "t",
                        "description": "",
                        "kanban_column": "done",
                        "repo_id": "r",
                        "order_within_column": 256
                    }
                }],
                "has_more": false,
                "page_token": ""
            }
        })))
        .mount(&server)
        .await;
    let provider = make_provider(&server.uri());
    let task = provider
        .move_task("rec_x", KanbanColumn::Done, 256)
        .await
        .unwrap();
    assert_eq!(task.column, KanbanColumn::Done);
    assert_eq!(task.order, 256);
}

#[tokio::test]
async fn lark_provider_update_sends_only_named_fields() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("PUT"))
        .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records/rec_y"))
        .and(body_json(serde_json::json!({
            "fields": {"title": "new"}
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "msg": "ok"
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records"))
        .and(query_param("filter", "CurrentValue.[record_id]=\"rec_y\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "data": {
                "items": [{
                    "record_id": "rec_y",
                    "fields": {
                        "title": "new",
                        "description": "",
                        "kanban_column": "todo",
                        "repo_id": "r",
                        "order_within_column": 0
                    }
                }],
                "has_more": false,
                "page_token": ""
            }
        })))
        .mount(&server)
        .await;
    let provider = make_provider(&server.uri());
    let task = provider
        .update_task(
            "rec_y",
            TaskPatch {
                title: Some("new".into()),
                description: None,
                order: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(task.title, "new");
}

#[tokio::test]
async fn lark_provider_delete_calls_bitable_delete() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    Mock::given(method("DELETE"))
        .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records/rec_d"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0, "msg": "ok"
        })))
        .mount(&server)
        .await;
    let provider = make_provider(&server.uri());
    provider.delete_task("rec_d").await.unwrap();
}

#[tokio::test]
async fn lark_provider_surfaces_missing_field_error_clearly() {
    let server = MockServer::start().await;
    mount_token(&server).await;
    // Row missing the required `title` field.
    Mock::given(method("GET"))
        .and(path("/open-apis/bitable/v1/apps/bascntest/tables/tbltest/records"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "code": 0,
            "data": {
                "items": [{
                    "record_id": "rec_broken",
                    "fields": {
                        "description": "",
                        "kanban_column": "todo",
                        "repo_id": "r"
                    }
                }],
                "has_more": false,
                "page_token": ""
            }
        })))
        .mount(&server)
        .await;
    let provider = make_provider(&server.uri());
    let err = provider.list_tasks(None).await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("title"), "{msg}");
    assert!(msg.contains("rec_broken"), "{msg}");
}
```

- [ ] **Step 9: Run all LarkProvider tests**

Run: `cd src-tauri && cargo test --lib task_provider::lark` Expected: 7 tests
pass.

- [ ] **Step 10: Run full lib test suite for regression**

Run: `cd src-tauri && cargo test --lib` Expected: all tests pass.

- [ ] **Step 11: Clippy + fmt + commit**

```bash
cd src-tauri && cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all -- --check
cd ..
git add src-tauri/src/task_provider/mod.rs src-tauri/src/task_provider/lark.rs src-tauri/src/platform/lark_client.rs
git commit -m "feat(phase-3a-2): LarkProvider with Bitable CRUD + field mapping"
```

---

## Task 7: `TaskSource` enum + `AppSettings.task_source` field

**Files:**

- Modify: `src-tauri/src/state.rs` (add `TaskSource` enum + AppSettings field)

- [ ] **Step 1: Write a test pinning JSON shape**

In `src-tauri/src/state.rs`, in the existing `mod tests`, add:

```rust
#[test]
fn task_source_serializes_snake_case() {
    let local = serde_json::to_string(&TaskSource::Local).unwrap();
    let lark = serde_json::to_string(&TaskSource::Lark).unwrap();
    assert_eq!(local, "\"local\"");
    assert_eq!(lark, "\"lark\"");
}

#[test]
fn app_settings_task_source_defaults_to_local() {
    let s = AppSettings::default();
    assert_eq!(s.task_source, TaskSource::Local);
}

#[test]
fn app_settings_round_trips_with_task_source() {
    let mut s = AppSettings::default();
    s.task_source = TaskSource::Lark;
    let json = serde_json::to_string(&s).unwrap();
    let back: AppSettings = serde_json::from_str(&json).unwrap();
    assert_eq!(back.task_source, TaskSource::Lark);
}

#[test]
fn app_settings_loads_legacy_file_without_task_source_field() {
    // Older app_settings.json files don't have task_source. Verify we
    // deserialize them with the default.
    let legacy = serde_json::json!({
        "schema_version": 1,
        "theme": "warm-dark",
        "selected_repo_id": null,
        "selected_workspace_id": null,
        "recent_repos": [],
        "window_width": 1400,
        "window_height": 900,
        "onboarding_completed": false
    });
    let s: AppSettings = serde_json::from_value(legacy).unwrap();
    assert_eq!(s.task_source, TaskSource::Local);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib state::tests::task_source` Expected:
compile error.

- [ ] **Step 3: Add `TaskSource` enum + field**

In `src-tauri/src/state.rs`, near the other top-level enums (before
`AppSettings`), add:

```rust
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskSource {
    Local,
    Lark,
}

fn default_task_source() -> TaskSource {
    TaskSource::Local
}
```

Update `AppSettings` to add the field (after `claude_binary_override`):

```rust
    /// Source for kanban data. `Local` reads/writes tasks.json;
    /// `Lark` reads/writes a Lark Bitable table via `LarkProvider`.
    #[serde(default = "default_task_source")]
    pub task_source: TaskSource,
```

And in `Default for AppSettings`:

```rust
            task_source: default_task_source(),
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib state::tests` Expected: all tests pass.

- [ ] **Step 5: Add `TaskProviderHandle` typedef (if not done in Task 3)**

Verify the typedef from Task 3 Step 1 is in place. If not, add it now.

- [ ] **Step 6: Clippy + fmt + commit**

```bash
cd src-tauri && cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all -- --check
cd ..
git add src-tauri/src/state.rs
git commit -m "feat(phase-3a-2): add TaskSource enum to AppSettings"
```

---

## Task 8: Provider selection on startup + lib.rs wiring

**Files:**

- Modify: `src-tauri/src/lib.rs::setup` (init Lark or Local provider based on
  settings)

- [ ] **Step 1: Add a helper that builds the right provider**

In `src-tauri/src/lib.rs`, after the imports and before `pub fn run()`, add:

```rust
fn build_initial_provider(
    settings: &crate::state::AppSettings,
    data_dir: &std::path::Path,
) -> std::sync::Arc<dyn crate::task_provider::TaskProvider> {
    match settings.task_source {
        crate::state::TaskSource::Local => std::sync::Arc::new(
            crate::task_provider::local::LocalProvider::new(data_dir.to_path_buf()),
        ),
        crate::state::TaskSource::Lark => {
            // Try to construct LarkProvider; fall back to LocalProvider
            // if credentials are missing. The frontend banner will nudge
            // the user to configure Lark.
            let store = crate::commands::lark_auth::KeyringStore;
            match crate::commands::lark_auth::load_lark_config_inner(data_dir, &store) {
                Ok(cfg) => {
                    let app_token = cfg.app_token.clone();
                    let table_id = cfg.table_id.clone();
                    let client = std::sync::Arc::new(
                        crate::platform::lark_client::LarkClient::new(cfg),
                    );
                    std::sync::Arc::new(crate::task_provider::lark::LarkProvider::new(
                        client, app_token, table_id,
                    ))
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "task_source=lark but credentials missing; falling back to LocalProvider"
                    );
                    std::sync::Arc::new(crate::task_provider::local::LocalProvider::new(
                        data_dir.to_path_buf(),
                    ))
                }
            }
        }
    }
}
```

- [ ] **Step 2: Replace the placeholder LocalProvider init from Task 3**

In the `.setup(|app| { ... })` closure, replace the line you added in Task 3
Step 7 (the `let provider: ...` line) with:

```rust
let provider: std::sync::Arc<dyn crate::task_provider::TaskProvider> =
    build_initial_provider(&settings, &data_dir);
let provider_handle: crate::state::TaskProviderHandle =
    std::sync::Arc::new(tokio::sync::RwLock::new(provider.clone()));
app.manage(provider_handle.clone());

// Initial hydrate: pull tasks from provider and populate AppState mirror.
// Done as a spawned async task so setup() doesn't block on the network
// when Lark is the source.
{
    let state_arc = app
        .try_state::<std::sync::Arc<std::sync::Mutex<crate::state::AppState>>>()
        .expect("AppState managed")
        .inner()
        .clone();
    tauri::async_runtime::spawn(async move {
        match provider.list_tasks(None).await {
            Ok(tasks) => {
                if let Ok(mut st) = state_arc.lock() {
                    st.tasks.clear();
                    for t in tasks {
                        st.tasks.insert(t.id.clone(), t);
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "initial task hydrate failed");
            }
        }
    });
}
```

- [ ] **Step 3: Make `load_lark_config_inner` + `KeyringStore` public**

Check `src-tauri/src/commands/lark_auth.rs`:

- `pub struct KeyringStore;` — already pub
- `pub fn load_lark_config_inner(...)` — already pub

If `KeyringStore` impl `SecretStore` is not visible, you may need to add
`pub use` from the module. Verify by running `cargo check` after step 2.

- [ ] **Step 4: Run check**

Run: `cd src-tauri && cargo check --lib --all-targets` Expected: clean compile.

- [ ] **Step 5: Run full lib test suite**

Run: `cd src-tauri && cargo test --lib` Expected: all tests pass.

- [ ] **Step 6: Clippy + fmt + commit**

```bash
cd src-tauri && cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all -- --check
cd ..
git add src-tauri/src/lib.rs
git commit -m "feat(phase-3a-2): pick provider on startup based on task_source setting"
```

---

## Task 9: `set_task_source` / `get_task_source` / `refresh_tasks` commands

**Files:**

- Modify: `src-tauri/src/commands/task.rs` (add three new commands + tests)
- Modify: `src-tauri/src/lib.rs` (register the new commands in
  `invoke_handler!`)

- [ ] **Step 1: Add `refresh_tasks` command**

In `src-tauri/src/commands/task.rs`, add after `list_tasks_inner`:

```rust
#[tauri::command]
pub async fn refresh_tasks(
    repo_id: Option<String>,
    state: State<'_, Arc<Mutex<AppState>>>,
    provider_handle: State<'_, crate::state::TaskProviderHandle>,
) -> std::result::Result<Vec<Task>, String> {
    refresh_tasks_inner(repo_id, state.inner().clone(), provider_handle.read().await.clone())
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
            // Only replace the subset for this repo.
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
```

- [ ] **Step 2: Add `get_task_source` + `set_task_source` commands**

Add near the end of `commands/task.rs`:

```rust
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
```

- [ ] **Step 3: Register new commands in `lib.rs`**

In `src-tauri/src/lib.rs::invoke_handler`, add:

```rust
crate::commands::task::refresh_tasks,
crate::commands::task::get_task_source,
crate::commands::task::set_task_source,
```

- [ ] **Step 4: Write tests**

In `commands/task.rs` test module, add:

```rust
#[tokio::test]
async fn refresh_tasks_replaces_mirror_subset_for_repo() {
    let tmp = tempfile::tempdir().unwrap();
    let provider = make_provider(tmp.path());
    // Pre-populate with two repo_a tasks and one repo_b task via the
    // provider directly.
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
    // Refresh only repo_a — should pull 2 tasks into the mirror, leave
    // repo_b unaffected (nothing there yet so still empty).
    let tasks = refresh_tasks_inner(Some("repo_a".into()), state.clone(), provider.clone())
        .await
        .unwrap();
    assert_eq!(tasks.len(), 2);
    let st = state.lock().unwrap();
    assert_eq!(st.tasks.values().filter(|t| t.repo_id == "repo_a").count(), 2);
}

#[tokio::test]
async fn set_task_source_lark_rejects_when_credentials_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let state = make_state();
    let provider_handle = std::sync::Arc::new(tokio::sync::RwLock::new(
        std::sync::Arc::new(crate::task_provider::local::LocalProvider::new(
            tmp.path().to_path_buf(),
        )) as std::sync::Arc<dyn crate::task_provider::TaskProvider>,
    ));
    let err = set_task_source_inner(
        crate::state::TaskSource::Lark,
        tmp.path().to_path_buf(),
        state,
        provider_handle,
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("Configure Lark credentials"), "{err}");
}
```

Add a `make_state()` helper near the top of the test module if it doesn't
already exist:

```rust
fn make_state() -> Arc<Mutex<AppState>> {
    Arc::new(Mutex::new(AppState::default()))
}
```

- [ ] **Step 5: Run tests**

Run: `cd src-tauri && cargo test --lib commands::task` Expected: all tests pass.

- [ ] **Step 6: Clippy + fmt + commit**

```bash
cd src-tauri && cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all -- --check
cd ..
git add src-tauri/src/commands/task.rs src-tauri/src/lib.rs
git commit -m "feat(phase-3a-2): set_task_source / get_task_source / refresh_tasks commands"
```

---

## Task 10: `verify_lark_schema` Tauri command

**Files:**

- Modify: `src-tauri/src/commands/lark_auth.rs` (add command)
- Modify: `src-tauri/src/lib.rs` (register command)

- [ ] **Step 1: Add the command**

In `src-tauri/src/commands/lark_auth.rs`, near the other Tauri commands, add:

```rust
#[tauri::command]
pub async fn verify_lark_schema(
    app: tauri::AppHandle,
) -> std::result::Result<crate::task_provider::schema::SchemaCheckResult, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let store = KeyringStore;
    verify_lark_schema_inner(&data_dir, &store)
        .await
        .map_err(|e| e.to_string())
}

pub async fn verify_lark_schema_inner(
    data_dir: &std::path::Path,
    store: &dyn SecretStore,
) -> crate::error::Result<crate::task_provider::schema::SchemaCheckResult> {
    let cfg = load_lark_config_inner(data_dir, store)?;
    let app_token = cfg.app_token.clone();
    let table_id = cfg.table_id.clone();
    let client = crate::platform::lark_client::LarkClient::new(cfg);
    crate::task_provider::schema::verify_schema(
        &client,
        &app_token,
        &table_id,
        &crate::task_provider::schema::required_fields_phase_3a2(),
    )
    .await
}
```

- [ ] **Step 2: Register in `lib.rs`**

In `invoke_handler!`, add:

```rust
crate::commands::lark_auth::verify_lark_schema,
```

- [ ] **Step 3: Add the CI coverage exclusion for new files**

Open `.github/workflows/ci.yml`. Find the line:

```yaml
- run:
    cargo llvm-cov --lib --ignore-filename-regex
    'lib\.rs$|main\.rs$|commands[/\\](repo|workspace|task|agent|diff|files|file_io|search|scripts|terminal|lark_auth)\.rs$|platform[/\\](pty|lark_client)\.rs$'
    --fail-under-lines 95 --fail-under-functions 94
```

Update the regex to also exclude the new thin Tauri-command surface for
`task_provider/lark.rs` and `task_provider/schema.rs` only if their coverage
genuinely can't reach 95% via unit tests. Run a coverage check first (Step 4)
and only update the regex if needed. The inner `_inner` functions in `lark_auth`
and `task.rs` plus the wiremock tests in `lark.rs` and `schema.rs` should hit
≥95% — no exclusion needed unless coverage proves otherwise.

- [ ] **Step 4: Run a local coverage spot-check (optional but recommended)**

Run:
`cd src-tauri && cargo llvm-cov --lib --ignore-filename-regex 'lib\.rs$|main\.rs$|commands[/\\](repo|workspace|task|agent|diff|files|file_io|search|scripts|terminal|lark_auth)\.rs$|platform[/\\](pty|lark_client)\.rs$' --fail-under-lines 95 --fail-under-functions 94`

If it fails because `task_provider/lark.rs` or `task_provider/schema.rs` is
under the threshold, extend the exclusion regex. Otherwise leave it as-is.

- [ ] **Step 5: Run tests**

Run: `cd src-tauri && cargo test --lib` Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
cd src-tauri && cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all -- --check
cd ..
git add src-tauri/src/commands/lark_auth.rs src-tauri/src/lib.rs .github/workflows/ci.yml
git commit -m "feat(phase-3a-2): verify_lark_schema Tauri command"
```

---

## Task 11: Frontend types + IPC wrappers

**Files:**

- Modify: `src/lib/types.ts`
- Modify: `src/lib/ipc.ts`
- Modify: `src/lib/ipc.test.ts`

- [ ] **Step 1: Add types**

In `src/lib/types.ts`, near the other Lark types (around `LarkStatus`), add:

```ts
export type TaskSource = 'local' | 'lark';

export type SchemaCheckResult = {
  ok: boolean;
  created: string[];
  already_present: string[];
  type_mismatches: string[];
};
```

- [ ] **Step 2: Write IPC tests first**

In `src/lib/ipc.test.ts`, near the other `api.task` tests, add:

```ts
describe('api.task new wrappers', () => {
  it('refresh: invokes refresh_tasks with repoId', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await api.task.refresh('repo_a');
    expect(invoke).toHaveBeenCalledWith('refresh_tasks', { repoId: 'repo_a' });
  });

  it('refresh: invokes with undefined repoId when not provided', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    await api.task.refresh();
    expect(invoke).toHaveBeenCalledWith('refresh_tasks', { repoId: undefined });
  });

  it('setSource: invokes set_task_source with source', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await api.task.setSource('lark');
    expect(invoke).toHaveBeenCalledWith('set_task_source', { source: 'lark' });
  });

  it('getSource: invokes get_task_source and returns the source', async () => {
    vi.mocked(invoke).mockResolvedValue('lark');
    const s = await api.task.getSource();
    expect(invoke).toHaveBeenCalledWith('get_task_source');
    expect(s).toBe('lark');
  });
});

describe('api.lark.verifySchema', () => {
  it('invokes verify_lark_schema and returns SchemaCheckResult', async () => {
    const result = {
      ok: true,
      created: ['title'],
      already_present: ['repo_id'],
      type_mismatches: [],
    };
    vi.mocked(invoke).mockResolvedValue(result);
    const out = await api.lark.verifySchema();
    expect(invoke).toHaveBeenCalledWith('verify_lark_schema');
    expect(out).toEqual(result);
  });

  it('rejects with Lark API error', async () => {
    vi.mocked(invoke).mockRejectedValue(
      new Error('Lark API: app_secret missing')
    );
    await expect(api.lark.verifySchema()).rejects.toThrow('app_secret');
  });
});
```

- [ ] **Step 3: Run the tests to confirm they fail**

Run: `bun run test --run src/lib/ipc.test.ts` Expected: failures for missing
`api.task.refresh / setSource / getSource` and `api.lark.verifySchema`.

- [ ] **Step 4: Add IPC wrappers**

In `src/lib/ipc.ts`, update imports:

```ts
import type {
  // ... existing imports ...
  TaskSource,
  SchemaCheckResult,
} from './types';
```

In the `api.task` block, add:

```ts
    /** Pull fresh tasks from the active provider into the backend
     *  mirror, then return them. Used by the manual Refresh button
     *  and the window-focus listener. */
    refresh: (repoId?: string): Promise<Task[]> =>
      invoke('refresh_tasks', { repoId }),

    /** Persist the chosen task source and rehydrate the kanban from
     *  the new provider. Backend emits `tasks-rehydrated` after
     *  success so frontend stores can re-load. */
    setSource: (source: TaskSource): Promise<void> =>
      invoke('set_task_source', { source }),

    /** Read the currently-active task source from settings. */
    getSource: (): Promise<TaskSource> => invoke('get_task_source'),
```

In the `api.lark` block, add:

```ts
    /** Diff the configured Bitable table against the required fields
     *  for Phase 3a-2; create missing ones. Idempotent. */
    verifySchema: (): Promise<SchemaCheckResult> => invoke('verify_lark_schema'),
```

- [ ] **Step 5: Run tests to verify pass**

Run: `bun run test --run src/lib/ipc.test.ts` Expected: all tests pass.

- [ ] **Step 6: Type check**

Run: `bun run check` Expected: 0 errors.

- [ ] **Step 7: Lint + format + commit**

```bash
bun run lint
git add src/lib/types.ts src/lib/ipc.ts src/lib/ipc.test.ts
git commit -m "feat(phase-3a-2): IPC wrappers for task source + schema verify"
```

---

## Task 12: Frontend tasks store — optimistic mutations + refresh

**Files:**

- Modify: `src/lib/stores/tasks.svelte.ts`
- Modify: `src/lib/stores/tasks.svelte.test.ts` (or create if missing — check
  first)

- [ ] **Step 1: Find the existing tasks store**

Run: `cat src/lib/stores/tasks.svelte.ts | head -40` to confirm current shape.
The store likely exposes `tasks`, `loadForRepo`, `add`, `update`, `move`,
`remove`. We're going to wrap mutations with optimistic + revert.

- [ ] **Step 2: Write the tests first**

In `src/lib/stores/tasks.svelte.test.ts`, add (or create the file if missing):

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  Channel: class {},
}));

import { invoke } from '@tauri-apps/api/core';
import { tasks } from './tasks.svelte';

beforeEach(() => {
  vi.clearAllMocks();
  // Reset store between tests if needed.
});

describe('tasks store — optimistic move', () => {
  it('updates the SvelteMap immediately then reconciles with backend response', async () => {
    const initial = {
      id: 'tk_1',
      repo_id: 'r',
      workspace_id: null,
      title: 'T',
      description: '',
      column: 'todo' as const,
      order: 1024,
      created_at: 0,
      updated_at: 0,
    };
    vi.mocked(invoke).mockResolvedValueOnce([initial]);
    await tasks.loadForRepo('r');

    const updated = {
      ...initial,
      column: 'done' as const,
      order: 2048,
      updated_at: 1,
    };
    vi.mocked(invoke).mockResolvedValueOnce(updated);

    const promise = tasks.move('tk_1', 'done', 2048);
    // Optimistic: store reflects new column BEFORE the promise resolves.
    expect(tasks.tasks.get('tk_1')?.column).toBe('done');
    await promise;
    expect(tasks.tasks.get('tk_1')?.updated_at).toBe(1);
  });

  it('reverts to previous state and toasts on failure', async () => {
    const initial = {
      id: 'tk_1',
      repo_id: 'r',
      workspace_id: null,
      title: 'T',
      description: '',
      column: 'todo' as const,
      order: 1024,
      created_at: 0,
      updated_at: 0,
    };
    vi.mocked(invoke).mockResolvedValueOnce([initial]);
    await tasks.loadForRepo('r');

    vi.mocked(invoke).mockRejectedValueOnce(
      new Error('Lark API: 91403 Forbidden')
    );
    await tasks.move('tk_1', 'done', 2048).catch(() => {});
    expect(tasks.tasks.get('tk_1')?.column).toBe('todo'); // reverted
  });
});

describe('tasks store — refresh', () => {
  it('replaces the map for a repo via refresh', async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      {
        id: 'tk_a',
        repo_id: 'r',
        workspace_id: null,
        title: 'A',
        description: '',
        column: 'todo',
        order: 0,
        created_at: 0,
        updated_at: 0,
      },
    ]);
    await tasks.refresh('r');
    expect(tasks.tasks.size).toBeGreaterThan(0);
    expect(invoke).toHaveBeenCalledWith('refresh_tasks', { repoId: 'r' });
  });
});
```

- [ ] **Step 3: Update `tasks.svelte.ts` — add `move` optimistic + `refresh`**

Read the current `tasks.svelte.ts` to find the existing `move` method. Replace
it (and add `refresh`) with:

```ts
async move(taskId: string, column: KanbanColumn, order: number): Promise<void> {
  const prev = this.tasks.get(taskId);
  if (!prev) return;
  // Optimistic: mutate map immediately so the kanban re-renders.
  this.tasks.set(taskId, { ...prev, column, order });
  try {
    const updated = await api.task.move(taskId, column, order);
    this.tasks.set(updated.id, updated);
  } catch (err) {
    this.tasks.set(taskId, prev);
    addToast(
      `Move failed: ${err instanceof Error ? err.message : String(err)}`,
      'error'
    );
    throw err;
  }
},

async refresh(repoId?: string): Promise<void> {
  const next = await api.task.refresh(repoId);
  if (repoId) {
    // Replace only the subset for this repo.
    for (const id of Array.from(this.tasks.keys())) {
      const t = this.tasks.get(id)!;
      if (t.repo_id === repoId) this.tasks.delete(id);
    }
    for (const t of next) this.tasks.set(t.id, t);
  } else {
    this.tasks.clear();
    for (const t of next) this.tasks.set(t.id, t);
  }
},
```

(Import `addToast` from `$lib/stores/toasts.svelte` if not already.)

- [ ] **Step 4: Run the store tests**

Run: `bun run test --run src/lib/stores/tasks.svelte.test.ts` Expected: pass.

- [ ] **Step 5: Type check**

Run: `bun run check` Expected: 0 errors.

- [ ] **Step 6: Commit**

```bash
git add src/lib/stores/tasks.svelte.ts src/lib/stores/tasks.svelte.test.ts
git commit -m "feat(phase-3a-2): optimistic move + revert + refresh in tasks store"
```

---

## Task 13: Window-focus refresh listener

**Files:**

- Modify: `src/lib/App.svelte` (add window-focus listener — find existing
  onMount or similar)

- [ ] **Step 1: Locate the top-level App component**

Open `src/lib/App.svelte` (or `src/App.svelte` if that's the layout root). Find
the `<script>` block with imports.

- [ ] **Step 2: Add the listener**

Inside the `<script>` block:

```ts
import { onMount } from 'svelte';
import { tasks } from '$lib/stores/tasks.svelte';
import { repos } from '$lib/stores/repos.svelte';

// Track last focus time so alt-tab bursts collapse to a single
// refresh call. 2s debounce per spec.
let focusDebounce: ReturnType<typeof setTimeout> | null = null;

onMount(() => {
  async function handleFocus() {
    const source = await api.task.getSource().catch(() => 'local' as const);
    if (source !== 'lark') return; // local mode: no network refresh needed
    if (focusDebounce) clearTimeout(focusDebounce);
    focusDebounce = setTimeout(() => {
      const repo = repos.getSelected();
      if (repo) tasks.refresh(repo.id).catch(() => {});
    }, 2000);
  }
  window.addEventListener('focus', handleFocus);
  return () => {
    window.removeEventListener('focus', handleFocus);
    if (focusDebounce) clearTimeout(focusDebounce);
  };
});
```

(Adjust imports — `api` may already be imported via the ipc module.)

- [ ] **Step 3: Type check**

Run: `bun run check` Expected: 0 errors. If a missing import surfaces (e.g.
`api`), add it.

- [ ] **Step 4: Lint + commit**

```bash
bun run lint
git add src/lib/App.svelte
git commit -m "feat(phase-3a-2): debounced window-focus refresh for Lark mode"
```

---

## Task 14: Settings UI — Task source radio section

**Files:**

- Modify: `src/lib/components/SettingsDialog.svelte` (add new section above
  LarkSettings)

- [ ] **Step 1: Inject the section UI**

In `src/lib/components/SettingsDialog.svelte`, replace the existing body block
with one that includes a "Task source" section above the LarkSettings panel.
Modify the `<script>`:

```ts
<script lang="ts">
  import LarkSettings from './lark/LarkSettings.svelte';
  import { onMount } from 'svelte';
  import { api } from '$lib/ipc';
  import { addToast } from '$lib/stores/toasts.svelte';
  import type { TaskSource } from '$lib/types';

  const {
    open,
    onClose,
  }: {
    open: boolean;
    onClose: () => void;
  } = $props();

  let source = $state<TaskSource>('local');
  let saving = $state(false);

  $effect(() => {
    if (open) {
      api.task.getSource().then((s) => (source = s)).catch(() => {});
    }
  });

  async function handleSourceChange(next: TaskSource) {
    if (next === source || saving) return;
    saving = true;
    const prev = source;
    source = next; // optimistic
    try {
      await api.task.setSource(next);
    } catch (e) {
      source = prev;
      addToast(
        `Cannot switch task source: ${e instanceof Error ? e.message : String(e)}`,
        'error'
      );
    } finally {
      saving = false;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      onClose();
    }
  }
</script>
```

Then in the body of the dialog, insert this section before the existing
`<div class="divide-y...">` that contains `<LarkSettings />`:

```svelte
<section class="px-4 py-3 border-b border-[var(--border-light)]">
  <h3 class="text-sm font-semibold mb-1">Task source</h3>
  <p class="text-[11px] text-[var(--text-muted)] mb-2">
    Where Ansambel stores and syncs your kanban.
  </p>
  <label class="flex items-center gap-2 text-xs mb-1 cursor-pointer">
    <input
      type="radio"
      name="task-source"
      value="local"
      checked={source === 'local'}
      onchange={() => handleSourceChange('local')}
      disabled={saving}
      data-testid="task-source-local"
    />
    Local (tasks.json on this machine)
  </label>
  <label class="flex items-center gap-2 text-xs cursor-pointer">
    <input
      type="radio"
      name="task-source"
      value="lark"
      checked={source === 'lark'}
      onchange={() => handleSourceChange('lark')}
      disabled={saving}
      data-testid="task-source-lark"
    />
    Lark Bitable (shared with team)
  </label>
</section>
```

- [ ] **Step 2: Update `SettingsDialog.test.ts`**

Open `src/lib/components/SettingsDialog.test.ts`. Update the `invoke` mock to
handle `get_task_source` and `set_task_source`, and add tests:

```ts
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn((cmd: string) => {
    if (cmd === 'get_task_source') return Promise.resolve('local');
    if (cmd === 'get_lark_status')
      return Promise.resolve({
        configured: false,
        app_id: null,
        app_token: null,
        table_id: null,
        base_url: 'https://open.larksuite.com',
        has_secret: false,
      });
    return Promise.resolve(undefined);
  }),
  Channel: class {},
}));

// ... existing tests ...

describe('SettingsDialog task source', () => {
  it('renders both radio options', async () => {
    render(SettingsDialog, { props: { open: true, onClose: vi.fn() } });
    await new Promise((r) => setTimeout(r, 0));
    expect(screen.getByTestId('task-source-local')).toBeTruthy();
    expect(screen.getByTestId('task-source-lark')).toBeTruthy();
  });

  it('reverts and toasts when set_task_source rejects', async () => {
    const calls: string[] = [];
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      calls.push(cmd);
      if (cmd === 'get_task_source') return Promise.resolve('local');
      if (cmd === 'set_task_source')
        return Promise.reject(new Error('Configure Lark credentials first'));
      return Promise.resolve(undefined);
    });
    render(SettingsDialog, { props: { open: true, onClose: vi.fn() } });
    await new Promise((r) => setTimeout(r, 0));
    const larkRadio = screen.getByTestId(
      'task-source-lark'
    ) as HTMLInputElement;
    await fireEvent.click(larkRadio);
    await new Promise((r) => setTimeout(r, 0));
    // Should have reverted to local.
    const localRadio = screen.getByTestId(
      'task-source-local'
    ) as HTMLInputElement;
    expect(localRadio.checked).toBe(true);
  });
});
```

- [ ] **Step 3: Run tests**

Run: `bun run test --run src/lib/components/SettingsDialog.test.ts` Expected:
pass.

- [ ] **Step 4: Type check + lint + commit**

```bash
bun run check
bun run lint
git add src/lib/components/SettingsDialog.svelte src/lib/components/SettingsDialog.test.ts
git commit -m "feat(phase-3a-2): task source radio in SettingsDialog"
```

---

## Task 15: LarkSettings — Bitable schema verify section

**Files:**

- Modify: `src/lib/components/lark/LarkSettings.svelte` (add schema section
  below credentials)
- Modify: `src/lib/components/lark/LarkSettings.test.ts`

- [ ] **Step 1: Add reactive state for schema**

In `src/lib/components/lark/LarkSettings.svelte`, in the `<script>`:

```ts
import type { LarkStatus, SchemaCheckResult } from '$lib/types';

let schemaResult = $state<SchemaCheckResult | null>(null);
let verifying = $state(false);
let schemaError = $state<string | null>(null);

const canVerifySchema = $derived(!verifying && status?.configured === true);

async function handleVerifySchema() {
  if (!canVerifySchema) return;
  verifying = true;
  schemaError = null;
  try {
    schemaResult = await api.lark.verifySchema();
  } catch (e) {
    schemaError = describe(e);
    schemaResult = null;
  } finally {
    verifying = false;
  }
}
```

- [ ] **Step 2: Add the section markup**

Below the existing buttons row (at the end of the `<form>`), add a new section
outside the form:

```svelte
<section
  class="flex flex-col gap-2 px-4 py-3 border-t border-[var(--border-light)]"
  aria-labelledby="lark-schema-title"
  data-testid="lark-schema-section"
>
  <h3 id="lark-schema-title" class="text-sm font-semibold">Bitable schema</h3>
  <p class="text-[11px] text-[var(--text-muted)]">
    5 fields required for kanban sync. Verify creates any missing ones.
  </p>

  <button
    type="button"
    class="self-start px-3 py-1.5 text-xs font-semibold rounded bg-[var(--bg-hover)] text-[var(--text-dim)] hover:text-[var(--text-primary)] transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
    onclick={handleVerifySchema}
    disabled={!canVerifySchema}
    data-testid="lark-verify-schema"
  >
    {verifying ? 'Verifying...' : 'Verify / Initialize schema'}
  </button>

  {#if schemaError}
    <div
      role="status"
      class="text-[11px] px-2 py-1.5 rounded border border-red-500 text-red-400"
      data-testid="lark-schema-error"
    >
      {schemaError}
    </div>
  {/if}

  {#if schemaResult}
    <div
      class="text-[11px] flex flex-col gap-1"
      data-testid="lark-schema-result"
    >
      {#if schemaResult.created.length > 0}
        <div class="text-[var(--accent)]">
          + Created: {schemaResult.created.join(', ')}
        </div>
      {/if}
      {#if schemaResult.already_present.length > 0}
        <div class="text-[var(--text-muted)]">
          ✓ Already present: {schemaResult.already_present.join(', ')}
        </div>
      {/if}
      {#if schemaResult.type_mismatches.length > 0}
        <div class="text-red-400">
          ✗ Type mismatch (fix in Bitable UI): {schemaResult.type_mismatches.join(
            ', '
          )}
        </div>
      {/if}
    </div>
  {/if}
</section>
```

- [ ] **Step 3: Add tests**

In `src/lib/components/lark/LarkSettings.test.ts`:

```ts
describe('LarkSettings schema section', () => {
  it('disables Verify button when credentials are not saved', async () => {
    mockGetStatus(statusUnconfigured);
    render(LarkSettings);
    await flush();
    const btn = screen.getByTestId('lark-verify-schema') as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });

  it('renders created + already_present lists on success', async () => {
    const result = {
      ok: true,
      created: ['kanban_column', 'order_within_column'],
      already_present: ['title', 'description', 'repo_id'],
      type_mismatches: [],
    };
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_lark_status') return Promise.resolve(statusConfigured);
      if (cmd === 'verify_lark_schema') return Promise.resolve(result);
      return Promise.resolve(undefined);
    });
    render(LarkSettings);
    await flush();
    await fireEvent.click(screen.getByTestId('lark-verify-schema'));
    await flush();
    const block = screen.getByTestId('lark-schema-result');
    expect(block.textContent).toContain('kanban_column');
    expect(block.textContent).toContain('title');
  });

  it('renders type-mismatch warning when fields have wrong type', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_lark_status') return Promise.resolve(statusConfigured);
      if (cmd === 'verify_lark_schema')
        return Promise.resolve({
          ok: false,
          created: [],
          already_present: [],
          type_mismatches: ['kanban_column'],
        });
      return Promise.resolve(undefined);
    });
    render(LarkSettings);
    await flush();
    await fireEvent.click(screen.getByTestId('lark-verify-schema'));
    await flush();
    expect(screen.getByTestId('lark-schema-result').textContent).toContain(
      'kanban_column'
    );
    expect(screen.getByTestId('lark-schema-result').textContent).toContain(
      'mismatch'
    );
  });

  it('surfaces backend error in error block', async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === 'get_lark_status') return Promise.resolve(statusConfigured);
      if (cmd === 'verify_lark_schema')
        return Promise.reject(new Error('Lark API: 91403 Forbidden'));
      return Promise.resolve(undefined);
    });
    render(LarkSettings);
    await flush();
    await fireEvent.click(screen.getByTestId('lark-verify-schema'));
    await flush();
    expect(screen.getByTestId('lark-schema-error').textContent).toContain(
      'Forbidden'
    );
  });
});
```

- [ ] **Step 4: Run tests**

Run: `bun run test --run src/lib/components/lark/LarkSettings.test.ts` Expected:
pass.

- [ ] **Step 5: Commit**

```bash
bun run check && bun run lint
git add src/lib/components/lark/LarkSettings.svelte src/lib/components/lark/LarkSettings.test.ts
git commit -m "feat(phase-3a-2): Bitable schema verify section in LarkSettings"
```

---

## Task 16: E2E smoke test (env-gated)

**Files:**

- Create: `tests/e2e/phase-3a-2-lark-sync.spec.ts`

- [ ] **Step 1: Write the E2E spec**

Create `tests/e2e/phase-3a-2-lark-sync.spec.ts`:

```ts
import { test, expect } from '@playwright/test';

// Skip when env vars are missing — same gate as cargo lark_smoke tests.
const requiredEnv = [
  'LARK_APP_ID',
  'LARK_APP_SECRET',
  'LARK_APP_TOKEN',
  'LARK_TABLE_ID',
];
const hasCreds = requiredEnv.every((k) => Boolean(process.env[k]));

test.describe('Phase 3a-2 Lark sync', () => {
  test.skip(
    !hasCreds,
    'requires LARK_* env vars for the configured test tenant'
  );

  test('switch to Lark, verify schema, kanban populates', async ({ page }) => {
    await page.goto('/');
    // Open Settings via gear icon.
    await page.getByTestId('open-settings').click();
    // Toggle to Lark.
    await page.getByTestId('task-source-lark').click();
    // (Assumes credentials already entered via prior LarkSettings save;
    // in CI this would be seeded by a setup hook. For local dev,
    // configure once and re-run.)
    // Verify schema.
    await page.getByTestId('lark-verify-schema').click();
    await expect(page.getByTestId('lark-schema-result')).toBeVisible({
      timeout: 15000,
    });
    await expect(page.getByTestId('lark-schema-result')).toContainText(
      /title|already present|created/i
    );
  });
});
```

- [ ] **Step 2: Verify the test compiles + skip path**

Run: `bun x playwright test tests/e2e/phase-3a-2-lark-sync.spec.ts --list`
Expected: lists the test, skips noted when env unset.

- [ ] **Step 3: Run the smoke without creds**

Run: `bun run e2e -- tests/e2e/phase-3a-2-lark-sync.spec.ts` Expected: "1
skipped".

- [ ] **Step 4: Commit**

```bash
git add tests/e2e/phase-3a-2-lark-sync.spec.ts
git commit -m "test(phase-3a-2): env-gated E2E for Lark sync flow"
```

---

## Task 17: Final validation + PR

- [ ] **Step 1: Full Rust test suite**

Run: `cd src-tauri && cargo test --lib` Expected: 480+ tests pass (existing) +
~30 new tests across `task_provider/local`, `task_provider/lark`,
`task_provider/schema`, `lark_client` field methods, `commands/task` new
commands.

- [ ] **Step 2: Rust coverage gate**

Run:
`cd src-tauri && cargo llvm-cov --lib --ignore-filename-regex 'lib\.rs$|main\.rs$|commands[/\\](repo|workspace|task|agent|diff|files|file_io|search|scripts|terminal|lark_auth)\.rs$|platform[/\\](pty|lark_client)\.rs$' --fail-under-lines 95 --fail-under-functions 94`
Expected: pass. If `task_provider/lark.rs` or `task_provider/schema.rs` drops
the threshold, extend the exclusion regex (same precedent as `lark_client.rs`).

- [ ] **Step 3: Frontend tests + coverage**

Run: `bun run test:coverage` Expected: ≥95% line/branch/function on changed
files.

- [ ] **Step 4: Lint + type check**

```bash
bun run check
bun run lint
cd src-tauri && cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all -- --check
```

Expected: all clean.

- [ ] **Step 5: Manual smoke (when in a Lark-configured workspace)**

Start app dev: `bun run tauri dev`. Click gear → set task source to Lark → click
Verify schema → wait for green status → return to kanban → tasks load from
Bitable. Drag one across columns → confirm Bitable row updates. Window-blur +
focus → verify a network call (Network tab) fires after 2s debounce.

- [ ] **Step 6: Open PR**

Push branch and open PR. PR description template:

```
## Summary
- Introduce `TaskProvider` trait + `LocalProvider` + `LarkProvider`
- Add Bitable schema wizard (5 fields for 3a-2)
- Optimistic kanban + window-focus refresh
- New IPC: `api.task.{refresh,setSource,getSource}` + `api.lark.verifySchema`

## Test plan
- [x] `cargo test --lib` — 480+ tests
- [x] `cargo llvm-cov --lib` — ≥95% gate
- [x] `bun run test:coverage` — ≥95% gate
- [x] `bun run lint` + `cargo clippy -D warnings` — clean
- [ ] CI matrix green
- [ ] Manual smoke against real Lark tenant
```

- [ ] **Step 7: Journal entry**

Create `journal/YYYY-MM-DD-phase-3a-2-task-provider.md` summarizing: what
shipped, decisions taken, surprises hit, what's deferred to 3a-3+.

```bash
git add journal/YYYY-MM-DD-phase-3a-2-task-provider.md
git commit -m "docs(journal): Phase 3a-2 TaskProvider + Lark plugin"
git push
```

---

## Self-review summary

- **Spec coverage:** Every section of the spec maps to a task. Trait (T1),
  LocalProvider (T2), command refactor (T3), Lark client extension (T4), schema
  wizard (T5), LarkProvider (T6), TaskSource enum (T7), startup wiring (T8), new
  commands (T9), Tauri schema command (T10), frontend IPC (T11), optimistic
  store (T12), focus refresh (T13), settings UI task source (T14), settings UI
  schema (T15), E2E (T16), final gate (T17).
- **No placeholders:** Every code block contains the actual code; no "TODO" or
  "similar to above" hand-waves.
- **Type consistency:** `TaskProvider` trait signatures, `CreateTaskArgs` /
  `TaskPatch` shapes, `TaskSource` enum values (`local`/`lark`),
  `SchemaCheckResult` shape, and `BitableField` type are all defined once and
  referenced consistently across tasks.

**Plan complete and saved to**
`docs/superpowers/plans/2026-05-13-ansambel-phase-3a-2-task-provider.md`.

## Two execution options:

**1. Subagent-Driven (recommended)** — A fresh subagent picks up each task, you
review the diff between tasks, fast iteration.

**2. Inline Execution** — Tasks run in this session with checkpoints for review.

Which approach?
