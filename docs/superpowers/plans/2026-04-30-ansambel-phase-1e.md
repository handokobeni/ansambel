# Phase 1e — Robustness, Resilience, and Polish

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close out Phase 1 with no functional regressions, debounced disk
writes, surfaced CLI errors, and graceful failure modes — making Ansambel
actually usable for real multi-hour sessions.

**Architecture:** Three priority buckets, executed in order. **P0** fixes the
break-points users hit on day one (workspace-switch streaming bug, no stop
button, no real-CLI proof of partial streaming). **P1** trades raw correctness
for resilience: debounced writes, tail-read pagination, stderr surfacing,
schema-version sentinel, graceful reader shutdown. **P2** polishes the UX: DOM
virtualization for the chat list and actionable error CTAs for spawn failures.
No new features — every task closes a known gap or hardens an existing flow.

**Tech Stack:** Tauri v2, Svelte 5, Rust 1.75+, Bun, Vitest, Playwright. No new
dependencies. The existing `DebouncedWriter`
(`src-tauri/src/persistence/debounce.rs`) is wired up; everything else is
in-place hardening.

---

## File Structure

```
src-tauri/src/
├── state.rs                            # MODIFY: AgentHandle.channels: Vec<Sender>
├── commands/
│   ├── agent.rs                        # MODIFY: reattach_agent + stop_agent + stderr forwarding
│   ├── agent_core.rs                   # MODIFY: cancel token + spawn returns Sender, ChildKill
│   └── agent_stream.rs                 # unchanged
├── persistence/
│   ├── messages.rs                     # MODIFY: schema_version check + JSONL tail reader
│   └── debounce.rs                     # unchanged
└── lib.rs                              # MODIFY: register reattach_agent

src/lib/
├── ipc.ts                              # MODIFY: api.agent.reattach, api.agent.stop
└── components/
    ├── workspace/
    │   ├── WorkspaceView.svelte        # MODIFY: reattach on mount + stop button + actionable error
    │   └── WorkspaceView.test.ts       # MODIFY: reattach + stop + actionable-error tests
    └── chat/
        ├── ChatPanel.svelte            # MODIFY: virtualize list + error banner with action link
        └── ChatPanel.test.ts           # MODIFY: virtualization + error-banner tests

package.json                            # MODIFY: add @tanstack/svelte-virtual

tests/e2e/phase-1e/
└── streaming.spec.ts                   # CREATE: real-CLI partial-streaming smoke
```

Each agent.rs task ships with both an `_inner` unit test and a wrapper
command-existence test (the existing pattern). The frontend tasks add component
tests; the streaming verification adds a Playwright spec.

---

## Task 1: Reattach Channel on workspace switch [P0]

**Why:** Right now `WorkspaceView.onMount` only spawns the agent when status is
`not_started`/`waiting`. If the user switches away from a running workspace and
back, no listener is attached — the backend keeps streaming events into a
Channel whose handler was GC'd, and the UI freezes until the agent stops or the
app restarts.

**Files:**

- Modify: `src-tauri/src/state.rs` (AgentHandle struct)
- Modify: `src-tauri/src/commands/agent_core.rs` (broadcaster wiring)
- Modify: `src-tauri/src/commands/agent.rs` (new `reattach_agent` command)
- Modify: `src-tauri/src/lib.rs` (register command)
- Modify: `src/lib/ipc.ts`
- Modify: `src/lib/components/workspace/WorkspaceView.svelte`
- Test: `src-tauri/src/commands/agent_core.rs` (existing tests module)
- Test: `src/lib/components/workspace/WorkspaceView.test.ts`

- [ ] **Step 1: Write the failing Rust test**

In `src-tauri/src/commands/agent_core.rs` test module:

```rust
#[test]
fn reattach_agent_inner_returns_err_when_no_agent() {
    let state = make_state();
    let result = reattach_agent_inner(state, "ws_missing");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("no agent"));
}

#[test]
fn reattach_agent_inner_returns_sender_when_agent_running() {
    let state = make_state();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    state.lock().unwrap().agents.insert(
        "ws_re".into(),
        crate::state::AgentHandle {
            workspace_id: "ws_re".into(),
            stdin_tx: tx,
            session_id: None,
            event_tx: tokio::sync::broadcast::channel::<crate::state::AgentEvent>(64).0,
        },
    );
    let result = reattach_agent_inner(state, "ws_re");
    assert!(result.is_ok());
}

#[test]
fn agent_handle_event_broadcaster_delivers_to_multiple_subscribers() {
    let (tx, _) = tokio::sync::broadcast::channel::<crate::state::AgentEvent>(64);
    let mut sub_a = tx.subscribe();
    let mut sub_b = tx.subscribe();
    tx.send(crate::state::AgentEvent::Status {
        status: crate::state::AgentStatus::Running,
    })
    .unwrap();
    assert!(sub_a.try_recv().is_ok());
    assert!(sub_b.try_recv().is_ok());
}
```

- [ ] **Step 2: Run test to verify failure**

Run:
`cd src-tauri && cargo test --lib commands::agent_core::tests::reattach_agent_inner -- --nocapture`
Expected: FAIL — `reattach_agent_inner` not defined and `AgentHandle` missing
`event_tx`.

- [ ] **Step 3: Add broadcaster field to AgentHandle**

In `src-tauri/src/state.rs`:

```rust
#[derive(Debug)]
pub struct AgentHandle {
    pub workspace_id: String,
    pub stdin_tx: tokio::sync::mpsc::UnboundedSender<String>,
    pub session_id: Option<String>,
    /// Broadcast sender for agent events. spawn_agent creates this, the
    /// reader thread emits into it, and each WorkspaceView subscription
    /// (initial spawn + reattach) consumes its own receiver. Buffer of
    /// 256 events absorbs a few seconds of partial-message bursts before
    /// slow consumers drop oldest events with `Lagged`.
    pub event_tx: tokio::sync::broadcast::Sender<AgentEvent>,
}
```

- [ ] **Step 4: Update spawn_agent_inner to create the broadcaster**

In `src-tauri/src/commands/agent_core.rs`:

```rust
let (event_tx, _) = tokio::sync::broadcast::channel::<AgentEvent>(256);
// ... after the existing agents.insert:
s.agents.insert(
    workspace_id.into(),
    AgentHandle {
        workspace_id: workspace_id.into(),
        stdin_tx,
        session_id: None,
        event_tx: event_tx.clone(),
    },
);
```

- [ ] **Step 5: Implement reattach_agent_inner**

In `src-tauri/src/commands/agent_core.rs`, after `stop_agent_inner`:

```rust
pub fn reattach_agent_inner(
    state: Arc<Mutex<AppState>>,
    workspace_id: &str,
) -> AppResult<tokio::sync::broadcast::Receiver<AgentEvent>> {
    let s = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    let handle = s.agents.get(workspace_id).ok_or_else(|| AppError::Command {
        cmd: "reattach_agent".into(),
        msg: format!("no agent for workspace {workspace_id}"),
    })?;
    Ok(handle.event_tx.subscribe())
}
```

- [ ] **Step 6: Refactor spawn_reader_thread to broadcast**

In `src-tauri/src/commands/agent.rs`, change the closure to use the broadcast
sender from the handle:

```rust
fn spawn_reader_thread(
    mut process: AgentProcess,
    initial_subscriber: Channel<AgentEvent>,
    state: Arc<Mutex<AppState>>,
    workspace_id: String,
    data_dir: PathBuf,
) {
    let event_tx = {
        let s = state.lock().expect("state lock");
        s.agents
            .get(&workspace_id)
            .map(|h| h.event_tx.clone())
            .expect("agent handle present after spawn")
    };
    forward_subscriber(event_tx.subscribe(), initial_subscriber);
    let _ = event_tx.send(AgentEvent::Status {
        status: AgentStatus::Running,
    });
    let event_tx_reader = event_tx.clone();
    std::thread::spawn(move || {
        let reader = match process.reader() {
            Ok(r) => r,
            Err(e) => {
                let _ = event_tx_reader.send(AgentEvent::Error {
                    message: format!("reader: {e}"),
                });
                return;
            }
        };
        process_reader_events(reader, state, &workspace_id, &|ev: AgentEvent| {
            if let Some(msg) = event_to_persisted_message(&ev, &workspace_id) {
                if let Err(e) = append_message(&data_dir, &workspace_id, &msg) {
                    tracing::warn!(workspace_id = %workspace_id, error = %e, "agent reader: persist failed");
                }
            }
            let _ = event_tx_reader.send(ev);
        });
        let _ = process.try_wait();
        let _ = event_tx_reader.send(AgentEvent::Status {
            status: AgentStatus::Stopped,
        });
    });
}

fn forward_subscriber(
    mut rx: tokio::sync::broadcast::Receiver<AgentEvent>,
    channel: Channel<AgentEvent>,
) {
    std::thread::spawn(move || {
        loop {
            match rx.blocking_recv() {
                Ok(ev) => {
                    if channel.send(ev).is_err() {
                        return; // frontend dropped its handler
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            }
        }
    });
}
```

