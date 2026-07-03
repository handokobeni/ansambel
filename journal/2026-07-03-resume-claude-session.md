# Journal — 2026-07-03 — Resume Claude Session Across Restart

## What shipped

Claude conversations now **resume automatically when Ansambel restarts** — you
open a workspace, chat, close the app, reopen tomorrow, and the agent picks up
mid-thread instead of starting cold. A kebab menu in the Chat panel exposes
"Restart agent (fresh session)" as the escape hatch for stuck sessions.

The whole feature is **~2-line runtime behavior change** in the spawn path, plus
a one-line restart command and a small UI affordance. All conversation state
persistence stays in Claude CLI (its own `~/.claude/projects/` store). Ansambel
adds zero new persistent files.

## Backend

- `commands/agent_core.rs::spawn_agent_inner` gains a sixth parameter
  `fresh: bool`. When `fresh == false`, `--continue` is appended to the claude
  args. Default spawn path (`spawn_agent` Tauri wrapper) passes `false`; the
  restart path passes `true`.
- New `commands/agent_core.rs::restart_agent_inner`: stops the current agent
  (silent no-op if none) via `stop_agent_inner_with_publisher` (with event tx)
  or `stop_agent_inner` (without), then spawns fresh.
- New Tauri command
  `commands/agent.rs::restart_agent(workspace_id, on_event, ...) -> Result<(), String>`.
  Mirrors `spawn_agent`'s wiring (`spawn_agent_inner` → `spawn_reader_thread`)
  but forces `fresh=true`.
- Registered in `lib.rs::tauri::generate_handler![...]` with the paired
  `restart_agent_command_is_registered` smoke test.

## Frontend

- `src/lib/ipc.ts`: `api.agent.restartFresh(workspaceId, channel)` — thin invoke
  wrapper on `restart_agent`.
- `src/lib/components/chat/ChatPanel.svelte`: new optional prop
  `onRestartAgent?: () => void | Promise<void>`. When defined, renders a kebab
  menu in a small header area with a single "Restart agent (fresh session)"
  item. Kebab and item carry the stable testids `chat-menu-trigger` and
  `chat-menu-restart-agent`.
- `src/lib/components/workspace/WorkspaceView.svelte`: implements
  `handleRestartAgent` that swaps in a fresh `Channel<AgentEvent>` (reusing the
  same event handler wired at initial spawn), calls `api.agent.restartFresh`,
  and toasts on success or failure. Also extracted the previously-duplicated
  agent-event handler into a shared `handleAgentEvent` function used by all
  three channel sites (initial spawn, re-spawn-on-send, restart).

## Decisions

- **Delegate all conversation persistence to Claude CLI.** Ansambel stores zero
  new state — no `sessions.json` writes, no `AppState.sessions` wiring. Claude
  CLI's `~/.claude/projects/<encoded-cwd>/*.jsonl` is the single source of
  truth. This matches the CLAUDE.md rule against "Checkpoint / restore of Claude
  conversation history" (we don't checkpoint or restore anything; Claude CLI
  does).
- **`--continue` gracefully degrades to fresh.** Empirically verified
  (2026-07-03: `timeout 15 claude --continue --print "reply with just OK"` in a
  fresh tempdir returned exit=0 with normal output). The plan therefore dropped
  the spec's speculative "fallback retry on exit-before-init" state machine. If
  a corrupt-session edge case ever surfaces in the wild, add the retry then.
- **No confirmation modal on Restart.** The kebab-menu two-click is deliberate
  enough; the action is not destructive (chat history in `messages/<wsId>.json`
  persists; only the Claude conversation-state pointer resets); and behavior is
  fully reversible — the next natural restart will `--continue` from the
  newly-fresh session.
- **Shared `handleAgentEvent`.** Task 3's implementer noticed the same
  event-routing body was duplicated at the initial-spawn and re-spawn-on- send
  sites in `WorkspaceView.svelte`; extracted to one function that all three
  channel sites (including the new restart) share. Prevents drift.
- **Module-level `channel` reassign with detach guard.** Reused the existing
  `handleSend` pattern
  (`if (channel) channel.onmessage = () => {}; channel = agentChannel(); channel.onmessage = handleAgentEvent`)
  so a stale `Status::Stopped` from the previous channel can't flap the
  freshly-started agent's UI status.

## Tests

- Rust (`commands/agent_core.rs` + `commands/agent.rs` + `lib.rs`):
  - `spawn_agent_inner_with_fresh_false_passes_continue_flag` — argv contains
    `--continue`.
  - `spawn_agent_inner_with_fresh_true_omits_continue_flag` — argv omits
    `--continue`.
  - `restart_agent_inner_stops_running_agent_and_respawns_fresh` — second
    spawn's argv omits `--continue`; `state.agents` still contains the workspace
    (new agent replaced old).
  - `restart_agent_inner_with_no_existing_agent_is_ok_and_spawns_fresh` — silent
    no-op stop path.
  - `restart_agent_inner_with_event_tx_uses_publisher_variant_and_respawns_fresh`
    — added post-review to cover the `Some(tx)` branch; observes the
    `StatusChanged { Waiting }` event via a subscribed rx.
  - `restart_agent_command_is_registered` smoke test in `lib.rs`.
- Frontend (`ChatPanel.test.ts`, `WorkspaceView.test.ts`):
  - Kebab visible when `onRestartAgent` provided.
  - Kebab absent when the prop is undefined.
  - Menu-item click invokes `onRestartAgent`.
  - WorkspaceView test asserts `invoke('restart_agent', ...)` fires from the
    restart callback.
- All new tests use the argv-dump fake-claude shell-script pattern established
  by earlier terminal-multitab work.

## Gates at merge-time

- `cargo test --lib`: **837 passed**, 0 failed.
- `bun run vitest run`: **1038 passed**, 0 failed (63 files).
- `cargo clippy --lib --all-targets -- -D warnings`: clean.
- `cargo fmt --all -- --check`: clean.
- `bun run check`: 500 files, 0 errors, 0 warnings.
- `bun run lint`: 0 errors, 1 pre-existing `no-explicit-any` warning in
  `lark-binding-filters.svelte.test.ts` (unrelated).

## Aftermath / known follow-ups

- **Kebab a11y polish.** Menu items are plain `<button>` inside `role="menu"`
  without `role="menuitem"`; no click-outside/Escape-to- close. Deferred —
  pattern-consistent with the existing kanban kebab menus (which also lack
  these); a repo-wide a11y pass is the right vehicle.
- **Dead scaffolding cleanup.** `AppState.sessions: HashMap<...>` and the
  `sessions.json` path constant in `platform/paths.rs` remain in place though
  nothing writes them. Separate housekeeping commit later.
- **Coverage constraint wording.** `vitest.config.ts`'s
  `thresholds: { branches: 93 }` is an aggregate gate, not per-file.
  `ChatPanel.svelte`'s branch coverage sits at 91.66% locally (single
  pre-existing untouched catch no-op); the aggregate stays above 93%. Plan text
  ("93% branch on changed files") is stricter than the enforced config; noted
  for future tightening if the project moves to `perFile: true`.
- **`.superpowers/` prettier ignore.** Added `.superpowers` to `.prettierignore`
  this task so subagent-driven scratch files (task briefs, reports, progress
  ledger) stop tripping `bun run lint`. The path is already `.gitignore`-covered
  inside `.superpowers/sdd/`, but prettier doesn't inherit that.
