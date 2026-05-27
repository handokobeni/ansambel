# Terminal Multi-Tab Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a workspace's Terminal tab hold multiple long-lived terminal
sessions (tabs), like the Editor holds multiple file tabs, so a user can run
e.g. a frontend and a backend dev server side by side.

**Architecture:** Re-key the backend terminal registry from one-per-workspace to
one-per-`terminal_id` (the handle still records its `workspace_id` for bulk
cleanup). The frontend gains a per-workspace in-memory `terminal-tabs` store + a
`TerminalTabBar`, and the current single-xterm `Terminal.svelte` is split into a
`TerminalPane` (one per terminal id) hosted by a `Terminal` container that keeps
every pane mounted (`display` toggled) so processes/scrollback survive
navigation. Session-only — nothing persists across an app restart.

**Tech Stack:** Rust (Tauri commands, `portable-pty`,
`tokio::broadcast`/`mpsc`), Svelte 5 runes, xterm.js, `bun` + `cargo`.

**Spec:** `docs/superpowers/specs/2026-05-27-terminal-multitab-design.md`

---

## File Structure

- `src-tauri/src/state.rs` — `AppState.terminals` stays
  `HashMap<String, TerminalHandle>` but the **key becomes `terminal_id`** (was
  `workspace_id`); `TerminalHandle.workspace_id` already exists and is now used
  for bulk cleanup.
- `src-tauri/src/commands/terminal.rs` — inner fns re-keyed by `terminal_id`;
  new `kill_workspace_terminals_inner`; the 5 Tauri command wrappers gain a
  `terminalId` arg.
- `src-tauri/src/commands/workspace.rs` — `remove_workspace_inner` kills all of
  the workspace's terminals.
- `src/lib/ipc.ts` — `terminal.*` wrappers gain `terminalId`.
- `src/lib/stores/terminal-tabs.svelte.ts` — NEW per-workspace tab store
  (mirrors `editor-tabs.svelte.ts`).
- `src/lib/components/workspace/TerminalTabBar.svelte` — NEW (mirrors
  `EditorTabBar.svelte`).
- `src/lib/components/workspace/TerminalPane.svelte` — NEW; the current
  `Terminal.svelte` body, keyed by `terminalId`.
- `src/lib/components/workspace/Terminal.svelte` — becomes the container (tab
  bar + panes).
- `tests/e2e/phase-3c-terminal-multitab/terminal-tabs.spec.ts` — NEW E2E.
- `journal/2026-05-27-terminal-multitab.md` — NEW journal.

Patterns to follow (read first):

- `commands/terminal.rs` tests module: `make_state(workspace_id, worktree)`,
  `make_worktree()`, and a `spawn_terminal` helper using `MockPty::new(..)` +
  `spawn_terminal_inner_with_pty`. Mirror these.
- `stores/editor-tabs.svelte.ts` (per-workspace `SvelteMap` state, `ensure`,
  add/close/setActive/forget/reset) — the store shape to mirror.
- `components/workspace/EditorTabBar.svelte` — the tab-bar markup/roles to
  mirror.
- `WorkspaceView.svelte` renders `<Terminal workspaceId={workspace.id} />`
  inside a `class:hidden={activeTab !== 'terminal'}` panel — display-toggled,
  never unmounted. Per-terminal panes must follow the same never-unmount
  discipline.

---

### Task 1: Re-key the backend terminal registry by `terminal_id`

**Files:**

