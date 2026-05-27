# Journal — 2026-05-27 — Workspace lifecycle cleanup

## What shipped

Predictable per-card workspace lifecycle, fixing two defects found in Phase 3a-4
manual testing:

- **No more duplicate workspaces.** `WorkspaceInfo.task_id` back-link +
  reattach-on-create make auto-create idempotent per `(repo_id, task_id)`.
  `refresh_tasks_inner` now preserves the `workspace_id`/`repo_id` link the Lark
  provider blanks on every refocus refresh — the root cause of the duplicates.
- **Empty workspaces auto-removed on return to Todo.** `move_task_inner` deletes
  the worktree + unlinks the card only when the workspace is empty (no chat, no
  commits ahead of base, clean worktree, no live agent). Any trace of work, or a
  git error, keeps the workspace (fail-safe). A toast surfaces the removal.

## Backend

- `state.rs`: `WorkspaceInfo.task_id: Option<String>` (`#[serde(default)]`).
- `commands/workspace.rs`: `is_workspace_empty` (4-signal, fail-safe;
  commit-ahead measured as `origin/<base>..HEAD`); `remove_workspace_inner`
  exposed `pub(crate)`.
- `commands/task.rs`: `refresh_tasks_inner` link preservation; `move_task_inner`
  reattach-or-create + empty-on-Todo removal.

## Frontend

- `App.svelte`: `handleMove` toasts "Removed empty workspace" when the backend
  clears the link on a move to Todo.
- `types.ts`: `Workspace.task_id`.

## Decisions

- A live agent (even idle/Waiting) makes a workspace non-empty → kept; we never
  auto-kill an agent to enable deletion.
- A stale link (task.workspace_id points at a workspace not in state) is left
  untouched — matches prior behavior; no surprise create, nothing to remove.

## Tests

- Rust: serde default round-trip; `is_workspace_empty` table (fresh / agent /
  commit / dirty / messages); refresh link preservation; reattach (no
  duplicate); empty-removed; non-empty-kept.
- Frontend: `App.test.ts` covers the `handleMove` removal toast — fires on (had
  workspace + move to Todo + backend returned `workspace_id: null`), and stays
  silent on the no-toast paths (move to In Progress, card had no workspace,
  backend kept the link). Driven via a minimal `KanbanBoard` test stub
  (`__mocks__/KanbanBoard.svelte`) since svelte-dnd-action isn't triggerable in
  jsdom.
- E2E: `tests/e2e/phase-3b-workspace-lifecycle/lifecycle.spec.ts` — move In
  Progress -> Todo -> In Progress keeps exactly one workspace, removed when
  empty, reattached not duplicated.
- Gates at merge: 785 Rust `cargo test --lib` + clippy `-D warnings` clean; 931
  vitest + `bun run check` clean; E2E green. `App.svelte` is excluded from the
  coverage report in `vite.config.ts` (thin orchestrator), so the
  95%-on-changed-files gate did not flag it; the toast branch is tested
  regardless.

## Aftermath

Built immediately after the Phase 3a-4 merge, on its own branch off `main` (PR
#32). The just-added `WorkspaceInfo.task_id` is a single owner today; a likely
next step (multi-card -> one shared workspace, for epics split into cards) would
generalise it to reference-counted ownership and make the empty-on-Todo cleanup
fire only when the last card unlinks.
