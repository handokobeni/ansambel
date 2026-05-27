# Terminal Multi-Tab — Design

**Date:** 2026-05-27 **Status:** Approved (brainstorming) **Author:** Handoko
Beni (with Claude)

## Goal

Let a workspace's **Terminal** tab hold multiple terminal sessions (tabs), like
the Editor tab holds multiple file tabs. Primary use case: running an app that
needs several long-lived processes (e.g. `kelola-app` = one tab for the frontend
dev server, one for the backend). While Ansambel is open, the terminal tabs and
their running processes must survive all in-session navigation (switching
workspace, switching the top-level tab, Plan↔Work toggle).

## Scope decisions (locked in brainstorming)

- **Session-only, no restart restore.** Terminals are child processes of
  Ansambel; they die when the app closes. We do NOT persist or restore tabs
  across an app restart. After a restart the workspace starts fresh (a single
  terminal is spawned when the Terminal tab is first opened).
- **Must not disappear during a session.** Within one app session, a workspace's
  terminal tabs — and the processes/scrollback inside them — persist across
  every UI navigation. Switching away and back shows the same live terminals.
- **No detached/daemonized processes** (Option C rejected): too complex/risky
  (orphans, output reattach, cross-platform PTY reattach).
- **No per-tab rename** in v1 (YAGNI). Labels are auto-generated.
- **Cap: 6 terminals per workspace** to prevent runaway process spawning.

## Background — current behaviour

- Backend `commands/terminal.rs`:
  `AppState.terminals: HashMap<String, TerminalHandle>` keyed by
  **workspace_id** — exactly **one** terminal per workspace.
  `spawn_terminal_inner` errors with "terminal already active" if one exists;
  `reattach` re-subscribes. IPC commands
  `terminal_spawn/write/resize/kill/reattach` all take `workspaceId`.
- Each terminal owns a PTY (`portable-pty`), reader/writer threads, a cancel
  flag, and a Tauri `Channel` for streaming output bytes to xterm.js.
- Frontend `components/workspace/Terminal.svelte` mounts a single xterm.js
  instance; PTY output never touches Svelte state (xterm owns its buffer).
- The multi-tab pattern already exists for files: `stores/editor-tabs.svelte.ts`
  (in-memory, per-workspace open-file list + active tab) +
  `components/workspace/EditorTabBar.svelte`. The top-level
  Chat/Diff/Files/Editor/Terminal switch is `stores/workspace-tabs.svelte.ts`.
- Workspaces survive navigation today because xterm instances use
  `display: none/block` and are never unmounted, and the backend PTY lives in
  `AppState` until explicitly killed.

## Design

### Backend (`commands/terminal.rs`, `state.rs`)

- Change the terminal registry from `HashMap<workspace_id, TerminalHandle>` to a
  **two-level map**:
  `HashMap<workspace_id, HashMap<terminal_id, TerminalHandle>>` (keeps
  per-workspace grouping for cheap "kill all for this workspace"). `terminal_id`
  is a short unique id generated per spawn (e.g. `term_<rand>`), opaque to the
  user.
- IPC commands gain a `terminalId: String` argument:
  - `terminal_spawn(workspaceId, terminalId, channel, cols, rows)` — spawns a
    PTY for `(workspaceId, terminalId)`; errors if that id is already active
    (caller generates the id).
  - `terminal_write(workspaceId, terminalId, bytes)`
  - `terminal_resize(workspaceId, terminalId, cols, rows)`
  - `terminal_kill(workspaceId, terminalId)` — idempotent.
  - `terminal_reattach(workspaceId, terminalId, channel)` — re-subscribe an
    existing live terminal's output to a fresh channel (used when the xterm
    component re-binds).
- Each terminal remains an independent long-lived PTY + reader/writer threads
  - cancel flag + channel — unchanged per-terminal logic, just keyed by id.
- **Kill-all on workspace removal**: when a workspace is removed
  (`remove_workspace_inner`), kill every terminal under its `workspace_id`.
  (Today only one terminal exists to clean up; generalise to the inner map.)
- **No persistence**: nothing about terminals is written to disk.

### Frontend