- Modify: `src-tauri/src/commands/terminal.rs` (inner fns + tests)
- Reference: `src-tauri/src/state.rs` (`AppState.terminals`, `TerminalHandle` —
  no struct change; only the map's key meaning changes)

- [ ] **Step 1: Write the failing tests**

In the `#[cfg(test)] mod tests` of `terminal.rs`, mirror the existing
`spawn_terminal` helper but spawn by `terminal_id`. Add a helper and tests
(adapt the existing `make_state`/`make_worktree`/MockPty helpers — read them
first):

```rust
/// Spawn a MockPty-backed terminal under (workspace_id, terminal_id).
fn spawn_term(
    workspace_id: &str,
    terminal_id: &str,
    state: &Arc<Mutex<AppState>>,
) -> crate::platform::pty::MockPtyHandle {
    let (mock, handle) = crate::platform::pty::MockPty::new(1234);
    spawn_terminal_inner_with_pty(workspace_id, terminal_id, Box::new(mock), 80, 24, Arc::clone(state))
        .expect("spawn");
    handle
}

#[test]
fn two_terminals_per_workspace_are_independent() {
    let (_tmp, wt) = make_worktree();
    let state = make_state("ws_a", &wt);
    let _h1 = spawn_term("ws_a", "term_1", &state);
    let _h2 = spawn_term("ws_a", "term_2", &state);
    let st = state.lock().unwrap();
    assert!(st.terminals.contains_key("term_1"));
    assert!(st.terminals.contains_key("term_2"));
    assert_eq!(st.terminals.len(), 2, "two distinct terminals coexist");
}

#[test]
fn write_and_kill_target_only_the_given_terminal() {
    let (_tmp, wt) = make_worktree();
    let state = make_state("ws_a", &wt);
    let _h1 = spawn_term("ws_a", "term_1", &state);
    let _h2 = spawn_term("ws_a", "term_2", &state);
    // Kill only term_1.
    kill_terminal_inner("term_1", Arc::clone(&state)).unwrap();
    let st = state.lock().unwrap();
    assert!(!st.terminals.contains_key("term_1"));
    assert!(st.terminals.contains_key("term_2"), "term_2 survives");
}

#[test]
fn spawn_duplicate_terminal_id_errors() {
    let (_tmp, wt) = make_worktree();
    let state = make_state("ws_a", &wt);
    let _h1 = spawn_term("ws_a", "term_1", &state);
    let (mock2, _h2) = crate::platform::pty::MockPty::new(2);
    let err = spawn_terminal_inner_with_pty("ws_a", "term_1", Box::new(mock2), 80, 24, Arc::clone(&state));
    assert!(err.is_err(), "duplicate terminal_id rejected");
}

#[test]
fn kill_workspace_terminals_removes_all_for_that_workspace_only() {
    let (_tmp, wt) = make_worktree();
    let state = make_state("ws_a", &wt);
    // ws_a has a second workspace's worktree too — seed ws_b.
    {
        let mut st = state.lock().unwrap();
        let ws_b = st.workspaces.get("ws_a").unwrap().clone();
        let mut ws_b = ws_b;
        ws_b.id = "ws_b".into();
        st.workspaces.insert("ws_b".into(), ws_b);
    }
    let _a1 = spawn_term("ws_a", "term_a1", &state);
    let _a2 = spawn_term("ws_a", "term_a2", &state);
    let _b1 = spawn_term("ws_b", "term_b1", &state);
    kill_workspace_terminals_inner("ws_a", Arc::clone(&state)).unwrap();
    let st = state.lock().unwrap();
    assert!(!st.terminals.contains_key("term_a1"));
    assert!(!st.terminals.contains_key("term_a2"));
    assert!(st.terminals.contains_key("term_b1"), "other workspace untouched");
}
```

Existing tests in this module call
`spawn_terminal_inner_with_pty(workspace_id, pty, ...)` and
`write_terminal_inner(workspace_id, ...)` etc. — they will need their call sites
updated in Step 3 to pass a `terminal_id`. Update those existing tests to use a
`terminal_id` equal to the workspace_id (or any string) so their intent is
preserved.

- [ ] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test --lib terminal` Expected: compile errors —
`spawn_terminal_inner_with_pty` / `kill_terminal_inner` signatures don't take
`terminal_id`; `kill_workspace_terminals_inner` doesn't exist.

- [ ] **Step 3: Re-key the inner functions**

In `src-tauri/src/commands/terminal.rs`, change the registry key from
`workspace_id` to `terminal_id`. Concretely:

- `spawn_terminal_inner(workspace_id: &str, terminal_id: &str, cols, rows, state)`
  — resolve `worktree_dir` from `workspace_id` (unchanged), but the
  duplicate-check and insert key on `terminal_id`:
  ```rust
  if st.terminals.contains_key(terminal_id) {
      return Err(AppError::InvalidState(format!(
          "terminal '{terminal_id}' already active — call reattach instead"
      )));
  }
  ```
  Pass `terminal_id` through to `spawn_terminal_inner_with_pty`.
- `spawn_terminal_inner_with_pty(workspace_id: &str, terminal_id: &str, pty, cols, rows, state)`
  — same duplicate-check on `terminal_id`; build the handle with
  `workspace_id: workspace_id.into()` (kept for bulk cleanup) and
  `insert(terminal_id.into(), handle)`.
- `write_terminal_inner(terminal_id: &str, bytes, state)` —
  `st.terminals.get(terminal_id)`.
- `resize_terminal_inner(terminal_id: &str, cols, rows, state)` —
  `st.terminals.get(terminal_id)`.
- `kill_terminal_inner(terminal_id: &str, state)` —
  `st.terminals.remove(terminal_id)`.
- `reattach_terminal_inner(terminal_id: &str, state)` —
  `st.terminals.get(terminal_id)`.
- Add:
  ```rust
  /// Kill every terminal belonging to a workspace. Used when the
  /// workspace is removed. Idempotent.
  pub fn kill_workspace_terminals_inner(
      workspace_id: &str,
      state: Arc<Mutex<AppState>>,
  ) -> Result<()> {
      let mut st = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
      let ids: Vec<String> = st
          .terminals
          .iter()
          .filter(|(_, h)| h.workspace_id == workspace_id)
          .map(|(id, _)| id.clone())
          .collect();
      for id in ids {
          if let Some(handle) = st.terminals.remove(&id) {
              handle.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
              if let Ok(mut pty) = handle.pty.lock() {
                  let _ = pty.kill();
                  pty.close_master();
              }
          }
      }
      Ok(())
  }
  ```

Update the existing tests' call sites to pass a `terminal_id`.

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib terminal` Expected: all terminal tests
pass (existing, updated + 4 new).

Run:
`cd src-tauri && cargo clippy --lib --all-targets -- -D warnings 2>&1 | tail -3`
Expected: clean (the new command-wrapper signatures change in Task 2; if clippy
flags an unused `kill_workspace_terminals_inner` here, add `#[allow(dead_code)]`
and REMOVE it in Task 2 when `remove_workspace_inner` calls it).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/terminal.rs
git commit -m "feat(terminal-multitab): re-key terminal registry by terminal_id + kill_workspace_terminals_inner"
```

---

### Task 2: Tauri command wrappers + IPC wrappers + kill-all on workspace removal

**Files:**

- Modify: `src-tauri/src/commands/terminal.rs` (the 5 `#[tauri::command]`
  wrappers)
- Modify: `src-tauri/src/commands/workspace.rs` (`remove_workspace_inner`)
- Modify: `src/lib/ipc.ts` (`terminal.*`)
- Test: `src-tauri/src/commands/workspace.rs` (kill-all wiring)

- [ ] **Step 1: Write the failing test (kill-all on workspace removal)**

Add to `workspace.rs` tests (mirror the existing `remove_workspace_inner` /
`create_workspace_inner` harness — `init_repo_with_remote` + `add_repo_inner`):

