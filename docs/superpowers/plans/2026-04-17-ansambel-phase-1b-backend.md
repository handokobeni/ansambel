# Ansambel — Phase 1b Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking. Execute **after** Phase 1a (both
> backend and frontend) is merged.

**Goal:** Build the task/kanban data layer on top of Phase 1a — `Task` struct,
`tasks.json` persistence, 5 task commands (add / list / update / move / remove),
auto-workspace-creation when a task moves into the `InProgress` column — so
Phase 1b-frontend can render the kanban UI and wire drag-drop actions to these
commands.

**Architecture:** Add a third primary entity (`Task`) alongside `RepoInfo` and
`WorkspaceInfo`. Tasks belong to a repo; when moved to `InProgress`, auto-spawn
a workspace (reusing Phase 1a's `create_workspace_inner`). Sparse ordering
within columns (×1024 stride) so drag-drop can insert tasks without reindexing.

**Tech Stack:** Rust 1.82, Tauri v2 command handlers, serde for JSON, reuses
Phase 1a's `persistence` + `platform::paths` + `ids` + `exec_git`
infrastructure.

**Prerequisite:** Phase 1a-backend + Phase 1a-frontend merged (PR #4, #5).

---

## Task 1: Add `Task` struct + `AppState.tasks` field

**Files:**

- Modify: `src-tauri/src/state.rs`

- [ ] **Step 1.1: Write failing tests**

```rust
// Add to src-tauri/src/state.rs  #[cfg(test)] mod tests:

#[test]
fn task_round_trips_json() {
    let t = Task {
        id: "tk_abc123".into(),
        repo_id: "repo_xyz".into(),
        workspace_id: None,
        title: "Fix login bug".into(),
        description: "Auth fails on mobile".into(),
        column: KanbanColumn::Todo,
        order: 1024,
        created_at: 1_776_000_000,
        updated_at: 1_776_099_000,
    };
    let json = serde_json::to_string(&t).unwrap();
    let back: Task = serde_json::from_str(&json).unwrap();
    assert_eq!(back, t);
}

#[test]
fn task_workspace_id_nullable() {
    let t = Task {
        id: "tk_aaa111".into(),
        repo_id: "repo_r1".into(),
        workspace_id: Some("ws_xyz".into()),
        title: "With workspace".into(),
        description: String::new(),
        column: KanbanColumn::InProgress,
        order: 2048,
        created_at: 0,
        updated_at: 0,
    };
    let json = serde_json::to_string(&t).unwrap();
    assert!(json.contains("\"workspace_id\":\"ws_xyz\""));
    let none_task = Task {
        workspace_id: None,
        id: "tk_bbb222".into(),
        repo_id: "repo_r2".into(),
        title: String::new(),
        description: String::new(),
        column: KanbanColumn::Todo,
        order: 0,
        created_at: 0,
        updated_at: 0,
    };
    let none_json = serde_json::to_string(&none_task).unwrap();
    assert!(none_json.contains("\"workspace_id\":null"));
}

#[test]
fn app_state_has_tasks_field() {
    let state = AppState::default();
    assert!(state.tasks.is_empty());
}

#[test]
fn task_column_uses_kanban_column_enum() {
    let t = Task {
        id: "tk_c1".into(),
        repo_id: "repo_r1".into(),
        workspace_id: None,
        title: "Review task".into(),
        description: String::new(),
        column: KanbanColumn::Review,
        order: 3072,
        created_at: 0,
        updated_at: 0,
    };
    let json = serde_json::to_string(&t).unwrap();
    assert!(json.contains("\"column\":\"review\""));
}
```

- [ ] **Step 1.2: Run tests to verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib state 2>&1 | tail -15
```

Expected: compile error — `Task` type not found.

- [ ] **Step 1.3: Implement**

Add to `src-tauri/src/state.rs` after the `WorkspaceInfo` struct and before
`app_version()`:

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Task {
    pub id: String,                    // prefix `tk_`
    pub repo_id: String,               // owning repo
    pub workspace_id: Option<String>,  // populated when moved to InProgress
    pub title: String,
    pub description: String,
    pub column: KanbanColumn,          // reuses Phase 1a enum
    pub order: i32,                    // within-column sort order (higher = top)
    pub created_at: i64,
    pub updated_at: i64,
}
```

Extend `AppState`:

```rust
#[derive(Default, Debug)]
pub struct AppState {
    pub repos: std::collections::HashMap<String, RepoInfo>,
    pub workspaces: std::collections::HashMap<String, WorkspaceInfo>,
    pub tasks: std::collections::HashMap<String, Task>,    // NEW
    pub settings: AppSettings,
}
```

- [ ] **Step 1.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib state 2>&1 | tail -10
```

Expected: `test result: ok. N passed`.

- [ ] **Step 1.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/state.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): add Task struct and AppState.tasks field

Add Task with id/repo_id/workspace_id/title/description/column/order/
created_at/updated_at. Reuse KanbanColumn from Phase 1a. Extend
AppState with tasks: HashMap<String, Task> defaulting to empty.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Add `task_id()` to `ids.rs`

**Files:**

- Modify: `src-tauri/src/ids.rs`

- [ ] **Step 2.1: Write failing tests**

```rust
// Add to src-tauri/src/ids.rs  #[cfg(test)] mod tests:

#[test]
fn task_id_has_prefix_and_length() {
    let id = task_id();
    assert!(id.starts_with("tk_"), "expected tk_ prefix, got {id}");
    assert_eq!(id.len(), "tk_".len() + 6);
}

#[test]
fn task_id_uses_only_allowed_alphabet() {
    let id = task_id();
    let body = id.strip_prefix("tk_").unwrap();
    for c in body.chars() {
        assert!(
            c.is_ascii_alphanumeric() && c.is_ascii_lowercase() || c.is_ascii_digit(),
            "Unexpected char {:?} in id {}",
            c,
            id
        );
    }
}

#[test]
fn task_id_no_collisions() {
    let set: std::collections::HashSet<String> = (0..1_000).map(|_| task_id()).collect();
    assert_eq!(set.len(), 1_000);
}
```

- [ ] **Step 2.2: Run tests to verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib ids 2>&1 | tail -15
```

Expected: compile error — `task_id` not found.

- [ ] **Step 2.3: Implement**

Add to `src-tauri/src/ids.rs` after `script_id()`:

```rust
pub fn task_id() -> String {
    format!("tk_{}", id_body())
}
```

- [ ] **Step 2.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib ids 2>&1 | tail -10
```

Expected: `test result: ok. N passed`.

- [ ] **Step 2.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/ids.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): add task_id() generator with tk_ prefix

Follows the same nanoid(6, ALPHABET) pattern as repo_id, workspace_id,
etc. Prefix tk_ is short, collision-resistant, and visually distinct.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: `persistence/tasks.rs` — load + save

**Files:**

- Create: `src-tauri/src/persistence/tasks.rs`
- Modify: `src-tauri/src/persistence/mod.rs`
- Modify: `src-tauri/src/platform/paths.rs`

- [ ] **Step 3.1: Write failing tests**

```rust
// New file src-tauri/src/persistence/tasks.rs — write the #[cfg(test)] mod first:

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{KanbanColumn, Task};

    fn make_task(id: &str) -> Task {
        Task {
            id: id.into(),
            repo_id: "repo_xyz".into(),
            workspace_id: None,
            title: "Sample task".into(),
            description: "A description".into(),
            column: KanbanColumn::Todo,
            order: 1024,
            created_at: 1_776_000_000,
            updated_at: 1_776_000_001,
        }
    }

    #[test]
    fn save_and_load_tasks_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut map = std::collections::HashMap::new();
        map.insert("tk_abc".into(), make_task("tk_abc"));
        save_tasks(tmp.path(), &map).unwrap();

        let loaded = load_tasks(tmp.path()).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded["tk_abc"].title, "Sample task");
    }

    #[test]
    fn load_tasks_missing_file_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let loaded = load_tasks(tmp.path()).unwrap();
        assert!(loaded.is_empty());
    }

    #[test]
    fn save_tasks_writes_schema_version() {
        let tmp = tempfile::tempdir().unwrap();
        let map: std::collections::HashMap<String, Task> = std::collections::HashMap::new();
        save_tasks(tmp.path(), &map).unwrap();

        let content =
            std::fs::read_to_string(crate::platform::paths::tasks_file(tmp.path())).unwrap();
        assert!(content.contains("\"schema_version\""));
        assert!(content.contains("\"tasks\""));
    }

    #[test]
    fn save_tasks_preserves_workspace_id() {
        let tmp = tempfile::tempdir().unwrap();
        let mut t = make_task("tk_ws");
        t.workspace_id = Some("ws_yyy".into());
        let mut map = std::collections::HashMap::new();
        map.insert("tk_ws".into(), t);
        save_tasks(tmp.path(), &map).unwrap();

        let loaded = load_tasks(tmp.path()).unwrap();
        assert_eq!(loaded["tk_ws"].workspace_id, Some("ws_yyy".into()));
    }
}
```

Also add a test for `tasks_file` path helper to
`src-tauri/src/platform/paths.rs #[cfg(test)] mod tests`:

