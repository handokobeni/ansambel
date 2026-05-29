# Journal — 2026-05-28 — Multi-card workspace

## What shipped

Multiple kanban cards can now share a single workspace. The relationship is
explicit (sticky-link primitive) and reversible: cards can be linked or unlinked
via a card-menu picker, the sidebar shows per-workspace card counts and an
expand list, and an auto-create undo toast gives a 10 s window to back out a
"Todo → InProgress" drag that produced an unwanted workspace. Bouncing a card
never destroys live work — workspaces are only torn down when their refcount
reaches zero AND they're empty (no commits, no live agent, no other attached
cards).

## Backend

- **T1 — `state.rs`** generalised `WorkspaceInfo.task_id: Option<String>` →
  `task_ids: Vec<String>` with `#[serde(from = "WorkspaceInfoRaw")]` so
  persisted files written by the old binary still load. Legacy
  `"task_id": "tk_x"` reads as `task_ids: vec!["tk_x"]`; both
  `"task_ids": [...]` and a missing field read cleanly. After a save cycle the
  legacy field disappears from disk.
- **T4 — `commands/task.rs::unlink_task_from_workspace`** preview-then-force
  protocol: `force=false` returns `kind: unlinked | would_remove` (the latter
  carries `workspace_title`); `force=true` actually performs the unlink and
  triggers refcount-aware cleanup if the workspace ends up empty.
- **T3 — `commands/task.rs::link_task_to_workspace`** idempotent (links the same
  workspace twice → no-op), cross-repo aware, atomic on switch (re-uses
  `unlink_task_from_workspace_inner(force=true)` under the workspaces lock so
  there's never a window where the card is unlinked but not yet relinked).
- **T5 — `commands/task.rs::move_task_inner`** sticky-link refcount
  short-circuit on `move-to-Todo`: a card returning to Todo keeps its workspace
  association unless that workspace's refcount drops to zero (the classic
  empty-workspace cleanup still fires for the last card; multi-card workspaces
  survive because another card is still attached).
- **T6 — `commands/task.rs::remove_task_inner`** refcount-aware: removing a card
  decrements the workspace's `task_ids`; cleanup only fires when refcount hits
  zero AND the safety-net guard reports the workspace is empty. (Force-gate
  retained: the explicit `force=true` parameter is still required from the
  frontend; the still-pending delete-confirm UX will gate it.)

## Frontend

- **T2 — `lib/types.ts`** `Workspace.task_ids: string[]` and a `UnlinkResult`
  discriminated union mirroring the Rust tagged enum
  (`unlinked | removed | would_remove`).
- **T7 — `lib/ipc.ts` + `lib/stores/tasks.svelte.ts`** new
  `api.task.linkToWorkspace` and `api.task.unlinkFromWorkspace` wrappers, plus
  `tasks.link(taskId, wsId, repoId)` / `tasks.unlink(taskId, force, repoId)`
  store methods that refresh both the tasks and workspaces maps on success so
  the sidebar's per-row counts and the card chip stay in sync.
- **T8 — `kanban/TaskCard.svelte`** workspace chip rendered when
  `task.workspace_id` is set; clicking selects the workspace and switches to
  Work mode via `workspaces.select` + `modeStore.set('work')`.
- **T9 — `Sidebar.svelte`** per-row card count badge
  (`data-testid="ws-row-card-count-{ws.id}"`) and expand chevron
  (`data-testid="ws-row-expand-{ws.id}"`); expanded state lives in a `SvelteSet`
  so each row toggles independently. Card titles surface as click-to-highlight
  buttons (`data-testid="ws-row-card-{taskId}"`).
- **T10 — `kanban/LinkWorkspacePicker.svelte`** modal picker triggered from the
  per-card menu's "Link to workspace…" item. Rows show title + branch +
  refcount + relative time; clicking a row calls `tasks.link`. Picker also
  accepts a `cleanupWorkspaceOnPick` prop used by the auto-create undo toast's
  "Link to existing instead" flow.
- **T11 — `kanban/UnlinkConfirmModal.svelte`** conditional confirm dialog
  triggered only when the preview `unlink_task_from_workspace(force=false)`
  returns `kind: would_remove`. Clicking Continue calls force=true; cancel
  leaves state untouched. When no cleanup would fire the unlink runs immediately
  without prompting.
- **T12 — `App.svelte::handleMove`** auto-create undo toast: when a card without
  a workspace lands in InProgress and the backend assigned a fresh workspace,
  surface a 10 s sticky toast with two actions — "Link to existing instead"
  (opens the picker with `cleanupWorkspaceOnPick` set to the just-auto-created
  workspace) and "Undo create" (unlinks + moves the card back to Todo). The
  picker is mounted at App level so it survives the toast's auto-dismiss.

## Decisions

- **Sticky-link + safety-net cleanup.** Refcount alone isn't enough — a
  workspace can hold live agent runs, commits, or scripts even with zero
  attached cards. Cleanup only fires when refcount=0 AND the safety-net guard
  reports "really empty"; the multi-card refcount adds the first half of the
  test while preserving the second.
- **Preview vs force unlink protocol.** Two-step
  `unlink_task_from_workspace(force=false → preview, force=true → execute)`
  keeps the destructive moment in the user's hands. The frontend modal closes
  the loop; if the preview returns `kind: unlinked` the call is silent (no
  modal).
- **Backward-compat serde migration.** `WorkspaceInfoRaw` with
  `#[serde(from = …)]` lets old persisted JSON (`"task_id": "tk_x"`) load
  alongside the new shape (`"task_ids": ["tk_x"]`). After the first save cycle
  the legacy field disappears from disk; no migration command, no startup flag.