```rust
#[tokio::test]
async fn remove_workspace_kills_its_terminals() {
    let tmp = tempfile::tempdir().unwrap();
    let (local, _) = init_repo_with_remote(&tmp);
    let data = tmp.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    let state: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState::default()));
    let repo = crate::commands::repo::add_repo_inner(
        local.to_str().unwrap().to_string(), data.clone(), Arc::clone(&state),
    ).await.unwrap();
    let ws = create_workspace_inner(
        repo.id, "T".into(), String::new(), None, data.clone(), Arc::clone(&state),
    ).await.unwrap();
    // Spawn a MockPty terminal under this workspace.
    let (mock, _h) = crate::platform::pty::MockPty::new(7);
    crate::commands::terminal::spawn_terminal_inner_with_pty(
        &ws.id, "term_x", Box::new(mock), 80, 24, Arc::clone(&state),
    ).unwrap();
    assert_eq!(state.lock().unwrap().terminals.len(), 1);
    remove_workspace_inner(ws.id.clone(), data.clone(), Arc::clone(&state)).await.unwrap();
    assert_eq!(state.lock().unwrap().terminals.len(), 0, "workspace removal killed its terminal");
}
```

(`remove_workspace_inner` was made `pub(crate)` in the workspace-lifecycle work;
if this branch is off a `main` that predates that, it is already `pub(crate)`
after that merge — confirm.)

- [ ] **Step 2: Run to verify it fails**

Run: `cd src-tauri && cargo test --lib remove_workspace_kills_its_terminals`
Expected: FAIL — terminal still present after removal.

- [ ] **Step 3: Update the command wrappers + ipc + wire kill-all**

In `terminal.rs`, add `terminal_id: String` to each wrapper and thread it
through:

```rust
#[tauri::command]
pub async fn terminal_spawn(
    workspace_id: String,
    terminal_id: String,
    cols: Option<u16>,
    rows: Option<u16>,
    channel: Channel<TerminalChunk>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    let cols = cols.unwrap_or(DEFAULT_COLS);
    let rows = rows.unwrap_or(DEFAULT_ROWS);
    let rx = spawn_terminal_inner(&workspace_id, &terminal_id, cols, rows, state.inner().clone())
        .map_err(|e| { tracing::error!(error = %e, "terminal_spawn failed"); e.to_string() })?;
    forward_to_channel(rx, channel);
    Ok(())
}

#[tauri::command]
pub async fn terminal_write(
    terminal_id: String,
    bytes: Vec<u8>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    write_terminal_inner(&terminal_id, bytes, state.inner().clone()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_resize(
    terminal_id: String,
    cols: u16,
    rows: u16,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    resize_terminal_inner(&terminal_id, cols, rows, state.inner().clone()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_kill(
    terminal_id: String,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    kill_terminal_inner(&terminal_id, state.inner().clone()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_reattach(
    terminal_id: String,
    channel: Channel<TerminalChunk>,
    state: State<'_, Arc<Mutex<AppState>>>,
) -> std::result::Result<(), String> {
    let rx = reattach_terminal_inner(&terminal_id, state.inner().clone()).map_err(|e| e.to_string())?;
    forward_to_channel(rx, channel);
    Ok(())
}
```

In `commands/workspace.rs::remove_workspace_inner`, before (or after) removing
the workspace entry, kill its terminals. Add this near the final state mutation:

```rust
    // Kill any terminals the workspace owned so their PTYs don't leak.
    let _ = crate::commands::terminal::kill_workspace_terminals_inner(&ws_id, Arc::clone(&state));
```

(place it before the lock that removes the workspace, or call it with its own
lock — it takes `Arc<Mutex<AppState>>` and locks internally, so call it OUTSIDE
any held lock to avoid double-lock/deadlock). Remove the `#[allow(dead_code)]`
from `kill_workspace_terminals_inner` if Task 1 added one.

In `src/lib/ipc.ts`, update the `terminal` namespace to thread `terminalId`:

```typescript
  terminal: {
    spawn: (
      workspaceId: string,
      terminalId: string,
      channel: Channel<TerminalChunk>,
      cols?: number,
      rows?: number
    ): Promise<void> =>
      invoke('terminal_spawn', { workspaceId, terminalId, channel, cols, rows }),
    write: (terminalId: string, bytes: number[]): Promise<void> =>
      invoke('terminal_write', { terminalId, bytes }),
    resize: (terminalId: string, cols: number, rows: number): Promise<void> =>
      invoke('terminal_resize', { terminalId, cols, rows }),
    kill: (terminalId: string): Promise<void> => invoke('terminal_kill', { terminalId }),
    reattach: (terminalId: string, channel: Channel<TerminalChunk>): Promise<void> =>
      invoke('terminal_reattach', { terminalId, channel }),
  },
```

Match the exact existing wrapper style/JSDoc in `ipc.ts`. Tauri maps
`terminalId` (camelCase) → `terminal_id` (snake) automatically.

- [ ] **Step 4: Run tests + checks**

Run: `cd src-tauri && cargo test --lib remove_workspace_kills_its_terminals` →
pass. Run: `cd src-tauri && cargo test --lib 2>&1 | tail -3` → all pass. Run:
`cd src-tauri && cargo clippy --lib --all-targets -- -D warnings 2>&1 | tail -3`
→ clean. Run: `bun run check` → `0 ERRORS 0 WARNINGS` (the ipc.ts callers in
`Terminal.svelte` will still type-check only after Task 5; if `bun run check`
errors because `Terminal.svelte` calls the old
`api.terminal.write(workspaceId, ...)` shape, that is EXPECTED to be fixed in
Task 5. To keep this task green, update the two `Terminal.svelte` call sites
minimally to pass a placeholder `workspaceId` as the terminalId for now, OR
sequence Task 5 immediately after — see note).

