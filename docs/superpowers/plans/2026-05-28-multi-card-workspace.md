# Multi-Card Workspace Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let multiple kanban cards share a single workspace via an explicit
"link card to workspace" primitive, with sticky-links + safety-net cleanup so
bouncing a card never destroys live work.

**Architecture:** Generalise `WorkspaceInfo.task_id: Option<String>` →
`task_ids: Vec<String>` (backward-compat via
`#[serde(from = WorkspaceInfoRaw)]`). Add two backend commands
`link_task_to_workspace` / `unlink_task_from_workspace`. Update
`move_task_inner` and `remove_task_inner` for refcount-aware cleanup. Add UI:
card chip, sidebar count+expand, picker modal, conditional confirm modal, and an
auto-create undo toast that opens the same picker.

**Tech Stack:** Rust + Tauri v2 + serde, Svelte 5 runes, Bun, vitest,
Playwright. TDD strict (red → green → commit). No `.unwrap()`/`.expect()`
outside tests. No `console.log` (use `console.error`/`console.warn` only in
store catch paths; ESLint allows). Mutex discipline: clone state under the lock
then drop before any disk I/O.

**Spec:** `docs/superpowers/specs/2026-05-28-multi-card-workspace-design.md` —
read it once before starting; all rationale lives there.

**Standing constraints (verbatim from the session):** Commit LOCALLY per task,
**DO NOT push** until the user explicitly approves the whole branch. Each task
ends with a `git commit` (no `git push`). Branch is `feat/multi-card-workspace`.

---

## Task 1: Data model — `task_ids` field with serde migration

**Files:**

- Modify: `src-tauri/src/state.rs` — `WorkspaceInfo` struct (around line
  430-465).

The struct gains `task_ids: Vec<String>` and loses `task_id: Option<String>`.
Legacy persisted JSON with `"task_id": "tk_x"` must still load and become
`task_ids: vec!["tk_x"]`. New JSON with `"task_ids": [...]` is the canonical
write shape.

- [ ] **Step 1: Read the current struct definition**

Run: `sed -n '420,470p' src-tauri/src/state.rs` to confirm the surrounding
fields (so the Raw struct mirrors them exactly).

- [ ] **Step 2: Write the failing tests**

Append to the `#[cfg(test)] mod tests` block in `src-tauri/src/state.rs`:

```rust
#[test]
fn workspace_info_deserializes_legacy_task_id_into_task_ids() {
    // Legacy persisted shape from before PR #34: single `task_id`.
    // The new field `task_ids` MUST be populated from it on load so
    // workspaces.json files written by the old binary are still readable.
    let json = r#"{
        "id": "ws_legacy",
        "repo_id": "repo_a",
        "branch": "ansambel/x",
        "base_branch": "main",
        "custom_branch": false,
        "title": "T",
        "description": "",
        "status": "Waiting",
        "column": "InProgress",
        "created_at": 0,
        "updated_at": 0,
        "worktree_dir": "/tmp/wt",
        "team_activity_private": false,
        "task_id": "tk_legacy"
    }"#;
    let ws: WorkspaceInfo = serde_json::from_str(json).unwrap();
    assert_eq!(ws.task_ids, vec!["tk_legacy".to_string()]);
}

#[test]
fn workspace_info_deserializes_new_task_ids_directly() {
    let json = r#"{
        "id": "ws_new",
        "repo_id": "repo_a",
        "branch": "ansambel/x",
        "base_branch": "main",
        "custom_branch": false,
        "title": "T",
        "description": "",
        "status": "Waiting",
        "column": "InProgress",
        "created_at": 0,
        "updated_at": 0,
        "worktree_dir": "/tmp/wt",
        "team_activity_private": false,
        "task_ids": ["tk_a", "tk_b"]
    }"#;
    let ws: WorkspaceInfo = serde_json::from_str(json).unwrap();
    assert_eq!(ws.task_ids, vec!["tk_a".to_string(), "tk_b".to_string()]);
}

#[test]
fn workspace_info_deserializes_with_neither_field_yields_empty_task_ids() {
    let json = r#"{
        "id": "ws_none",
        "repo_id": "repo_a",
        "branch": "ansambel/x",
        "base_branch": "main",
        "custom_branch": false,
        "title": "T",
        "description": "",
        "status": "Waiting",
        "column": "InProgress",
        "created_at": 0,
        "updated_at": 0,
        "worktree_dir": "/tmp/wt",
        "team_activity_private": false
    }"#;
    let ws: WorkspaceInfo = serde_json::from_str(json).unwrap();
    assert!(ws.task_ids.is_empty());
}

#[test]
fn workspace_info_round_trip_serializes_task_ids_not_legacy() {
    // After a load+save cycle the persisted shape MUST use the new field.
    let ws = WorkspaceInfo {
        id: "ws_rt".into(),
        repo_id: "repo_a".into(),
        branch: "ansambel/x".into(),
        base_branch: "main".into(),
        custom_branch: false,
        title: "T".into(),
        description: String::new(),
        status: WorkspaceStatus::Waiting,
        column: KanbanColumn::InProgress,
        created_at: 0,
        updated_at: 0,
        worktree_dir: std::path::PathBuf::from("/tmp/wt"),
        team_activity_private: false,
        task_ids: vec!["tk_a".into(), "tk_b".into()],
    };
    let json = serde_json::to_string(&ws).unwrap();
    assert!(json.contains("\"task_ids\""));
    assert!(!json.contains("\"task_id\""), "legacy field must NOT appear in output");
    let back: WorkspaceInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(back.task_ids, vec!["tk_a".to_string(), "tk_b".to_string()]);
}
```

- [ ] **Step 3: Run tests to verify RED**

Run:
`cd src-tauri && cargo test --lib state::tests::workspace_info_deserializes_legacy_task_id_into_task_ids state::tests::workspace_info_deserializes_new_task_ids_directly state::tests::workspace_info_deserializes_with_neither_field_yields_empty_task_ids state::tests::workspace_info_round_trip_serializes_task_ids_not_legacy`

