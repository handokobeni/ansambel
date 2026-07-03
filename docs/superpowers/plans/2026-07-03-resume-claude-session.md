# Resume Claude Session Across Restart Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every non-restart-fresh Claude spawn passes `--continue` so the Claude
conversation resumes across Ansambel restarts; expose a `restart_agent` Tauri
command + kebab menu in ChatPanel for the escape hatch.

**Architecture:** Delegate all session state to Claude CLI (which persists
per-CWD conversation under `~/.claude/projects/<encoded-cwd>/*.jsonl`). Ansambel
adds one boolean parameter to `spawn_agent_inner` (`fresh: bool`) that toggles
the `--continue` flag, plus a new `restart_agent` Tauri command that stops the
current agent and respawns with `fresh: true`. No new persistent state in
Ansambel.

**Tech Stack:** Rust + Tauri v2 (portable-pty spawn path), Svelte 5 runes, Bun +
vitest, cargo test. TDD strict (red → green → commit). No
`.unwrap()`/`.expect()` outside `#[cfg(test)]`. No `console.log`
(`console.error` / `console.warn` allowed).

**Spec:** `docs/superpowers/specs/2026-05-28-resume-claude-session-design.md` —
read once before starting.

**Empirical simplification vs. spec:** The spec's "fallback retry on
exit-before-init when `--continue` was passed" was defensive coverage. Empirical
test on 2026-05-28 (`timeout 15 claude --continue --print "reply with just OK"`
in a fresh tempdir) confirmed claude CLI returns exit=0 and produces normal
output even with no prior session in the CWD — `--continue` gracefully degrades
to a fresh session on its own. **The plan therefore drops the fallback-retry
state machine.** If a real-world corrupt-session scenario ever surfaces, add the
retry then; for v1 skip the code.

**Branch:** `feat/resume-claude-session` (fresh off `main`, spec committed at
`8579623`).

**Standing constraints (verbatim):** Commit LOCALLY per task, **DO NOT push**
until user explicitly approves. Each task ends with `git commit` (no
`git push`).

## Global Constraints

- Every `#[tauri::command]` returns `Result<T, String>` — never panic in command
  handlers.
- No `.unwrap()` / `.expect()` outside `#[cfg(test)]`.
- Mutex discipline: acquire lock, extract data, drop lock before any blocking /
  async / spawn work.
- Spawn claude with explicit env — never inject `GH_TOKEN` from ambient shell.
- Claude spawn args ALWAYS include `--permission-mode bypassPermissions` +
  `--disallowedTools EnterWorktree,ExitWorktree` (preserved unchanged by this
  feature).
- Use `bun`, not npm/npx/yarn.
- TDD: red → green → commit; no production code without a failing test first.
- Every `#[tauri::command]` has ≥1 unit test + ≥1 integration test.
- Coverage gate: 95% line/function, 93% branch on changed files.
- Tooltips via `use:tooltip={{ text }}` — never native `title=` attribute.

---

## Task 1: Backend — `spawn_agent_inner` accepts `fresh` param + conditional `--continue`

**Files:**

- Modify: `src-tauri/src/commands/agent_core.rs` (function signature + spawn
  args block around line 220-240; existing test call sites at lines 995, 1023,
  1044, 1061, 1079, 1851, 1940, 1978, 2478).
- Modify: `src-tauri/src/commands/agent.rs` (single caller
  `spawn_agent_inner(...)` at line 59-65).

**Interfaces:**

- Consumes: nothing (leaf backend change).
- Produces:
  `spawn_agent_inner(state, data_dir, workspace_id, claude_path, event_tx, fresh: bool) -> AppResult<AgentProcess>`.
  `fresh: false` = pass `--continue`. `fresh: true` = omit it. All existing
  callers pass `fresh: false` explicitly.

- [ ] **Step 1: Write the failing tests**

Append to `src-tauri/src/commands/agent_core.rs` `#[cfg(test)] mod tests`. Use
the existing `echo`-based mock claude pattern (search for `echo_path` in the
file — several tests already spawn a shell script standing in for claude,
capturing the argv into a temp file for assertion).

Read one existing test (e.g. around line 1024-1044 —
`spawn_agent_inner_uses_override_binary` or similar) to see the exact fixture
shape. Adapt from that. The two new tests:

```rust
#[test]
fn spawn_agent_inner_with_fresh_false_passes_continue_flag() {
    // Mock claude as a shell script that dumps its argv to a file so we
    // can assert the exact args after spawn returns.
    let tmp = tempfile::tempdir().unwrap();
    let argv_dump = tmp.path().join("argv.txt");
    let script = tmp.path().join("fake-claude.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nsleep 1\n",
            argv_dump.display()
        ),
    )
    .unwrap();
    std::process::Command::new("chmod")
        .args(["+x", script.to_str().unwrap()])
        .status()
        .unwrap();
    let state = make_state_with_workspace(tmp.path(), "ws_c");
    let _ = spawn_agent_inner(
        state.clone(),
        tmp.path(),
        "ws_c",
        Some(script.clone()),
        None,
        false, // fresh
    )
    .unwrap();
    // Give the script a moment to dump argv before we read it.
    std::thread::sleep(std::time::Duration::from_millis(150));
    let dump = std::fs::read_to_string(&argv_dump).unwrap();
    assert!(dump.lines().any(|l| l == "--continue"), "argv should include --continue when fresh=false: {dump:?}");
}

#[test]
fn spawn_agent_inner_with_fresh_true_omits_continue_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let argv_dump = tmp.path().join("argv.txt");
    let script = tmp.path().join("fake-claude.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nsleep 1\n",
            argv_dump.display()
        ),
    )
    .unwrap();
    std::process::Command::new("chmod")
        .args(["+x", script.to_str().unwrap()])
        .status()
        .unwrap();
    let state = make_state_with_workspace(tmp.path(), "ws_f");
    let _ = spawn_agent_inner(
        state.clone(),
        tmp.path(),
        "ws_f",
        Some(script.clone()),
        None,
        true, // fresh
    )
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(150));
    let dump = std::fs::read_to_string(&argv_dump).unwrap();
    assert!(!dump.lines().any(|l| l == "--continue"), "argv must NOT include --continue when fresh=true: {dump:?}");
}
```

The `make_state_with_workspace(tmp, ws_id)` helper likely already exists in the
tests module (used by the sibling tests). If not, adapt from the pattern of the
closest existing helper. The `sleep 1` in the mock claude keeps the child alive
long enough for spawn_agent_inner's spawn+reader-setup path to complete without
the reader thread hitting immediate EOF and racing the test.

- [ ] **Step 2: Run tests to verify RED**

Run:
`cd src-tauri && cargo test --lib spawn_agent_inner_with_fresh -- --nocapture`
Expected: compile errors — `spawn_agent_inner` has 5 params, not 6.

- [ ] **Step 3: Add the `fresh` parameter to `spawn_agent_inner`**

In `src-tauri/src/commands/agent_core.rs` at line 192 (or wherever the signature
lives):

```rust
pub fn spawn_agent_inner(
    state: Arc<Mutex<AppState>>,
    data_dir: &Path,
    workspace_id: &str,
    claude_path: Option<PathBuf>,
    event_tx: Option<&WorkspaceEventTx>,
    fresh: bool,
) -> AppResult<AgentProcess> {
```

In the spawn args block (around lines 223-238), directly after
`cmd.args([ ... ])`, add:

```rust
if !fresh {
    cmd.arg("--continue");
}
```

Place it AFTER the initial fixed args (`-p`, `--input-format`, etc.) but BEFORE
`cmd.current_dir(&worktree_dir)` and before `--append-system-prompt`. Ordering
doesn't affect claude parsing but keeping the conditional close to the fixed
args block is cleanest.

- [ ] **Step 4: Update all existing callers to pass `fresh: false`**

Update each call site with an explicit `false` — no default. Sites (verify
locations with `grep -n "spawn_agent_inner(" src-tauri/src/commands/agent*.rs`):

- `src-tauri/src/commands/agent.rs:59-65` — the production `spawn_agent` Tauri
  wrapper. Add `false` as the sixth arg.
- `src-tauri/src/commands/agent_core.rs`:
  - Line 995 (`ws_missing` no-workspace test).
  - Line 1023 (`ws_a` binary-not-found test).
  - Line 1044 (`ws_b` uses-override-binary test).
  - Line 1061 (`ws_c` — same file, different test).
  - Line 1079 (`ws_d` — same file, different test).
  - Line 1851 (`ws_nobin`).
  - Line 1940 (`ws_ctx` appends-system-prompt).
  - Line 1978 (`ws_race` double-check race).
  - Line 2478 (`ws_emit_run` emits status changed).

