# Workspace Lifecycle Cleanup — Design

**Date:** 2026-05-27 **Status:** Approved (brainstorming) **Author:** Handoko
Beni (with Claude)

## Goal

Make the per-card workspace lifecycle predictable: a workspace is created when a
card first enters **In Progress**, is **reattached** (never duplicated) when the
card re-enters In Progress, and is **auto-removed when the card returns to Todo
only if the workspace is empty** (no work would be lost). Eliminate the two
observed defects:

1. **Duplicate workspaces for the same card** — bouncing a Lark-bound card into
   In Progress more than once creates a second worktree.
2. **Orphan workspaces** — a card bounced In Progress → Todo leaves its
   workspace behind even when nothing was ever done in it.

## Background — current behaviour (grounded in code)

- **Creation** (`commands/task.rs::move_task_inner`): when a task moves into
  `InProgress` and `task.workspace_id.is_none()`, it calls
  `create_workspace_inner` (git worktree + branch + `WorkspaceInfo` metadata)
  and stamps `workspace_id` onto the task (in-memory mirror + `tasks.json` via
  `save_tasks`).
- **No removal** on any column move. `InProgress → Todo` leaves the workspace
  and the `workspace_id` link intact.
- **The duplicate bug**: `commands/task.rs::refresh_tasks_inner` (invoked by
  `tasks.refresh`, which `App.svelte`'s window-focus handler calls on every
  refocus for Lark-bound repos) does:
  ```rust
  st.tasks.retain(|_, t| t.repo_id != rid); // drop existing for repo
  for t in &tasks { st.tasks.insert(t.id.clone(), t.clone()); } // fresh from Lark
  ```
  Lark `Task`s carry `workspace_id = None` (it is a local-only concept the Lark
  provider cannot store). So every refresh wipes the link in the mirror. The
  next move into InProgress sees `workspace_id.is_none()` → creates a **second**
  workspace. `move_task_inner` and `update_task_inner` already re-stamp
  local-only fields; `refresh_tasks_inner` does not.
- **A workspace is real, destructive-to-delete state**: a git worktree + branch
  (possibly with commits), a chat transcript (`messages/<workspace-id>.jsonl`),
  and possibly a live agent process.
- `WorkspaceInfo` (`state.rs`) has no back-link to its originating task. It
  already uses the `#[serde(default)]` migration pattern (`worktree_dir`,
  `team_activity_private`).

## Design

### 1. Reattach-on-create (duplicate prevention, layer 1)

Add a back-link field to `WorkspaceInfo`:

```rust
/// Originating kanban task id, when the workspace was auto-created by
/// moving a card into In Progress. `serde(default)` → workspaces
/// persisted before this change deserialise as `None`. Used to reattach
/// a card to its existing workspace instead of creating a duplicate when
/// the local `workspace_id` link is lost (e.g. a Lark refresh wipes it).
#[serde(default)]
pub task_id: Option<String>,
```

`create_workspace_inner` gains an optional `task_id` parameter; the manual "+"
sidebar create passes `None`, the auto-create path passes the task id.

`move_task_inner` creation logic becomes:

1. If `task.workspace_id` is set **and** that workspace still exists → use it
   (current behaviour).
2. Else, search `AppState.workspaces` for an entry whose `task_id` equals this
   task's id **and** `repo_id` matches → **reattach**: stamp that workspace's id
   back onto the task; do **not** create a new one.
3. Else → create a new workspace, recording `task_id` on it.

This makes auto-create idempotent per `(repo_id, task_id)` regardless of whether
the `workspace_id` link survived in the mirror.

### 2. Preserve the link across refresh (duplicate prevention, layer 2 + UX)

`refresh_tasks_inner` snapshots the existing mirror's local-only fields
(`repo_id`, `workspace_id`) keyed by task id **before** the `retain`/`insert`
replacement, then re-stamps them onto the fresh tasks — the same pattern
`move_task_inner` / `update_task_inner` already use. This keeps
`task.workspace_id` accurate after a Lark refocus refresh (so the sidebar
selection and the cleanup check below find the right workspace), and prevents
the link loss that triggers layer 1 in the first place.

### 3. Auto-remove empty workspace on return to Todo

In `move_task_inner`, when the destination column is `Todo` and the task has a
linked workspace, evaluate whether the workspace is **empty**. If empty → delete
the worktree + `WorkspaceInfo` metadata (reusing the existing
`remove_workspace_inner` path) and clear `task.workspace_id`. If not empty →
leave everything intact (the card keeps its link; re-entering In Progress
reattaches per §1).

**"Empty" = ALL of the following hold** (evaluated in the backend, before
deletion):

- **No chat**: `messages/<workspace-id>.jsonl` is absent or has zero message
  records.
- **No commits ahead of base**:
  `git -C <worktree> rev-list --count <base_branch>..HEAD` is `0`.