```rust
#[test]
fn tasks_file_is_at_data_dir_root() {
    let data = std::path::PathBuf::from("/tmp/ansambel");
    let p = tasks_file(&data);
    assert_eq!(p, std::path::PathBuf::from("/tmp/ansambel/tasks.json"));
}
```

- [ ] **Step 3.2: Run tests to verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib persistence::tasks 2>&1 | tail -15
```

Expected: compile error — module not found.

- [ ] **Step 3.3: Implement**

Add to `src-tauri/src/platform/paths.rs`:

```rust
pub fn tasks_file(data_dir: &Path) -> PathBuf {
    data_dir.join("tasks.json")
}
```

Create `src-tauri/src/persistence/tasks.rs`:

```rust
use crate::error::Result;
use crate::persistence::atomic::{load_or_default, write_atomic};
use crate::platform::paths::tasks_file;
use crate::state::Task;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Serialize, Deserialize, Default)]
struct TasksFile {
    schema_version: u32,
    tasks: HashMap<String, Task>,
}

pub fn load_tasks(data_dir: &Path) -> Result<HashMap<String, Task>> {
    let file: TasksFile = load_or_default(&tasks_file(data_dir))?;
    Ok(file.tasks)
}

pub fn save_tasks(data_dir: &Path, tasks: &HashMap<String, Task>) -> Result<()> {
    let file = TasksFile {
        schema_version: 1,
        tasks: tasks.clone(),
    };
    write_atomic(&tasks_file(data_dir), &file)
}

