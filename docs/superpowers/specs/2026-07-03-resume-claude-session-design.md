# Resume Claude Session Across Restart — Design

**Date:** 2026-07-03 **Status:** Approved (brainstorming) **Author:** Handoko
Beni (with Claude)

## Goal

When Ansambel is closed and reopened, the Claude agent for each workspace should
**resume the previous conversation** by default instead of starting a fresh
session. User keeps a kebab-menu escape hatch to force a fresh session when
context is polluted or the agent is stuck.

## Background — current state

- `commands/agent_core.rs::spawn_agent_inner` spawns claude with
  `-p --input-format stream-json --output-format stream-json --verbose --include-partial-messages --permission-mode bypassPermissions --disallowedTools EnterWorktree,ExitWorktree`
  — **no `--continue` or `--resume`** — so every spawn is a fresh Claude
  session.
- Chat message history persists in `messages/<wsId>.json` (UI display); it is
  NOT replayed to the new Claude session.
- `AgentHandle.session_id` captures the id from the `init` event but only
  in-memory; nothing is written to disk.
- `AppState.sessions: HashMap<...>` and `sessions.json` are scaffolded in
  `state.rs` + `paths.rs` but there is no code that reads or writes
  `sessions.json`. Dead scaffolding.

## Design

### Anchor: Claude CLI's own `--continue`

The `claude --continue` flag ("`-c`") resumes the most recent conversation in
the current directory. Claude CLI persists per-CWD session state under
`~/.claude/projects/<encoded-cwd>/*.jsonl`. Since Ansambel already spawns claude
with `cmd.current_dir(&worktree_dir)` and workspaces have distinct worktrees,
`--continue` will naturally pick up each workspace's own last session without
Ansambel tracking session IDs.

**Result: Ansambel needs zero new persistent state.** Claude CLI is the single
source of truth for session state. Ansambel only decides _whether_ to pass
`--continue` on each spawn.

### Spawn flow (only real behavior change)

`spawn_agent_inner` gains one boolean parameter `fresh: bool`:

- **`fresh == false`** (default, used on every normal spawn): add `--continue`
  to the claude args.
- **`fresh == true`** (used by the restart-fresh command): omit `--continue`.