- **Clean worktree**: `git -C <worktree> status --porcelain` is empty.
- **No live agent**: `AppState.agents` contains no handle for the workspace id.

If any check fails (or a git invocation errors — treat errors as "not empty /
unknown" to fail safe), the workspace is kept. Deletion only ever happens when
all four are unambiguously clean, so **no real work can be destroyed**.

A subtle toast (`addToast('Removed empty workspace', 'info')`) surfaces on the
frontend after a successful auto-remove so the disappearance is not mysterious.
The Rust layer signals this back to the frontend (e.g. the `move_task` IPC
result indicating a workspace was removed, or the existing
`workspaces.loadForRepo` resync after move surfaces the absence — the plan will
pick the least invasive wiring).

### 4. Lifecycle summary

| Transition                                          | Workspace effect                                  |
| --------------------------------------------------- | ------------------------------------------------- |
| Todo → In Progress, no link                         | Reattach if one exists for this task; else create |
| Todo → In Progress, valid link                      | Use existing (idempotent)                         |
| In Progress/Review/Done → Todo, empty workspace     | Delete worktree + unlink                          |
| In Progress/Review/Done → Todo, non-empty workspace | Keep, retain link                                 |
| Any move, no linked workspace                       | No-op                                             |
| Tasks refresh (Lark refocus)                        | `workspace_id` preserved (no link loss)           |
| Manual × in sidebar                                 | Unchanged — explicit delete, any state            |

The "→ Todo" rule applies regardless of source column. Moving Review/Done → Todo
runs the same empty check; such workspaces are virtually always non-empty, so
they are kept without a special case.

## Decisions (locked during brainstorming)

- **Spawned-but-idle agent counts as work**: opening the Work tab may auto-spawn
  an agent (status Waiting, no prompt sent). A live agent of any status makes
  the workspace "not empty" → kept. We do **not** auto-kill an agent to enable
  deletion. Consequence: merely opening Work mode and returning the card to Todo
  leaves the workspace; accepted for safety and simplicity. _(The plan verifies
  whether the Work tab auto-spawns; if it does not, this case is moot.)_
- **Provider-agnostic**: applies to local and Lark tasks. `workspace_id` and
  `task_id` are local-only; the Lark provider ignores them on its remote.

## Components / files touched

- `src-tauri/src/state.rs` — add `WorkspaceInfo.task_id: Option<String>`
  (`#[serde(default)]`).
- `src-tauri/src/commands/workspace.rs` —
  `create_workspace_inner(_with_publisher)` gains a `task_id: Option<String>`
  parameter, stored on the new `WorkspaceInfo`. A small
  `is_workspace_empty(state, ws) -> bool` helper (chat + git + agent checks).
- `src-tauri/src/commands/task.rs` —
  - `move_task_inner`: reattach-or-create logic; empty-check-and-remove on move
    to Todo.
  - `refresh_tasks_inner`: preserve `repo_id`/`workspace_id` across the mirror
    replacement.
- `src/lib/stores/*` + the move handler — surface the "removed empty workspace"
  toast and resync the workspace list after a move that removed one.

## Error handling

- Git invocations in the empty-check have timeouts and are treated as "not
  empty" on error (fail safe — never delete on uncertainty).
- Workspace deletion reuses `remove_workspace_inner`, which already returns
  descriptive errors; a failed auto-remove leaves the workspace intact and
  surfaces the error rather than half-deleting.
- All `#[tauri::command]` wrappers keep returning `Result<T, String>`.

## Testing

- **Reattach (no duplicate)**: with `task.workspace_id` cleared but a workspace
  bearing this `task_id` present, moving into InProgress reattaches and creates
  **no** second workspace.
- **Refresh preservation**: `refresh_tasks_inner` keeps `workspace_id` on a task
  after a Lark-style re-hydrate that returns `workspace_id = None`.
- **Empty check** — table-driven: deletes when all four signals clean; keeps
  when any one of {has chat, has commit ahead, dirty worktree, live agent} is
  present.
- **Move to Todo**: empty workspace removed + task unlinked; non-empty kept
  - link retained.
- **Idempotent bounce**: Todo → In Progress → Todo → In Progress on a clean card
  ends with exactly one (or zero, if removed) workspace, never two.
- **E2E**: golden path — create card, move to In Progress (workspace appears),
  move back to Todo (empty → disappears), move forward again (single workspace),
  all without duplicates.
- Coverage gate (95% on changed files) and existing Rust/clippy gates hold.

## Out of scope

- Changing the auto-create trigger (still "first entry to In Progress").
- A confirmation dialog for deleting non-empty workspaces (non-empty are never
  auto-deleted; manual × already confirms).
- Reworking how Lark tasks persist locally beyond the link-preservation fix.