#[cfg(test)]
mod tests {
    // ... (tests written in step 3.1 above)
}
```

Add `pub mod tasks;` to `src-tauri/src/persistence/mod.rs`.

- [ ] **Step 3.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib persistence::tasks 2>&1 | tail -10
cargo test --lib platform::paths 2>&1 | tail -10
```

Expected: `test result: ok. N passed` for both.

- [ ] **Step 3.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/persistence/tasks.rs \
        src-tauri/src/persistence/mod.rs \
        src-tauri/src/platform/paths.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): add persistence/tasks.rs and paths::tasks_file

load_tasks/save_tasks follow the same {schema_version, tasks} wrapper
pattern as repos.rs/workspaces.rs. tasks_file() helper added to
platform/paths.rs alongside repos_file and workspaces_file.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Startup hydration — load `tasks.json` into `AppState`

**Files:**

- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 4.1: Write failing tests**

The hydration path runs in the Tauri setup closure, which is not unit-testable
in isolation. The test here is a compile-only gate: if `AppState` construction
in `lib.rs` doesn't include `tasks`, the code won't compile. Write a unit test
inside `lib.rs` to verify the construction is coherent:

```rust
// This test goes at the bottom of src-tauri/src/lib.rs — gated by #[cfg(test)]:
#[cfg(test)]
mod tests {
    #[test]
    fn app_state_construction_includes_tasks_field() {
        use std::collections::HashMap;
        use crate::state::{AppState, AppSettings};
        // Verify the struct literal compiles with all three entity maps.
        let state = AppState {
            repos: HashMap::new(),
            workspaces: HashMap::new(),
            tasks: HashMap::new(),
            settings: AppSettings::default(),
        };
        assert!(state.tasks.is_empty());
    }
}
```

- [ ] **Step 4.2: Run tests to verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib 2>&1 | grep "app_state_construction" | tail -5
```

Expected: compile error if tasks field is missing from AppState (already added
in Task 1, so this may pass immediately — the test validates correctness not
failure here).

- [ ] **Step 4.3: Implement**

Update the `setup` closure in `src-tauri/src/lib.rs`:

```rust
.setup(|app| {
    let data_dir = app.path().app_data_dir().expect("resolve app data dir");
    crate::platform::paths::ensure_data_dirs(&data_dir)?;
    let guard = crate::logging::init(&data_dir)?;
    app.manage(std::sync::Arc::new(std::sync::Mutex::new(Some(guard))));
    crate::panic::install_hook(data_dir.clone());

    // Hydrate AppState from disk
    let repos = crate::persistence::repos::load_repos(&data_dir)?;
    let workspaces = crate::persistence::workspaces::load_and_reset_running(&data_dir)?;
    let tasks = crate::persistence::tasks::load_tasks(&data_dir)?;
    let settings = crate::persistence::settings::load_settings(&data_dir)?;

    let state = crate::state::AppState {
        repos,
        workspaces,
        tasks,
        settings,
    };

    app.manage(std::sync::Arc::new(std::sync::Mutex::new(state)));
    Ok(())
})
```

- [ ] **Step 4.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib 2>&1 | tail -10
```

