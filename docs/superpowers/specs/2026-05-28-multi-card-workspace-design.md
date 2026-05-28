# Multi-Card Workspace — Design

**Date:** 2026-05-28 **Status:** Approved (brainstorming) **Author:** Handoko
Beni (with Claude)

## Goal

Let multiple kanban cards share a single workspace — one worktree, one branch,
one chat, one agent — for two interchangeable use cases:

- **Epic + subtasks** — one feature, several cards, all worked in the same
  workspace.
- **Continuation** — card A finished, card B picks up in the same workspace
  without spawning a new one.

The mechanism is a single "link card to workspace" primitive that serves both;
no separate "epic" abstraction.

## Background — current state

After PR #32 (workspace-lifecycle-cleanup):

- `WorkspaceInfo.task_id: Option<String>` — single-owner back-link. Set when the
  workspace was auto-created by moving a card Todo→InProgress.
- `Task.workspace_id: Option<String>` — forward link.
- `move_task_inner` rules:
  `needs_create = column==InProgress && reattach_ws_id.is_none() && linked.is_none()`.
  On move-to-Todo, run `is_workspace_empty(W)` and remove if true.
- `is_workspace_empty(W)` checks four fail-safe signals: no chat messages, no
  commits ahead of `origin/<base>`, clean worktree, no live agent.

PR #32's own journal flagged the next step: _"generalise to reference-counted
ownership and make the empty-on-Todo cleanup fire only when the last card
unlinks."_ This spec is that work.

## Data model

- `WorkspaceInfo.task_ids: Vec<String>` replaces `task_id: Option<String>`.
  Refcount is `task_ids.len()`.
- `#[serde(default)]` + a one-time migration on load: if a persisted
  `WorkspaceInfo` carries the legacy `task_id: Some(id)` and no `task_ids`
  field, normalise to `task_ids = vec![id]`. The next atomic save persists the
  new shape and the legacy field stops appearing.
- `Task.workspace_id: Option<String>` is unchanged. One card links to at most
  one workspace; the link is the canonical record of "this card belongs to W".

The two sides stay in sync via the backend mutation commands — never two sources
of truth that can drift.

## UX flows

### 1. Drag Todo → InProgress (unchanged default)

Auto-create workspace exactly as today. The link is the new card's
`workspace_id` → new W; W's `task_ids = [new_card_id]`.

A toast appears for 10 seconds with two actions:

- **`[Link to existing instead]`** — opens the picker described in §2; on pick,
  removes the just-created workspace and links the card to the chosen one.
- **`[Undo create]`** — removes the workspace and reverts the card to Todo.

If the toast times out without action, the new workspace stays. (Same as today;
the toast is added value, not a constraint.)

### 2. Explicit link via card menu — "Link to workspace…"

Available on cards in any column (Todo / InProgress / Done). Opens a picker:

- Source list: all workspaces in the same repo as the card.
- Sort: most recently active first (`updated_at` desc).
- Each row shows: title · branch · "`N` card(s)" · last-modified.
- Selection: sets `card.workspace_id = W` and appends the card id to
  `W.task_ids`. Card column is **not** changed (a Todo card stays Todo while
  linked).
- If the card was already linked to a different W' — atomic switch: decrement W'
  refcount (run cleanup check), increment W. Single transaction; no intermediate
  unlinked state visible to the user.

### 3. Explicit unlink via card menu — "Unlink from workspace"

Visible only on linked cards.

- If the unlink would not trigger cleanup (refcount > 1 OR workspace is not
  empty), execute immediately — no modal.
- If the unlink would trigger cleanup (refcount == 1 AND
  `is_workspace_empty(W)`), show a confirmation modal: _"This is the only card
  linked to «W». The workspace will be removed because it is empty. Continue?"_
  Confirm runs unlink and cleanup; Cancel is a no-op.

### 4. Refcount cleanup ("sticky links + safety-net cleanup")

The rule the user models:

> **Link is sticky. Exception: when the last card linked to W moves back to Todo
> AND the workspace shows no signs of work, the safety-net unlinks the card and
> removes the workspace.**

Pseudocode:

```
on move_to_todo(card):
  W = card.workspace_id
  if W is None:
    return  # nothing to do
  refcount = len(W.task_ids)
  if refcount > 1:
    # Shared workspace — link stays sticky. No cleanup.
    return
  if refcount == 1 and is_workspace_empty(W):
    # Safety net: only linked card moving away from a workspace with
    # no chat / no commits ahead of base / clean worktree / no live
    # agent. Treat as accidental — unlink + remove.
    unlink(card, W)
    remove_workspace(W)
    toast("Removed empty workspace «W»")
    return
  # refcount == 1 but workspace has work — keep the link so the user
  # doesn't lose state by bouncing.

on explicit_unlink(card) or delete_card(card):
  W = card.workspace_id
  if W is None: return
  W.task_ids.remove(card.id)
  card.workspace_id = None
  if len(W.task_ids) == 0 and is_workspace_empty(W):
    remove_workspace(W)
```

Why this is robust:

- **No silent destruction.** Workspace removal happens only when (a) the
  safety-net's narrow precondition holds, or (b) the user explicitly unlinks /
  deletes / removes the workspace.
- **Bouncing is safe** when the workspace has any work — `is_workspace_empty`
  returns false, safety-net stays its hand.
- **Accidental-drag protection (PR #32's win) is preserved** — refcount=1 +
  empty + move-to-Todo still fires the cleanup. Plus the new 10-second undo
  toast catches the case immediately.

Why this is scalable:

- One rule with one well-defined exception. No "owner vs joiner" distinction;
  all links are equal.
- Generalises to any N without new edge cases.
- Test surface is small: 3 cases (refcount > 1, refcount == 1 + empty, refcount
  == 1 + not-empty) × 3 triggers (move-to-Todo, explicit unlink, delete-card).

### 5. Move-to-InProgress on a card already linked

No-op for workspace lifecycle. The card's column updates; `workspace_id` stays;
`task_ids` already contains the card id. No auto-create.

This means: a Todo card that the user explicitly linked via the menu, when moved
to InProgress, just slots into its workspace — no second workspace.

## Visual representation

### Kanban card

Linked cards show a small chip near the bottom-right of the card body:

```
+-------------------------------+
| Fix login session bug         |
| short description…            |
|                               |
| [ⓘ ws: payment-refactor]  ← chip
+-------------------------------+
```

- Chip text: workspace title (truncated to fit; tooltip shows full title).
- Click chip: jump to the linked workspace (select it in the sidebar + flip mode
  to Work).
- **Show the chip whenever the card is linked**, regardless of refcount. Solo
  workspaces show it too — without the chip, a linked Todo card is visually
  indistinguishable from an unlinked one, and the chip is the primary affordance
  to navigate from card to workspace. The "shared" signal lives on the sidebar
  row's card count, not on the card.

### Sidebar workspace row

```
WORKSPACES                +
  ● payment-refactor  · 3 cards   ← refcount badge
     │ (click row to expand linked-cards list)
     ├─ Fix login session bug
     ├─ Add password reset
     └─ OAuth callback fix
  ● solo-workspace
```

- Cards count badge always shows current `task_ids.len()`. For solo workspaces,
  "1 card" is shown (consistent rendering; no special case for refcount==1).
- Expand reveals the linked cards' titles in insertion order. Each title is a
  click-target that jumps to that card in the kanban (selects the card and
  scrolls into view).

## Backend command surface

New IPC commands:

- `link_task_to_workspace(task_id: String, workspace_id: String) -> Result<(), String>`
  - Validates same-repo (both `task.repo_id` and `workspace.repo_id` match).
  - Idempotent: if already linked to that workspace, no-op success.
  - If already linked to a different workspace W', performs the atomic switch
    (unlink from W' with refcount/cleanup check, then link to W).
- `unlink_task_from_workspace(task_id: String) -> Result<{ workspace_removed: bool }, String>`
  - Clears `task.workspace_id`, removes the card from `workspace.task_ids`.
  - Runs the cleanup check (refcount==0 AND empty → remove).
  - Returns whether the workspace was removed, so the UI can display the
    confirmation toast accurately.
- `list_linkable_workspaces(repo_id: String) -> Result<Vec<WorkspaceInfo>, String>`
  - Source for the picker. Same-repo only. Sorted `updated_at` desc.

Changed existing commands:

- `move_task_inner`:
  - `needs_create = column==InProgress && reattach_ws_id.is_none() && linked.is_none()`
    — unchanged.
  - Move-to-Todo path applies the new sticky + safety-net rule (see §4
    pseudocode).
- `remove_task_inner` (delete card) — runs the refcount decrement + cleanup
  check identically to explicit unlink.

## Edge cases

| Scenario                                                   | Behavior                                                                                                                                 |
| ---------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| Bouncing a solo card on a workspace with chat/commits      | Sticky — workspace and link preserved.                                                                                                   |
| Accidental drag → Undo within 10s                          | Workspace removed via toast button.                                                                                                      |
| Accidental drag → no undo, then drag back to Todo          | Safety-net fires (refcount=1, empty) → cleanup. Matches PR #32.                                                                          |
| Epic A+B+C share W; A→Done; B continues                    | Workspace sticky (refcount=3).                                                                                                           |
| Shared abandoned: A+B linked, both moved to Todo           | Both sticky (refcount > 1 each move). User must explicit-unlink or delete W. Friction accepted.                                          |
| Delete one card                                            | Refcount decrement; cleanup if 0 + empty.                                                                                                |
| Link to a different workspace                              | Atomic switch — single transaction.                                                                                                      |
| Link card whose source repo doesn't match workspace's repo | Backend rejects with `InvalidState("repo mismatch")`. UI hides cross-repo entries in picker.                                             |
| `is_workspace_empty(W)` errors out (git failure)           | Fail-safe: treat as non-empty (keep workspace). Matches PR #32.                                                                          |
| Picker shown for repo with zero workspaces                 | Empty-state hint: _"No workspaces in this repo yet. Move a card to In Progress to create one."_ No fallback action in the picker itself. |
| User dismisses the undo toast manually                     | Workspace stays. Same as toast timeout.                                                                                                  |

## Out of scope (YAGNI)

- Multi-workspace per card (one card linked to N workspaces).
- Cross-repo linking.
- A dedicated workspace-rename UI (the spec only needs a stable workspace title;
  an explicit rename affordance can ship later if asked).
- Card ordering / grouping in the kanban driven by workspace membership.
- "Workspace templates" (starter pack of cards) or any workspace-side
  card-create flow.
- Migration of historical/deleted workspaces' linked cards.

## Testing

- **Rust unit (`commands/task.rs`, `commands/workspace.rs`):**
  - `task_ids` serde default + legacy `task_id` migration round-trip.
  - `move_task_inner` × move-to-Todo × {refcount>1, refcount==1+empty,
    refcount==1+not-empty}.
  - `link_task_to_workspace` × {fresh link, idempotent re-link, atomic switch,
    cross-repo rejection}.
  - `unlink_task_from_workspace` × {no cleanup needed, cleanup fires +
    `workspace_removed` true, cleanup blocked by non-empty}.
  - `list_linkable_workspaces` returns same-repo only, sorted by `updated_at`.
- **Frontend stores:**
  - Tasks store: link / unlink / move flows update local state and refire
    derived workspace card lists.
  - Workspaces store: `task_ids` reflected in the sidebar row's count + expand.
- **Frontend components:**
  - Card chip renders when linked; click navigates to workspace.
  - Sidebar workspace row count + expand renders linked card titles.
  - Card menu "Link to workspace…" / "Unlink from workspace" entries appear in
    the right conditions; picker renders sorted entries.
  - Confirmation modal fires only when unlinking the last linked card and
    `is_workspace_empty` is true.
  - Undo toast appears on auto-create; both actions (`Link to existing instead`,
    `Undo create`) drive the right commands.
- **E2E (Playwright):**
  - Auto-create + Undo toast removes the workspace.
  - Explicit link via card menu attaches two cards to one workspace; sidebar
    shows count=2 and expand lists both titles.
  - Explicit unlink fires the confirmation when last + empty; cleans up.
  - Move-to-Todo with refcount > 1 keeps the workspace.

Gates as today: 95% coverage on changed files (Rust + frontend), clippy
`-D warnings`, `bun run check`, prettier, ESLint, E2E smoke green.

## Open follow-ups (not in this plan)

- Workspace rename UI — currently we only set the title at auto-create. Once
  multi-card is in, a rename affordance becomes more valuable; tracked
  separately.
- Cross-card progress signals in the workspace header (e.g. "2/3 cards done") if
  requested.