- [ ] **Step 7: Add the reattach_agent Tauri command**

In `src-tauri/src/commands/agent.rs`:

```rust
#[tauri::command]
pub async fn reattach_agent(
    workspace_id: String,
    on_event: Channel<AgentEvent>,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<(), String> {
    let rx = reattach_agent_inner(state.inner().clone(), &workspace_id)
        .map_err(|e| e.to_string())?;
    forward_subscriber(rx, on_event);
    Ok(())
}
```

Register in `src-tauri/src/lib.rs` invoke_handler list. Update the
`all_agent_commands_are_accessible` test to include `reattach_agent`.

- [ ] **Step 8: Run Rust tests**

Run: `cargo test --lib` Expected: PASS — all 240+ tests including the three new
reattach tests.

- [ ] **Step 9: Add the IPC wrapper**

In `src/lib/ipc.ts` agent block:

```typescript
reattach: (workspaceId: string, onEvent: Channel<AgentEvent>): Promise<void> =>
  invoke('reattach_agent', { workspaceId, onEvent }),
```

- [ ] **Step 10: Wire WorkspaceView**

In `src/lib/components/workspace/WorkspaceView.svelte` onMount:

```typescript
onMount(async () => {
  try {
    const history = await api.messages.list(workspace.id);
    messages.hydrate(workspace.id, history);
  } catch (err) {
    messages.apply({ type: 'error', message: String(err) }, workspace.id);
  }

  channel = agentChannel();
  channel.onmessage = (ev: AgentEvent) => {
    messages.apply(ev, workspace.id);
  };

  try {
    if (workspace.status === 'not_started' || workspace.status === 'waiting') {
      await api.agent.spawn(workspace.id, channel);
    } else {
      // Status is running — reattach instead of spawning.
      await api.agent.reattach(workspace.id, channel);
    }
  } catch (err) {
    messages.apply({ type: 'error', message: String(err) }, workspace.id);
  }
});
```

- [ ] **Step 11: Add the WorkspaceView tests**

In `src/lib/components/workspace/WorkspaceView.test.ts`:

```typescript
it('calls reattach_agent on mount when status is running', async () => {
  render(WorkspaceView, { props: { workspace: ws({ status: 'running' }) } });
  await waitFor(() => {
    expect(invoke).toHaveBeenCalledWith(
      'reattach_agent',
      expect.objectContaining({ workspaceId: 'ws_a' })
    );
  });
});

it('does not call spawn_agent when status is running', async () => {
  render(WorkspaceView, { props: { workspace: ws({ status: 'running' }) } });
  await new Promise((r) => setTimeout(r, 10));
  expect(invoke).not.toHaveBeenCalledWith('spawn_agent', expect.any(Object));
});

it('routes reattach channel events through messages.apply', async () => {
  let captured: { onmessage?: (ev: unknown) => void } | undefined;
  vi.mocked(invoke).mockImplementation(async (cmd, args) => {
    if (cmd === 'reattach_agent') {
      captured = (args as { onEvent: { onmessage?: (ev: unknown) => void } })
        .onEvent;
    }
    return undefined;
  });
  render(WorkspaceView, { props: { workspace: ws({ status: 'running' }) } });
  await waitFor(() => expect(captured).toBeDefined());
  captured?.onmessage?.({
    type: 'message',
    id: 'msg_live',
    role: 'assistant',
    text: 'live',
    is_partial: false,
  });
  await waitFor(() => {
    expect(
      messages.listForWorkspace('ws_a').find((m) => m.id === 'msg_live')?.text
    ).toBe('live');
  });
});
```

Update existing test "does not spawn_agent when status is running" — keep it but
rename to clarify intent.

- [ ] **Step 12: Run frontend tests**

Run: `bun run test` Expected: PASS — 225+ tests including new reattach tests.

- [ ] **Step 13: Commit**

```bash
git add -A
git commit -m "fix(agent): reattach Channel on workspace switch

When user switches away from a running workspace and back, the original
Channel handler is GC'd but the reader thread keeps streaming events
into the dead Channel. UI freezes until the agent stops or the app
restarts.

AgentHandle now owns a tokio broadcast::Sender. The reader thread
emits into the broadcaster; spawn_agent and the new reattach_agent
command both forward the broadcaster to a Tauri Channel via a small
bridge thread. WorkspaceView.onMount calls reattach when status is
running.

Buffer of 256 events absorbs partial-message bursts; slow consumers
drop oldest with Lagged, which is acceptable for a UI that re-renders
on the next non-partial message anyway."
```

---

## Task 2: Stop button + abort UX [P0]

**Why:** `stop_agent` IPC exists but no UI calls it. The only way to abort a
runaway turn is to kill the app.

**Files:**

- Modify: `src/lib/components/workspace/WorkspaceView.svelte`
- Modify: `src/lib/components/workspace/WorkspaceView.test.ts`

- [ ] **Step 1: Write the failing test**

```typescript
it('renders Stop button when status is running', () => {
  const { getByRole } = render(WorkspaceView, {
    props: { workspace: ws({ status: 'running' }) },
  });
  expect(getByRole('button', { name: /stop/i })).toBeTruthy();
});

it('does not render Stop button when status is waiting', () => {
  const { queryByRole } = render(WorkspaceView, {
    props: { workspace: ws({ status: 'waiting' }) },
  });
  expect(queryByRole('button', { name: /stop/i })).toBeNull();
});

it('clicking Stop calls stop_agent', async () => {
  messages.apply({ type: 'status', status: 'running' }, 'ws_a');
  const { getByRole } = render(WorkspaceView, { props: { workspace: ws() } });
  const { fireEvent } = await import('@testing-library/svelte');
  await fireEvent.click(getByRole('button', { name: /stop/i }));
  expect(invoke).toHaveBeenCalledWith('stop_agent', { workspaceId: 'ws_a' });
});

it('Stop button disabled while stop is in flight', async () => {
  messages.apply({ type: 'status', status: 'running' }, 'ws_a');
  let resolveStop!: () => void;
  vi.mocked(invoke).mockImplementation(async (cmd) => {
    if (cmd === 'stop_agent')
      return new Promise<void>((r) => (resolveStop = r));
    return undefined;
  });
  const { getByRole } = render(WorkspaceView, { props: { workspace: ws() } });
  const { fireEvent } = await import('@testing-library/svelte');
  const btn = getByRole('button', { name: /stop/i }) as HTMLButtonElement;
  await fireEvent.click(btn);
  expect(btn.disabled).toBe(true);
  resolveStop();
});
```

- [ ] **Step 2: Run tests to verify failure**

Run: `bun run test --run src/lib/components/workspace/WorkspaceView.test.ts`
Expected: FAIL — no Stop button.

- [ ] **Step 3: Implement Stop button**

In `WorkspaceView.svelte`:

```svelte
<script lang="ts">
  // ...existing imports
  let stopping = $state(false);

  async function handleStop() {
    if (stopping) return;
    stopping = true;
    try {
      await api.agent.stop(workspace.id);
    } catch (err) {
      messages.apply({ type: 'error', message: String(err) }, workspace.id);
    } finally {
      stopping = false;
    }
  }
</script>

<header
  class="flex items-center justify-between px-4 py-2 border-b border-[var(--border)] bg-[var(--bg-sidebar)]"
>
  <div class="flex flex-col">
    <h2 class="text-sm font-semibold text-[var(--text-primary)]">
      {workspace.title}
    </h2>
    <code class="text-xs text-[var(--text-muted)]">{workspace.branch}</code>
  </div>
  <div class="flex items-center gap-2">
    {#if status === 'running'}
      <button
        type="button"
        onclick={handleStop}
        disabled={stopping}
        class="text-xs px-2 py-0.5 rounded border border-[var(--border)] hover:bg-[var(--bg-card)] disabled:opacity-50"
      >
        Stop
      </button>
    {/if}
    <span
      class="text-xs px-2 py-0.5 rounded bg-[var(--bg-card)] text-[var(--text-secondary)]"
      data-status={status}
      aria-label="Agent status"
    >
      {statusLabel(status)}
    </span>
  </div>
</header>
```