Expected: all tests pass, no regressions.

- [ ] **Step 4.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/lib.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): hydrate tasks.json into AppState on startup

Extend the Tauri setup closure to call persistence::tasks::load_tasks
and populate AppState.tasks alongside repos and workspaces. Tasks are
loaded fresh from disk on every app launch; no status coercion needed
(tasks have no runtime-only state like WorkspaceStatus::Running).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: `commands/task.rs` — `add_task`

**Files:**

- Create: `src-tauri/src/commands/task.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 5.1: Write failing tests**

```rust
// New file src-tauri/src/commands/task.rs — write #[cfg(test)] mod first:

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{AppState, KanbanColumn, RepoInfo};
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;

    fn make_state_with_repo(data_dir: &std::path::Path) -> Arc<Mutex<AppState>> {
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
}
```

- [ ] **Step 5.2: Run tests to verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::task 2>&1 | tail -15
```

Expected: compile error — module not found.

- [ ] **Step 5.3: Implement**

Create `src-tauri/src/commands/task.rs`:

```rust
use crate::error::{AppError, Result};
use crate::ids::task_id;
use crate::persistence::tasks::save_tasks;
use crate::state::{AppState, KanbanColumn, Task};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{Manager, State};

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
    add_task_inner(repo_id, title, description, column, data_dir, state.inner().clone())
        .map_err(|e| {
            tracing::error!(error = %e, "add_task failed");
            e.to_string()
        })
}

// ── Inner implementation ─────────────────────────────────────────────

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

#[cfg(test)]
mod tests {
    // ... (tests written in step 5.1 above)
}
```

Add `pub mod task;` to `src-tauri/src/commands/mod.rs`.

- [ ] **Step 5.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::task 2>&1 | tail -10
```

Expected: `test result: ok. N passed`.

- [ ] **Step 5.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/commands/task.rs src-tauri/src/commands/mod.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): add commands/task.rs with add_task

add_task_inner validates repo existence, computes sparse order
(max_in_column + 1024), generates tk_ id, persists atomically.
Unknown repo returns NotFound error. Default column is Todo.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: `commands/task.rs` — `list_tasks`

**Files:**

- Modify: `src-tauri/src/commands/task.rs`

- [ ] **Step 6.1: Write failing tests**

```rust
// Add to src-tauri/src/commands/task.rs  #[cfg(test)] mod tests:

#[test]
fn list_tasks_filters_by_repo_id() {
    let tmp = tempdir().unwrap();
    let state = make_state_with_repo(tmp.path());

    add_task_inner(
        "repo_r1".into(), "Task A".into(), String::new(), None,
        tmp.path().to_path_buf(), Arc::clone(&state),
    ).unwrap();
    add_task_inner(
        "repo_r1".into(), "Task B".into(), String::new(), None,
        tmp.path().to_path_buf(), Arc::clone(&state),
    ).unwrap();

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
            Task { id: "tk_t1".into(), repo_id: "repo_r1".into(), workspace_id: None,
                   title: "T1".into(), description: String::new(),
                   column: KanbanColumn::Todo, order: 1024, created_at: 0, updated_at: 0 },
            Task { id: "tk_t2".into(), repo_id: "repo_r1".into(), workspace_id: None,
                   title: "T2".into(), description: String::new(),
                   column: KanbanColumn::Todo, order: 2048, created_at: 0, updated_at: 0 },
            Task { id: "tk_ip1".into(), repo_id: "repo_r1".into(), workspace_id: None,
                   title: "IP1".into(), description: String::new(),
                   column: KanbanColumn::InProgress, order: 1024, created_at: 0, updated_at: 0 },
        ];
        for t in tasks {
            st.tasks.insert(t.id.clone(), t);
        }
    }

    let listed = list_tasks_inner("repo_r1".into(), Arc::clone(&state)).unwrap();
    assert_eq!(listed.len(), 3);

    // Verify column ordering: Todo first, then InProgress
    // Within Todo: order desc (2048 before 1024)
    let todo_tasks: Vec<_> = listed.iter().filter(|t| t.column == KanbanColumn::Todo).collect();
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
```

- [ ] **Step 6.2: Run tests to verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::task::tests::list_tasks 2>&1 | tail -15
```