- **Force-gate retained on `remove_task`.** Even with refcount-aware cleanup,
  `remove_task` still requires `force=true` for the actual delete step, pending
  the future delete-confirm UX. Removing this guard prematurely would risk
  silent data loss from a misclick on the kanban card's × button.

## Tests + gates

- **Vitest:** 989 passed across 61 files (61 → 61, +0 files; existing
  `TaskCard`, `Sidebar`, `LinkWorkspacePicker`, `UnlinkConfirmModal` suites
  cover the new wiring already).
- **Rust:** 819 passed; 0 failed; 0 ignored (covers `state.rs` serde migration,
  the two new commands, `move_task_inner` sticky path, `remove_task_inner`
  refcount path).
- **Clippy:** clean (`-D warnings`).
- **`cargo fmt --check`:** clean.
- **`bun run check`:** 496 files, 0 errors, 0 warnings.
- **E2E phase-3d:** 3 / 3 passing —
  `auto-create + Undo toast removes the workspace`,
  `link via card menu attaches a second card to W`,
  `unlink confirm modal fires when last + empty`.

## Aftermath

- **Delete-card confirm modal still pending.** `remove_task_inner` keeps the
  `force=true` guard; the frontend × button currently invokes without a confirm
  step. The plan kept this out of scope; landing a confirm modal is a small
  follow-up that lets the gate drop.
- **Menu dropdown lacks close-on-outside-click.** Opening a card menu and
  clicking elsewhere on the kanban leaves the menu open; only clicking the ⋯
  trigger again or selecting an item closes it. Same story for the picker /
  modal escape key — both close on backdrop click but not on Esc.
- **DnD-in-E2E synthetic finalize.** Phase-3d's spec dispatches a synthetic
  `finalize` CustomEvent on the destination column rather than driving real
  pointer events through svelte-dnd-action. Identical to phase-1b's pattern, but
  worth revisiting once Playwright's DnD support matures.
- **Sidebar reactivity gotcha (E2E only).** The phase-3d shim deep-clones
  workspaces on every `list_workspaces` read so the workspaces store's
  `SvelteMap.set(k, v)` sees a different reference per refresh — without the
  clone, the map's `prev_res !== value` check short-circuits and the sidebar
  doesn't re-derive. Documented inline in the spec; no impact on the real
  backend, which returns fresh serde-deserialized objects every call.

## Addendum — Slash command autocomplete (folded into same branch)

### What shipped

Typing `/` at the start of a line in the workspace chat input now opens an
autocomplete picker listing built-in + user (`~/.claude/commands/*.md`) +
plugin/skill (`~/.claude/plugins/*/{commands,skills}/...`) slash commands with
name + description. Keyboard nav (↑↓/Enter/Tab/Esc) + click-to-select. Selection
inserts `/full-name ` into the textarea; the user submits with Enter and the
existing chat→claude IPC carries the command through (verified: claude CLI in
stream-JSON mode still parses leading `/` as a slash command).

### Backend

- New `commands/slash_commands.rs`: `SlashCommand` + `SlashCommandSource` types,
  `list_slash_commands` Tauri command, `discover(claude_dir)` helper.
- Sources: hardcoded built-in list, `~/.claude/commands/`, `~/.claude/plugins/`.
  Hand-rolled minimal YAML frontmatter parser for `name:` and `description:`;
  falls back to the first non-blank body line.
- Dedupe: user > plugin > builtin. Sort: bucket Builtin → User → Plugin, then
  alphabetical within bucket.
- Fail-soft: missing `~/.claude`, unreadable files, malformed frontmatter all
  log + skip rather than erroring the entire call.

### Frontend

- TS types `SlashCommand`, `SlashCommandSource` mirror Rust serde shape.
- `api.slashCommands.list()` IPC wrapper.
- `SlashCommandsStore.load()` + `filtered(prefix)`.
- New `SlashCommandPicker.svelte` popover (no overlay backdrop — inline-anchored
  to the chat textarea via `anchorRect`). Owns keyboard nav via a `document`-
  level keydown listener while open.
- ChatInput wires the trigger regex `^/([\w-]*)$` on the current line, mounts
  the picker, and handles token replacement on selection.
- App boot fires `slashCommands.load()` fire-and-forget.

### Decisions

- **Discovery happens at app boot, once.** Plugin files don't change mid-session
  in practice. A `refresh` IPC was scaffolded but not surfaced in UI yet.
- **Submission goes through existing chat→claude IPC.** Claude CLI parses
  leading `/` as a slash command in stream-JSON mode too — the autocomplete is a
  discovery-only feature, not an execution path.
- **Hand-rolled YAML frontmatter parser** rather than pulling in `serde_yaml` or
  `yaml-rust2`. The spec only needs `name:` and `description:` lines; a ~25-line
  parser handles the entire surface and removes a dependency.
- **Dedupe priority User > Plugin > Builtin** so user-authored overrides always
  win; alphabetical plugin tie-break for determinism.

### Tests

- Rust (`commands/slash_commands.rs`): empty-claude-dir returns builtins only;
  builtin descriptions non-empty; user commands discovered from frontmatter and
  first-line fallback; plugin commands + skills discovered; dedupe user >
  plugin > builtin; bucket sort order; fail-soft on broken frontmatter.
- Frontend store: load + filtered (prefix, case-insensitive, empty-string,
  no-match).
- Picker component: all-items render, prefix filter, ArrowDown highlight + Enter
  selection, Esc close, click select, empty-state.
- ChatInput integration: opens on `/`, closes on space, selection replaces
  partial with `/full-name ` and positions the cursor.
- Final cumulative counts at branch tip: ~1003 vitest, ~829 cargo, clippy +
  fmt + check clean.