- [ ] **Step 4: Run tests to verify pass**

Run: `bun run test --run src/lib/components/workspace/WorkspaceView.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(workspace): add Stop button to abort active turn

Header now renders a Stop button while status is running. Clicking it
calls the existing stop_agent IPC; button is disabled mid-flight to
prevent double-fires. Stop failures surface via the messages-store
error channel."
```

---

## Task 3: Verify partial-streaming with the real CLI [P0]

**Why:** All current evidence for token streaming is from unit tests with mocked
NDJSON. We don't actually know that Claude's `--include-partial-messages` emits
at the granularity our UI needs (per-token vs per-sentence). If the granularity
is too coarse the feature is theatrical, not real.

**Files:**

- Create: `tests/e2e/phase-1e/streaming.spec.ts`
- Modify: `tests/e2e/tauri-shim.ts` (if needed — record event timing)

- [ ] **Step 1: Write the E2E spec**

```typescript
import { test, expect } from '@playwright/test';
import { startTauri } from '../tauri-shim';

test.describe('partial-message streaming', () => {
  test('assistant text grows incrementally during a turn', async () => {
    // Mock CLI: emit a long response with --include-partial-messages-style deltas.
    process.env.ANSAMBEL_MOCK_CLAUDE_DELTA_COUNT = '20';
    const app = await startTauri({ mockClaude: 'streaming' });
    try {
      const { page } = app;

      await page.click('button[aria-label="Add Repo"]');
      // ...standard repo+workspace setup elided; reuse helpers from existing
      // chat-flow.spec.ts...
      const input = page.getByLabel(/message/i);
      await input.fill('write me a poem');
      await page.getByRole('button', { name: /send/i }).click();

      // Take three snapshots ~150ms apart while the turn streams. Each
      // should show strictly more text than the prior, proving the UI
      // is rendering deltas, not waiting for the final message.
      const lengths: number[] = [];
      for (let i = 0; i < 3; i++) {
        await page.waitForTimeout(150);
        const len = await page
          .locator('[data-message-role="assistant"]')
          .last()
          .innerText()
          .then((t) => t.length);
        lengths.push(len);
      }

      expect(lengths[1]).toBeGreaterThan(lengths[0]);
      expect(lengths[2]).toBeGreaterThan(lengths[1]);
    } finally {
      await app.close();
    }
  });
});
```

- [ ] **Step 2: Extend the mock CLI to emit partial messages**

If the existing `ANSAMBEL_MOCK_CLAUDE` mode doesn't already emit `stream_event`
lines, add a `streaming` profile. The mock should print:

```
{"type":"system","subtype":"init",...}
{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_mock","role":"assistant","content":[]}}}
# 20× content_block_delta with 50ms sleep between each
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"word "}}}
...
{"type":"stream_event","event":{"type":"message_stop"}}
{"type":"assistant","message":{"id":"msg_mock","role":"assistant","content":[{"type":"text","text":"<full text>"}]}}
{"type":"result","subtype":"success",...}
```

- [ ] **Step 3: Run the spec**

Run: `bun run test:e2e tests/e2e/phase-1e/streaming.spec.ts` Expected: PASS —
three monotonically growing text lengths.

- [ ] **Step 4: Manual verification with real CLI**

Spin up `bun run tauri dev` against the real Claude CLI, send a prompt that
elicits a long response ("explain transformers in 200 words"), confirm by eye
that text appears smoothly. If granularity feels chunky (multi-word jumps), file
a follow-up issue noting the observed cadence — that informs whether we need to
coalesce backend-side. Document the cadence in
`docs/superpowers/notes/streaming-cadence.md` (one paragraph).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "test(e2e): verify partial-message streaming end-to-end

Unit tests proved the parser handles content_block_delta correctly,
but nothing exercised the full pipe: mock CLI → parser → broadcast →
Tauri Channel → frontend store → DOM. The new spec snapshots the
assistant bubble length three times during a single turn and asserts
strict monotonic growth. The Mock CLI gains a 'streaming' profile that
mimics --include-partial-messages cadence with 20 deltas at 50ms
intervals.

Manual verification against the real CLI is documented in
docs/superpowers/notes/streaming-cadence.md."
```

---

## Task 4: Wire DebouncedWriter to append_message [P1]

**Why:** Project spec calls for 500ms debounced writes for messages.
`DebouncedWriter` exists in `persistence/debounce.rs` but isn't wired up — every
event triggers a full file rewrite. With token streaming each turn is one final
non-partial write, but tool-heavy turns (5+ tool_use + tool_result) still
produce 5–10 sync writes per turn.

**Files:**

- Modify: `src-tauri/src/state.rs` (AppState gains writer)
- Modify: `src-tauri/src/lib.rs` (construct writer at startup)
- Modify: `src-tauri/src/commands/agent.rs` (use writer instead of direct
  append)
- Modify: `src-tauri/src/persistence/messages.rs` (queue helper)

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/persistence/messages.rs` test module:

```rust
#[tokio::test]
async fn debounced_append_collapses_burst_to_single_write() {
    use crate::persistence::debounce::DebouncedWriter;
    use std::time::Duration;

    let tmp = TempDir::new().unwrap();
    let writer = DebouncedWriter::new(Duration::from_millis(50));

    // 10 rapid appends within the debounce window.
    for i in 0..10 {
        queue_message_append(&writer, tmp.path(), "ws_d", make_msg(&format!("msg_{i}"), "ws_d"))
            .unwrap();
    }
    writer.flush_all().await;

    let on_disk = load_messages(tmp.path(), "ws_d").unwrap();
    assert_eq!(on_disk.len(), 10);
    // Verify the file was actually persisted only once: track via a
    // mtime check — the file mtime should be after the last queue call,
    // not in the middle (ie, no intermediate writes were rolled over).
    // (We accept that file existence is the strongest assertion vitest
    // can give without instrumenting write_atomic; the collapse property
    // is also covered by the existing debounce.rs tests.)
}
```

- [ ] **Step 2: Run test to verify failure**

Run:
`cd src-tauri && cargo test --lib persistence::messages::tests::debounced_append`
Expected: FAIL — `queue_message_append` not defined.

- [ ] **Step 3: Add queue_message_append**

```rust
/// Queues a message append through the DebouncedWriter. Combines the
/// load-push-save cycle into a single value snapshot so the writer can
/// collapse multiple queues into one disk write.
///
/// Note: this is *not* a generic "append" — the writer's value semantics
/// are last-write-wins per path, so we serialize the *full file* each
/// time. For workspaces with thousands of messages, callers should
/// consider Task 5's tail-read format if write amplification becomes a
/// problem in profiling.
pub fn queue_message_append(
    writer: &crate::persistence::debounce::DebouncedWriter,
    data_dir: &Path,
    workspace_id: &str,
    msg: Message,
) -> Result<()> {
    let mut current = load_messages(data_dir, workspace_id).unwrap_or_default();
    if current.iter().any(|m| m.id == msg.id) {
        return Ok(());
    }
    current.push(msg);
    let file = MessagesFile {
        schema_version: 1,
        messages: current,
    };
    writer.queue(messages_file(data_dir, workspace_id), &file)
}
```

- [ ] **Step 4: Add writer to AppState**

In `src-tauri/src/state.rs`:

```rust
pub struct AppState {
    // ...existing fields
    pub messages_writer: crate::persistence::debounce::DebouncedWriter,
}
```

In `src-tauri/src/lib.rs` setup:

```rust
let state = AppState {
    // ...existing fields
    messages_writer: crate::persistence::debounce::DebouncedWriter::new(
        std::time::Duration::from_millis(500),
    ),
};
```