Expected: compile error — `list_tasks_inner` not found.

- [ ] **Step 6.3: Implement**

Add to `src-tauri/src/commands/task.rs`:

```rust
#[tauri::command]
pub fn list_tasks(
    repo_id: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<Vec<Task>, String> {
    list_tasks_inner(repo_id, state.inner().clone()).map_err(|e| e.to_string())
}

pub(crate) fn list_tasks_inner(
    repo_id: String,
    state: Arc<Mutex<AppState>>,
) -> Result<Vec<Task>> {
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
```

- [ ] **Step 6.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::task 2>&1 | tail -10
```

Expected: `test result: ok. N passed`.

- [ ] **Step 6.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/commands/task.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): add list_tasks command

Filters by repo_id; sorts by column (Todo→InProgress→Review→Done)
then by order descending (higher order = top of column). Pure in-memory
read — no disk I/O.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: `commands/task.rs` — `update_task` with `TaskPatch`

**Files:**

- Modify: `src-tauri/src/commands/task.rs`

- [ ] **Step 7.1: Write failing tests**

```rust
// Add to src-tauri/src/commands/task.rs  #[cfg(test)] mod tests:

#[test]
fn update_task_title_and_description() {
    let tmp = tempdir().unwrap();
    let state = make_state_with_repo(tmp.path());

    let task = add_task_inner(
        "repo_r1".into(), "Original title".into(), "Original desc".into(),
        None, tmp.path().to_path_buf(), Arc::clone(&state),
    ).unwrap();

    let patch = TaskPatch {
        title: Some("Updated title".into()),
        description: Some("Updated desc".into()),
        order: None,
    };
    let updated = update_task_inner(
        task.id.clone(), patch, tmp.path().to_path_buf(), Arc::clone(&state),
    ).unwrap();

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
        "repo_r1".into(), "Keep me".into(), "Keep desc".into(),
        None, tmp.path().to_path_buf(), Arc::clone(&state),
    ).unwrap();

    let patch = TaskPatch {
        title: Some("New title only".into()),
        description: None,
        order: None,
    };
    let updated = update_task_inner(
        task.id.clone(), patch, tmp.path().to_path_buf(), Arc::clone(&state),
    ).unwrap();

    assert_eq!(updated.title, "New title only");
    assert_eq!(updated.description, "Keep desc"); // unchanged
}

#[test]
fn update_task_order_change() {
    let tmp = tempdir().unwrap();
    let state = make_state_with_repo(tmp.path());

    let task = add_task_inner(
        "repo_r1".into(), "T".into(), String::new(),
        None, tmp.path().to_path_buf(), Arc::clone(&state),
    ).unwrap();
    assert_eq!(task.order, 1024);

    let patch = TaskPatch { title: None, description: None, order: Some(512) };
    let updated = update_task_inner(
        task.id.clone(), patch, tmp.path().to_path_buf(), Arc::clone(&state),
    ).unwrap();

    assert_eq!(updated.order, 512);
}

#[test]
fn update_task_not_found_returns_err() {
    let tmp = tempdir().unwrap();
    let state = Arc::new(Mutex::new(AppState::default()));
    let patch = TaskPatch { title: Some("X".into()), description: None, order: None };
    let result = update_task_inner(
        "tk_nonexistent".into(), patch, tmp.path().to_path_buf(), state,
    );
    assert!(result.is_err());
}
```

- [ ] **Step 7.2: Run tests to verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::task::tests::update_task 2>&1 | tail -15
```

Expected: compile error — `TaskPatch` and `update_task_inner` not found.

- [ ] **Step 7.3: Implement**

Add to `src-tauri/src/commands/task.rs` (add `TaskPatch` struct near the top,
after imports):

```rust
#[derive(serde::Deserialize, Debug)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub order: Option<i32>,
}
```

Add the command and inner function:

```rust
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
```

