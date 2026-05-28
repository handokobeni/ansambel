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

## Follow-up fixes (same branch)

Three issues surfaced after the multi-tab tasks landed were fixed on this
branch:

- **`detect_default_branch` offline failure.** `add_repo` failed offline because
  detection went straight to a network `ls-remote`. Added a true offline tier: a
  local remote-tracking ref check
  (`git show-ref --verify refs/remotes/origin/{main,master}`) before the network
  fallback, so add-repo works offline / when the remote needs auth. Still never
  falls back to local _branches_.
- **Plan↔Work state loss (Fix A).** Toggling Work→Plan→Work used to blank a
  running terminal. `App.svelte` now keeps both the Plan content (Kanban) and
  the Work content (`WorkspaceView`) mounted as siblings, toggled with
  `class:hidden` instead of mutually-exclusive `{:else if mode}` branches — so
  the toggle no longer remounts the panes. Lossless: xterm + scrollback +
  process all preserved.
- **Cross-workspace scrollback (Fix B).** A remount (workspace switch, mirror
  flow) used to restore only the process, not the visible screen. Now each
  `TerminalPane` serializes its live xterm grid (`@xterm/addon-serialize`) into
  a session-only stash (`stores/terminal-snapshots.ts`) on destroy, and the next
  mount of the same `terminalId` repaints from that snapshot before
  resubscribing to live PTY output (backend `reattach` is receiver-only). So ANY
  remount now restores the visible terminal, not just the process. First tried a
  backend 256 KiB raw-byte ring buffer replayed on reattach — discarded because
  raw bytes re-execute a full-screen program's cursor/clear sequences against a
  differently-sized terminal, which garbled vite's banner (a large blank gap) on
  remount. A serialized grid has no such dependency. Trade-off: output emitted
  in the brief dispose→reattach window is not restored (the live stream resumes
  immediately); fresh `spawn` shows the `[xterm ready]` marker, not a restore.

Together Fix A + Fix B resolve the original "terminal loses its `npm run dev`
state on Plan↔Work" report: A makes the common toggle lossless, B restores the
view on any deeper remount.

## Tests

- Rust: two terminals per workspace coexist + isolated; write/kill target one
  id; duplicate id rejected; `kill_workspace_terminals` scoped to a workspace;
  `remove_workspace` kills its terminals.
- Frontend: terminal-tabs store (add/cap/close/neighbour/labels/isolation);
  TerminalTabBar (render/active/＋disabled/×); TerminalPane + container tests.
- E2E: open Terminal → auto-spawn one → "+" → two panes mounted and live.
- Follow-ups: `detect_default_branch` offline tier (local-tracking-ref hit +
  ls-remote fallback when both Tier 1 and Tier 2 refs are removed); Fix A
  regression (`App.test.ts` asserts `WorkspaceView` stays mounted/hidden, not
  removed, when toggling to Plan); Fix B (snapshot stash one-shot
  `take`/`drop`/isolation; `TerminalPane` restores a stashed snapshot on mount
  instead of the ready marker, serializes on destroy, and a tab close drops its
  stash; backend reattach reverted to receiver-only).