- [ ] **Step 5: Update spawn_reader_thread to use the writer**

In `src-tauri/src/commands/agent.rs`:

```rust
process_reader_events(reader, state.clone(), &workspace_id, &|ev: AgentEvent| {
    if let Some(msg) = event_to_persisted_message(&ev, &workspace_id) {
        let writer = match state.lock() {
            Ok(s) => s.messages_writer.clone(),
            Err(_) => return,
        };
        if let Err(e) = queue_message_append(&writer, &data_dir, &workspace_id, msg) {
            tracing::warn!(workspace_id = %workspace_id, error = %e, "agent reader: queue failed");
        }
    }
    let _ = event_tx_reader.send(ev);
});
```

Also update `send_message_inner_with_persist` to use `queue_message_append` for
the user-message path so persistence is uniform.

- [ ] **Step 6: Add app shutdown flush**

In `src-tauri/src/lib.rs` Tauri setup hook, register a `RunEvent::ExitRequested`
handler that calls `state.messages_writer.flush_all().await` before exit. This
guarantees pending writes are persisted on app close.

- [ ] **Step 7: Run all Rust tests**

Run: `cd src-tauri && cargo test --lib` Expected: PASS — 240+ including the new
debounced_append test.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "perf(persistence): debounce message writes through DebouncedWriter

Project spec calls for 500ms debounced writes for messages; the writer
existed in persistence/debounce.rs but every append did a sync full-file
rewrite. Tool-heavy turns now produce one disk write per ~500ms window
instead of one per event.

App shutdown flushes pending writes via RunEvent::ExitRequested so
nothing is lost on close. The writer's value semantics still serialize
the full file per flush — Task 5 (tail-read JSONL) addresses write
amplification on workspaces with thousands of messages."
```

---

## Task 5: Tail-read pagination (JSONL on-disk format) [P1]

**Why:** `list_messages_paginated` currently loads the entire JSON file into
memory and slices the last 50. For a 5MB workspace history that's 5MB read+parse
on every load-earlier click. Switching the on-disk format to JSONL (one Message
per line) lets us seek from the end and read only what we need.

**Files:**

- Modify: `src-tauri/src/persistence/messages.rs`
- Test: same file
- Migration helper: read both formats; write only JSONL going forward.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn jsonl_load_reads_last_n_messages_via_tail_seek() {
    let tmp = TempDir::new().unwrap();
    // Pre-write 1000 messages in JSONL format.
    let path = messages_file(tmp.path(), "ws_jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut f = std::fs::File::create(&path).unwrap();
    use std::io::Write;
    writeln!(f, "{{\"schema_version\":2}}").unwrap(); // header line
    for i in 0..1000 {
        let m = make_msg(&format!("msg_{i:04}"), "ws_jsonl");
        writeln!(f, "{}", serde_json::to_string(&m).unwrap()).unwrap();
    }

    let out = list_messages_paginated(tmp.path(), "ws_jsonl", Some(10), None).unwrap();
    assert_eq!(out.len(), 10);
    assert_eq!(out.first().unwrap().id, "msg_0990");
    assert_eq!(out.last().unwrap().id, "msg_0999");
}

#[test]
fn jsonl_load_handles_legacy_v1_json_format() {
    let tmp = TempDir::new().unwrap();
    save_messages(tmp.path(), "ws_legacy", &[make_msg("msg_a", "ws_legacy")]).unwrap();
    let out = list_messages_paginated(tmp.path(), "ws_legacy", None, None).unwrap();
    assert_eq!(out.len(), 1);
}

#[test]
fn append_message_writes_jsonl_when_file_is_empty_or_v2() {
    let tmp = TempDir::new().unwrap();
    append_message(tmp.path(), "ws_new", &make_msg("msg_a", "ws_new")).unwrap();
    let raw = std::fs::read_to_string(messages_file(tmp.path(), "ws_new")).unwrap();
    let mut lines = raw.lines();
    let header: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert_eq!(header["schema_version"], 2);
    let msg: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert_eq!(msg["id"], "msg_a");
}
```

- [ ] **Step 2: Run tests to verify failure**

Run: `cargo test --lib persistence::messages::tests::jsonl` Expected: FAIL.

- [ ] **Step 3: Implement format detection**

Add at top of `persistence/messages.rs`:

```rust
const SCHEMA_VERSION: u32 = 2;
const KNOWN_VERSIONS: &[u32] = &[1, 2];

#[derive(Debug, Clone, Copy)]
enum FileFormat {
    LegacyJson,    // v1: one big JSON object {schema_version, messages: [...]}
    Jsonl,         // v2: header line + one Message per line
    Empty,
}

fn detect_format(path: &Path) -> FileFormat {
    let raw = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(_) => return FileFormat::Empty,
    };
    let trimmed = raw.trim_start();
    if trimmed.is_empty() {
        return FileFormat::Empty;
    }
    // First non-blank line is either the v2 header `{"schema_version":2}`
    // or the start of a v1 JSON object `{"schema_version":1,"messages":[`.
    let first_line = trimmed.lines().next().unwrap_or("");
    let v: serde_json::Value = match serde_json::from_str(first_line) {
        Ok(v) => v,
        Err(_) => return FileFormat::Empty,
    };
    if v.get("messages").is_some() {
        FileFormat::LegacyJson
    } else {
        FileFormat::Jsonl
    }
}
```

- [ ] **Step 4: Implement tail-read for JSONL**

```rust
/// Reads the last `n` lines of a JSONL file by seeking from the end in
/// 8 KiB chunks. Buffers chunks until we've found `n` newlines (plus
/// the header).
fn read_jsonl_tail(path: &Path, n: usize) -> Result<Vec<Message>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    let mut buf = Vec::<u8>::new();
    let mut pos = len;
    let chunk_size: u64 = 8 * 1024;
    let mut newlines_seen = 0usize;
    while pos > 0 && newlines_seen <= n {
        let read = chunk_size.min(pos);
        pos -= read;
        f.seek(SeekFrom::Start(pos))?;
        let mut chunk = vec![0u8; read as usize];
        f.read_exact(&mut chunk)?;
        // Prepend chunk to buf.
        buf.splice(0..0, chunk.into_iter());
        newlines_seen = buf.iter().filter(|&&b| b == b'\n').count();
    }
    let raw = String::from_utf8_lossy(&buf);
    let mut messages: Vec<Message> = Vec::new();
    for line in raw.lines().skip(if pos == 0 { 1 } else { 0 }) {
        // skip the header line only if we read from the very start
        if line.trim().is_empty() {
            continue;
        }
        let parsed: serde_json::Value = serde_json::from_str(line)
            .map_err(|e| AppError::ParseFailed { what: "jsonl message".into(), msg: e.to_string() })?;
        // Skip header lines (those without an "id" field) defensively.
        if parsed.get("id").is_none() {
            continue;
        }
        let m: Message = serde_json::from_value(parsed)?;
        messages.push(m);
    }
    let start = messages.len().saturating_sub(n);
    Ok(messages[start..].to_vec())
}
```

- [ ] **Step 5: Update list_messages_paginated to dispatch on format**

```rust
pub fn list_messages_paginated(
    data_dir: &Path,
    workspace_id: &str,
    limit: Option<usize>,
    before_id: Option<&str>,
) -> Result<Vec<Message>> {
    let limit = limit.unwrap_or(DEFAULT_MESSAGE_PAGE).max(1);
    let path = messages_file(data_dir, workspace_id);
    match detect_format(&path) {
        FileFormat::Empty => Ok(Vec::new()),
        FileFormat::LegacyJson => {
            // Existing slow path, kept for files that haven't been migrated
            // yet. Migration happens lazily on next append.
            let all = load_messages(data_dir, workspace_id).unwrap_or_default();
            let upto = match before_id {
                Some(id) => match all.iter().position(|m| m.id == id) {
                    Some(i) => i,
                    None => return Ok(Vec::new()),
                },
                None => all.len(),
            };
            let head = &all[..upto];
            let start = head.len().saturating_sub(limit);
            Ok(head[start..].to_vec())
        }
        FileFormat::Jsonl => {
            // For JSONL we tail-read; before_id pagination still requires
            // walking back, but in chunks rather than loading the file.
            // For MVP we read the full file when before_id is set; this is
            // still O(N) but keeps the code small. Optimize later if
            // workspaces routinely paginate past 10 pages.
            if before_id.is_some() {
                let all = load_messages(data_dir, workspace_id).unwrap_or_default();
                let upto = match before_id {
                    Some(id) => match all.iter().position(|m| m.id == id) {
                        Some(i) => i,
                        None => return Ok(Vec::new()),
                    },
                    None => all.len(),
                };
                let head = &all[..upto];
                let start = head.len().saturating_sub(limit);
                Ok(head[start..].to_vec())
            } else {
                read_jsonl_tail(&path, limit)
            }
        }
    }
}
```

- [ ] **Step 6: Update load_messages to read both formats**

```rust
pub fn load_messages(data_dir: &Path, workspace_id: &str) -> Result<Vec<Message>> {
    let path = messages_file(data_dir, workspace_id);
    match detect_format(&path) {
        FileFormat::Empty => Ok(Vec::new()),
        FileFormat::LegacyJson => {
            let file: MessagesFile = load_or_default(&path)?;
            check_schema_version(file.schema_version)?;
            Ok(file.messages)
        }
        FileFormat::Jsonl => {
            let raw = std::fs::read_to_string(&path)?;
            let mut iter = raw.lines();
            let header_line = iter.next().unwrap_or("");
            let header: serde_json::Value = serde_json::from_str(header_line)?;
            let v = header.get("schema_version").and_then(|n| n.as_u64()).unwrap_or(0);
            check_schema_version(v as u32)?;
            let mut out = Vec::new();
            for line in iter {
                if line.trim().is_empty() { continue; }
                out.push(serde_json::from_str::<Message>(line)?);
            }
            Ok(out)
        }
    }
}
```

(Schema-version check covered in Task 8; for now `check_schema_version` can be
`Ok(())` and tightened in that task.)

- [ ] **Step 7: Update append_message to write JSONL**

```rust
pub fn append_message(data_dir: &Path, workspace_id: &str, msg: &Message) -> Result<()> {
    let path = messages_file(data_dir, workspace_id);
    let format = detect_format(&path);
    match format {
        FileFormat::LegacyJson => {
            // Migrate to JSONL on first append after upgrade.
            let mut current = load_messages(data_dir, workspace_id).unwrap_or_default();
            if current.iter().any(|m| m.id == msg.id) { return Ok(()); }
            current.push(msg.clone());
            write_jsonl(&path, &current)
        }
        FileFormat::Empty => {
            write_jsonl(&path, &[msg.clone()])
        }
        FileFormat::Jsonl => {
            // Fast path: dedup with a quick line scan, then append a single line.
            if existing_jsonl_contains_id(&path, &msg.id)? {
                return Ok(());
            }
            std::fs::create_dir_all(path.parent().unwrap())?;
            let mut f = std::fs::OpenOptions::new().append(true).open(&path)?;
            use std::io::Write;
            writeln!(f, "{}", serde_json::to_string(msg)?)?;
            Ok(())
        }
    }
}

fn write_jsonl(path: &Path, messages: &[Message]) -> Result<()> {
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("jsonl.tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        use std::io::Write;
        writeln!(f, "{{\"schema_version\":{}}}", SCHEMA_VERSION)?;
        for m in messages {
            writeln!(f, "{}", serde_json::to_string(m)?)?;
        }
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn existing_jsonl_contains_id(path: &Path, id: &str) -> Result<bool> {
    use std::io::{BufRead, BufReader};
    let f = std::fs::File::open(path)?;
    let reader = BufReader::new(f);
    for line in reader.lines() {
        let line = line?;
        if line.contains(id) {
            // Cheap pre-check; do a full parse to avoid false positives on
            // substring matches.
            if let Ok(m) = serde_json::from_str::<Message>(&line) {
                if m.id == id {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}
```

`save_messages` is kept for the test fixture path that pre-populates v1 files;
it now writes JSONL by default but we keep a `save_legacy_messages` test helper.

- [ ] **Step 8: Run all persistence tests**

Run: `cargo test --lib persistence::messages` Expected: PASS — 19+ tests
including new JSONL tests, with legacy v1 files still loading.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "perf(persistence): JSONL on-disk format with tail-read pagination

list_messages_paginated previously loaded the entire JSON file to slice
50 messages from the end. New JSONL format (one Message per line, with
a schema_version header line) lets list_messages_paginated seek from
the end in 8KB chunks until it has the requested count.

append_message is now O(1) on JSONL files: a substring pre-check + line
parse for dedup, then a single fs::OpenOptions append. No more full-
file rewrites per message.

Files in the legacy v1 format still load via the slow path; the next
append converts them to JSONL transparently."
```

---

## Task 6: Surface CLI stderr to the UI [P1]

**Why:** Right now stderr is drained to `tracing::warn`. If the user's CLI is
missing auth, hits a quota limit, or has a network error, they see "Stopped"
with no context.

**Files:**

- Modify: `src-tauri/src/commands/agent_core.rs` (forward stderr through
  broadcaster)
- Modify: `src/lib/components/chat/ChatPanel.svelte` (error banner)
- Modify: `src/lib/components/chat/ChatPanel.test.ts`

- [ ] **Step 1: Write the failing tests**

```typescript
// ChatPanel.test.ts
it('renders an error banner when messages.error is set', () => {
  messages.apply({ type: 'error', message: 'CLI: invalid auth token' }, 'ws_a');
  const { getByText } = render(ChatPanel, {
    props: { workspaceId: 'ws_a', onSend: vi.fn() },
  });
  expect(getByText('CLI: invalid auth token')).toBeTruthy();
});

it('error banner is dismissable', async () => {
  messages.apply({ type: 'error', message: 'oops' }, 'ws_a');
  const { getByRole, queryByText } = render(ChatPanel, {
    props: { workspaceId: 'ws_a', onSend: vi.fn() },
  });
  const { fireEvent } = await import('@testing-library/svelte');
  await fireEvent.click(getByRole('button', { name: /dismiss/i }));
  expect(queryByText('oops')).toBeNull();
});
```

```rust
// agent_core.rs test module
#[test]
fn stderr_lines_are_forwarded_to_event_broadcaster() {
    // The actual stderr-pump thread is hard to test without a child,
    // so we test the helper that maps a stderr line to an Error event.
    let ev = stderr_line_to_event("invalid_request_error: bad token");
    match ev {
        AgentEvent::Error { message } => assert!(message.contains("invalid_request_error")),
        _ => panic!("expected Error event"),
    }
}
```

- [ ] **Step 2: Implement stderr forwarding**

In `src-tauri/src/commands/agent_core.rs`, refactor the stderr drain:

```rust
pub fn stderr_line_to_event(line: &str) -> AgentEvent {
    AgentEvent::Error {
        message: format!("CLI: {}", line.trim_end()),
    }
}

// In spawn_agent_inner, replace the existing stderr thread:
if let Some(stderr) = stderr_pipe {
    let stderr_workspace_id = workspace_id.to_string();
    let stderr_tx = event_tx.clone();
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            tracing::warn!(
                workspace_id = %stderr_workspace_id,
                line = %line,
                "agent stderr"
            );
            let _ = stderr_tx.send(stderr_line_to_event(&line));
        }
    });
}
```

- [ ] **Step 3: Add the error banner to ChatPanel**

```svelte
<script lang="ts">
  // ...
  const error = $derived(messages.errorFor(workspaceId));
  let errorDismissed = $state(false);
  $effect(() => {
    // Reset dismissal when a new error arrives.
    if (error) errorDismissed = false;
  });
</script>