- [ ] **Step 7.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::task 2>&1 | tail -10
```

Expected: `test result: ok. N passed`.

- [ ] **Step 7.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/commands/task.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): add update_task with TaskPatch partial update

TaskPatch supports optional title, description, and order. Column is
intentionally excluded — use move_task to change column. All None
fields leave the original value intact. updated_at is always refreshed.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: `commands/task.rs` — `move_task` with auto-workspace

**Files:**

- Modify: `src-tauri/src/commands/task.rs`

**Edge cases documented here:**

- Moving Todo→InProgress with no workspace: creates workspace via
  `create_workspace_inner`, links `task.workspace_id`.
- Moving InProgress→Review: workspace stays linked; no workspace created; no
  workspace removed.
- Moving Review→Done: same as above — workspace stays.
- Moving Todo→Review (skipping InProgress): does NOT create workspace (only
  `InProgress` triggers auto-creation).
- Moving any column→InProgress when `task.workspace_id` is already Some: does
  NOT create a second workspace.
- `move_task` does NOT auto-remove workspaces (user removes via sidebar
  explicitly).

- [ ] **Step 8.1: Write failing tests**

```rust
// Add to src-tauri/src/commands/task.rs  #[cfg(test)] mod tests:
// NOTE: move_task tests that involve workspace creation require a real git repo.

fn init_repo_with_remote_for_move(tmp: &tempfile::TempDir) -> (PathBuf, PathBuf) {
    use std::process::Command;
    let remote = tmp.path().join("remote.git");
    std::fs::create_dir_all(&remote).unwrap();
    Command::new("git").args(["init", "--bare"]).current_dir(&remote).output().unwrap();
    let local = tmp.path().join("local");
    Command::new("git")
        .args(["clone", remote.to_str().unwrap(), local.to_str().unwrap()])
        .output().unwrap();
    Command::new("git").args(["config", "user.email", "t@t.com"]).current_dir(&local).output().unwrap();
    Command::new("git").args(["config", "user.name", "T"]).current_dir(&local).output().unwrap();
    std::fs::write(local.join("f"), b"x").unwrap();
    Command::new("git").args(["add", "."]).current_dir(&local).output().unwrap();
    Command::new("git").args(["commit", "-m", "init"]).current_dir(&local).output().unwrap();
    Command::new("git").args(["push", "origin", "HEAD:main"]).current_dir(&local).output().unwrap();
    Command::new("git").args(["remote", "set-head", "origin", "main"]).current_dir(&local).output().unwrap();
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
    ).await.unwrap();

    let task = add_task_inner(
        repo.id.clone(), "Auto WS task".into(), "Description".into(),
        None, data.clone(), Arc::clone(&state),
    ).unwrap();
    assert!(task.workspace_id.is_none());

    let moved = move_task_inner(
        task.id.clone(), KanbanColumn::InProgress, task.order,
        data.clone(), Arc::clone(&state),
    ).await.unwrap();

    assert_eq!(moved.column, KanbanColumn::InProgress);
    assert!(moved.workspace_id.is_some(), "workspace_id should be populated");

    // Verify workspace exists in state
    let ws_id = moved.workspace_id.as_ref().unwrap();
    let st = state.lock().unwrap();
    assert!(st.workspaces.contains_key(ws_id), "workspace should be in AppState");
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
    ).await.unwrap();

    let task = add_task_inner(
        repo.id.clone(), "Review task".into(), String::new(),
        None, data.clone(), Arc::clone(&state),
    ).unwrap();

    // Move to InProgress first (creates workspace)
    let in_progress = move_task_inner(
        task.id.clone(), KanbanColumn::InProgress, task.order,
        data.clone(), Arc::clone(&state),
    ).await.unwrap();
    let ws_id = in_progress.workspace_id.clone().unwrap();

    // Move to Review
    let review = move_task_inner(
        task.id.clone(), KanbanColumn::Review, in_progress.order,
        data.clone(), Arc::clone(&state),
    ).await.unwrap();

    assert_eq!(review.column, KanbanColumn::Review);
    assert_eq!(review.workspace_id.as_deref(), Some(ws_id.as_str()),
               "workspace_id should remain after moving to Review");
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
    ).await.unwrap();

    let task = add_task_inner(
        repo.id.clone(), "Done task".into(), String::new(),
        None, data.clone(), Arc::clone(&state),
    ).unwrap();

    let in_progress = move_task_inner(
        task.id.clone(), KanbanColumn::InProgress, task.order,
        data.clone(), Arc::clone(&state),
    ).await.unwrap();
    let ws_id = in_progress.workspace_id.clone().unwrap();

    let review = move_task_inner(
        task.id.clone(), KanbanColumn::Review, in_progress.order,
        data.clone(), Arc::clone(&state),
    ).await.unwrap();

    let done = move_task_inner(
        task.id.clone(), KanbanColumn::Done, review.order,
        data.clone(), Arc::clone(&state),
    ).await.unwrap();

    assert_eq!(done.column, KanbanColumn::Done);
    assert_eq!(done.workspace_id.as_deref(), Some(ws_id.as_str()));
}

