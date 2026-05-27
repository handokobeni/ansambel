# Journal — 2026-05-27 — Terminal multi-tab

## What shipped

A workspace's Terminal tab now holds multiple terminal sessions (tabs), like the
Editor's file tabs — so a user can run e.g. a frontend and a backend dev server
side by side. Session-only: tabs + processes survive in-session navigation; not
restored across an app restart.

## Backend

- `commands/terminal.rs`: the terminal registry is keyed by `terminal_id` (the
  handle still records `workspace_id`); the five IPC commands take a
  `terminalId`. New `kill_workspace_terminals_inner` tears down every terminal a
  workspace owned.
- `commands/workspace.rs`: `remove_workspace_inner` kills the workspace's
  terminals so PTYs don't leak.

## Frontend

- `stores/terminal-tabs.svelte.ts`: per-workspace in-memory tab list + active
  id + monotonic "Terminal N" labels; 6-terminal cap.
- `components/workspace/TerminalTabBar.svelte`: tab strip + "+" (disabled at
  cap) + "×".
- `components/workspace/TerminalPane.svelte`: the single-terminal xterm logic,
  keyed by `terminalId` (reattach-then-spawn).
- `components/workspace/Terminal.svelte`: container — renders the tab bar and
  one kept-alive pane per tab (`display`-toggled, never unmounted within a
  workspace, so PTYs + scrollback survive tab switching). First-open
  auto-creates one terminal; closing the last shows a "+ New terminal" empty
  state.

## Decisions

- Session-only; no restart restore; no detached processes.
- No per-tab rename; auto "Terminal N", numbers not reused. Cap 6.

## Known limitation

App renders one `WorkspaceView` for the selected workspace (no per-workspace
`{#key}`), so switching to a different workspace and back remounts the panes:
the backend PTYs survive and `reattach` reconnects (processes keep running), but
xterm **scrollback is lost across a workspace switch**. Within a single
workspace, tab switching preserves scrollback + processes. This is a
pre-existing architectural property surfaced by the refactor, not introduced by
it; preserving cross-workspace scrollback is a future polish.

## Tests

- Rust: two terminals per workspace coexist + isolated; write/kill target one
  id; duplicate id rejected; `kill_workspace_terminals` scoped to a workspace;
  `remove_workspace` kills its terminals.
- Frontend: terminal-tabs store (add/cap/close/neighbour/labels/isolation);
  TerminalTabBar (render/active/＋disabled/×); TerminalPane + container tests.
- E2E: open Terminal → auto-spawn one → "+" → two panes mounted and live.