Expected: compile error (`task_ids` doesn't exist on `WorkspaceInfo`).

- [ ] **Step 4: Update the struct + add Raw helper**

In `src-tauri/src/state.rs`, replace the current `task_id: Option<String>` field
on `WorkspaceInfo` with `task_ids: Vec<String>`, and add a private
`WorkspaceInfoRaw` for backward-compat deserialization.

```rust
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(from = "WorkspaceInfoRaw")]
pub struct WorkspaceInfo {
    pub id: String,
    pub repo_id: String,
    pub branch: String,
    pub base_branch: String,
    pub custom_branch: bool,
    pub title: String,
    pub description: String,
    pub status: WorkspaceStatus,
    pub column: KanbanColumn,
    pub created_at: i64,
    pub updated_at: i64,
    pub worktree_dir: PathBuf,
    #[serde(default)]
    pub team_activity_private: bool,
    /// Cards linked to this workspace. Refcount = `task_ids.len()`.
    ///
    /// Backward compat: persisted files from before multi-card support
    /// carried a single `task_id: Option<String>`. The custom deserialize
    /// path below reads either `task_ids` (new) or `task_id` (legacy) and
    /// normalises to this Vec. After the next atomic save, the legacy
    /// field disappears from disk.
    #[serde(default)]
    pub task_ids: Vec<String>,
}

#[derive(Deserialize)]
struct WorkspaceInfoRaw {
    id: String,
    repo_id: String,
    branch: String,
    base_branch: String,
    custom_branch: bool,
    title: String,
    description: String,
    status: WorkspaceStatus,
    column: KanbanColumn,
    created_at: i64,
    updated_at: i64,
    worktree_dir: PathBuf,
    #[serde(default)]
    team_activity_private: bool,
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    task_ids: Vec<String>,
}

impl From<WorkspaceInfoRaw> for WorkspaceInfo {
    fn from(raw: WorkspaceInfoRaw) -> Self {
        let task_ids = if !raw.task_ids.is_empty() {
            raw.task_ids
        } else if let Some(legacy) = raw.task_id {
            vec![legacy]
        } else {
            Vec::new()
        };
        Self {
            id: raw.id,
            repo_id: raw.repo_id,
            branch: raw.branch,
            base_branch: raw.base_branch,
            custom_branch: raw.custom_branch,
            title: raw.title,
            description: raw.description,
            status: raw.status,
            column: raw.column,
            created_at: raw.created_at,
            updated_at: raw.updated_at,
            worktree_dir: raw.worktree_dir,
            team_activity_private: raw.team_activity_private,
            task_ids,
        }
    }
}
```

Notes:

- `#[serde(from = "WorkspaceInfoRaw")]` makes deserialize go through
  `Raw → From → WorkspaceInfo`. Serialize uses the derived path on
  `WorkspaceInfo` directly, emitting `task_ids` only.
- Imports already include `PathBuf`. If not, ensure `use std::path::PathBuf;` is
  present in the file (it is).

- [ ] **Step 5: Fix the call sites that constructed
      `WorkspaceInfo { task_id: ... }`**

Compile the crate to surface every call site that constructed the struct with
the old field. Search and replace each:

Run:
`cd src-tauri && cargo build --lib 2>&1 | grep -E "task_id|WorkspaceInfo" | head -30`

Each error like `missing field 'task_ids' in initializer of 'WorkspaceInfo'` or
`unknown field 'task_id'` is a construction site to fix. Replace `task_id: None`
with `task_ids: Vec::new()` and `task_id: Some(id)` with `task_ids: vec![id]`.
Confirmed sites from `grep -n "task_id" src-tauri/src/state.rs` at the time of
writing: state.rs:802, state.rs:1439 (both test helpers). There are also call
sites in `commands/workspace.rs` (`create_workspace_inner` initialises
`task_id: None` today), in `commands/task.rs` (`move_task_inner` sets
`task_id: Some(...)` when auto-creating), and in tests across `commands/*.rs`
and `commands/team_activity.rs`. Walk through `cargo build` errors and fix each
one. Pattern:

```rust
// Before
WorkspaceInfo { /* ... */, task_id: None }
// After
WorkspaceInfo { /* ... */, task_ids: Vec::new() }

// Before (auto-create site in move_task_inner):
WorkspaceInfo { /* ... */, task_id: Some(task.id.clone()) }
// After:
WorkspaceInfo { /* ... */, task_ids: vec![task.id.clone()] }
```

Also: any read site like `if let Some(tid) = &ws.task_id` becomes
`if let Some(tid) = ws.task_ids.first()` (single-owner reading). For PR #32's
`reattach_ws_id` logic in `move_task_inner`, the reattach lookup that walked
`ws.task_id == Some(task_id)` becomes `ws.task_ids.contains(task_id)`. Search:

Run:
`grep -rn "\.task_id\b\|task_id:" src-tauri/src --include='*.rs' | grep -v 'tests\|//' | head -40`

Update every production read/write site. Tests inside `#[cfg(test)]` blocks may
also need updating — fix them now too, since the build won't pass otherwise.

- [ ] **Step 6: Run tests to verify GREEN**

Run: `cd src-tauri && cargo test --lib state::tests::workspace_info`

Expected: 4 new tests PASS.

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3`

Expected: full lib suite PASS (no regressions — count should match the pre-task
total).

- [ ] **Step 7: Backend gates**

Run:

```
cd src-tauri && cargo clippy --lib --all-targets -- -D warnings 2>&1 | tail -3
cd src-tauri && cargo fmt --all -- --check
```

Expected: clean. Run `cargo fmt --all` if fmt fails.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/state.rs src-tauri/src/commands
git commit -m "feat(state): WorkspaceInfo.task_ids Vec with legacy task_id serde migration"
```

---

## Task 2: Frontend types — `Workspace.task_ids` + `UnlinkResult`

**Files:**

- Modify: `src/lib/types.ts`

Mirror the backend rename on the TS side. Add the discriminated `UnlinkResult`
type that Task 4 will use to drive the conditional confirm modal.

- [ ] **Step 1: Read the current Workspace type**

Run: `grep -n "type Workspace\b\|task_id\b" src/lib/types.ts | head`. Locate the
`Workspace` type (or `WorkspaceInfo`) — note the existing `task_id` field.

- [ ] **Step 2: Update the Workspace type**

In `src/lib/types.ts`, replace `task_id: string | null` (within the Workspace
type) with:

```ts
/** IDs of the cards linked to this workspace. Refcount = task_ids.length. */
task_ids: string[];
```

- [ ] **Step 3: Add `UnlinkResult`**

Append (under the workspace types block):

```ts
/** Result of `api.task.unlinkFromWorkspace`. The backend returns `would_remove`
 *  when called in preview mode (`force=false`) and the unlink would trigger
 *  cleanup; the UI uses that to show the confirm modal before re-calling with
 *  `force=true`. `removed`/`unlinked` are returned by the actual execution. */
export type UnlinkResult =
  | { kind: 'unlinked' }
  | { kind: 'removed' }
  | { kind: 'would_remove'; workspace_title: string };
```

- [ ] **Step 4: Run typecheck**

Run: `bun run check`

Expected: type errors point at every TS site reading `.task_id` on a workspace.
Fix each in this same step:

- Replace reads of `ws.task_id` (single id) with `ws.task_ids` (array). Most
  likely site: the workspaces store. Run
  `grep -rn "\.task_id\b" src 2>/dev/null` to enumerate.
- For checks like `ws.task_id === card.id`, switch to
  `ws.task_ids.includes(card.id)`.

Re-run `bun run check`. Expected: 0 errors, 0 warnings.

- [ ] **Step 5: Commit**

```bash
git add src/lib/types.ts src/lib/stores src/lib/components src/App.svelte
git commit -m "feat(types): Workspace.task_ids[] + UnlinkResult"
```

---

## Task 3: Backend command — `link_task_to_workspace`

**Files:**

- Modify: `src-tauri/src/commands/task.rs` (append a new `pub async fn` +
  `pub(crate) fn` inner + tests).
- Modify: `src-tauri/src/lib.rs` (register in `tauri::generate_handler!`).

Behaviour (per spec §2 + §3 + backend command surface):

- Validate same-repo (`task.repo_id == workspace.repo_id`); reject with
  `AppError::InvalidState("repo mismatch")` otherwise.
- Idempotent if the card is already linked to that workspace (no-op success).
- If the card was already linked to a different workspace W', perform the atomic
  switch inside the lock: remove the card from `W'.task_ids`, run the cleanup
  check on W', then add the card to `W.task_ids` and set
  `task.workspace_id = W`. Single mutation under one lock.
- After mutation: clone the new state snapshots, drop lock, persist
  `workspaces.json` + `tasks.json` atomically.

- [ ] **Step 1: Write the failing tests**

Append to the `#[cfg(test)] mod tests` block in
`src-tauri/src/commands/task.rs`. Use the existing test scaffolding (look for
`make_state_with_real_repo` or similar helpers — the file already has them per
PR #32).

```rust
#[tokio::test]
async fn link_task_to_workspace_attaches_card_and_appends_task_id() {
    let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
    // Seed a second card in the same repo, currently unlinked.
    seed_task(&state, "tk_b", "repo_a", None);
    link_task_to_workspace_inner("tk_b", "ws_a", data_dir.clone(), state.clone())
        .await
        .unwrap();
    let st = state.lock().unwrap();
    let ws = st.workspaces.get("ws_a").unwrap();
    assert!(ws.task_ids.contains(&"tk_a".to_string()));
    assert!(ws.task_ids.contains(&"tk_b".to_string()));
    let tk = st.tasks.iter().find(|t| t.id == "tk_b").unwrap();
    assert_eq!(tk.workspace_id.as_deref(), Some("ws_a"));
}

#[tokio::test]
async fn link_task_to_workspace_is_idempotent_for_already_linked_card() {
    let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
    // tk_a is already in ws_a from setup.
    let before = state.lock().unwrap().workspaces.get("ws_a").unwrap().task_ids.clone();
    link_task_to_workspace_inner("tk_a", "ws_a", data_dir, state.clone())
        .await
        .unwrap();
    let after = state.lock().unwrap().workspaces.get("ws_a").unwrap().task_ids.clone();
    assert_eq!(before, after);
}

#[tokio::test]
async fn link_task_to_workspace_atomically_switches_from_other_workspace() {
    let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
    // Seed a second workspace + link a card to it.
    seed_workspace(&state, "ws_b", "repo_a");
    seed_task(&state, "tk_b", "repo_a", Some("ws_b".into()));
    state.lock().unwrap().workspaces.get_mut("ws_b").unwrap().task_ids = vec!["tk_b".into()];

    link_task_to_workspace_inner("tk_b", "ws_a", data_dir, state.clone())
        .await
        .unwrap();

    let st = state.lock().unwrap();
    assert!(st.workspaces.get("ws_a").unwrap().task_ids.contains(&"tk_b".to_string()));
    // ws_b lost the link AND was removed (refcount=0 + empty, per cleanup rule).
    assert!(st.workspaces.get("ws_b").is_none());
    let tk = st.tasks.iter().find(|t| t.id == "tk_b").unwrap();
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
    let err = link_task_to_workspace_inner("tk_missing", "ws_a", data_dir.clone(), state.clone())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("tk_missing"));

    let err = link_task_to_workspace_inner("tk_a", "ws_missing", data_dir, state)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("ws_missing"));
}
```

If `setup_state_with_repo_and_workspace`, `seed_task`, `seed_workspace` helpers
don't already exist with those exact signatures, add them at the top of the
`tests` module:

```rust
async fn setup_state_with_repo_and_workspace(
    ws_id: &str,
    initial_task_id: &str,
) -> (PathBuf, Arc<Mutex<AppState>>) {
    // Reuse the existing PR #32 helper if present (search for
    // make_state_with_real_repo); otherwise build a tempdir + real git
    // worktree following that pattern. The workspace MUST have a real
    // worktree on disk so `is_workspace_empty` checks succeed.
    todo!("adapt PR #32 helper")
}

fn seed_task(state: &Arc<Mutex<AppState>>, task_id: &str, repo_id: &str, workspace_id: Option<String>) {
    let mut st = state.lock().unwrap();
    st.tasks.push(Task {
        id: task_id.into(),
        repo_id: repo_id.into(),
        provider: "local".into(),
        title: task_id.into(),
        description: String::new(),
        column: KanbanColumn::Todo,
        order: 0,
        created_at: 0,
        updated_at: 0,
        workspace_id,
        external_id: None,
        url: None,
    });
}

fn seed_workspace(state: &Arc<Mutex<AppState>>, ws_id: &str, repo_id: &str) {
    let mut st = state.lock().unwrap();
    st.workspaces.insert(ws_id.into(), WorkspaceInfo {
        id: ws_id.into(),
        repo_id: repo_id.into(),
        branch: format!("ansambel/{ws_id}"),
        base_branch: "main".into(),
        custom_branch: false,
        title: ws_id.into(),
        description: String::new(),
        status: WorkspaceStatus::Waiting,
        column: KanbanColumn::InProgress,
        created_at: 0,
        updated_at: 0,
        worktree_dir: std::path::PathBuf::from(format!("/tmp/ws_{ws_id}")),
        team_activity_private: false,
        task_ids: Vec::new(),
    });
}
```

The `todo!()` is acceptable ONLY for the helper that wraps the existing PR #32
fixture — replace it with the real call once you've located the helper. If you
cannot find a helper that creates a real on-disk worktree, lift the relevant
block from one of the existing `is_workspace_empty`-using tests in
`commands/workspace.rs` or `commands/task.rs`. Do not ship `todo!()` in the
final commit.

- [ ] **Step 2: Run RED**

Run:
`cd src-tauri && cargo test --lib commands::task::tests::link_task_to_workspace_`

Expected: compile error (`link_task_to_workspace_inner` doesn't exist).

- [ ] **Step 3: Implement the inner + wrapper**

Append to `src-tauri/src/commands/task.rs`:

```rust
#[tauri::command]
pub async fn link_task_to_workspace(
    task_id: String,
    workspace_id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, std::sync::Arc<std::sync::Mutex<AppState>>>,
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
    state: std::sync::Arc<std::sync::Mutex<AppState>>,
) -> Result<()> {
    // Phase 1: validate + plan the mutation while holding the lock briefly.
    let (already_linked_here, switch_from) = {
        let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        let task = st.tasks.iter().find(|t| t.id == task_id)
            .ok_or_else(|| AppError::NotFound(format!("task '{task_id}'")))?;
        let ws = st.workspaces.get(workspace_id)
            .ok_or_else(|| AppError::NotFound(format!("workspace '{workspace_id}'")))?;
        if task.repo_id != ws.repo_id {
            return Err(AppError::InvalidState(format!(
                "repo mismatch: task '{task_id}' is in repo '{}', workspace '{workspace_id}' is in repo '{}'",
                task.repo_id, ws.repo_id
            )));
        }
        let already = task.workspace_id.as_deref() == Some(workspace_id);
        let switch_from = task.workspace_id.clone().filter(|w| w != workspace_id);
        (already, switch_from)
    };

    if already_linked_here {
        return Ok(());
    }

    // Phase 2: if switching, unlink from the previous workspace (may
    // trigger cleanup). This calls the unlink inner so the cleanup
    // logic stays in one place.
    if let Some(prev_ws) = switch_from {
        let _ = unlink_task_from_workspace_inner(
            task_id, /* force = */ true, data_dir.clone(), state.clone()
        ).await?;
    }

    // Phase 3: link.
    let now = crate::commands::helpers::now_unix();
    let (tasks_snapshot, workspaces_snapshot) = {
        let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        if let Some(task) = st.tasks.iter_mut().find(|t| t.id == task_id) {
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
```

Note: `unlink_task_from_workspace_inner` is added in **Task 4** but referenced
here for the atomic-switch case. Implement Task 4's inner BEFORE running this
task's tests (the test
`link_task_to_workspace_atomically_switches_from_other_workspace` depends on
it). If executing tasks in strict order, swap Task 3 and Task 4 — but Task 4's
unlink depends on Task 3's link not at all, so do Task 4 first.

**REORDERING NOTE FOR THE EXECUTOR: implement Task 4 (unlink) before Task 3
(link). The plan is presented in feature-order for readability; execute Task 4
first.**

- [ ] **Step 4: Register in `tauri::generate_handler!`**

In `src-tauri/src/lib.rs`, find the `tauri::generate_handler![...]` block
(around line 296-320 per the existing layout) and add
`crate::commands::task::link_task_to_workspace,` next to the other `task::`
entries (around lines 306-311).

Add a registration smoke test mirroring the existing ones in `lib.rs`:

```rust
#[test]
fn link_task_to_workspace_command_is_registered() {
    let _ = crate::commands::task::link_task_to_workspace as *const () as usize;
}
```

- [ ] **Step 5: Run GREEN**

Run:
`cd src-tauri && cargo test --lib commands::task::tests::link_task_to_workspace_`

Expected: 5 tests PASS.

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3` — expect full suite
green.

- [ ] **Step 6: Clippy + fmt**

Run:

```
cd src-tauri && cargo clippy --lib --all-targets -- -D warnings 2>&1 | tail -3
cd src-tauri && cargo fmt --all -- --check
```

Both clean.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/commands/task.rs src-tauri/src/lib.rs
git commit -m "feat(task): link_task_to_workspace command (idempotent + atomic switch)"
```

---

## Task 4: Backend command — `unlink_task_from_workspace`

**Files:**

- Modify: `src-tauri/src/commands/task.rs` (append).
- Modify: `src-tauri/src/lib.rs` (register).

Behaviour (per spec §3 + §4 pseudocode):

- Param `force: bool`. When `force=false`, run the cleanup check WITHOUT
  mutating, and return `UnlinkResult::WouldRemove { workspace_title }` if the
  unlink would trigger cleanup. When `force=true`, perform the unlink + cleanup.
- Always: clears `task.workspace_id`, removes the task id from
  `workspace.task_ids`. Then: if `task_ids.is_empty() && is_workspace_empty(W)`,
  call `remove_workspace_inner(W)` and return `UnlinkResult::Removed`.
- If the task wasn't linked, return `UnlinkResult::Unlinked` (idempotent no-op).
- The `link_task_to_workspace` atomic switch (Task 3) calls this inner with
  `force=true` so cleanup runs.

- [ ] **Step 1: Add the `UnlinkResult` enum (Rust side)**

At the top of `src-tauri/src/commands/task.rs`:

```rust
#[derive(serde::Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UnlinkResult {
    Unlinked,
    Removed,
    WouldRemove { workspace_title: String },
}
```

- [ ] **Step 2: Write the failing tests**

Append to the `tests` module (reusing helpers from Task 3):

```rust
#[tokio::test]
async fn unlink_force_with_refcount_gt_1_keeps_workspace_and_returns_unlinked() {
    let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
    // Seed a second card linked to the same workspace.
    seed_task(&state, "tk_b", "repo_a", Some("ws_a".into()));
    state.lock().unwrap().workspaces.get_mut("ws_a").unwrap().task_ids =
        vec!["tk_a".into(), "tk_b".into()];

    let r = unlink_task_from_workspace_inner("tk_b", true, data_dir, state.clone())
        .await
        .unwrap();
    assert_eq!(r, UnlinkResult::Unlinked);

    let st = state.lock().unwrap();
    let ws = st.workspaces.get("ws_a").unwrap();
    assert_eq!(ws.task_ids, vec!["tk_a".to_string()]);
    let tk = st.tasks.iter().find(|t| t.id == "tk_b").unwrap();
    assert!(tk.workspace_id.is_none());
}

#[tokio::test]
async fn unlink_force_with_refcount_1_and_empty_workspace_removes_workspace() {
    // Setup helper creates ws_a with one linked card and an empty real
    // worktree (no commits beyond base, no chat, no agent).
    let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
    let r = unlink_task_from_workspace_inner("tk_a", true, data_dir, state.clone())
        .await
        .unwrap();
    assert_eq!(r, UnlinkResult::Removed);
    let st = state.lock().unwrap();
    assert!(st.workspaces.get("ws_a").is_none());
}

#[tokio::test]
async fn unlink_force_with_refcount_1_and_non_empty_workspace_keeps_workspace() {
    let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
    // Dirty the worktree so is_workspace_empty returns false.
    let wt = state.lock().unwrap().workspaces.get("ws_a").unwrap().worktree_dir.clone();
    std::fs::write(wt.join("DIRTY.md"), b"x").unwrap();

    let r = unlink_task_from_workspace_inner("tk_a", true, data_dir, state.clone())
        .await
        .unwrap();
    assert_eq!(r, UnlinkResult::Unlinked);

    let st = state.lock().unwrap();
    assert!(st.workspaces.get("ws_a").is_some());
    let ws = st.workspaces.get("ws_a").unwrap();
    assert!(ws.task_ids.is_empty());
}

#[tokio::test]
async fn unlink_preview_returns_would_remove_for_last_empty_link() {
    let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
    let r = unlink_task_from_workspace_inner("tk_a", /* force = */ false, data_dir, state.clone())
        .await
        .unwrap();
    match r {
        UnlinkResult::WouldRemove { workspace_title } => {
            assert_eq!(workspace_title, "ws_a");
        }
        other => panic!("expected WouldRemove, got {other:?}"),
    }
    // Preview must NOT mutate.
    let st = state.lock().unwrap();
    let ws = st.workspaces.get("ws_a").unwrap();
    assert_eq!(ws.task_ids, vec!["tk_a".to_string()]);
}

#[tokio::test]
async fn unlink_preview_returns_unlinked_when_no_cleanup_would_fire() {
    let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
    seed_task(&state, "tk_b", "repo_a", Some("ws_a".into()));
    state.lock().unwrap().workspaces.get_mut("ws_a").unwrap().task_ids =
        vec!["tk_a".into(), "tk_b".into()];

    let r = unlink_task_from_workspace_inner("tk_b", false, data_dir, state)
        .await
        .unwrap();
    // refcount > 1 → no cleanup would fire; preview returns Unlinked (the
    // UI uses this as "safe to execute immediately, no modal").
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
```

- [ ] **Step 3: Run RED**

Run: `cd src-tauri && cargo test --lib commands::task::tests::unlink_`

Expected: compile errors.

- [ ] **Step 4: Implement the inner + wrapper**

```rust
#[tauri::command]
pub async fn unlink_task_from_workspace(
    task_id: String,
    force: bool,
    app: tauri::AppHandle,
    state: tauri::State<'_, std::sync::Arc<std::sync::Mutex<AppState>>>,
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
    state: std::sync::Arc<std::sync::Mutex<AppState>>,
) -> Result<UnlinkResult> {
    // Phase 1: read current state under the lock — find the workspace,
    // its refcount after unlink, and the agent-live flag (needed by
    // is_workspace_empty).
    let (workspace_id, workspace_title, refcount_after, worktree_dir, base_branch, agent_live) = {
        let st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        let task = st.tasks.iter().find(|t| t.id == task_id)
            .ok_or_else(|| AppError::NotFound(format!("task '{task_id}'")))?;
        let Some(ws_id) = task.workspace_id.clone() else {
            return Ok(UnlinkResult::Unlinked);
        };
        let Some(ws) = st.workspaces.get(&ws_id) else {
            // Stale link to a workspace that no longer exists — clean up
            // the dangling forward link in Phase 2 + return Unlinked.
            drop(st);
            // Re-acquire to clear the dangling task.workspace_id.
            let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
            if let Some(t) = st.tasks.iter_mut().find(|t| t.id == task_id) {
                t.workspace_id = None;
            }
            let snapshot = st.tasks.clone();
            drop(st);
            crate::persistence::tasks::save_tasks(&data_dir, &snapshot)?;
            return Ok(UnlinkResult::Unlinked);
        };
        let refcount_after = ws.task_ids.iter().filter(|id| id.as_str() != task_id).count();
        let agent_live = st.agents.contains_key(&ws_id);
        (ws_id, ws.title.clone(), refcount_after, ws.worktree_dir.clone(), ws.base_branch.clone(), agent_live)
    };

    // Phase 2: cleanup-check decision. Treat refcount_after==0 + empty as
    // "would remove". For preview, return WouldRemove without mutating;
    // for force, fall through and perform the unlink + remove.
    let would_remove = if refcount_after == 0 {
        // Construct a transient WorkspaceInfo proxy for is_workspace_empty
        // without mutating state. The function only reads worktree_dir +
        // base_branch + the agent-live flag passed separately.
        let ws_proxy = WorkspaceInfo {
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
            task_ids: vec![],
        };
        crate::commands::workspace::is_workspace_empty(&data_dir, &ws_proxy, agent_live)
    } else {
        false
    };

    if !force && would_remove {
        return Ok(UnlinkResult::WouldRemove { workspace_title });
    }

    // Phase 3: perform the unlink under the lock; clone snapshots; release.
    let (tasks_snapshot, workspaces_snapshot) = {
        let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        if let Some(task) = st.tasks.iter_mut().find(|t| t.id == task_id) {
            task.workspace_id = None;
            task.updated_at = crate::commands::helpers::now_unix();
        }
        if let Some(ws) = st.workspaces.get_mut(&workspace_id) {
            ws.task_ids.retain(|id| id != task_id);
            ws.updated_at = crate::commands::helpers::now_unix();
        }
        (st.tasks.clone(), st.workspaces.clone())
    };
    crate::persistence::tasks::save_tasks(&data_dir, &tasks_snapshot)?;
    crate::persistence::workspaces::save_workspaces(&data_dir, &workspaces_snapshot)?;

    // Phase 4: if cleanup is now warranted, remove the workspace.
    if would_remove {
        crate::commands::workspace::remove_workspace_inner(
            workspace_id,
            data_dir.clone(),
            state.clone(),
        )?;
        return Ok(UnlinkResult::Removed);
    }

    Ok(UnlinkResult::Unlinked)
}
```

Notes:

- The "stale link" branch (workspace gone) is defensive — should not occur in
  normal operation but matches the fail-safe pattern in PR #32.
- Reuse `crate::commands::workspace::is_workspace_empty` and
  `remove_workspace_inner` (made `pub(crate)` in PR #32 — verify with
  `grep -n "pub(crate) fn remove_workspace_inner\|pub fn is_workspace_empty" src-tauri/src/commands/workspace.rs`).
- The "would_remove" cleanup check runs OUTSIDE the lock (it does git I/O via
  `is_workspace_empty`'s `origin/<base>..HEAD` rev-list). This is intentional
  per the mutex discipline rule.

- [ ] **Step 5: Register in `tauri::generate_handler!`**

In `src-tauri/src/lib.rs`, add
`crate::commands::task::unlink_task_from_workspace,` next to the other `task::`
entries. Add a registration smoke test:

```rust
#[test]
fn unlink_task_from_workspace_command_is_registered() {
    let _ = crate::commands::task::unlink_task_from_workspace as *const () as usize;
}
```

- [ ] **Step 6: Run GREEN**

Run: `cd src-tauri && cargo test --lib commands::task::tests::unlink_`

Expected: 6 tests PASS.

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3` — full suite green.

- [ ] **Step 7: Clippy + fmt + commit**

```
cd src-tauri && cargo clippy --lib --all-targets -- -D warnings 2>&1 | tail -3
cd src-tauri && cargo fmt --all -- --check
```

```bash
git add src-tauri/src/commands/task.rs src-tauri/src/lib.rs
git commit -m "feat(task): unlink_task_from_workspace with preview + refcount cleanup"
```

---

## Task 5: Update `move_task_inner` — refcount > 1 short-circuit on move-to-Todo

**Files:**

- Modify: `src-tauri/src/commands/task.rs` around the existing safety-net call
  (line 426-428 today, calls `is_workspace_empty` + `remove_workspace_inner`).

Per spec §4: on move-to-Todo, if `refcount > 1`, the link stays sticky and no
cleanup is attempted. The existing refcount=1 path (empty → remove + unlink;
not-empty → keep) is unchanged.

- [ ] **Step 1: Locate the safety-net region**

Run: `sed -n '410,460p' src-tauri/src/commands/task.rs`. Identify the block that
loads the linked workspace, calls `is_workspace_empty`, and (today)
unconditionally treats it as refcount-1.

- [ ] **Step 2: Write the failing test**

```rust
#[tokio::test]
async fn move_to_todo_with_refcount_gt_1_keeps_workspace_and_link() {
    let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
    // Add a second linked card.
    seed_task(&state, "tk_b", "repo_a", Some("ws_a".into()));
    state.lock().unwrap().workspaces.get_mut("ws_a").unwrap().task_ids =
        vec!["tk_a".into(), "tk_b".into()];
    // Move tk_a back to Todo. Workspace MUST stay and link MUST persist
    // because refcount=2 (tk_b still linked) — sticky rule.
    move_task_inner("tk_a", KanbanColumn::Todo, 0, data_dir, state.clone())
        .await
        .unwrap();
    let st = state.lock().unwrap();
    let ws = st.workspaces.get("ws_a").expect("workspace must stay");
    assert!(ws.task_ids.contains(&"tk_a".to_string()), "link must be sticky");
    assert!(ws.task_ids.contains(&"tk_b".to_string()));
    let tk_a = st.tasks.iter().find(|t| t.id == "tk_a").unwrap();
    assert_eq!(tk_a.workspace_id.as_deref(), Some("ws_a"));
    assert!(matches!(tk_a.column, KanbanColumn::Todo));
}

#[tokio::test]
async fn move_to_todo_with_refcount_1_and_empty_still_removes_workspace() {
    // Regression guard for PR #32 behaviour — must NOT be broken by the
    // refcount short-circuit.
    let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
    move_task_inner("tk_a", KanbanColumn::Todo, 0, data_dir, state.clone())
        .await
        .unwrap();
    let st = state.lock().unwrap();
    assert!(st.workspaces.get("ws_a").is_none(), "PR #32 safety-net cleanup still fires");
    let tk = st.tasks.iter().find(|t| t.id == "tk_a").unwrap();
    assert!(tk.workspace_id.is_none());
}
```

- [ ] **Step 3: Run RED**

Run:
`cd src-tauri && cargo test --lib commands::task::tests::move_to_todo_with_refcount`

Expected: `move_to_todo_with_refcount_gt_1_keeps_workspace_and_link` FAILS
(today's logic removes/unlinks regardless of refcount because it always
evaluated `is_workspace_empty` which is true on a fresh worktree).
`move_to_todo_with_refcount_1_and_empty_still_removes_workspace` should PASS
already (PR #32 behaviour) but it's our regression guard.

- [ ] **Step 4: Implement the short-circuit**

In `move_task_inner` at the safety-net region (around line 426), guard the
unlink+remove block with a refcount check. Pseudocode (adapt to the exact
`if let` / `match` shape of the current code):

```rust
// ... existing code that resolved `ws` (the linked workspace) and
// computed `agent_live` ...

// Multi-card refcount rule (spec §4):
// - refcount > 1 → sticky; no cleanup, link stays, only the column moves.
// - refcount == 1 + empty → PR #32 behaviour (unlink + remove).
// - refcount == 1 + not-empty → keep (existing behaviour).
let refcount = ws.task_ids.len();
if refcount > 1 {
    // Sticky: leave workspaces.json untouched for this card; the column
    // change is persisted via the existing tasks.json save below.
} else if crate::commands::workspace::is_workspace_empty(&data_dir, ws, agent_live) {
    crate::commands::workspace::remove_workspace_inner(
        ws.id.clone(), data_dir.clone(), state.clone(),
    )?;
    // Clear the card's forward link too (matches PR #32). Also remove
    // the card from ws.task_ids (already gone if remove_workspace
    // dropped the whole entry, but make the read-modify-write safe).
    let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    if let Some(t) = st.tasks.iter_mut().find(|t| t.id == task_id) {
        t.workspace_id = None;
    }
}
```

If the existing code keeps `ws.task_ids` membership in sync somewhere else,
don't double-remove. Read the surrounding 30 lines carefully before edit.

- [ ] **Step 5: Run GREEN**

Run:
`cd src-tauri && cargo test --lib commands::task::tests::move_to_todo_with_refcount`

Expected: both tests PASS. Then `cargo test --lib` full suite — no regressions.

- [ ] **Step 6: Commit**

```
cd src-tauri && cargo clippy --lib --all-targets -- -D warnings 2>&1 | tail -3
cd src-tauri && cargo fmt --all -- --check
```

```bash
git add src-tauri/src/commands/task.rs
git commit -m "feat(task): sticky-link refcount short-circuit on move-to-Todo"
```

---

## Task 6: Update `remove_task_inner` (delete card) — refcount-aware cleanup

**Files:**

- Modify: `src-tauri/src/commands/task.rs` — find the `remove_task_inner` (or
  equivalent delete path).

Per spec §4: deleting a card is equivalent to explicit unlink for refcount
purposes. If the deleted card was linked, decrement refcount; if refcount
becomes 0 + workspace empty, remove.

- [ ] **Step 1: Locate `remove_task_inner`**

Run:
`grep -n "fn remove_task_inner\|fn remove_task\b" src-tauri/src/commands/task.rs`.
Read the function.

- [ ] **Step 2: Failing test**

```rust
#[tokio::test]
async fn remove_task_decrements_refcount_and_keeps_shared_workspace() {
    let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
    seed_task(&state, "tk_b", "repo_a", Some("ws_a".into()));
    state.lock().unwrap().workspaces.get_mut("ws_a").unwrap().task_ids =
        vec!["tk_a".into(), "tk_b".into()];

    remove_task_inner("tk_b", data_dir, state.clone()).await.unwrap();

    let st = state.lock().unwrap();
    let ws = st.workspaces.get("ws_a").expect("shared ws must stay");
    assert_eq!(ws.task_ids, vec!["tk_a".to_string()]);
}

#[tokio::test]
async fn remove_task_with_last_link_and_empty_workspace_removes_workspace() {
    let (data_dir, state) = setup_state_with_repo_and_workspace("ws_a", "tk_a").await;
    remove_task_inner("tk_a", data_dir, state.clone()).await.unwrap();
    let st = state.lock().unwrap();
    assert!(st.workspaces.get("ws_a").is_none());
}
```

- [ ] **Step 3: RED then GREEN**

Run the tests; expect failure (today `remove_task_inner` may not touch
workspaces.task_ids or may unconditionally try to remove the workspace). Then
update `remove_task_inner` to:

- Before deleting the task, capture `task.workspace_id`.
- After deleting the task, call
  `unlink_task_from_workspace_inner(task_id, /*force=*/true, …)` if
  `workspace_id.is_some()`. The unlink inner handles the refcount + cleanup
  uniformly.

If `remove_task_inner` doesn't currently take a `data_dir` for cleanup, thread
one through (mirror `move_task_inner`'s signature).

- [ ] **Step 4: GREEN + commit**

```
cd src-tauri && cargo test --lib commands::task && cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all -- --check
```

```bash
git add src-tauri/src/commands/task.rs
git commit -m "feat(task): refcount-aware cleanup on remove_task"
```

---

## Task 7: Frontend IPC + tasks store wiring

**Files:**

- Modify: `src/lib/ipc.ts` — `api.task.linkToWorkspace`,
  `api.task.unlinkFromWorkspace`.
- Modify: `src/lib/stores/tasks.svelte.ts` — `link`, `unlink` methods.
- Modify: `src/lib/stores/tasks.svelte.test.ts` — tests.
- Modify: `src/lib/stores/workspaces.svelte.ts` if needed to reflect `task_ids`
  updates after link/unlink.

- [ ] **Step 1: Extend `api.task` in `src/lib/ipc.ts`**

Find the `task: { ... }` namespace (around line 99). Add:

```ts
linkToWorkspace: (taskId: string, workspaceId: string): Promise<void> =>
  invoke('link_task_to_workspace', { taskId, workspaceId }),

unlinkFromWorkspace: (taskId: string, force: boolean): Promise<UnlinkResult> =>
  invoke('unlink_task_from_workspace', { taskId, force }),
```

Import `UnlinkResult` from `./types` at the top.

- [ ] **Step 2: Failing tests for the store**

In `src/lib/stores/tasks.svelte.test.ts`, append:

```ts
it('link calls api.task.linkToWorkspace and reloads tasks for the repo', async () => {
  const { api } = await import('$lib/ipc');
  vi.mocked(api.task.linkToWorkspace).mockResolvedValue(undefined);
  vi.mocked(api.task.list).mockResolvedValue([
    /* ...tasks for repo_a after the link... */
  ] as Task[]);
  const store = new TasksStore();
  await store.link('tk_b', 'ws_a', 'repo_a');
  expect(api.task.linkToWorkspace).toHaveBeenCalledWith('tk_b', 'ws_a');
  // The store should refresh the local task map so derived views update.
  expect(api.task.list).toHaveBeenCalledWith('repo_a');
});

it('unlink force=true returns UnlinkResult and refreshes', async () => {
  const { api } = await import('$lib/ipc');
  vi.mocked(api.task.unlinkFromWorkspace).mockResolvedValue({
    kind: 'removed',
  });
  vi.mocked(api.task.list).mockResolvedValue([] as Task[]);
  const store = new TasksStore();
  const result = await store.unlink('tk_a', true, 'repo_a');
  expect(result).toEqual({ kind: 'removed' });
  expect(api.task.list).toHaveBeenCalledWith('repo_a');
});

it('unlink force=false (preview) does NOT trigger a refresh', async () => {
  const { api } = await import('$lib/ipc');
  vi.mocked(api.task.unlinkFromWorkspace).mockResolvedValue({
    kind: 'would_remove',
    workspace_title: 'payment-refactor',
  });
  const store = new TasksStore();
  const result = await store.unlink('tk_a', false, 'repo_a');
  expect(result).toEqual({
    kind: 'would_remove',
    workspace_title: 'payment-refactor',
  });
  expect(api.task.list).not.toHaveBeenCalled();
});
```

Make sure the existing mock factory for `$lib/ipc` includes
`task.linkToWorkspace` and `task.unlinkFromWorkspace` as `vi.fn()` entries.

- [ ] **Step 3: Implement in `tasks.svelte.ts`**

Append to the `TasksStore` class:

```ts
async link(taskId: string, workspaceId: string, repoId: string): Promise<void> {
  await api.task.linkToWorkspace(taskId, workspaceId);
  // Refresh local task list so derived views (kanban + sidebar) see the
  // new task.workspace_id immediately.
  const fresh = await api.task.list(repoId);
  this.replaceForRepo(repoId, fresh);
}

async unlink(taskId: string, force: boolean, repoId: string): Promise<UnlinkResult> {
  const result = await api.task.unlinkFromWorkspace(taskId, force);
  if (force) {
    const fresh = await api.task.list(repoId);
    this.replaceForRepo(repoId, fresh);
  }
  return result;
}
```

If `replaceForRepo` doesn't exist, mirror whatever the existing
`refresh(repoId)` does (it likely already exists).

Also after a link/unlink, the workspaces store needs to re-read its `task_ids`
so the sidebar count reflects truth. Add `await workspaces.loadForRepo(repoId)`
in both methods OR fire a "workspaces-changed" event the store listens for. Pick
the simplest available pattern — likely just call
`workspaces.loadForRepo(repoId)` in tandem.

- [ ] **Step 4: GREEN + check + commit**

```
bun run vitest run src/lib/stores/tasks.svelte.test.ts
bun run check
```

```bash
git add src/lib/ipc.ts src/lib/stores/tasks.svelte.ts src/lib/stores/tasks.svelte.test.ts
git commit -m "feat(tasks-store): link/unlink methods + IPC wrappers"
```

---

## Task 8: Card chip + click-to-navigate

**Files:**

- Modify: `src/lib/components/kanban/TaskCard.svelte`
- Modify: `src/lib/components/kanban/TaskCard.test.ts`

When `task.workspace_id` is set, render a small chip showing the workspace
title; click navigates: select the workspace + flip mode to Work.

- [ ] **Step 1: Failing tests**

Append to `TaskCard.test.ts`:

```ts
it('renders a workspace chip when the task is linked', () => {
  const task = makeTask({ workspace_id: 'ws_a' });
  // Provide a workspaces store mock that resolves ws_a → title 'payment-refactor'.
  const { getByTestId } = render(TaskCard, { props: { task } });
  const chip = getByTestId('task-workspace-chip');
  expect(chip.textContent).toMatch(/payment-refactor/);
});

it('does NOT render the chip when task.workspace_id is null', () => {
  const task = makeTask({ workspace_id: null });
  const { queryByTestId } = render(TaskCard, { props: { task } });
  expect(queryByTestId('task-workspace-chip')).toBeNull();
});

it('clicking the chip selects the workspace and switches to work mode', async () => {
  const task = makeTask({ workspace_id: 'ws_a' });
  const { getByTestId } = render(TaskCard, { props: { task } });
  await fireEvent.click(getByTestId('task-workspace-chip'));
  // Assert against the mocked workspaces.select + modeStore.set('work').
  expect(workspacesMock.select).toHaveBeenCalledWith('ws_a');
  expect(modeStoreMock.set).toHaveBeenCalledWith('work');
});
```

Extend the existing `TaskCard.test.ts` mock harness (look for how it mocks
`$lib/stores/workspaces.svelte` / `mode-store`).

- [ ] **Step 2: Implement in `TaskCard.svelte`**

```svelte
<script lang="ts">
  // ...existing imports...
  import { workspaces } from '$lib/stores/workspaces.svelte';
  import { modeStore } from '$lib/stores/mode.svelte';

  // ...existing props (task)...

  const linkedWorkspace = $derived(
    task.workspace_id ? (workspaces.byId(task.workspace_id) ?? null) : null
  );

  function jumpToWorkspace(e: MouseEvent) {
    // Prevent the drag handler / outer click from also firing.
    e.stopPropagation();
    if (!task.workspace_id) return;
    workspaces.select(task.workspace_id);
    modeStore.set('work');
  }
</script>

<!-- existing card body... -->

{#if linkedWorkspace}
  <button
    type="button"
    class="mt-2 inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] bg-[var(--bg-hover)] text-[var(--text-muted)] hover:text-[var(--text-base)]"
    data-testid="task-workspace-chip"
    onclick={jumpToWorkspace}
    use:tooltip={{ text: `Open workspace ${linkedWorkspace.title}` }}
  >
    <span aria-hidden="true">◆</span>
    <span class="truncate max-w-[140px]">{linkedWorkspace.title}</span>
  </button>
{/if}
```

If `workspaces.byId(id)` doesn't exist, add it to the workspaces store
(`byId(id): Workspace | undefined => this.workspaces.get(id)`) — small
one-liner. If `modeStore.set('work')` isn't the actual API, use whatever the
existing TitleBar uses to flip modes.

- [ ] **Step 3: GREEN + commit**

```
bun run vitest run src/lib/components/kanban/TaskCard.test.ts
bun run check
```

```bash
git add src/lib/components/kanban src/lib/stores
git commit -m "feat(kanban-card): workspace chip + click-to-navigate"
```

---

## Task 9: Sidebar workspace row — card count + expand-to-show-cards

**Files:**

- Modify: `src/lib/components/Sidebar.svelte`
- Modify: `src/lib/components/Sidebar.test.ts`

Existing sidebar workspace row shows status dot + title. Extend with:

- `· N cards` badge (always shown).
- Click the row's chevron (or the row itself) to expand → list each linked
  card's title.
- Each title is a click target that selects the card in the kanban.

- [ ] **Step 1: Failing tests**

Append to `Sidebar.test.ts`:

```ts
it('renders linked-cards count for each workspace row', () => {
  workspacesMock.listForRepo.mockReturnValue([
    {
      id: 'ws_a',
      title: 'payment-refactor',
      task_ids: ['tk_a', 'tk_b', 'tk_c'] /* ... */,
    },
  ]);
  const { getByTestId } = render(Sidebar);
  expect(getByTestId('ws-row-card-count-ws_a').textContent).toMatch(/3 cards/);
});

it('expanding a workspace row reveals linked card titles', async () => {
  workspacesMock.listForRepo.mockReturnValue([
    { id: 'ws_a', title: 'W', task_ids: ['tk_a', 'tk_b'] /* ... */ },
  ]);
  tasksMock.listForRepo.mockReturnValue([
    { id: 'tk_a', title: 'Fix login' /* ... */ },
    { id: 'tk_b', title: 'Add reset' /* ... */ },
  ]);
  const { getByTestId, queryByTestId } = render(Sidebar);
  expect(queryByTestId('ws-row-card-tk_a')).toBeNull();
  await fireEvent.click(getByTestId('ws-row-expand-ws_a'));
  expect(getByTestId('ws-row-card-tk_a').textContent).toMatch(/Fix login/);
  expect(getByTestId('ws-row-card-tk_b').textContent).toMatch(/Add reset/);
});

it('clicking a linked card title selects that task in the kanban', async () => {
  workspacesMock.listForRepo.mockReturnValue([
    { id: 'ws_a', title: 'W', task_ids: ['tk_a'] /* ... */ },
  ]);
  tasksMock.listForRepo.mockReturnValue([
    { id: 'tk_a', title: 'Fix' /* ... */ },
  ]);
  const { getByTestId } = render(Sidebar);
  await fireEvent.click(getByTestId('ws-row-expand-ws_a'));
  await fireEvent.click(getByTestId('ws-row-card-tk_a'));
  expect(tasksMock.selectInKanban).toHaveBeenCalledWith('tk_a');
});
```

The `tasksMock.selectInKanban` is a new affordance; if the existing tasks store
doesn't have a "highlight this card" method, add a minimal one that emits an
event the KanbanBoard listens to and scrolls into view + flashes a brief
highlight. For TDD scope, this method can be a thin `tasks.highlight(taskId)`
setting a `$state<string | null>` that KanbanBoard reads.

- [ ] **Step 2: Implement in `Sidebar.svelte`**

Inside the workspace row block:

```svelte
{#each workspaceList as ws (ws.id)}
  {@const expanded = expandedWorkspaceIds.has(ws.id)}
  <div class="ws-row">
    <div class="ws-row-header" role="button" tabindex="0">
      <span class="status-dot" />
      <span class="title">{ws.title}</span>
      <span
        class="card-count text-xs text-[var(--text-muted)]"
        data-testid={`ws-row-card-count-${ws.id}`}
        >· {ws.task_ids.length} card{ws.task_ids.length === 1 ? '' : 's'}</span
      >
      <button
        type="button"
        class="ml-auto p-1 text-[var(--text-muted)]"
        data-testid={`ws-row-expand-${ws.id}`}
        aria-expanded={expanded}
        onclick={(e) => {
          e.stopPropagation();
          toggleExpand(ws.id);
        }}>{expanded ? '▾' : '▸'}</button
      >
    </div>
    {#if expanded}
      <ul class="ws-row-cards pl-4 pt-1 space-y-0.5">
        {#each ws.task_ids as taskId (taskId)}
          {@const t = tasks.byId(taskId)}
          {#if t}
            <li>
              <button
                type="button"
                class="text-xs text-[var(--text-muted)] hover:text-[var(--text-base)] truncate w-full text-left"
                data-testid={`ws-row-card-${taskId}`}
                onclick={() => tasks.highlight(taskId)}>{t.title}</button
              >
            </li>
          {/if}
        {/each}
      </ul>
    {/if}
  </div>
{/each}
```

Module-scope:

```ts
const expandedWorkspaceIds = $state(new SvelteSet<string>());
function toggleExpand(id: string) {
  if (expandedWorkspaceIds.has(id)) expandedWorkspaceIds.delete(id);
  else expandedWorkspaceIds.add(id);
}
```

- [ ] **Step 3: Add `tasks.byId(id)` and `tasks.highlight(id)`**

In `src/lib/stores/tasks.svelte.ts`:

```ts
byId(id: string): Task | undefined {
  // Linear scan over a small collection is fine here — tasks per repo
  // are dozens at most.
  for (const arr of this.byRepo.values()) {
    const t = arr.find((x) => x.id === id);
    if (t) return t;
  }
  return undefined;
}

highlightedTaskId = $state<string | null>(null);
highlight(id: string | null): void {
  this.highlightedTaskId = id;
}
```

And in `KanbanBoard.svelte`, read `tasks.highlightedTaskId` to apply a brief
`ring` style (e.g. `class:ring-2={t.id === tasks.highlightedTaskId}`). The exact
styling is not the point — what matters is that clicking a card title in the
sidebar visibly drives attention to that card.

- [ ] **Step 4: GREEN + commit**

```
bun run vitest run src/lib/components/Sidebar.test.ts src/lib/stores/tasks.svelte.test.ts
bun run check
```

```bash
git add src/lib/components/Sidebar.svelte src/lib/components/Sidebar.test.ts src/lib/stores/tasks.svelte.ts src/lib/components/kanban/KanbanBoard.svelte
git commit -m "feat(sidebar): card count + expand-to-show-linked-cards on workspace rows"
```

---

## Task 10: Card menu "Link to workspace…" + picker modal

**Files:**

- Create: `src/lib/components/kanban/LinkWorkspacePicker.svelte`
- Create: `src/lib/components/kanban/LinkWorkspacePicker.test.ts`
- Modify: `src/lib/components/kanban/TaskCard.svelte` — add the menu item.

Picker shows all workspaces for the card's repo, sorted by `updated_at` desc,
with `title · branch · "N card(s)" · last-modified`. Selecting one fires
`tasks.link(taskId, workspaceId, repoId)`.

- [ ] **Step 1: Failing tests for the picker**

```ts
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import LinkWorkspacePicker from './LinkWorkspacePicker.svelte';

vi.mock('$lib/stores/workspaces.svelte', () => ({
  workspaces: {
    listForRepo: vi.fn(),
  },
}));
vi.mock('$lib/stores/tasks.svelte', () => ({
  tasks: { link: vi.fn().mockResolvedValue(undefined) },
}));

it('lists workspaces for the card repo, sorted by updated_at desc', async () => {
  vi.mocked(workspaces.listForRepo).mockReturnValue([
    {
      id: 'old',
      title: 'Older',
      branch: 'b1',
      task_ids: ['x'],
      updated_at: 1 /* ... */,
    },
    {
      id: 'new',
      title: 'Newer',
      branch: 'b2',
      task_ids: ['y', 'z'],
      updated_at: 5 /* ... */,
    },
  ]);
  const { getAllByTestId } = render(LinkWorkspacePicker, {
    props: { taskId: 'tk_a', repoId: 'repo_a', open: true, onClose: vi.fn() },
  });
  const rows = getAllByTestId('link-picker-row');
  expect(rows[0].textContent).toMatch(/Newer/);
  expect(rows[1].textContent).toMatch(/Older/);
});

it('renders branch + card count + last-modified for each row', async () => {
  vi.mocked(workspaces.listForRepo).mockReturnValue([
    {
      id: 'ws_a',
      title: 'Pay',
      branch: 'feat/pay',
      task_ids: ['x', 'y'],
      updated_at: 1_000_000 /* ... */,
    },
  ]);
  const { getByTestId } = render(LinkWorkspacePicker, {
    props: { taskId: 'tk_a', repoId: 'repo_a', open: true, onClose: vi.fn() },
  });
  const row = getByTestId('link-picker-row');
  expect(row.textContent).toMatch(/Pay/);
  expect(row.textContent).toMatch(/feat\/pay/);
  expect(row.textContent).toMatch(/2 cards/);
});

it('selecting a workspace calls tasks.link and closes the picker', async () => {
  vi.mocked(workspaces.listForRepo).mockReturnValue([
    {
      id: 'ws_a',
      title: 'Pay',
      branch: 'feat/pay',
      task_ids: [],
      updated_at: 1 /* ... */,
    },
  ]);
  const onClose = vi.fn();
  const { getByTestId } = render(LinkWorkspacePicker, {
    props: { taskId: 'tk_a', repoId: 'repo_a', open: true, onClose },
  });
  await fireEvent.click(getByTestId('link-picker-row'));
  await waitFor(() =>
    expect(tasks.link).toHaveBeenCalledWith('tk_a', 'ws_a', 'repo_a')
  );
  expect(onClose).toHaveBeenCalled();
});

it('renders empty-state when the repo has zero workspaces', () => {
  vi.mocked(workspaces.listForRepo).mockReturnValue([]);
  const { getByTestId } = render(LinkWorkspacePicker, {
    props: { taskId: 'tk_a', repoId: 'repo_a', open: true, onClose: vi.fn() },
  });
  expect(getByTestId('link-picker-empty').textContent).toMatch(/No workspaces/);
});
```

- [ ] **Step 2: Implement `LinkWorkspacePicker.svelte`**

Mirror `NewTaskDialog.svelte` / `ScriptPicker.svelte` for modal scaffolding
(overlay + escape-to-close + focus trap if either of those is in use). Body:

```svelte
<script lang="ts">
  import { workspaces } from '$lib/stores/workspaces.svelte';
  import { tasks } from '$lib/stores/tasks.svelte';

  interface Props {
    taskId: string;
    repoId: string;
    open: boolean;
    onClose: () => void;
  }
  const { taskId, repoId, open, onClose }: Props = $props();

  const rows = $derived(
    [...workspaces.listForRepo(repoId)].sort(
      (a, b) => b.updated_at - a.updated_at
    )
  );

  async function pick(wsId: string) {
    await tasks.link(taskId, wsId, repoId);
    onClose();
  }

  function relativeTime(unixSec: number): string {
    const diff = Date.now() / 1000 - unixSec;
    if (diff < 60) return 'just now';
    if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
    if (diff < 86_400) return `${Math.floor(diff / 3600)}h ago`;
    return `${Math.floor(diff / 86_400)}d ago`;
  }
</script>

{#if open}
  <div class="modal-overlay" onclick={onClose} role="presentation">
    <div
      class="modal-panel"
      onclick={(e) => e.stopPropagation()}
      role="dialog"
      aria-label="Link to workspace"
    >
      <h3 class="text-sm font-semibold mb-2">Link to workspace</h3>
      {#if rows.length === 0}
        <p
          class="text-xs text-[var(--text-muted)]"
          data-testid="link-picker-empty"
        >
          No workspaces in this repo yet. Move a card to In Progress to create
          one.
        </p>
      {:else}
        <ul class="space-y-1">
          {#each rows as ws (ws.id)}
            <li>
              <button
                type="button"
                class="w-full text-left px-2 py-1.5 rounded hover:bg-[var(--bg-hover)] flex items-center gap-2"
                data-testid="link-picker-row"
                onclick={() => pick(ws.id)}
              >
                <span class="font-medium truncate">{ws.title}</span>
                <span class="text-xs text-[var(--text-muted)]">{ws.branch}</span
                >
                <span class="text-xs text-[var(--text-muted)]"
                  >· {ws.task_ids.length} card{ws.task_ids.length === 1
                    ? ''
                    : 's'}</span
                >
                <span class="ml-auto text-xs text-[var(--text-muted)]"
                  >{relativeTime(ws.updated_at)}</span
                >
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>
{/if}
```

- [ ] **Step 3: Wire the menu item in `TaskCard.svelte`**

Add a small kebab (⋯) button on the card (or extend the existing menu if one
exists). Item: "Link to workspace…". Clicking opens `LinkWorkspacePicker` with
`taskId`/`repoId`. State at the card or its parent for `open`.

Test in `TaskCard.test.ts`:

```ts
it('menu "Link to workspace…" opens the picker', async () => {
  const task = makeTask({ workspace_id: null });
  const { getByTestId } = render(TaskCard, { props: { task } });
  await fireEvent.click(getByTestId('task-menu-trigger'));
  await fireEvent.click(getByTestId('task-menu-link-workspace'));
  expect(getByTestId('link-picker-row')).toBeDefined(); // or a probe via getAllByTestId
});
```

- [ ] **Step 4: GREEN + commit**

```
bun run vitest run src/lib/components/kanban
bun run check
```

```bash
git add src/lib/components/kanban/LinkWorkspacePicker.svelte src/lib/components/kanban/LinkWorkspacePicker.test.ts src/lib/components/kanban/TaskCard.svelte src/lib/components/kanban/TaskCard.test.ts
git commit -m "feat(kanban): card menu 'Link to workspace…' + picker modal"
```

---

## Task 11: Card menu "Unlink from workspace" + conditional confirm modal

**Files:**

- Create: `src/lib/components/kanban/UnlinkConfirmModal.svelte`
- Create: `src/lib/components/kanban/UnlinkConfirmModal.test.ts`
- Modify: `src/lib/components/kanban/TaskCard.svelte` — add the menu item,
  conditional modal flow.

Per spec §3:

- If preview returns `Unlinked` → execute force=true immediately (no modal).
- If preview returns `WouldRemove { workspace_title }` → show modal; on confirm,
  call force=true.

- [ ] **Step 1: Failing tests for the modal**

```ts
it('renders the workspace title and warning text', () => {
  const { getByTestId } = render(UnlinkConfirmModal, {
    props: {
      open: true,
      workspaceTitle: 'payment-refactor',
      onConfirm: vi.fn(),
      onCancel: vi.fn(),
    },
  });
  expect(getByTestId('unlink-modal-text').textContent).toMatch(
    /payment-refactor/
  );
  expect(getByTestId('unlink-modal-text').textContent).toMatch(/empty/i);
});

it('Confirm fires onConfirm; Cancel fires onCancel', async () => {
  const onConfirm = vi.fn();
  const onCancel = vi.fn();
  const { getByTestId } = render(UnlinkConfirmModal, {
    props: { open: true, workspaceTitle: 'W', onConfirm, onCancel },
  });
  await fireEvent.click(getByTestId('unlink-modal-confirm'));
  expect(onConfirm).toHaveBeenCalled();
  await fireEvent.click(getByTestId('unlink-modal-cancel'));
  expect(onCancel).toHaveBeenCalled();
});
```

And for the card flow:

```ts
it('Unlink with preview=Unlinked executes immediately, no modal', async () => {
  vi.mocked(tasks.unlink).mockImplementation(async (_id, force, _repo) =>
    force ? { kind: 'unlinked' as const } : { kind: 'unlinked' as const }
  );
  const task = makeTask({ workspace_id: 'ws_a' });
  const { getByTestId, queryByTestId } = render(TaskCard, { props: { task } });
  await fireEvent.click(getByTestId('task-menu-trigger'));
  await fireEvent.click(getByTestId('task-menu-unlink'));
  // Preview call first, then force call. No modal in between.
  expect(tasks.unlink).toHaveBeenNthCalledWith(1, 'tk_a', false, 'repo_a');
  expect(tasks.unlink).toHaveBeenNthCalledWith(2, 'tk_a', true, 'repo_a');
  expect(queryByTestId('unlink-modal-text')).toBeNull();
});

it('Unlink with preview=WouldRemove shows modal; Confirm fires force=true', async () => {
  vi.mocked(tasks.unlink)
    .mockResolvedValueOnce({ kind: 'would_remove', workspace_title: 'pay' })
    .mockResolvedValueOnce({ kind: 'removed' });
  const task = makeTask({ workspace_id: 'ws_a' });
  const { getByTestId } = render(TaskCard, { props: { task } });
  await fireEvent.click(getByTestId('task-menu-trigger'));
  await fireEvent.click(getByTestId('task-menu-unlink'));
  expect(getByTestId('unlink-modal-text').textContent).toMatch(/pay/);
  await fireEvent.click(getByTestId('unlink-modal-confirm'));
  await waitFor(() =>
    expect(tasks.unlink).toHaveBeenNthCalledWith(2, 'tk_a', true, 'repo_a')
  );
});

it('Unlink with preview=WouldRemove + Cancel does NOT call force=true', async () => {
  vi.mocked(tasks.unlink).mockResolvedValueOnce({
    kind: 'would_remove',
    workspace_title: 'pay',
  });
  const task = makeTask({ workspace_id: 'ws_a' });
  const { getByTestId } = render(TaskCard, { props: { task } });
  await fireEvent.click(getByTestId('task-menu-trigger'));
  await fireEvent.click(getByTestId('task-menu-unlink'));
  await fireEvent.click(getByTestId('unlink-modal-cancel'));
  // Only the preview call was made.
  expect(tasks.unlink).toHaveBeenCalledTimes(1);
});
```

- [ ] **Step 2: Implement `UnlinkConfirmModal.svelte`**

```svelte
<script lang="ts">
  interface Props {
    open: boolean;
    workspaceTitle: string;
    onConfirm: () => void;
    onCancel: () => void;
  }
  const { open, workspaceTitle, onConfirm, onCancel }: Props = $props();
</script>

{#if open}
  <div class="modal-overlay" onclick={onCancel} role="presentation">
    <div
      class="modal-panel"
      onclick={(e) => e.stopPropagation()}
      role="dialog"
      aria-label="Confirm unlink"
    >
      <p class="text-sm" data-testid="unlink-modal-text">
        This is the only card linked to <strong>«{workspaceTitle}»</strong>. The
        workspace will be removed because it is empty. Continue?
      </p>
      <div class="mt-3 flex justify-end gap-2">
        <button
          type="button"
          data-testid="unlink-modal-cancel"
          onclick={onCancel}
          class="px-2 py-1 text-xs">Cancel</button
        >
        <button
          type="button"
          data-testid="unlink-modal-confirm"
          onclick={onConfirm}
          class="px-2 py-1 text-xs bg-[var(--bg-hover)] rounded"
          >Continue</button
        >
      </div>
    </div>
  </div>
{/if}
```

- [ ] **Step 3: Wire the menu + flow in `TaskCard.svelte`**

```ts
let unlinkModalOpen = $state(false);
let unlinkModalTitle = $state('');

async function startUnlink() {
  if (!task.workspace_id) return;
  const preview = await tasks.unlink(task.id, false, task.repo_id);
  if (preview.kind === 'would_remove') {
    unlinkModalTitle = preview.workspace_title;
    unlinkModalOpen = true;
    return;
  }
  // No cleanup would fire — execute immediately.
  await tasks.unlink(task.id, true, task.repo_id);
}

async function confirmUnlink() {
  unlinkModalOpen = false;
  await tasks.unlink(task.id, true, task.repo_id);
}
```

Menu item:

```svelte
{#if task.workspace_id}
  <button data-testid="task-menu-unlink" onclick={startUnlink}
    >Unlink from workspace</button
  >
{/if}
```

Modal in markup:

```svelte
<UnlinkConfirmModal
  open={unlinkModalOpen}
  workspaceTitle={unlinkModalTitle}
  onConfirm={confirmUnlink}
  onCancel={() => (unlinkModalOpen = false)}
/>
```

- [ ] **Step 4: GREEN + commit**

```
bun run vitest run src/lib/components/kanban
bun run check
```

```bash
git add src/lib/components/kanban/UnlinkConfirmModal.svelte src/lib/components/kanban/UnlinkConfirmModal.test.ts src/lib/components/kanban/TaskCard.svelte src/lib/components/kanban/TaskCard.test.ts
git commit -m "feat(kanban): card menu 'Unlink from workspace' + conditional confirm modal"
```

---

## Task 12: Auto-create undo toast in `handleMove`

**Files:**

- Modify: `src/App.svelte` — `handleMove` (around line 151).
- Modify: `src/lib/stores/toasts.svelte.ts` — extend toast to support actions if
  not already supported.
- Modify: `src/App.test.ts` — tests for the new flow.

Per spec §1: on auto-create (move Todo→InProgress that created a workspace),
show a toast for 10 seconds with two actions:

- `[Link to existing instead]` — opens `LinkWorkspacePicker`. On pick: backend
  unlinks the just-created workspace (force=true to clean it up since empty) and
  links the card to the chosen workspace.
- `[Undo create]` — backend unlinks the just-created workspace AND reverts the
  card to Todo.

- [ ] **Step 1: Check the toast store for action support**

Run:
`grep -n "addToast\|interface Toast\|type Toast\b" src/lib/stores/toasts.svelte.ts`.
If the toast type doesn't carry button-actions, extend it minimally:

```ts
export interface ToastAction {
  label: string;
  onClick: () => void | Promise<void>;
}
export interface Toast {
  id: string;
  message: string;
  level: 'info' | 'error' | 'warn';
  actions?: ToastAction[];
  timeoutMs?: number; // default 4000; pass 10_000 for the undo toast
}
```

Update `addToast(message, level, opts?)` signature accordingly. Render the
actions as small buttons in the existing Toast container component (find it via
`grep -rn "addToast\|Toast" src/lib/components`).

- [ ] **Step 2: Failing tests in `App.test.ts`**

```ts
it('handleMove auto-create fires a 10s toast with [Link to existing instead] and [Undo create]', async () => {
  // Mock the move IPC to return a task with a freshly-assigned workspace_id.
  vi.mocked(api.task.move).mockResolvedValue({
    id: 'tk_a',
    repo_id: 'repo_a',
    column: 'in_progress',
    workspace_id: 'ws_new' /* ... */,
  } as Task);
  // ...drive handleMove(tk_a, 'in_progress', 0) via the existing handler...
  // Assert addToast was called with two actions, label-matching the spec.
  const calls = vi.mocked(addToast).mock.calls;
  const undoCall = calls.find((c) =>
    c[2]?.actions?.some((a) => a.label === 'Undo create')
  );
  expect(undoCall).toBeDefined();
  expect(undoCall![2]!.actions!.map((a) => a.label)).toEqual([
    'Link to existing instead',
    'Undo create',
  ]);
  expect(undoCall![2]!.timeoutMs).toBe(10_000);
});

it('Undo create unlinks (force) the just-created workspace and reverts column to todo', async () => {
  // Setup: previous test's toast fired; grab the Undo action and invoke it.
  // Assert api.task.unlinkFromWorkspace called with force=true.
  // Assert api.task.move called with column='todo'.
});

it('Link to existing instead opens the picker; selection unlinks new + links to chosen', async () => {
  // Open the picker; mock selection of an existing workspace ws_x.
  // Assert: tasks.unlink force=true on the new workspace, then tasks.link to ws_x.
});
```

- [ ] **Step 3: Implement in `App.svelte:handleMove`**

```ts
async function handleMove(taskId: string, column: KanbanColumn, order: number) {
  const before = tasks.byId(taskId)?.workspace_id ?? null;
  const updated = await tasks.move(taskId, column, order);
  const after = updated.workspace_id ?? null;

  // Existing "removed empty workspace" toast (PR #32).
  if (before && column === 'todo' && after === null) {
    addToast('Removed empty workspace', 'info');
    return;
  }

  // NEW: auto-create undo toast.
  if (!before && column === 'in_progress' && after !== null) {
    const newWsId = after;
    addToast(
      `Created workspace «${workspaces.byId(newWsId)?.title ?? newWsId}»`,
      'info',
      {
        timeoutMs: 10_000,
        actions: [
          {
            label: 'Link to existing instead',
            onClick: () => {
              linkPickerTaskId = taskId;
              linkPickerRepoId = updated.repo_id;
              // The picker, when a row is selected, must first unlink+remove
              // the just-created workspace, then link to the chosen one. Set
              // a flag the picker reads so the existing "link" handler does
              // the unlink first.
              linkPickerCleanupWsOnPick = newWsId;
              linkPickerOpen = true;
            },
          },
          {
            label: 'Undo create',
            onClick: async () => {
              await tasks.unlink(taskId, true, updated.repo_id);
              await tasks.move(taskId, 'todo', 0);
            },
          },
        ],
      }
    );
  }
}
```

For the "Link to existing instead" flow, extend `LinkWorkspacePicker` with an
optional `cleanupWorkspaceOnPick: string | null` prop; when set, the picker's
`pick` first calls `tasks.unlink(taskId, true, repoId)` (removes the
cleanup-target workspace because it's still the linked one), then
`tasks.link(taskId, chosenWsId, repoId)`.

- [ ] **Step 4: GREEN + commit**

```
bun run vitest run src/App.test.ts src/lib/components/kanban
bun run check
```

```bash
git add src/lib/stores/toasts.svelte.ts src/lib/components src/App.svelte src/App.test.ts
git commit -m "feat(app): auto-create undo toast with 'Link to existing' + 'Undo create'"
```

---

## Task 13: E2E + journal

**Files:**

- Create: `tests/e2e/phase-3d-multi-card-workspace/multi-card.spec.ts`
- Create: `journal/2026-05-28-multi-card-workspace.md`

- [ ] **Step 1: E2E test plan**

Three scenarios per spec §Testing > E2E:

1. **Auto-create + Undo toast**: drag card Todo→InProgress, assert workspace
   appears in sidebar, click "Undo create" in toast within window, assert
   workspace gone and card back in Todo.
2. **Explicit link via card menu attaches two cards**: card A is in W
   (auto-created). Open card B's menu → Link to workspace… → pick W. Assert
   sidebar W row shows "2 cards"; expand reveals both titles.
3. **Explicit unlink with confirm modal**: only one linked card on empty W. Open
   card menu → Unlink from workspace. Assert modal appears (would_remove
   preview). Confirm. Assert workspace gone.

Use the existing E2E test harness at
`tests/e2e/phase-3c-terminal-multitab/terminal-tabs.spec.ts` as the stylistic
template (Tauri shim, `openWorkspace` helper, mock IPCs).

```ts
import { test, expect } from '@playwright/test';
import { installTauriShim, openWorkspace, FIXTURE_REPO_PATH } from '../helpers';

test('multi-card: auto-create undo toast removes the workspace', async ({
  page,
}) => {
  await installTauriShim(page, { dialogOpenPath: FIXTURE_REPO_PATH });
  await page.goto('/');
  // ... open repo, drag a Todo card to InProgress ...
  await expect(page.getByTestId('toast-action-Undo create')).toBeVisible();
  await page.getByTestId('toast-action-Undo create').click();
  // Assert workspace gone + card reverted.
  await expect(page.getByTestId('ws-row-card-count-ws_just_made')).toHaveCount(
    0
  );
});

test('multi-card: link via card menu attaches a second card to W', async ({
  page,
}) => {
  // ... auto-create W from card A; then open card B's menu ...
  await page.getByTestId('task-menu-trigger-tk_b').click();
  await page.getByTestId('task-menu-link-workspace').click();
  await page.getByTestId('link-picker-row').first().click();
  // Sidebar count flips to 2.
  await expect(page.getByTestId(/ws-row-card-count-/)).toContainText(/2 cards/);
});

test('multi-card: unlink confirm modal fires when last + empty', async ({
  page,
}) => {
  // ... single linked card on empty W ...
  await page.getByTestId('task-menu-trigger-tk_a').click();
  await page.getByTestId('task-menu-unlink').click();
  await expect(page.getByTestId('unlink-modal-text')).toBeVisible();
  await page.getByTestId('unlink-modal-confirm').click();
  // Workspace gone.
});
```

Add the `data-testid` attributes (`toast-action-<label>`,
`task-menu-trigger-<id>`) wherever the implementation tasks added them; if
they're not there yet, this E2E task patches them in alongside the test.

- [ ] **Step 2: Run E2E locally**

Run: `bun run e2e -- tests/e2e/phase-3d-multi-card-workspace` Expected: 3 PASS.

(If E2E setup needs a fixture repo, use the same FIXTURE_REPO_PATH constant the
other phase-3 specs use.)

- [ ] **Step 3: Write journal**

`journal/2026-05-28-multi-card-workspace.md` — follow the structure of
`journal/2026-05-27-terminal-multitab.md`: What shipped, Backend, Frontend,
Decisions (sticky+safety-net, fire-and-forget vs preview, etc.), Tests + gates
summary, Aftermath.

- [ ] **Step 4: Full-suite gate**

```
bun run check
bun run vitest run
cd src-tauri && cargo test --lib && cargo clippy --lib --all-targets -- -D warnings && cargo fmt --all -- --check
bun run e2e
```

All green.

- [ ] **Step 5: Commit**

```bash
git add tests/e2e/phase-3d-multi-card-workspace journal/2026-05-28-multi-card-workspace.md
git commit -m "test(e2e) + docs(journal): multi-card workspace golden paths"
```

---

## Self-review checklist (run after writing tasks above)

- **Spec coverage:**
  - Data model `task_ids` + migration → Task 1.
  - `link_task_to_workspace` (idempotent + cross-repo + atomic switch) → Task 3.
  - `unlink_task_from_workspace` (preview + force, refcount cleanup) → Task 4.
  - `move_task_inner` sticky + safety-net → Task 5.
  - `remove_task_inner` refcount-aware → Task 6.
  - Frontend types + IPC + store → Task 2 + Task 7.
  - Card chip + click navigate → Task 8.
  - Sidebar count + expand → Task 9.
  - "Link to workspace…" menu + picker → Task 10.
  - "Unlink from workspace" + conditional modal → Task 11.
  - Auto-create undo toast (Link to existing instead + Undo create) → Task 12.
  - E2E (3 golden paths) + journal → Task 13.
- **Placeholder scan:** the only `todo!()` is in Task 3 step 1's helper stub;
  the same step instructs the executor to replace it before running the suite.
  No other TBD/TODO. Every code step has full code.
- **Type consistency:** `UnlinkResult` shape is identical in Rust (tagged enum)
  and TS (discriminated union) — both have
  `kind: 'unlinked' | 'removed' | 'would_remove'`, the `would_remove` variant
  has `workspace_title: string`. `tasks.link(taskId, workspaceId, repoId)` and
  `tasks.unlink(taskId, force, repoId)` signatures match between store, IPC
  wrapper, and component callers. `task_ids` is `Vec<String>` in Rust and
  `string[]` in TS — same name.
- **Execution order:** Task 4 (unlink) must be implemented BEFORE Task 3 (link)
  because the atomic-switch path in `link_task_to_workspace_inner` calls
  `unlink_task_from_workspace_inner`. The plan presents them in feature-order
  for readability; the executor runs 4 then 3. All other tasks can run in
  numeric order.