- New store `stores/terminal-tabs.svelte.ts` (mirrors `editor-tabs.svelte.ts`,
  in-memory, per-workspace): for each `workspace_id`, an ordered list of
  `{ id, label }` plus the active terminal id. API roughly: `list(wsId)`,
  `activeId(wsId)`, `setActive(wsId, id)`, `add(wsId)` (returns the new id;
  enforces the 6-cap), `close(wsId, id)`, `nextLabel(wsId)`. Because the store
  is in-memory and keyed by workspace, the tab set is preserved across all
  in-session navigation automatically.
- New `components/workspace/TerminalTabBar.svelte` (mirrors
  `EditorTabBar.svelte`): renders the workspace's terminal tabs ("Terminal 1",
  "Terminal 2", …), a "+" button (disabled at the cap), and an "×" per tab.
- `Terminal.svelte` becomes per-`(workspaceId, terminalId)`: it renders the tab
  bar, and one **kept-alive** xterm instance per terminal id — all mounted,
  toggled with `display: none/block`, **never unmounted** (so a running dev
  server keeps streaming and scrollback survives when its tab or the whole
  workspace isn't visible). Each xterm binds to its terminal via the id-scoped
  IPC + a Tauri channel.
- **First-open behaviour**: when the Terminal tab is opened for a workspace that
  has no terminals yet, auto-create one (`add`) and spawn it — matching today's
  "open Terminal → one shell" UX.
- **Close-last behaviour**: closing the last tab leaves an empty state with a "+
  New terminal" button (no terminal running).

### Labels

Auto `Terminal N` where `N` comes from a per-workspace monotonically increasing
counter. Closing "Terminal 2" then adding again yields "Terminal 4" (numbers are
not reused) — simple and unambiguous.

## Lifecycle summary

| Event                                         | Effect                                                               |
| --------------------------------------------- | -------------------------------------------------------------------- |
| Open Terminal tab, workspace has no terminals | Auto-create + spawn 1                                                |
| Click "+" (below cap)                         | Spawn a new terminal, make it active                                 |
| Click "+" (at cap of 6)                       | No-op (button disabled)                                              |
| Click "×" on a tab                            | Kill that terminal's PTY, drop the tab; activate a neighbour         |
| Close the last tab                            | Empty state with "+ New terminal"                                    |
| Switch workspace / top-level tab / Plan↔Work  | Tabs + live processes preserved (xterm display-toggled, PTYs alive)  |
| Remove workspace                              | Kill all of its terminals                                            |
| App restart                                   | All terminals gone; workspace starts fresh on next Terminal-tab open |

## Error handling

- Spawn failure surfaces an error in that terminal pane (existing single-
  terminal error path, per id); other terminals are unaffected.
- `kill`/`reattach` for a missing id are safe no-ops / descriptive errors.
- All `#[tauri::command]` wrappers keep returning `Result<T, String>`.
- PTY reader threads handle EOF/errors gracefully and signal the channel on exit
  (unchanged).

## Testing

- **Backend unit** (`terminal.rs`): spawn two terminals under one workspace
  (distinct ids) → both live + isolated; write/resize target the right id;
  `terminal_kill` removes only that id; kill-all-on-workspace-remove clears the
  inner map; spawning a duplicate id errors.
- **Store** (`terminal-tabs.svelte.ts`): add (returns id, respects 6-cap), close
  (activates neighbour, empty when last closed), setActive, label counter
  monotonic, per-workspace isolation (two workspaces keep separate tab sets).
- **Component** (`TerminalTabBar.svelte`): renders tabs, "+" disabled at cap,
  "×" closes, click switches active.
- **E2E** (env-gated if it needs a real shell): open a workspace's Terminal, add
  a second terminal, run a command in each, switch top-level tab and back → both
  terminals still present and live.
- Coverage gate (95% on changed files), clippy, `bun run check` all green.

## Out of scope

- Restoring terminals/tabs across an app restart.
- Per-tab rename / custom titles.
- Detached/background processes that outlive the app.
- Splitting/paneling terminals within a tab (only tabs, not splits).
- Integrating with the existing "Scripts" feature (separate surface).