Fallback: if the child process exits before emitting the `init` event AND we
passed `--continue`, retry the spawn ONCE without `--continue`. This handles the
rare case of a corrupt / missing Claude session file. Cap at one retry — if the
fresh spawn also dies early, surface the error normally. The wait-for- init flow
already exists (that's how `AgentHandle.session_id` gets populated today); the
retry branch is a small state-machine addition.

### Restart-agent (escape hatch)

New Tauri command `restart_agent(workspace_id)`:

1. Call the existing `stop_agent_inner(workspace_id)` to kill the current
   agent + drop the `AgentHandle`.
2. Call `spawn_agent_inner(workspace_id, fresh=true)` to respawn without
   `--continue`.

That is the entire logic. No new persistence step, no confirm dialog.

### UI

`ChatPanel.svelte` gains a kebab (`⋮`) menu in its header (or in whichever
existing controls area the panel has — implementer decides based on layout).
Menu contains one item:

- **Restart agent (fresh session)** → calls
  `api.agent.restartFresh(workspaceId)`. On success, toast:
  `"Agent restarted (fresh session)"`.

**No confirmation modal.** The kebab menu itself is a deliberate two-click
gesture; the action is not destructive (chat history in `messages/<wsId>.json`
persists; only the Claude conversation-state pointer is reset); and behavior is
fully reversible in practice — the next natural restart will `--continue` from
the newly-fresh session, so the user is never locked out.

### Data model

**No new fields.** In particular:

- `AppState.sessions` is not touched (leave the dead scaffolding alone or remove
  in a separate cleanup — out of scope here).
- No `sessions.json` writes.
- No new `persistence/sessions.rs` module.

## Command surface

### New

- `commands/agent.rs::restart_agent(workspace_id: String) -> Result<(), String>`
  Tauri wrapper. Calls `stop_agent_inner` then `spawn_agent_inner(fresh=true)`.
  Registered in `lib.rs::tauri::generate_handler![...]`.

### Modified

- `commands/agent_core.rs::spawn_agent_inner` — new `fresh: bool` parameter.
  Existing callers (from `commands/agent.rs::spawn_agent` and any test fixtures)
  update to pass `fresh: false` explicitly. The `--continue` arg is
  conditionally appended based on this param. The wait-for-init flow gains one
  branch: on child-exit-before-init when `--continue` was passed, retry once
  without `--continue`.

### Frontend IPC

- `api.agent.restartFresh(workspaceId: string): Promise<void>` in
  `src/lib/ipc.ts` calling `invoke('restart_agent', { workspaceId })`.

## Edge cases

| Scenario                                                          | Behavior                                                                                                                                                                      |
| ----------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| First-ever spawn for a workspace (no prior Claude session in CWD) | `--continue` is passed anyway; if Claude exits early, fallback retry without `--continue` completes the spawn as a fresh session. One extra spawn attempt, invisible to user. |
| Corrupt / missing Claude session file                             | Same as above — fallback fires, fresh spawn succeeds.                                                                                                                         |
| Both `--continue` spawn AND fallback spawn fail                   | Bubble the error up as a normal spawn failure (surfaced in the UI).                                                                                                           |
| Agent Running at app close                                        | Next reopen: `--continue` picks up the last session state. Interrupted-turn resumption is Claude CLI's problem, not Ansambel's.                                               |
| User deletes `~/.claude/projects/*` manually                      | Fallback fires — Ansambel does not need to detect the deletion.                                                                                                               |
| User runs `claude --resume <id>` in a separate terminal           | Ansambel is unaware and does not care — Claude CLI is the single source of truth.                                                                                             |
| Two Ansambel workspaces share the same worktree path              | Can't happen — workspace-to-worktree is 1:1 in the current model (see `WorkspaceInfo.worktree_dir` semantics).                                                                |

## Testing

### Rust

- `spawn_agent_inner` with `fresh=false` includes `--continue` in the spawned
  command args.
- `spawn_agent_inner` with `fresh=true` omits `--continue`.
- Fallback retry: mock a claude binary that exits early when given `--continue`;
  verify `spawn_agent_inner` retries once without `--continue` and completes.
- Cap on retries: mock a claude binary that always exits early; verify
  `spawn_agent_inner` gives up after the one retry and returns an error.
- `restart_agent_inner`: with a workspace whose agent is running, verify the
  current handle is stopped and a new one is created with `fresh=true`.

### Frontend

- `ChatPanel.svelte`: kebab menu opens on click; item click fires
  `api.agent.restartFresh(workspaceId)` and shows the success toast.
- IPC wrapper is a thin invoke — covered by the ChatPanel test through the
  mocked `$lib/ipc`.

### E2E

Not needed for v1. The behavior is Claude-CLI-mediated; a meaningful E2E would
require running a real Claude binary, which the existing E2E harness does not do
(`ANSAMBEL_MOCK_CLAUDE=1`).

### Gates

Standard: 95% coverage on changed files, clippy `-D warnings`, `bun run check`,
prettier + ESLint, `cargo fmt --check`.

## Out of scope (YAGNI)

- **Undo restart** — no UI to restore the previous session after a
  restart-fresh; the operation is intentional and reversible in practice (next
  spawn will `--continue` from the fresh one).
- **Session metadata in UI** ("session started 2h ago", "23 turns") — requires
  reading Claude CLI's own JSONL files; premature.
- **Session picker** (choose from a list of past sessions in this CWD) — Claude
  CLI already offers `--resume` interactive picker if a user wants this;
  Ansambel does not need to mirror it.
- **Terminal PTY sessions** — orthogonal, already covered by phase-3c
  terminal-multitab.
- **Cross-machine session sync** — Claude CLI sessions are per-machine; syncing
  them is a Claude CLI concern.
- **Cleaning up the dead `AppState.sessions` scaffolding + `sessions.json` path
  constant** — separate housekeeping commit if we want it later.

## Effort estimate

~2–4 hours end-to-end via the standard subagent-driven flow:

- **T1:** backend spawn wiring (`fresh` param + `--continue` + fallback retry)
  with Rust tests.
- **T2:** `restart_agent` Tauri command + registration + tests.
- **T3:** frontend IPC + ChatPanel kebab menu + tests.
- **T4:** journal + full-suite gates.