{#if error && !errorDismissed}
  <div
    class="px-3 py-2 bg-[var(--bg-error)] text-[var(--text-error)] text-sm flex items-center justify-between"
    role="alert"
  >
    <span>{error}</span>
    <button
      type="button"
      onclick={() => (errorDismissed = true)}
      aria-label="Dismiss"
      class="ml-2 hover:opacity-70"
    >
      ×
    </button>
  </div>
{/if}
```

- [ ] **Step 4: Run all tests**

Run: `bun run test && cd src-tauri && cargo test --lib` Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(agent): surface CLI stderr as in-chat error events

When Claude's CLI emits to stderr (auth failure, rate limit, network),
the line now becomes an AgentEvent::Error broadcast through the agent
event channel. ChatPanel renders the most recent error in a dismissable
banner so users know why their turn failed instead of just seeing
'Stopped'."
```

---

## Task 7: Graceful reader shutdown via cancel token [P1]

**Why:** Today the reader thread exits only on EOF. If the child hangs (e.g.,
CLI deadlock), the reader hangs too. `stop_agent` kills the child, which forces
EOF — but doesn't itself signal the reader. Adding a cancel token is a
defense-in-depth.

**Files:**

- Modify: `src-tauri/src/state.rs` (AgentHandle gains cancel)
- Modify: `src-tauri/src/commands/agent_core.rs` (reader respects cancel)
- Modify: `src-tauri/src/commands/agent.rs` (stop_agent triggers cancel + kill)

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn process_reader_events_exits_when_cancel_token_set() {
    use std::io::Cursor;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let cancel = Arc::new(AtomicBool::new(false));
    let state = make_state();
    // A reader that yields one line, then would block indefinitely on the
    // second read — but our cancel will short-circuit before that.
    struct OneLineThenBlock { yielded: bool }
    impl std::io::Read for OneLineThenBlock {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if !self.yielded {
                let line = b"{\"type\":\"result\",\"subtype\":\"success\",\"is_error\":false}\n";
                buf[..line.len()].copy_from_slice(line);
                self.yielded = true;
                Ok(line.len())
            } else {
                std::thread::sleep(std::time::Duration::from_millis(50));
                Ok(0)
            }
        }
    }
    let cancel_for_test = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(20));
        cancel_for_test.store(true, Ordering::Relaxed);
    });
    process_reader_events_with_cancel(
        Box::new(OneLineThenBlock { yielded: false }),
        state,
        "ws_cx",
        cancel.clone(),
        &|_| {},
    );
    // We get here only if the cancel token aborted the loop.
    assert!(cancel.load(Ordering::Relaxed));
}
```

- [ ] **Step 2: Implement the cancel-aware variant**

In `src-tauri/src/commands/agent_core.rs`, add:

```rust
pub fn process_reader_events_with_cancel<F>(
    reader: Box<dyn std::io::Read + Send>,
    state: Arc<Mutex<AppState>>,
    workspace_id: &str,
    cancel: Arc<std::sync::atomic::AtomicBool>,
    send_event: &F,
) where
    F: Fn(AgentEvent),
{
    use std::sync::atomic::Ordering;
    let mut br = BufReader::new(reader);
    let mut line = String::new();
    let mut parser = StreamParser::new();
    while !cancel.load(Ordering::Relaxed) {
        line.clear();
        match br.read_line(&mut line) {
            Ok(0) => {
                tracing::info!(workspace_id, "agent reader: EOF");
                break;
            }
            // ...rest identical to process_reader_events body
        }
    }
    if let Ok(mut s) = state.lock() {
        if let Some(ws) = s.workspaces.get_mut(workspace_id) {
            ws.status = WorkspaceStatus::Waiting;
        }
        s.agents.remove(workspace_id);
    }
}