> Sequencing note: Tasks 2 and 5 both touch the IPC contract↔`Terminal.svelte`.
> To avoid a transient broken `bun run check`, the implementer may keep
> `Terminal.svelte` compiling by passing `workspaceId` as the `terminalId`
> argument temporarily (single-terminal behaviour preserved), then Task 5
> replaces the component entirely. State clearly in the commit if you do this.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/terminal.rs src-tauri/src/commands/workspace.rs src/lib/ipc.ts
git commit -m "feat(terminal-multitab): terminalId on terminal IPC + kill workspace terminals on removal"
```

---

### Task 3: `terminal-tabs.svelte.ts` store

**Files:**

- Create: `src/lib/stores/terminal-tabs.svelte.ts`
- Test: `src/lib/stores/terminal-tabs.svelte.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `src/lib/stores/terminal-tabs.svelte.test.ts`:

```typescript
import { describe, it, expect, beforeEach } from 'vitest';
import { terminalTabs } from './terminal-tabs.svelte';

describe('terminalTabs', () => {
  beforeEach(() => terminalTabs.reset());

  it('add returns a unique id, appends, and activates it', () => {
    const a = terminalTabs.add('ws');
    const b = terminalTabs.add('ws');
    expect(a).not.toBe(b);
    expect(terminalTabs.list('ws').map((t) => t.id)).toEqual([a, b]);
    expect(terminalTabs.activeId('ws')).toBe(b);
  });

  it('labels are monotonic "Terminal N" and not reused after close', () => {
    terminalTabs.add('ws'); // Terminal 1
    const t2 = terminalTabs.add('ws'); // Terminal 2
    expect(terminalTabs.list('ws').map((t) => t.label)).toEqual([
      'Terminal 1',
      'Terminal 2',
    ]);
    terminalTabs.close('ws', t2);
    const t3 = terminalTabs.add('ws');
    expect(terminalTabs.list('ws').find((t) => t.id === t3)?.label).toBe(
      'Terminal 3'
    );
  });

  it('caps at 6 terminals per workspace', () => {
    const ids = Array.from({ length: 6 }, () => terminalTabs.add('ws'));
    expect(ids.every((id) => id !== null)).toBe(true);
    expect(terminalTabs.add('ws')).toBeNull();
    expect(terminalTabs.list('ws')).toHaveLength(6);
  });

  it('close activates a neighbour; empties to null when last closed', () => {
    const a = terminalTabs.add('ws');
    const b = terminalTabs.add('ws');
    terminalTabs.setActive('ws', a);
    terminalTabs.close('ws', a);
    expect(terminalTabs.activeId('ws')).toBe(b);
    terminalTabs.close('ws', b);
    expect(terminalTabs.activeId('ws')).toBeNull();
    expect(terminalTabs.list('ws')).toHaveLength(0);
  });

  it('isolates workspaces', () => {
    const a = terminalTabs.add('ws1');
    terminalTabs.add('ws2');
    expect(terminalTabs.list('ws1').map((t) => t.id)).toEqual([a]);
    expect(terminalTabs.list('ws2')).toHaveLength(1);
  });

  it('forget drops a workspace', () => {
    terminalTabs.add('ws');
    terminalTabs.forget('ws');
    expect(terminalTabs.list('ws')).toHaveLength(0);
    expect(terminalTabs.activeId('ws')).toBeNull();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `bun run vitest run src/lib/stores/terminal-tabs.svelte.test.ts` Expected:
`Cannot find module './terminal-tabs.svelte'`.

- [ ] **Step 3: Implement the store**

Create `src/lib/stores/terminal-tabs.svelte.ts` (mirrors
`editor-tabs.svelte.ts`):

```typescript
import { SvelteMap } from 'svelte/reactivity';

/** One terminal session tab. `id` is the stable backend terminal_id; the
 *  PTY for it lives in AppState until killed. */
export interface TerminalTab {
  id: string;
  label: string;
}

interface WorkspaceTerminals {
  tabs: TerminalTab[];
  active: string | null;
  /** Monotonic counter for "Terminal N" labels; never reused. */
  counter: number;
}

const MAX_TERMINALS = 6;

const states = new SvelteMap<string, WorkspaceTerminals>();

function ensure(workspaceId: string): WorkspaceTerminals {
  let s = states.get(workspaceId);
  if (!s) {
    s = { tabs: [], active: null, counter: 0 };
    states.set(workspaceId, s);
  }
  return s;
}

function newId(): string {
  return `term_${Math.random().toString(36).slice(2, 10)}${Date.now().toString(36)}`;
}