#[test]
fn move_task_todo_to_review_does_not_create_workspace() {
    let tmp = tempdir().unwrap();
    let state = make_state_with_repo(tmp.path());

    let task = add_task_inner(
        "repo_r1".into(), "Skip InProgress".into(), String::new(),
        None, tmp.path().to_path_buf(), Arc::clone(&state),
    ).unwrap();

    // move_task_inner is async; use tokio::runtime for this sync-ish test
    let rt = tokio::runtime::Runtime::new().unwrap();
    let moved = rt.block_on(move_task_inner(
        task.id.clone(), KanbanColumn::Review, task.order,
        tmp.path().to_path_buf(), Arc::clone(&state),
    )).unwrap();

    assert_eq!(moved.column, KanbanColumn::Review);
    assert!(moved.workspace_id.is_none(),
            "Moving Todo→Review should NOT create a workspace");
    let st = state.lock().unwrap();
    assert!(st.workspaces.is_empty());
}
```

- [ ] **Step 8.2: Run tests to verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::task::tests::move_task 2>&1 | tail -15
```

Expected: compile error — `move_task_inner` not found.

- [ ] **Step 8.3: Implement**

Add to `src-tauri/src/commands/task.rs`:

```rust
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
        (task.repo_id.clone(), task.title.clone(), task.description.clone(), needs)
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
```

- [ ] **Step 8.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::task 2>&1 | tail -10
```

Expected: `test result: ok. N passed`.

- [ ] **Step 8.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/commands/task.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): add move_task with auto-workspace side effect

When a task is moved into InProgress and has no workspace yet,
create_workspace_inner is called and task.workspace_id is linked.
Moving to any other column never creates or removes a workspace.
Already-linked workspace is preserved on all subsequent moves.
Mutex lock is always dropped before the async workspace creation.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: `commands/task.rs` — `remove_task`

**Files:**

- Modify: `src-tauri/src/commands/task.rs`

- [ ] **Step 9.1: Write failing tests**

```rust
// Add to src-tauri/src/commands/task.rs  #[cfg(test)] mod tests:

#[test]
fn remove_task_without_workspace_succeeds() {
    let tmp = tempdir().unwrap();
    let state = make_state_with_repo(tmp.path());

    let task = add_task_inner(
        "repo_r1".into(), "To remove".into(), String::new(),
        None, tmp.path().to_path_buf(), Arc::clone(&state),
    ).unwrap();

    remove_task_inner(
        task.id.clone(), false, tmp.path().to_path_buf(), Arc::clone(&state),
    ).unwrap();

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
        st.tasks.insert(task_id.clone(), Task {
            id: task_id.clone(),
            repo_id: "repo_r1".into(),
            workspace_id: Some("ws_exists".into()),
            title: "Has workspace".into(),
            description: String::new(),
            column: KanbanColumn::InProgress,
            order: 1024,
            created_at: 0,
            updated_at: 0,
        });
    }

    let result = remove_task_inner(
        task_id.clone(), false, tmp.path().to_path_buf(), Arc::clone(&state),
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
        st.tasks.insert(task_id.clone(), Task {
            id: task_id.clone(),
            repo_id: "repo_r1".into(),
            workspace_id: Some("ws_exists".into()),
            title: "Force remove".into(),
            description: String::new(),
            column: KanbanColumn::InProgress,
            order: 1024,
            created_at: 0,
            updated_at: 0,
        });
    }

    remove_task_inner(
        task_id.clone(), true, tmp.path().to_path_buf(), Arc::clone(&state),
    ).unwrap();

    let st = state.lock().unwrap();
    assert!(!st.tasks.contains_key(&task_id));
}

#[test]
fn remove_task_not_found_returns_err() {
    let tmp = tempdir().unwrap();
    let state = Arc::new(Mutex::new(AppState::default()));
    let result = remove_task_inner(
        "tk_ghost".into(), false, tmp.path().to_path_buf(), state,
    );
    assert!(result.is_err());
}
```

- [ ] **Step 9.2: Run tests to verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::task::tests::remove_task 2>&1 | tail -15
```

Expected: compile error — `remove_task_inner` not found.

- [ ] **Step 9.3: Implement**