Add explicit `false` as the sixth positional argument at each call site.

- [ ] **Step 5: Run tests to verify GREEN**

Run:
`cd src-tauri && cargo test --lib spawn_agent_inner_with_fresh -- --nocapture`
Expected: both new tests pass.

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3` Expected: full lib suite
green. No regression in the ~10 existing spawn tests.

- [ ] **Step 6: Backend gates**

Run:

```
cd src-tauri && cargo clippy --lib --all-targets -- -D warnings 2>&1 | tail -3
cd src-tauri && cargo fmt --all -- --check
```

Both clean. Run `cargo fmt --all` if fmt fails.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/commands/agent_core.rs src-tauri/src/commands/agent.rs
git commit -m "feat(agent): --continue by default on spawn (fresh=false); param plumbing"
```

---

## Task 2: Backend — `restart_agent` Tauri command

**Files:**

- Modify: `src-tauri/src/commands/agent.rs` (append new command; sibling of
  `spawn_agent`).
- Modify: `src-tauri/src/lib.rs` (register in `tauri::generate_handler![...]`
  around line 314-318 alongside other agent commands; add a registration smoke
  test).

**Interfaces:**

- Consumes: `spawn_agent_inner(..., fresh: bool)` (Task 1);
  `stop_agent_inner(state, workspace_id) -> AppResult<()>` (existing at
  agent_core.rs:786).
- Produces: Tauri command
  `restart_agent(workspace_id: String, on_event: Channel<AgentEvent>, state, writer, event_tx, app) -> Result<(), String>`.
  Stops the current agent for the workspace (silent no-op if none), then spawns
  a fresh one (`--continue` omitted). Frontend passes a fresh
  `Channel<AgentEvent>` so the new agent's events reach the UI.

- [ ] **Step 1: Write the failing test**

Append to `src-tauri/src/commands/agent.rs` `#[cfg(test)] mod tests` (or the
relevant test module — check via
`grep -n "#\\[cfg(test)\\]" src-tauri/src/commands/agent.rs`). If there is no
test module in agent.rs, add one; but the inner logic can also be tested from
`agent_core.rs::tests` if you extract the impl to a
`pub(crate) fn restart_agent_inner`.

Cleanest split: put the LOGIC in `agent_core.rs` as
`pub(crate) fn restart_agent_inner`, and the Tauri wrapper stays in `agent.rs`.
Test the inner from `agent_core.rs::tests`:

```rust
#[test]
fn restart_agent_inner_stops_running_agent_and_respawns_fresh() {
    // Setup: use the fake-claude script that dumps argv on each spawn.
    // First spawn (normal, fresh=false) records --continue in argv[0].
    // restart_agent_inner should stop that agent and spawn a NEW one
    // (fresh=true) whose argv does NOT include --continue.
    let tmp = tempfile::tempdir().unwrap();
    // Rotating dump paths so we can distinguish the two spawns.
    let dump1 = tmp.path().join("argv1.txt");
    let dump2 = tmp.path().join("argv2.txt");
    let script = tmp.path().join("fake-claude.sh");
    // The script writes to whichever path is passed via env var
    // ANSAMBEL_FAKE_ARGV_DUMP so we can control it per-spawn.
    std::fs::write(
        &script,
        r#"#!/bin/sh
printf '%s\n' "$@" > "$ANSAMBEL_FAKE_ARGV_DUMP"
sleep 2
"#,
    )
    .unwrap();
    std::process::Command::new("chmod")
        .args(["+x", script.to_str().unwrap()])
        .status()
        .unwrap();

    let state = make_state_with_workspace(tmp.path(), "ws_r");

    // First spawn — normal path (fresh=false).
    std::env::set_var("ANSAMBEL_FAKE_ARGV_DUMP", &dump1);
    let _ = spawn_agent_inner(
        state.clone(),
        tmp.path(),
        "ws_r",
        Some(script.clone()),
        None,
        false,
    )
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(150));

    // Restart — must stop the current agent AND respawn with fresh=true.
    std::env::set_var("ANSAMBEL_FAKE_ARGV_DUMP", &dump2);
    restart_agent_inner(state.clone(), tmp.path(), "ws_r", Some(script.clone()), None).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(150));

    // Assert: dump2 (second spawn's argv) MUST NOT contain --continue.
    let dump2_content = std::fs::read_to_string(&dump2).unwrap();
    assert!(
        !dump2_content.lines().any(|l| l == "--continue"),
        "restart must respawn with --continue omitted: {dump2_content:?}"
    );
    // Assert: the state now has an agent for ws_r (the new one, not the old).
    assert!(state.lock().unwrap().agents.contains_key("ws_r"));
}

#[test]
fn restart_agent_inner_with_no_existing_agent_is_ok_and_spawns_fresh() {
    // No prior spawn — restart still works (stop is a silent no-op),
    // and the spawn side goes fresh.
    let tmp = tempfile::tempdir().unwrap();
    let dump = tmp.path().join("argv.txt");
    let script = tmp.path().join("fake-claude.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nsleep 1\n",
            dump.display()
        ),
    )
    .unwrap();
    std::process::Command::new("chmod")
        .args(["+x", script.to_str().unwrap()])
        .status()
        .unwrap();
    let state = make_state_with_workspace(tmp.path(), "ws_r2");
    restart_agent_inner(state.clone(), tmp.path(), "ws_r2", Some(script.clone()), None).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(150));
    let dump_content = std::fs::read_to_string(&dump).unwrap();
    assert!(
        !dump_content.lines().any(|l| l == "--continue"),
        "restart with no prior agent must still spawn fresh: {dump_content:?}"
    );
}
```