export const terminalTabs = {
  list(workspaceId: string): TerminalTab[] {
    return states.get(workspaceId)?.tabs ?? [];
  },

  activeId(workspaceId: string): string | null {
    return states.get(workspaceId)?.active ?? null;
  },

  /** Add a terminal tab. Returns its id, or `null` when at the cap. */
  add(workspaceId: string): string | null {
    const s = ensure(workspaceId);
    if (s.tabs.length >= MAX_TERMINALS) return null;
    const counter = s.counter + 1;
    const tab: TerminalTab = { id: newId(), label: `Terminal ${counter}` };
    states.set(workspaceId, {
      tabs: [...s.tabs, tab],
      active: tab.id,
      counter,
    });
    return tab.id;
  },

  setActive(workspaceId: string, id: string): void {
    const s = ensure(workspaceId);
    if (!s.tabs.some((t) => t.id === id) || s.active === id) return;
    states.set(workspaceId, { ...s, active: id });
  },

  /** Close a tab. Activates a neighbour; active becomes null when the
   *  last tab closes. Caller is responsible for killing the PTY. */
  close(workspaceId: string, id: string): void {
    const s = ensure(workspaceId);
    const idx = s.tabs.findIndex((t) => t.id === id);
    if (idx === -1) return;
    const tabs = s.tabs.filter((t) => t.id !== id);
    let active = s.active;
    if (active === id) {
      active = tabs[idx]?.id ?? tabs[idx - 1]?.id ?? null;
    }
    states.set(workspaceId, { ...s, tabs, active });
  },

  /** Drop all terminal-tab state for a workspace (on workspace remove). */
  forget(workspaceId: string): void {
    states.delete(workspaceId);
  },

  reset(): void {
    states.clear();
  },
};
```

- [ ] **Step 4: Run tests + check**

Run: `bun run vitest run src/lib/stores/terminal-tabs.svelte.test.ts` → 6
passed. Run: `bun run check` → `0 ERRORS 0 WARNINGS`.

- [ ] **Step 5: Commit**

```bash
git add src/lib/stores/terminal-tabs.svelte.ts src/lib/stores/terminal-tabs.svelte.test.ts
git commit -m "feat(terminal-multitab): per-workspace terminal-tabs store"
```

---

### Task 4: `TerminalTabBar.svelte`

**Files:**

- Create: `src/lib/components/workspace/TerminalTabBar.svelte`
- Test: `src/lib/components/workspace/TerminalTabBar.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `src/lib/components/workspace/TerminalTabBar.test.ts`:

```typescript
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import TerminalTabBar from './TerminalTabBar.svelte';
import { terminalTabs } from '$lib/stores/terminal-tabs.svelte';

describe('TerminalTabBar', () => {
  beforeEach(() => terminalTabs.reset());

  it('renders a tab per terminal and marks the active one', () => {
    terminalTabs.add('ws');
    const b = terminalTabs.add('ws');
    const { getAllByTestId, getByText } = render(TerminalTabBar, {
      props: { workspaceId: 'ws' },
    });
    expect(getAllByTestId('terminal-tab')).toHaveLength(2);
    expect(getByText('Terminal 1')).toBeTruthy();
    const tabs = getAllByTestId('terminal-tab');
    expect(
      tabs
        .find((t) => t.getAttribute('data-id') === b)
        ?.getAttribute('aria-selected')
    ).toBe('true');
  });

  it('clicking a tab activates it', async () => {
    const a = terminalTabs.add('ws');
    terminalTabs.add('ws');
    const { getAllByTestId } = render(TerminalTabBar, {
      props: { workspaceId: 'ws' },
    });
    const tabA = getAllByTestId('terminal-tab').find(
      (t) => t.getAttribute('data-id') === a
    )!;
    await fireEvent.click(tabA);
    expect(terminalTabs.activeId('ws')).toBe(a);
  });

  it('the + button calls onAdd', async () => {
    terminalTabs.add('ws');
    const onAdd = vi.fn();
    const { getByLabelText } = render(TerminalTabBar, {
      props: { workspaceId: 'ws', onAdd, onClose: vi.fn() },
    });
    await fireEvent.click(getByLabelText(/new terminal/i));
    expect(onAdd).toHaveBeenCalled();
  });

  it('+ is disabled at the cap of 6', () => {
    for (let i = 0; i < 6; i++) terminalTabs.add('ws');
    const { getByLabelText } = render(TerminalTabBar, {
      props: { workspaceId: 'ws', onAdd: vi.fn(), onClose: vi.fn() },
    });
    expect(
      (getByLabelText(/new terminal/i) as HTMLButtonElement).disabled
    ).toBe(true);
  });

  it('the × button calls onClose with the tab id', async () => {
    const a = terminalTabs.add('ws');
    const onClose = vi.fn();
    const { getAllByTestId } = render(TerminalTabBar, {
      props: { workspaceId: 'ws', onAdd: vi.fn(), onClose },
    });
    const closeBtn = getAllByTestId('terminal-tab-close').find(
      (b) => b.getAttribute('data-id') === a
    )!;
    await fireEvent.click(closeBtn);
    expect(onClose).toHaveBeenCalledWith(a);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `bun run vitest run src/lib/components/workspace/TerminalTabBar.test.ts`
Expected: `Cannot find module './TerminalTabBar.svelte'`.

- [ ] **Step 3: Implement the component**

Create `src/lib/components/workspace/TerminalTabBar.svelte` (mirror
`EditorTabBar.svelte`; the parent owns spawn/kill so the bar emits
`onAdd`/`onClose` callbacks):

```svelte
<script lang="ts">
  import { terminalTabs } from '$lib/stores/terminal-tabs.svelte';

  interface Props {
    workspaceId: string;
    onAdd?: () => void;
    onClose?: (id: string) => void;
  }
  const { workspaceId, onAdd, onClose }: Props = $props();

  const MAX = 6;
  const tabs = $derived(terminalTabs.list(workspaceId));
  const active = $derived(terminalTabs.activeId(workspaceId));

  function close(e: MouseEvent, id: string): void {
    e.stopPropagation();
    onClose?.(id);
  }
</script>

<div
  role="tablist"
  aria-label="Terminals"
  class="flex items-stretch gap-px border-b border-[var(--border)] bg-[var(--bg-sidebar)] text-xs overflow-x-auto"
  data-testid="terminal-tab-bar"