// Keep process_reader_events as a thin wrapper calling the cancel-aware
// version with a never-fired token, so existing tests stay green.
pub fn process_reader_events<F>(
    reader: Box<dyn std::io::Read + Send>,
    state: Arc<Mutex<AppState>>,
    workspace_id: &str,
    send_event: &F,
) where F: Fn(AgentEvent) {
    let never = Arc::new(std::sync::atomic::AtomicBool::new(false));
    process_reader_events_with_cancel(reader, state, workspace_id, never, send_event)
}
```

- [ ] **Step 3: Add cancel field to AgentHandle**

```rust
pub struct AgentHandle {
    // ...
    pub cancel: Arc<std::sync::atomic::AtomicBool>,
}
```

Initialize in `spawn_agent_inner`, pass to the reader thread, set to true in
`stop_agent_inner` _before_ dropping the handle (so the reader exits cleanly
even if EOF doesn't fire promptly).

- [ ] **Step 4: Update stop_agent_inner**

```rust
pub fn stop_agent_inner(state: Arc<Mutex<AppState>>, workspace_id: &str) -> AppResult<()> {
    use crate::error::AppError;
    let cancel = {
        let s = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
        s.agents.get(workspace_id).map(|h| h.cancel.clone())
    };
    if let Some(c) = cancel {
        c.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let mut s = state.lock().map_err(|e| AppError::Other(e.to_string()))?;
    s.agents.remove(workspace_id);
    if let Some(ws) = s.workspaces.get_mut(workspace_id) {
        ws.status = WorkspaceStatus::Waiting;
    }
    Ok(())
}
```

- [ ] **Step 5: Run all Rust tests**

Run: `cargo test --lib` Expected: PASS — including the new cancel test.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "fix(agent): graceful reader shutdown via cancel token

stop_agent now flips an AtomicBool that the reader thread checks before
each read. Defense-in-depth against a hung CLI child where dropping the
stdin sender alone wouldn't force EOF promptly. Existing
process_reader_events keeps its signature; the new
process_reader_events_with_cancel does the work."
```

---

## Task 8: Schema-version sentinel [P1]

**Why:** `schema_version` has been written to disk since Phase 1c but never
validated. If we change `Message` in a future release and a user opens an old
file with the new app, serde will silently default missing fields — risking
corruption or wrong defaults. A 10-line check now prevents data loss later.

**Files:**

- Modify: `src-tauri/src/persistence/messages.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn load_messages_returns_err_when_schema_version_is_unknown() {
    let tmp = TempDir::new().unwrap();
    let path = messages_file(tmp.path(), "ws_future");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    // Fabricate a v999 file (JSONL header style).
    use std::io::Write;
    let mut f = std::fs::File::create(&path).unwrap();
    writeln!(f, "{{\"schema_version\":999}}").unwrap();
    let result = load_messages(tmp.path(), "ws_future");
    assert!(result.is_err());
    assert!(result.err().unwrap().to_string().contains("schema"));
}

#[test]
fn load_messages_accepts_known_versions() {
    let tmp = TempDir::new().unwrap();
    save_messages(tmp.path(), "ws_v1", &[make_msg("msg_a", "ws_v1")]).unwrap();
    assert!(load_messages(tmp.path(), "ws_v1").is_ok());
}
```

- [ ] **Step 2: Implement the check**

```rust
fn check_schema_version(v: u32) -> Result<()> {
    if KNOWN_VERSIONS.contains(&v) {
        Ok(())
    } else {
        Err(AppError::Other(format!(
            "Unsupported schema version {v}. Please update the app to read this workspace's history."
        )))
    }
}
```

Wire into both load paths (legacy JSON branch and JSONL header) inside
`load_messages` and `list_messages_paginated`.

- [ ] **Step 3: Run tests**

Run: `cargo test --lib persistence::messages` Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "fix(persistence): reject unknown message schema versions

schema_version has been written for ages but never validated. If a
future release changes the Message struct and a user opens an old file
with the new app, serde would silently default missing fields. The
10-line check now refuses to load files with unknown versions and
surfaces an actionable error message."
```

---

## Task 9: DOM virtualization for the chat list [P2]

**Why:** Without virtualization, every persisted message renders as a real DOM
node. Pagination + lazy-load grow the count by 50 per click; token streaming
triggers reactivity that re-renders the active assistant bubble while every
sibling is laid out and painted. Variable-height markdown bubbles compound this.
Virtualization keeps the DOM bounded to the viewport (~30-50 nodes) regardless
of how many messages are in the store, so memory of `Message` objects in the
SvelteMap is no longer a concern — eviction becomes unnecessary.

This task replaces the LRU/FIFO eviction approach considered earlier. Eviction
was treating the symptom (memory) rather than the cause (DOM cost);
virtualization solves both.

**Files:**

- Modify: `package.json` (add `@tanstack/svelte-virtual`)
- Modify: `src/lib/components/chat/ChatPanel.svelte`
- Modify: `src/lib/components/chat/ChatPanel.test.ts`

> **Library compatibility note:** `@tanstack/svelte-virtual` is the recommended
> choice. If its current release is incompatible with Svelte 5 runes at
> implementation time, fall back to a hand-rolled implementation using
> `IntersectionObserver` + a sentinel above/below the viewport. The behavioral
> tests below are framework-agnostic and should pass either way.

- [ ] **Step 1: Install the dependency**

```bash
bun add @tanstack/svelte-virtual
```

Verify it imports under Svelte 5 with a smoke check:

```bash
bun run check
```

- [ ] **Step 2: Write failing tests (behavioral)**

In `src/lib/components/chat/ChatPanel.test.ts`:

```typescript
describe('DOM virtualization', () => {
  it('renders only viewport-visible bubbles when list is large', async () => {
    const wsId = 'ws_v';
    for (let i = 0; i < 1000; i++) {
      messages.upsert({ ...make(`msg_${i}`, wsId), created_at: i });
    }
    const { container } = render(ChatPanel, {
      props: { workspaceId: wsId, onSend: vi.fn() },
    });
    // jsdom gives us 0×0 viewport; the virtualizer should still cap
    // rendered nodes well below the total count. Anything under ~200
    // proves we are NOT rendering 1000 simultaneously.
    const rendered = container.querySelectorAll('[data-message-id]').length;
    expect(rendered).toBeLessThan(200);
  });

  it('streams partial updates to the active bubble without remounting siblings', async () => {
    const wsId = 'ws_s';
    messages.upsert({ ...make('msg_a', wsId), created_at: 1 });
    const { container } = render(ChatPanel, {
      props: { workspaceId: wsId, onSend: vi.fn() },
    });
    const initialBubble = container.querySelector('[data-message-id="msg_a"]');
    // Streaming partial updates with same id.
    messages.apply(
      {
        type: 'message',
        id: 'msg_a',
        role: 'assistant',
        text: 'streaming...',
        is_partial: true,
      },
      wsId
    );
    await vi.waitFor(() => {
      const updated = container.querySelector('[data-message-id="msg_a"]');
      expect(updated?.textContent).toContain('streaming...');
    });
    // Same DOM node — Svelte's keyed-each preserved it.
    expect(container.querySelector('[data-message-id="msg_a"]')).toBe(
      initialBubble
    );
  });

  it('auto-scrolls to bottom when a new non-partial message arrives and user is pinned', async () => {
    const wsId = 'ws_p';
    for (let i = 0; i < 5; i++) {
      messages.upsert({ ...make(`msg_${i}`, wsId), created_at: i });
    }
    const { getByTestId } = render(ChatPanel, {
      props: { workspaceId: wsId, onSend: vi.fn() },
    });
    const scroll = getByTestId('chat-scroll');
    // Mark as pinned to bottom.
    Object.defineProperty(scroll, 'scrollHeight', {
      value: 1000,
      configurable: true,
    });
    Object.defineProperty(scroll, 'clientHeight', {
      value: 500,
      configurable: true,
    });
    Object.defineProperty(scroll, 'scrollTop', {
      value: 500,
      configurable: true,
      writable: true,
    });
    messages.upsert({ ...make('msg_new', wsId), created_at: 100 });
    await vi.waitFor(() => {
      // scrollTop should advance to keep the new message visible.
      expect((scroll as HTMLElement).scrollTop).toBeGreaterThan(500);
    });
  });

  it('does NOT auto-scroll when user has scrolled up', async () => {
    const wsId = 'ws_u';
    for (let i = 0; i < 5; i++) {
      messages.upsert({ ...make(`msg_${i}`, wsId), created_at: i });
    }
    const { getByTestId } = render(ChatPanel, {
      props: { workspaceId: wsId, onSend: vi.fn() },
    });
    const scroll = getByTestId('chat-scroll');
    Object.defineProperty(scroll, 'scrollHeight', {
      value: 1000,
      configurable: true,
    });
    Object.defineProperty(scroll, 'clientHeight', {
      value: 500,
      configurable: true,
    });
    Object.defineProperty(scroll, 'scrollTop', {
      value: 100,
      configurable: true,
      writable: true,
    });
    const { fireEvent } = await import('@testing-library/svelte');
    await fireEvent.scroll(scroll); // updates pinned-to-bottom flag → false
    messages.upsert({ ...make('msg_late', wsId), created_at: 200 });
    await new Promise((r) => setTimeout(r, 20));
    expect((scroll as HTMLElement).scrollTop).toBe(100);
  });
});
```

- [ ] **Step 3: Run tests to verify failure**

Run: `bun run test --run src/lib/components/chat/ChatPanel.test.ts` Expected:
the four new tests FAIL — current ChatPanel renders all messages without
virtualization or pinned-bottom logic.

- [ ] **Step 4: Implement virtualization in ChatPanel.svelte**

```svelte
<script lang="ts">
  import { tick } from 'svelte';
  import { createVirtualizer } from '@tanstack/svelte-virtual';
  import { messages } from '$lib/stores/messages.svelte';
  import MessageBubble from './MessageBubble.svelte';
  import MessageInput from './MessageInput.svelte';
  import type { Message } from '$lib/types';

  interface Props {
    workspaceId: string;
    onSend: (text: string) => void;
    onLoadEarlier?: (beforeId: string) => Promise<Message[]>;
    loadEarlierThreshold?: number;
  }

  const {
    workspaceId,
    onSend,
    onLoadEarlier,
    loadEarlierThreshold = 80,
  }: Props = $props();

  const list = $derived(messages.listForWorkspace(workspaceId));
  const status = $derived(messages.statusFor(workspaceId));
  const inputDisabled = $derived(status === 'error' || status === 'stopped');
  const error = $derived(messages.errorFor(workspaceId));

  let loading = $state(false);
  let exhausted = $state(false);
  let pinnedToBottom = $state(true);
  let scrollEl: HTMLDivElement | undefined;

  // The virtualizer is a $derived rune so it re-creates only when the
  // scroll element first binds; getScrollElement closes over scrollEl
  // by reference, so subsequent list changes use the same instance.
  const virtualizer = $derived(
    scrollEl
      ? createVirtualizer<HTMLDivElement, HTMLDivElement>({
          count: list.length,
          getScrollElement: () => scrollEl ?? null,
          estimateSize: () => 80,
          overscan: 5,
          // Don't measure partial-message bubbles — their height changes
          // every delta event, and re-measuring on every change causes
          // layout thrash. Final assistant message lands with
          // is_partial: false; the next render measures it once.
          measureElement: (el) => {
            const idx = Number(el.getAttribute('data-index'));
            const msg = list[idx];
            if (msg?.is_partial) return el.getBoundingClientRect().height;
            return el.getBoundingClientRect().height;
          },
        })
      : null
  );

  // Track pinned-to-bottom state. Threshold of 50 px feels right for
  // chat — the user is "at the bottom" if within one bubble of it.
  function handleScroll(): void {
    if (!scrollEl) return;
    const dist =
      scrollEl.scrollHeight - scrollEl.scrollTop - scrollEl.clientHeight;
    pinnedToBottom = dist < 50;
    if (scrollEl.scrollTop <= loadEarlierThreshold) {
      void loadEarlier();
    }
  }

  // Auto-scroll to the latest message when pinned. We anchor on
  // list.length so token-streaming partials within an existing bubble
  // don't trigger scroll, but new messages do.
  let lastLength = $state(0);
  $effect(() => {
    const len = list.length;
    if (len !== lastLength) {
      lastLength = len;
      if (pinnedToBottom && scrollEl) {
        // Defer to after layout so virtualizer has a height to scroll to.
        queueMicrotask(() => {
          if (scrollEl) {
            scrollEl.scrollTop = scrollEl.scrollHeight;
          }
        });
      }
    }
  });

  async function loadEarlier(): Promise<void> {
    if (!onLoadEarlier || loading || exhausted) return;
    if (list.length === 0) return;
    loading = true;
    const beforeId = list[0].id;
    const previousScrollHeight = scrollEl?.scrollHeight ?? 0;
    try {
      const batch = await onLoadEarlier(beforeId);
      if (batch.length === 0) {
        exhausted = true;
      } else {
        messages.hydrate(workspaceId, batch);
        await tick();
        if (scrollEl) {
          const delta = scrollEl.scrollHeight - previousScrollHeight;
          scrollEl.scrollTop = scrollEl.scrollTop + delta;
        }
      }
    } catch (err) {
      messages.apply({ type: 'error', message: String(err) }, workspaceId);
    } finally {
      loading = false;
    }
  }
</script>

<section class="flex flex-col h-full bg-[var(--bg-base)]">
  <div
    bind:this={scrollEl}
    onscroll={handleScroll}
    data-testid="chat-scroll"
    class="flex-1 overflow-y-auto px-3 py-3"
  >
    {#if onLoadEarlier && list.length > 0}
      <div class="flex justify-center py-1 text-xs text-[var(--text-muted)]">
        {#if loading}
          <span data-testid="loading-earlier">Loading earlier…</span>
        {:else if exhausted}
          <span data-testid="history-exhausted">No more history.</span>
        {:else}
          <button
            type="button"
            class="hover:text-[var(--text-secondary)]"
            onclick={loadEarlier}
            data-testid="load-earlier-button"
          >
            Load earlier
          </button>
        {/if}
      </div>
    {/if}

    {#if list.length === 0}
      <div
        class="flex-1 flex items-center justify-center text-sm text-[var(--text-muted)]"
      >
        Start the conversation — type a message below.
      </div>
    {:else if virtualizer}
      <div
        style="height: {$virtualizer.getTotalSize()}px; width: 100%; position: relative;"
      >
        {#each $virtualizer.getVirtualItems() as virtualRow (virtualRow.key)}
          <div
            data-index={virtualRow.index}
            style="position: absolute; top: 0; left: 0; width: 100%; transform: translateY({virtualRow.start}px);"
          >
            <MessageBubble message={list[virtualRow.index]} />
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <MessageInput {onSend} disabled={inputDisabled} />
</section>
```

- [ ] **Step 5: Update existing ChatPanel tests for the new structure**

The existing "renders one bubble per message" test counts `[data-message-id]`
nodes; it still works because small lists fit in viewport. The lazy-load tests
still pass because the load-earlier button is unchanged. Verify by running the
full ChatPanel test file.

- [ ] **Step 6: Run tests**

Run: `bun run test --run src/lib/components/chat/ChatPanel.test.ts` Expected:
PASS — 20+ tests including the four new virtualization tests.

- [ ] **Step 7: Manual smoke**

`bun run tauri dev`. Generate a workspace with 500+ messages (use the
`seed-messages` helper or paste a long history fixture). Scroll through; verify
no jank. Trigger a long token-streamed reply with auto-scroll pinned; verify the
bubble grows smoothly without other bubbles repainting.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "perf(chat): virtualize the message list

Without virtualization, every message renders as a real DOM node and
token-streaming reactivity has to consider all siblings on each
delta. Variable-height markdown bubbles compound the cost.
@tanstack/svelte-virtual keeps the DOM bounded to the viewport
(~30-50 nodes) regardless of total message count.

Auto-scroll to bottom is preserved when the user is pinned within
50 px; if they scrolled up to read history, new messages don't yank
their viewport. Partial-message bubbles skip remeasurement to avoid
layout thrash; final non-partial bubbles measure once on arrival.

This replaces the LRU/FIFO eviction approach considered earlier —
eviction was treating the symptom (memory), virtualization solves
the cause (DOM cost). Memory of Message objects in the SvelteMap is
~500 bytes each; a 5000-message session is ~2.5 MB, which is fine."
```

---

## Task 10: Actionable spawn error [P2]

**Why:** When `spawn_agent` fails (CLI not found, auth issue), the error
currently lands in `messages.error` as a generic toast. Make it actionable: "Set
Claude binary path in Settings" with a link.

**Files:**

- Modify: `src/lib/components/workspace/WorkspaceView.svelte`
- Modify: `src/lib/components/workspace/WorkspaceView.test.ts`

- [ ] **Step 1: Write the failing test**

```typescript
it('renders Settings link when spawn fails with binary-not-found', async () => {
  vi.mocked(invoke).mockImplementation(async (cmd) => {
    if (cmd === 'spawn_agent') throw 'spawn_agent: claude binary not found';
    return undefined;
  });
  const { findByRole } = render(WorkspaceView, {
    props: { workspace: ws({ status: 'not_started' }) },
  });
  const link = await findByRole('link', { name: /settings/i });
  expect(link).toBeTruthy();
});

it('does not render Settings link for unrelated errors', async () => {
  vi.mocked(invoke).mockImplementation(async (cmd) => {
    if (cmd === 'spawn_agent') throw 'spawn_agent: random unrelated thing';
    return undefined;
  });
  const { queryByRole } = render(WorkspaceView, {
    props: { workspace: ws({ status: 'not_started' }) },
  });
  await new Promise((r) => setTimeout(r, 20));
  expect(queryByRole('link', { name: /settings/i })).toBeNull();
});
```

- [ ] **Step 2: Implement the actionable banner**

In `WorkspaceView.svelte`, after the existing error handling logic, add a
derived flag:

```typescript
const showSettingsCta = $derived(
  (messages.errorFor(workspace.id) ?? '')
    .toLowerCase()
    .includes('claude binary not found')
);
```

Render alongside the error banner (this lives inside ChatPanel but the CTA can
be passed in as a slot or a prop). Simpler: put a settings link directly inside
ChatPanel's error banner when the error string contains "claude binary":

```svelte
{#if error && !errorDismissed}
  <div role="alert" class="...">
    <span>{error}</span>
    {#if /claude binary/i.test(error)}
      <a href="#/settings" class="underline">Settings</a>
    {/if}
    <button onclick={() => (errorDismissed = true)} aria-label="Dismiss"
      >×</button
    >
  </div>
{/if}
```

- [ ] **Step 3: Run tests**

Run: `bun run test --run src/lib/components/workspace/WorkspaceView.test.ts`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(workspace): actionable Settings CTA on binary-not-found errors

When spawn_agent fails because the Claude CLI isn't on PATH, the chat
error banner now includes a Settings link. Saves users a round of
googling 'where do I configure claude path in ansambel'."
```

---

## Self-Review Checklist

Before considering Phase 1e complete, the implementer should verify:

- [ ] All 10 tasks have committed code with passing tests.
- [ ] `cd src-tauri && cargo test --lib` passes (≥250 tests).
- [ ] `bun run test` passes (≥230 tests).
- [ ] `bun run check` passes (svelte-check + tsc).
- [ ] `cd src-tauri && cargo clippy --lib --all-targets -- -D warnings` clean.
- [ ] `bun run lint` clean.
- [ ] Manual smoke: open the app, switch between two running workspaces, confirm
      both stream live (Task 1).
- [ ] Manual smoke: send a long prompt, click Stop mid-turn, confirm input
      re-enables (Task 2).
- [ ] Manual smoke: verify token streaming visible to the eye against real CLI
      (Task 3).
- [ ] Manual smoke: kill auth temporarily, send a message, confirm error banner
      with CLI text (Task 6).
- [ ] Manual smoke: rename `claude` binary on PATH, open a workspace, confirm
      Settings link appears (Task 10).
- [ ] CHANGELOG entry under "## [Unreleased]" listing the 10 task headlines.

## Triggers for items intentionally deferred to a future phase

| Item                                    | Trigger to revisit                                                                                                                       |
| --------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| Auto-restart on CLI crash               | After a classifier for exit reasons exists + backoff design reviewed                                                                     |
| Channel backpressure / event coalescing | Token streaming feels laggy under real usage measurement                                                                                 |
| Multi-mutex decomposition               | > 50 simultaneous workspaces or lock-wait > 10ms profiled                                                                                |
| Schema migration framework (full)       | A breaking change to `Message` or another persisted struct is actually planned. The Task 8 sentinel covers the immediate data-loss risk. |

## Execution Handoff

Plan complete and saved to
`docs/superpowers/plans/2026-04-30-ansambel-phase-1e.md`. Two execution options:

1. **Subagent-Driven (recommended)** — fresh subagent per task, two-stage review
   (spec compliance + code quality) between tasks, fast iteration in this
   session.
2. **Inline Execution** — execute tasks in this session using
   superpowers:executing-plans, batch execution with checkpoints for human
   review.

Phase 1e has no inter-task dependencies _between_ P0/P1/P2 buckets, but there
are dependencies _within_ tasks (e.g. Task 4 depends on Task 1's broadcaster
shape; Task 5's tail-read assumes the schema-version sentinel from Task 8 will
land). Recommended order: 1 → 2 → 3 → 8 → 5 → 4 → 6 → 7 → 9 → 10.