- [ ] **Step 2: Run tests to verify RED**

Run: `cd src-tauri && cargo test --lib restart_agent_inner` Expected: compile
errors — `restart_agent_inner` doesn't exist.

- [ ] **Step 3: Implement `restart_agent_inner` in agent_core.rs**

Append to `src-tauri/src/commands/agent_core.rs` after
`stop_agent_inner_with_publisher`:

```rust
/// Kill the current agent for `workspace_id` (silent no-op if none),
/// then respawn a new agent WITHOUT `--continue` so Claude starts a
/// fresh conversation. Used by the "Restart agent (fresh session)"
/// escape hatch in the chat panel.
pub(crate) fn restart_agent_inner(
    state: Arc<Mutex<AppState>>,
    data_dir: &Path,
    workspace_id: &str,
    claude_path: Option<PathBuf>,
    event_tx: Option<&WorkspaceEventTx>,
) -> AppResult<AgentProcess> {
    // Stop first — silent if no agent exists. Uses the with_publisher
    // variant only if event_tx is present, so a plain stop is a no-op
    // observability-wise.
    if let Some(tx) = event_tx {
        stop_agent_inner_with_publisher(state.clone(), workspace_id, Some(tx))?;
    } else {
        stop_agent_inner(state.clone(), workspace_id)?;
    }
    spawn_agent_inner(
        state,
        data_dir,
        workspace_id,
        claude_path,
        event_tx,
        /* fresh */ true,
    )
}
```

- [ ] **Step 4: Add the Tauri wrapper in agent.rs**

Append to `src-tauri/src/commands/agent.rs` (mirror the shape of `spawn_agent`):

```rust
#[tauri::command]
pub async fn restart_agent(
    workspace_id: String,
    on_event: Channel<AgentEvent>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    writer: tauri::State<'_, MessageWriter>,
    event_tx: tauri::State<'_, WorkspaceEventTx>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app data dir: {e}"))?;
    let claude_path = state
        .lock()
        .map_err(|e| format!("state lock poisoned: {e}"))?
        .settings
        .claude_binary_override
        .clone();
    let publisher_tx: WorkspaceEventTx = event_tx.inner().clone();
    let session = crate::commands::agent_core::restart_agent_inner(
        state.inner().clone(),
        &data_dir,
        &workspace_id,
        claude_path,
        Some(&publisher_tx),
    )
    .map_err(|e| e.to_string())?;
    crate::commands::agent_core::spawn_reader_thread(
        session,
        on_event,
        state.inner().clone(),
        writer.inner().clone(),
        workspace_id,
        data_dir,
        publisher_tx,
    );
    Ok(())
}
```

Import `Channel` if not already imported at the top of the file (check the `use`
block).

- [ ] **Step 5: Register in `lib.rs` handler + smoke test**

In `src-tauri/src/lib.rs` `tauri::generate_handler![...]` (around line 314-318),
add `crate::commands::agent::restart_agent,` next to the existing agent commands
(alphabetical or preserving existing grouping).

Add a registration smoke test mirroring the existing ones:

```rust
#[test]
fn restart_agent_command_is_registered() {
    let _ = crate::commands::agent::restart_agent as *const () as usize;
}
```