>
  {#each tabs as tab (tab.id)}
    <button
      type="button"
      role="tab"
      aria-selected={active === tab.id}
      data-testid="terminal-tab"
      data-id={tab.id}
      onclick={() => terminalTabs.setActive(workspaceId, tab.id)}
      class="px-3 py-1.5 flex items-center gap-2 border-b-2 {active === tab.id
        ? 'border-b-[var(--accent)] text-[var(--text-primary)]'
        : 'border-transparent text-[var(--text-secondary)] hover:bg-[var(--bg-card)]'}"
    >
      <span class="truncate">{tab.label}</span>
      <span
        role="button"
        tabindex="0"
        aria-label="Close {tab.label}"
        data-testid="terminal-tab-close"
        data-id={tab.id}
        onclick={(e) => close(e, tab.id)}
        onkeydown={(e) => {
          if (e.key === 'Enter' || e.key === ' ')
            close(e as unknown as MouseEvent, tab.id);
        }}
        class="ml-1 px-1 text-[var(--text-muted)] hover:text-[var(--text-primary)] cursor-pointer"
      >
        ✕
      </span>
    </button>
  {/each}
  <button
    type="button"
    aria-label="New terminal"
    data-testid="terminal-tab-add"
    disabled={tabs.length >= MAX}
    onclick={() => onAdd?.()}
    class="px-3 py-1.5 text-[var(--text-muted)] hover:text-[var(--text-primary)] disabled:opacity-40 disabled:cursor-not-allowed"
  >
    +
  </button>
</div>
```

- [ ] **Step 4: Run tests + check**

Run: `bun run vitest run src/lib/components/workspace/TerminalTabBar.test.ts` →
5 passed. Run: `bun run check` → clean.

- [ ] **Step 5: Commit**

```bash
git add src/lib/components/workspace/TerminalTabBar.svelte src/lib/components/workspace/TerminalTabBar.test.ts
git commit -m "feat(terminal-multitab): TerminalTabBar component"
```

---

### Task 5: Extract `TerminalPane` + turn `Terminal.svelte` into the container

**Files:**

- Create: `src/lib/components/workspace/TerminalPane.svelte` (the current
  `Terminal.svelte` body, keyed by `terminalId`)
- Modify: `src/lib/components/workspace/Terminal.svelte` (becomes the container)
- Reference: `src/lib/components/workspace/WorkspaceView.svelte` (no change
  needed — it still renders `<Terminal workspaceId={...} />`)

This task is integration-heavy (xterm.js + jsdom) and is verified by
`bun run check` + the Task 6 E2E + manual smoke, not new vitest unit tests.

- [ ] **Step 1: Create `TerminalPane.svelte`**

Copy the **entire current `Terminal.svelte`** into a new `TerminalPane.svelte`,
then make these changes:

- Props become `{ workspaceId: string; terminalId: string }`.
- Every `api.terminal.write(workspaceId, bytes)` →
  `api.terminal.write(terminalId, bytes)`.
- `api.terminal.resize(workspaceId, ...)` →
  `api.terminal.resize(terminalId, ...)`.
- The reattach/spawn pair becomes:
  ```typescript
  try {
    await api.terminal.reattach(terminalId, makeChannel());
  } catch {
    try {
      await api.terminal.spawn(
        workspaceId,
        terminalId,
        makeChannel(),
        term.cols,
        term.rows
      );
    } catch (err) {
      if (term) {
        term.writeln(`\r\n[failed to start shell: ${String(err)}]`);
        exited = true;
      }
    }
  }
  ```
  (spawn now needs BOTH `workspaceId` — to resolve the worktree cwd — and
  `terminalId`.)
- Keep all the xterm init / FitAddon / ResizeObserver / waitForLayout /
  onDestroy logic unchanged.
- Drop the header row (the `<span>Terminal</span>` + status) from the pane — the
  container's tab bar replaces it. Keep the `terminal-container` div and the
  `attaching/exited` status as a small inline indicator if desired, or move
  status into the tab label later (out of scope). Keep
  `data-testid="terminal-container"` on the xterm host div.

- [ ] **Step 2: Rewrite `Terminal.svelte` as the container**

Replace `Terminal.svelte` with a container that renders the tab bar + one
kept-alive `TerminalPane` per tab, `display`-toggled by active (never
unmounted), with first-open auto-create and close-last empty state:

```svelte
<script lang="ts">
  import TerminalTabBar from './TerminalTabBar.svelte';
  import TerminalPane from './TerminalPane.svelte';
  import { terminalTabs } from '$lib/stores/terminal-tabs.svelte';
  import { api } from '$lib/ipc';

  interface Props {
    workspaceId: string;
  }
  const { workspaceId }: Props = $props();

  const tabs = $derived(terminalTabs.list(workspaceId));
  const active = $derived(terminalTabs.activeId(workspaceId));

  // First-open: when this workspace has no terminals yet, create one so
  // opening the Terminal tab always shows a live shell (matches the old
  // single-terminal UX). $effect runs after mount + on workspace change.
  $effect(() => {
    if (terminalTabs.list(workspaceId).length === 0) {
      terminalTabs.add(workspaceId);
    }
  });

  function addTerminal(): void {
    terminalTabs.add(workspaceId); // spawn happens lazily inside the new pane on mount
  }

  function closeTerminal(id: string): void {
    // Kill the PTY, then drop the tab. Order matters so the pane's
    // onDestroy doesn't race a reattach.
    void api.terminal.kill(id);
    terminalTabs.close(workspaceId, id);
  }
</script>

<div
  class="flex flex-col h-full bg-[var(--bg-base)]"
  data-testid="terminal-view"