Add to `src-tauri/src/commands/task.rs`:

```rust
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
```

- [ ] **Step 9.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib commands::task 2>&1 | tail -10
```

Expected: `test result: ok. N passed`.

- [ ] **Step 9.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/commands/task.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): add remove_task with force guard

Refuses to remove a task that has a linked workspace unless force=true.
This prevents accidental task deletion while a workspace is active.
The workspace itself is not removed — user controls that from the sidebar.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: Register commands in `invoke_handler!` + coverage verification

**Files:**

- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 10.1: Write failing tests**

The test here is functional: `cargo check` must succeed with all five task
commands registered. A compile-time assertion verifies the handler list is
complete:

```rust
// This is a compile-only verification. Add to src-tauri/src/lib.rs #[cfg(test)] mod tests:
#[test]
fn all_task_commands_exist_as_public_fns() {
    // Verify all five command symbols are resolvable — catches accidental renames.
    let _ = crate::commands::task::add_task as usize;
    let _ = crate::commands::task::list_tasks as usize;
    let _ = crate::commands::task::update_task as usize;
    let _ = crate::commands::task::move_task as usize;
    let _ = crate::commands::task::remove_task as usize;
}
```

- [ ] **Step 10.2: Run to verify fail**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib 2>&1 | grep "all_task_commands" | tail -5
```

Expected: compile error if any command is not yet public.

- [ ] **Step 10.3: Implement**

Update `invoke_handler!` in `src-tauri/src/lib.rs`:

```rust
.invoke_handler(tauri::generate_handler![
    crate::commands::system::get_app_version,
    crate::commands::repo::add_repo,
    crate::commands::repo::list_repos,
    crate::commands::repo::remove_repo,
    crate::commands::repo::update_gh_profile,
    crate::commands::workspace::create_workspace,
    crate::commands::workspace::list_workspaces,
    crate::commands::workspace::remove_workspace,
    crate::commands::task::add_task,
    crate::commands::task::list_tasks,
    crate::commands::task::update_task,
    crate::commands::task::move_task,
    crate::commands::task::remove_task,
])
```

Update `cargo llvm-cov` ignore regex to exclude Tauri wrapper fns in `task.rs`
(same rationale as repo.rs / workspace.rs — these fns cannot be called without a
live Tauri app handle):

In `.cargo/config.toml` or `Cargo.toml` llvm-cov config, extend the existing
ignore pattern to include `commands/task.rs` wrapper functions. The inner
functions (`add_task_inner`, `list_tasks_inner`, `update_task_inner`,
`move_task_inner`, `remove_task_inner`) are covered by the unit tests above.

Run full suite and check coverage:

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib 2>&1 | tail -15
```

Then run clippy and fmt:

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo fmt --all -- --check
cargo clippy --lib --all-targets -- -D warnings
```

- [ ] **Step 10.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor/src-tauri
export PATH="$HOME/.cargo/bin:$PATH"
cargo test --lib 2>&1 | tail -10
```

Expected: `test result: ok. ~117 passed` (102 existing + ~15 new).

- [ ] **Step 10.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src-tauri/src/lib.rs
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1b): register task commands in invoke_handler

Add add_task, list_tasks, update_task, move_task, remove_task to the
Tauri invoke_handler. All five are now callable from the frontend via
the typed ipc.ts wrappers (Phase 1b-frontend).

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Phase 1b-backend shipping criteria

- [ ] `cargo test --lib` — all tests pass (~102 existing + ~15 new = ~117)
- [ ] `cargo llvm-cov` with updated ignore regex — 95%+ lines / 95%+ functions
      on changed files
- [ ] `cargo fmt --check` + `cargo clippy --lib --all-targets -- -D warnings`
      clean
- [ ] AppState hydrates `tasks.json` on startup (Task 4)
- [ ] `move_task` auto-workspace behavior verified by integration tests (Task 8:
      4 async tests)

---

## Known deferrals to Phase 1b-frontend

- Kanban board UI, TaskCard component, drag-drop library (dnd-kit or SortableJS
  via Svelte action)
- Plan/Work mode toggle in TitleBar
- Keyboard shortcuts (5 baseline: switch mode ×2, new task, new workspace,
  settings, repo dropdown)
- `ipc.ts` typed wrappers for all five task commands
- E2E tests for kanban golden path (Todo→InProgress drag, auto-workspace
  creation visible in sidebar)
- `tasks.svelte.ts` store (SvelteMap-based, mirrors `workspaces.svelte.ts`)