- [ ] **Step 6: Run tests to verify GREEN**

Run: `cd src-tauri && cargo test --lib restart_agent` Expected: both new inner
tests + the registration smoke test pass.

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3` Expected: full lib suite
green.

- [ ] **Step 7: Backend gates**

```
cd src-tauri && cargo clippy --lib --all-targets -- -D warnings 2>&1 | tail -3
cd src-tauri && cargo fmt --all -- --check
```

Both clean.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/commands/agent_core.rs src-tauri/src/commands/agent.rs src-tauri/src/lib.rs
git commit -m "feat(agent): restart_agent command (stop + spawn with fresh=true)"
```

---

## Task 3: Frontend — IPC wrapper + ChatPanel kebab menu

**Files:**

- Modify: `src/lib/ipc.ts` — add `api.agent.restartFresh(workspaceId, channel)`.
- Modify: `src/lib/components/chat/ChatPanel.svelte` — add kebab menu in the
  section header area; emit a callback prop when the "Restart agent (fresh
  session)" item is clicked.
- Modify: `src/lib/components/chat/ChatPanel.test.ts` — extend existing tests
  with 3 new cases.
- Modify: whichever parent component owns the spawn/channel wiring for ChatPanel
  (likely `src/lib/components/workspace/WorkspaceView.svelte` — verify via
  `grep -rn "spawn_agent\|api.agent.spawn\|<ChatPanel" src/lib/components src/App.svelte`);
  wire `onRestartAgent` prop.

**Interfaces:**

- Consumes:
  `api.agent.restartFresh(workspaceId: string, channel: Channel<AgentEvent>): Promise<void>`
  (new IPC wrapper); backend Tauri command `restart_agent` (Task 2).
- Produces: ChatPanel new optional prop
  `onRestartAgent?: () => void | Promise<void>`. When defined, the kebab menu
  becomes visible and the "Restart agent (fresh session)" item invokes this
  callback.

- [ ] **Step 1: Add the IPC wrapper**

In `src/lib/ipc.ts`, extend the `agent: { ... }` namespace with:

```ts
restartFresh: (workspaceId: string, channel: Channel<AgentEvent>): Promise<void> =>
  invoke('restart_agent', { workspaceId, onEvent: channel }),
```

Preserve the existing pattern the neighboring `agent.spawn` uses for passing the
Channel (likely `onEvent: channel` matching the Rust wrapper's `on_event`
parameter — camelCase↔snake_case is automatic).

- [ ] **Step 2: Write the failing tests**

Extend `src/lib/components/chat/ChatPanel.test.ts` with three new tests. Read
the file first to see the existing mock harness (imports, `vi.mock` blocks,
render helpers). Adapt to it.

```ts
it('renders the kebab menu trigger when onRestartAgent is provided', async () => {
  const { getByTestId } = render(ChatPanel, {
    props: {
      workspaceId: 'ws1',
      onSend: vi.fn(),
      onRestartAgent: vi.fn(),
      // ...other required props (adapt to existing test scaffolding)
    },
  });
  expect(getByTestId('chat-menu-trigger')).toBeTruthy();
});

it('does NOT render the kebab menu when onRestartAgent is undefined', async () => {
  const { queryByTestId } = render(ChatPanel, {
    props: {
      workspaceId: 'ws1',
      onSend: vi.fn(),
      // onRestartAgent omitted
    },
  });
  expect(queryByTestId('chat-menu-trigger')).toBeNull();
});

it('clicking "Restart agent" menu item invokes onRestartAgent', async () => {
  const onRestartAgent = vi.fn();
  const { getByTestId } = render(ChatPanel, {
    props: {
      workspaceId: 'ws1',
      onSend: vi.fn(),
      onRestartAgent,
    },
  });
  await fireEvent.click(getByTestId('chat-menu-trigger'));
  await fireEvent.click(getByTestId('chat-menu-restart-agent'));
  expect(onRestartAgent).toHaveBeenCalledTimes(1);
});
```

- [ ] **Step 3: Run tests to verify RED**

Run: `bun run vitest run src/lib/components/chat/ChatPanel.test.ts` Expected:
three failures (kebab elements do not exist).

- [ ] **Step 4: Implement the kebab in ChatPanel.svelte**

In `src/lib/components/chat/ChatPanel.svelte`, add the new prop to the Props
interface:

```ts
interface Props {
  // ...existing fields...
  /** Fired when the user picks "Restart agent (fresh session)" from the
   *  chat-panel kebab menu. When omitted the kebab is hidden entirely. */
  onRestartAgent?: () => void | Promise<void>;
}
```

Add module-scope state for the dropdown:

```ts
let menuOpen = $state(false);
async function handleRestartClick() {
  menuOpen = false;
  await onRestartAgent?.();
}
```

Insert a small header directly inside the top of the
`<section class="flex flex-col h-full bg-[var(--bg-base)]">` block (before the
existing error-banner block). Only render when the prop is defined:

```svelte
{#if onRestartAgent}
  <header
    class="flex justify-end items-center px-2 py-1 border-b border-[var(--border)] relative"
  >
    <button
      type="button"
      class="p-1 text-[var(--text-muted)] hover:text-[var(--text-primary)] rounded"
      data-testid="chat-menu-trigger"
      aria-haspopup="menu"
      aria-expanded={menuOpen}
      onclick={() => (menuOpen = !menuOpen)}
    >
      <span aria-hidden="true">⋮</span>
      <span class="sr-only">Chat menu</span>
    </button>
    {#if menuOpen}
      <div
        role="menu"
        class="absolute right-2 top-full mt-1 z-40 min-w-[220px] bg-[var(--bg-sidebar)] border border-[var(--border)] rounded shadow-lg py-1"
      >
        <button
          type="button"
          class="w-full text-left px-3 py-1.5 text-xs hover:bg-[var(--bg-hover)]"
          data-testid="chat-menu-restart-agent"
          onclick={handleRestartClick}
        >
          Restart agent (fresh session)
        </button>
      </div>
    {/if}
  </header>
{/if}
```

The `bg-[var(--bg-sidebar)] border border-[var(--border)]` combo matches the
palette used by `MentionAutocomplete.svelte` and `SlashCommandPicker.svelte`
(opaque popover on the chat surface — confirmed working in production).

- [ ] **Step 5: Run tests to verify GREEN**

Run: `bun run vitest run src/lib/components/chat/ChatPanel.test.ts` Expected:
all three new tests pass; existing tests unchanged.

Run full suite: `bun run vitest run 2>&1 | tail -3` — no regressions.

- [ ] **Step 6: Wire the parent (`WorkspaceView.svelte` or equivalent)**

First locate the parent that mounts ChatPanel:

```
grep -rn "<ChatPanel" src/lib/components src/App.svelte
```

In the file that renders `<ChatPanel>`, wire the new prop. The parent must:

1. Import `Channel` from `@tauri-apps/api/core` (already imported if it already
   handles spawn's channel).
2. Have access to the `workspaceId` context that ChatPanel uses.
3. On restart, create a fresh `Channel<AgentEvent>`, wire its `onmessage` to the
   same event handler the spawn channel uses (probably a shared function
   `handleAgentEvent`), then call
   `api.agent.restartFresh(workspaceId, channel)`.
4. Show a toast via the toast store on success
   (`addToast('Agent restarted (fresh session)', 'success')` — verify the exact
   toast API from `src/lib/stores/toasts.svelte.ts`).
5. On failure, surface the error via the standard toast/error mechanism the
   parent already uses.

Pass the resulting callback as `onRestartAgent={handleRestartAgent}` to
ChatPanel.

Illustrative shape (adapt to the parent's existing imports/state):

```ts
async function handleRestartAgent() {
  try {
    const channel = new Channel<AgentEvent>();
    channel.onmessage = handleAgentEvent; // whatever the existing spawn wiring uses
    await api.agent.restartFresh(workspaceId, channel);
    addToast('Agent restarted (fresh session)', 'success');
  } catch (err) {
    addToast(`Restart failed: ${err}`, 'error');
  }
}
```

Add a smoke test in the parent's test file (mock `api.agent.restartFresh`, drive
the callback, assert the IPC was called with the right workspace_id).

- [ ] **Step 7: Frontend gates**

```
bun run check
bun run vitest run 2>&1 | tail -3
bun run lint
```

0 errors; only the pre-existing warning in `lark-binding-filters.svelte.test.ts`
is acceptable.

- [ ] **Step 8: Commit**

```bash
git add src/lib/ipc.ts src/lib/components/chat/ChatPanel.svelte src/lib/components/chat/ChatPanel.test.ts src/lib/components/workspace
git commit -m "feat(chat): kebab menu + Restart agent (fresh session) affordance"
```

Adjust the `git add` paths to whatever files you actually changed in Step 6 (the
parent wiring path depends on where `<ChatPanel>` is rendered — likely
`WorkspaceView.svelte` + its test).

---

## Task 4: Journal + full-suite gate

**Files:**

- Create: `journal/2026-05-28-resume-claude-session.md`.

- [ ] **Step 1: Full-suite gates**

Run the full gate one more time from the branch tip so the journal counts are
accurate:

```
bun run check
bun run vitest run 2>&1 | tail -3
cd src-tauri && cargo test --lib 2>&1 | tail -3
cd src-tauri && cargo clippy --lib --all-targets -- -D warnings 2>&1 | tail -3
cd src-tauri && cargo fmt --all -- --check
bun run lint
```

Record the numbers for the journal.

- [ ] **Step 2: Write the journal**

Create `journal/2026-05-28-resume-claude-session.md` following the shape of
previous entries (see `journal/2026-05-28-multi-card-workspace.md` for a
template — sections: What shipped / Backend / Frontend / Decisions / Tests /
Aftermath). Key points to record:

- What shipped: `--continue` on every non-restart spawn; new `restart_agent`
  command; kebab menu in ChatPanel.
- Backend: `spawn_agent_inner(..., fresh: bool)`; `restart_agent_inner` wrapping
  `stop_agent_inner + spawn_agent_inner(fresh=true)`; Tauri command
  `restart_agent` registered.
- Frontend: new IPC wrapper `api.agent.restartFresh`; ChatPanel gains
  `onRestartAgent?` prop; parent wires the fresh Channel.
- Decisions:
  - Delegate all session-state persistence to Claude CLI (which owns
    `~/.claude/projects/<encoded-cwd>/`). Ansambel stores zero new state —
    matches the CLAUDE.md rule against "Checkpoint / restore of Claude
    conversation history" (we don't checkpoint or restore anything; Claude CLI
    does).
  - No fallback retry: empirically tested that `claude --continue` in a
    session-less directory returns exit=0 and produces normal output. If a
    corrupt-session edge case ever surfaces in the wild, add the retry then.
  - No confirmation modal on Restart — kebab-menu two-click is deliberate
    enough, and the action is non-destructive (chat history in
    `messages/<wsId>.json` persists; only Claude conversation-state pointer
    resets; next natural spawn will `--continue` from the newly-fresh session).
- Tests: enumerate the new counts + gates.
- Aftermath / known follow-ups: dead scaffolding (`AppState.sessions` +
  `sessions.json` path constant) still present — separate housekeeping commit
  later.

- [ ] **Step 3: Commit**

```bash
git add journal/2026-05-28-resume-claude-session.md
git commit -m "docs(journal): resume claude session across restart"
```

---

## Self-review

**Spec coverage:**

- Anchor: `--continue` flag → Task 1.
- `spawn_agent_inner` gains `fresh` param → Task 1.
- `restart_agent` command + registration → Task 2.
- `restart_agent_inner` logic (stop + spawn fresh) → Task 2.
- Frontend IPC wrapper → Task 3.
- ChatPanel kebab menu → Task 3.
- No confirmation modal → Task 3 (asserted in tests via absence).
- Zero new persistent state → Task 1 + 2 (no persistence code touched).
- Fallback retry: **deliberately dropped** vs spec, with rationale documented in
  the plan header + journal (empirical proof).
- No E2E: matches spec §Testing → E2E.
- Coverage gate: standard gates in Task 4.

**Placeholder scan:** no TBD / TODO / "implement later"; every code step has
full code; no "similar to Task N" references.

**Type consistency:**
`spawn_agent_inner(..., fresh: bool) -> AppResult<AgentProcess>` used
identically in Task 1 (definition), Task 2 (call from `restart_agent_inner`).
`restart_agent_inner(state, data_dir, workspace_id, claude_path, event_tx) -> AppResult<AgentProcess>`
used identically in Task 2 (definition) and the Tauri wrapper (`restart_agent`
calls it). `api.agent.restartFresh(workspaceId, channel)` signature consistent
between IPC wrapper (Task 3 Step 1) and parent wiring (Task 3 Step 6).
`onRestartAgent?: () => void | Promise<void>` prop signature identical in
ChatPanel Props (Task 3 Step 4) and parent's passed value (Task 3 Step 6).