>
  {#if tabs.length === 0}
    <div
      class="flex-1 flex items-center justify-center text-sm text-[var(--text-muted)]"
    >
      <button
        type="button"
        class="px-3 py-1.5 rounded border border-[var(--border)] hover:bg-[var(--bg-hover)]"
        onclick={addTerminal}
      >
        + New terminal
      </button>
    </div>
  {:else}
    <TerminalTabBar {workspaceId} onAdd={addTerminal} onClose={closeTerminal} />
    <div class="flex-1 relative overflow-hidden">
      {#each tabs as tab (tab.id)}
        <div
          class="absolute inset-0"
          class:hidden={active !== tab.id}
          data-testid="terminal-pane-host"
          data-id={tab.id}
        >
          <TerminalPane {workspaceId} terminalId={tab.id} />
        </div>
      {/each}
    </div>
  {/if}
</div>
```

Key requirements (call them out in the commit):

- Panes are rendered for ALL tabs and only **hidden** via `class:hidden`
  (Tailwind `display:none`) — never `{#if active}`-gated — so switching
  tabs/workspaces keeps each xterm + its PTY alive (CLAUDE.md xterm rule).
- `closeTerminal` kills the PTY (`api.terminal.kill(id)`) before dropping the
  tab.
- The `$effect` first-open guard must not loop: it only `add`s when the list is
  empty; after adding, the list is non-empty so it won't re-fire. Verify no
  infinite loop in `bun run check`/runtime.

- [ ] **Step 3: Checks**

Run: `bun run check` → `0 ERRORS 0 WARNINGS`. Run: `bun run lint` → no new
errors on the changed/new files. Run: `bun run vitest run 2>&1 | tail -3` →
existing suite still green (no Terminal unit test regressions; if an old
`Terminal.test.ts` asserted the removed header, update it to the new structure
or move the assertion to `TerminalPane`).

- [ ] **Step 4: Commit**

```bash
git add src/lib/components/workspace/TerminalPane.svelte src/lib/components/workspace/Terminal.svelte
git commit -m "feat(terminal-multitab): Terminal container hosts per-tab panes (display-toggled, never unmounted)"
```

---

### Task 6: E2E golden path + journal + coverage gate

**Files:**

- Create: `tests/e2e/phase-3c-terminal-multitab/terminal-tabs.spec.ts`
- Create: `journal/2026-05-27-terminal-multitab.md`

- [ ] **Step 1: E2E spec**

Create `tests/e2e/phase-3c-terminal-multitab/terminal-tabs.spec.ts`. Mirror the
shim pattern from an existing phase E2E
(`tests/e2e/phase-3a-4/team-activity-sidebar.spec.ts` — read it). Seed a repo +
workspace via `installTauriShim`, layer an `addInitScript` that mocks the
`terminal_spawn/reattach/kill` invokes to record live terminal ids, open the
workspace's Terminal tab, add a second terminal, switch tabs, and assert both
panes persist (two `terminal-pane-host` elements) and that closing kills.

```typescript
import { test, expect } from '../helpers/fixtures';
import { installTauriShim } from '../helpers/tauri-shim';

test('a workspace can open multiple terminal tabs that persist across switching', async ({
  page,
  harness,
}) => {
  void harness;
  await installTauriShim(page, {
    initialRepos: [
      {
        id: 'repo_e2e',
        name: 'term-repo',
        path: '/tmp/term-repo',
        gh_profile: null,
        default_branch: 'main',
        created_at: 1700000000,
        updated_at: 1700000000,
      },
    ],
    initialWorkspaces: [
      {
        id: 'ws_e2e',
        repo_id: 'repo_e2e',
        branch: 'main',
        base_branch: 'main',
        custom_branch: false,
        title: 'T',
        description: '',
        status: 'waiting',
        column: 'in_progress',
        created_at: 1,
        updated_at: 1,
        worktree_dir: '/tmp/term-repo',
        team_activity_private: false,
        task_id: null,
      },
    ],
    initialTasks: [],
  });
  await page.addInitScript(() => {
    const internals = (window as unknown as Record<string, unknown>)[
      '__TAURI_INTERNALS__'
    ] as {
      invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
    };
    const original = internals.invoke.bind(internals);
    const live = new Set<string>();
    (window as unknown as Record<string, unknown>)['__TERMS__'] = () =>
      live.size;
    internals.invoke = async (cmd: string, args: Record<string, unknown>) => {
      if (cmd === 'terminal_spawn') {
        live.add(args.terminalId as string);
        return undefined;
      }
      if (cmd === 'terminal_reattach') {
        return live.has(args.terminalId as string)
          ? undefined
          : Promise.reject('no session');
      }
      if (cmd === 'terminal_kill') {
        live.delete(args.terminalId as string);
        return undefined;
      }
      if (cmd === 'terminal_write' || cmd === 'terminal_resize')
        return undefined;
      return original(cmd, args);
    };
  });
  await page.goto('/');

  // Open the workspace, go to Work mode, open the Terminal tab.
  // (Selectors depend on the app shell — open the workspace from the sidebar,
  //  ensure Work mode, click the Terminal tab. Reuse selectors from other
  //  workspace E2E specs.)
  // After the Terminal tab is shown, the first terminal auto-spawns:
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as unknown as Record<string, () => number>)['__TERMS__']()
      )
    )
    .toBeGreaterThanOrEqual(1);

  // Add a second terminal.
  await page.getByTestId('terminal-tab-add').click();
  await expect
    .poll(() =>
      page.evaluate(() =>
        (window as unknown as Record<string, () => number>)['__TERMS__']()
      )
    )
    .toBe(2);

  // Two panes are mounted (kept alive, display-toggled).
  await expect(page.getByTestId('terminal-pane-host')).toHaveCount(2);
});
```

> The exact navigation selectors (open workspace, switch to Work, click Terminal
> tab) should be copied from an existing workspace-flow E2E. If reaching the
> Terminal tab through the UI is brittle, gate this spec behind an env flag like
> the other phase specs and assert the contract you can reach. Keep the core
> assertions: ≥1 auto-spawned, "+" → 2, two pane hosts mounted.

- [ ] **Step 2: Run E2E**

Run: `bun run e2e tests/e2e/phase-3c-terminal-multitab/terminal-tabs.spec.ts`
Expected: 1 passed. Fix selectors until green; keep the assertions.

- [ ] **Step 3: Journal**

Create `journal/2026-05-27-terminal-multitab.md`:

```markdown
# Journal — 2026-05-27 — Terminal multi-tab

## What shipped

A workspace's Terminal tab now holds multiple terminal sessions (tabs), like the
Editor's file tabs — so a user can run e.g. a frontend and a backend dev server
side by side. Session-only: terminals survive every in-session navigation
(workspace switch, top-level tab switch, Plan↔Work) but are not restored across
an app restart.

## Backend

- `commands/terminal.rs`: the terminal registry is now keyed by `terminal_id`
  (the handle still records `workspace_id`). The five IPC commands take a
  `terminalId`. New `kill_workspace_terminals_inner` tears down every terminal a
  workspace owned.
- `commands/workspace.rs`: `remove_workspace_inner` kills the workspace's
  terminals so PTYs don't leak.

## Frontend

- `stores/terminal-tabs.svelte.ts`: per-workspace in-memory tab list + active
  id + monotonic "Terminal N" labels; 6-terminal cap. Because it's in-memory and
  per-workspace, tabs survive navigation.
- `components/workspace/TerminalTabBar.svelte`: tab strip + "+" (disabled at
  cap) + "×".
- `components/workspace/TerminalPane.svelte`: the old single-terminal xterm
  logic, keyed by `terminalId` (reattach-then-spawn).
- `components/workspace/Terminal.svelte`: container — renders the tab bar and
  one kept-alive pane per tab (`display`-toggled, never unmounted, so PTYs +
  scrollback survive). First-open auto-creates one terminal; closing the last
  shows a "+ New terminal" empty state.

## Decisions

- Session-only; no restart restore, no detached processes (child PTYs die with
  the app).
- No per-tab rename; auto "Terminal N", numbers not reused.
- Cap 6 per workspace.

## Tests

- Rust: two terminals per workspace coexist + isolated; write/kill target one
  id; duplicate id rejected; `kill_workspace_terminals` scoped to a workspace;
  `remove_workspace` kills its terminals.
- Frontend: terminal-tabs store (add/cap/close/neighbour/labels/isolation);
  TerminalTabBar (render/active/＋disabled/×).
- E2E: open Terminal, auto-spawn one, "+" → two panes mounted and persisting.
```

- [ ] **Step 4: Full gate**

Run: `bun run vitest run 2>&1 | tail -4` (all pass), `bun run check` (clean),
`cd src-tauri && cargo test --lib 2>&1 | tail -3 && cargo clippy --lib --all-targets -- -D warnings 2>&1 | tail -3 && cd ..`
(pass + clean),
`bun run e2e tests/e2e/phase-3c-terminal-multitab/terminal-tabs.spec.ts` (1
pass). Coverage gate (95% changed files) — the new store + tab bar are
unit-tested; `Terminal.svelte`/`TerminalPane.svelte` are E2E-covered (consistent
with `App.svelte`'s coverage exclusion for thin/integration components — confirm
against baseline, don't add brittle xterm unit tests).

- [ ] **Step 5: Commit**

```bash
git add tests/e2e/phase-3c-terminal-multitab/terminal-tabs.spec.ts journal/2026-05-27-terminal-multitab.md
git commit -m "test(terminal-multitab): E2E golden path + journal"
```

---

## Self-Review

**Spec coverage:**

- Multiple terminals per workspace → Tasks 1 (backend re-key) + 3 (store) + 5
  (container/panes).
- IPC keyed by terminal_id → Task 2.
- Tabs survive in-session navigation → Task 5 (never-unmount panes) + Task 3
  (in-memory per-workspace store).
- No restart persistence → by omission (no persistence file); stated in
  journal/spec.
- Kill-all on workspace removal → Tasks 1 + 2.
- First-open auto-create, close-last empty state → Task 5.
- Cap 6, "Terminal N" monotonic labels → Task 3 + Task 4 (disabled +).
- Tab bar UI (+/×/switch) → Task 4.
- Testing (backend/store/component/E2E) → Tasks 1–6.

**Placeholder scan:** no TBD/TODO; the only deferrals are E2E navigation
selectors (explicitly "copy from existing workspace E2E", with a fallback) and
the optional transient `Terminal.svelte` compile-bridge in Task 2 — both are
concrete instructions, not gaps.

**Type consistency:** `terminal_id`/`terminalId` used consistently;
`terminalTabs.add → string|null`, `list/activeId/setActive/close/forget/reset`;
IPC `spawn(workspaceId, terminalId, channel, cols?, rows?)`,
`write(terminalId, bytes)`, `resize(terminalId, cols, rows)`,
`kill(terminalId)`, `reattach(terminalId, channel)` — match across Tasks
2/3/4/5. `kill_workspace_terminals_inner(workspace_id, state)` consistent.

**Note (verify during impl):** confirm `MockPty::new` / `MockPtyHandle`
signatures and the `make_state`/`make_worktree` helper names in `terminal.rs`
tests; confirm `remove_workspace_inner` is `pub(crate)` on this branch's base.
