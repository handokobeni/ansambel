# Ansambel — Phase 1c Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking. Execute **after** Phase 1c-backend is
> merged.

**Goal:** Build the chat UI on top of Phase 1c-backend — a `MessagesStore` keyed
by workspace, three chat components (`ChatPanel`, `MessageBubble`,
`MessageInput`), a `WorkspaceView` wrapper that replaces the Work-mode
placeholder in `App.svelte`, and Tauri Channel wiring that streams `AgentEvent`s
into the store. Ships the **first usable product**: drag a task to In Progress →
spawn agent → chat with Claude → see streaming reply.

**Architecture:** Add a fourth runes-based store (`MessagesStore`) using nested
`SvelteMap<wsId, SvelteMap<msgId, Message>>` (same pattern as TasksStore from
Phase 1b). The chat surface owns its own subscription to the agent Channel;
mounting `WorkspaceView` calls `spawn_agent` if the workspace status is
`not_started`/`waiting`, then receives `AgentEvent`s and applies them in-place
to the store. xterm/diff are explicitly out of scope (Phase 2). E2E spec proves
the end-to-end loop using a mock claude binary.

**Tech Stack:** Svelte 5 runes, TypeScript strict, Vitest +
`@testing-library/svelte`, Playwright with the existing `TauriDevHarness`

- `tauri-shim` (extend with `spawn_agent` / `send_message` / `stop_agent` mocks
  that fake AgentEvents on a JS Channel). No new runtime deps.

**Prerequisite:** Phase 1c-backend merged.

---

## Table of Contents

1. [Task 1](#task-1-add-agentevent--message-types-to-typests) — Add
   `AgentEvent` + `Message` + related types (~6 tests)
2. [Task 2](#task-2-add-apiagent-ipc-wrappers-and-channel-helper) —
   `api.agent.*` IPC wrappers + `agentChannel()` helper (~8 tests)
3. [Task 3](#task-3-messagessveltetss--messagesstore) — `MessagesStore` with
   nested SvelteMap (~12 tests)
4. [Task 4](#task-4-messagebubblesveltes--single-bubble-component) —
   `MessageBubble.svelte` (~6 tests)
5. [Task 5](#task-5-messageinputsveltes--input--send) — `MessageInput.svelte`
   (~6 tests)
6. [Task 6](#task-6-chatpanelsveltes--scrollback--bubble-list) —
   `ChatPanel.svelte` (~6 tests)
7. [Task 7](#task-7-workspaceviewsveltes--mount-spawn-receive-events) —
   `WorkspaceView.svelte` orchestrator (~7 tests)
8. [Task 8](#task-8-wire-workspaceview-into-appsvelte-work-mode) — Replace the
   Work-mode placeholder in `App.svelte` (~3 tests)
9. [Task 9](#task-9-extend-tauri-shim-with-agent-mocks) — Add agent mock to
   `tests/e2e/helpers/tauri-shim.ts` (no Vitest tests; covered by E2E)
10. [Task 10](#task-10-e2e-testse2ephase-1cchat-flowspects) — E2E: spawn → send
    → assert streamed reply

---

## Task 1: Add `AgentEvent` + `Message` types to `types.ts`

**Files:**

- Modify: `src/lib/types.ts`

- [ ] **Step 1.1: Write failing tests**

```typescript
// Append to src/lib/types.test.ts (create if missing):

import { describe, it, expectTypeOf } from 'vitest';
import type {
  Message,
  MessageRole,
  ToolUse,
  ToolResult,
  AgentEvent,
  AgentStatus,
} from './types';

describe('Phase 1c types', () => {
  it('MessageRole is a union of the 4 roles', () => {
    expectTypeOf<MessageRole>().toEqualTypeOf<
      'user' | 'assistant' | 'system' | 'tool'
    >();
  });

  it('Message has all expected fields', () => {
    const m: Message = {
      id: 'msg_a',
      workspace_id: 'ws_x',
      role: 'assistant',
      text: 'hi',
      is_partial: false,
      tool_use: null,
      tool_result: null,
      created_at: 0,
    };
    expectTypeOf(m.id).toBeString();
    expectTypeOf(m.is_partial).toBeBoolean();
  });

  it('AgentStatus is the 4-variant union', () => {
    expectTypeOf<AgentStatus>().toEqualTypeOf<
      'running' | 'waiting' | 'error' | 'stopped'
    >();
  });

  it('AgentEvent.Init carries session_id and model', () => {
    const ev: AgentEvent = {
      type: 'init',
      session_id: 'ses_a',
      model: 'claude-sonnet-4-6',
    };
    expectTypeOf(ev).toMatchTypeOf<{ type: 'init' }>();
  });

  it('AgentEvent.Message carries id/role/text/is_partial', () => {
    const ev: AgentEvent = {
      type: 'message',
      id: 'msg_a',
      role: 'assistant',
      text: 'hi',
      is_partial: false,
    };
    expectTypeOf(ev).toMatchTypeOf<{ type: 'message' }>();
  });

  it('AgentEvent.Status carries status field', () => {
    const ev: AgentEvent = { type: 'status', status: 'running' };
    expectTypeOf(ev.status).toEqualTypeOf<AgentStatus>();
  });

  it('AgentEvent.Error carries message field', () => {
    const ev: AgentEvent = { type: 'error', message: 'spawn failed' };
    expectTypeOf(ev.message).toBeString();
  });
});
```

- [ ] **Step 1.2: Run tests — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/types.test.ts 2>&1 | tail -10
```

Expected: type errors — `Message`, `AgentEvent`, etc. not exported.

- [ ] **Step 1.3: Implement**

Append to `src/lib/types.ts`:

```typescript
export type MessageRole = 'user' | 'assistant' | 'system' | 'tool';

export interface ToolUse {
  id: string;
  name: string;
  input: unknown;
}

export interface ToolResult {
  tool_use_id: string;
  content: string;
  is_error: boolean;
}

export interface Message {
  id: string;
  workspace_id: string;
  role: MessageRole;
  text: string;
  is_partial: boolean;
  tool_use: ToolUse | null;
  tool_result: ToolResult | null;
  created_at: number;
}

export type AgentStatus = 'running' | 'waiting' | 'error' | 'stopped';

export type AgentEvent =
  | { type: 'init'; session_id: string; model: string }
  | {
      type: 'message';
      id: string;
      role: MessageRole;
      text: string;
      is_partial: boolean;
    }
  | { type: 'tool_use'; message_id: string; tool_use: ToolUse }
  | { type: 'tool_result'; message_id: string; tool_result: ToolResult }
  | { type: 'status'; status: AgentStatus }
  | { type: 'error'; message: string };
```

- [ ] **Step 1.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/types.test.ts 2>&1 | tail -10
bun run check 2>&1 | tail -5
```

Expected: 7 type-tests pass; `bun run check` green.

- [ ] **Step 1.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/types.ts src/lib/types.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): add Message, AgentEvent, ToolUse, ToolResult TS types

Mirrors the Rust state.rs definitions. AgentEvent uses the discriminated
union form (type: 'init' | 'message' | 'tool_use' | ...) matching the
Rust serde tag. Frontend stores and components type against these.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Add `api.agent.*` IPC wrappers and Channel helper

**Files:**

- Modify: `src/lib/ipc.ts`
- Modify: `src/lib/ipc.test.ts`

- [ ] **Step 2.1: Write failing tests**

```typescript
// Append to src/lib/ipc.test.ts:

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { api, agentChannel } from './ipc';

const invokeMock = vi.fn();
beforeEach(() => {
  invokeMock.mockReset();
  (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'] = {
    invoke: invokeMock,
    transformCallback: () => 0,
  };
});

describe('api.agent', () => {
  it('spawn passes workspaceId and channel to invoke', async () => {
    invokeMock.mockResolvedValue(undefined);
    const ch = agentChannel();
    await api.agent.spawn('ws_a', ch);
    expect(invokeMock).toHaveBeenCalledWith('spawn_agent', {
      workspaceId: 'ws_a',
      onEvent: ch,
    });
  });

  it('send passes workspaceId and text', async () => {
    invokeMock.mockResolvedValue(undefined);
    await api.agent.send('ws_a', 'Hello world');
    expect(invokeMock).toHaveBeenCalledWith('send_message', {
      workspaceId: 'ws_a',
      text: 'Hello world',
    });
  });

  it('stop passes workspaceId only', async () => {
    invokeMock.mockResolvedValue(undefined);
    await api.agent.stop('ws_a');
    expect(invokeMock).toHaveBeenCalledWith('stop_agent', {
      workspaceId: 'ws_a',
    });
  });

  it('spawn rejects when invoke rejects', async () => {
    invokeMock.mockRejectedValue('spawn failed');
    const ch = agentChannel();
    await expect(api.agent.spawn('ws_a', ch)).rejects.toBe('spawn failed');
  });

  it('send rejects when invoke rejects', async () => {
    invokeMock.mockRejectedValue('no agent');
    await expect(api.agent.send('ws_a', 'hi')).rejects.toBe('no agent');
  });
});

describe('agentChannel', () => {
  it('returns a Tauri Channel-shaped object with onmessage setter', () => {
    const ch = agentChannel();
    expect(typeof ch).toBe('object');
    // Tauri Channel has an onmessage property that callers assign.
    // We verify the type accepts a function assignment.
    ch.onmessage = (_ev) => undefined;
    expect(typeof ch.onmessage).toBe('function');
  });

  it('two channels are independent instances', () => {
    const a = agentChannel();
    const b = agentChannel();
    expect(a).not.toBe(b);
  });

  it('Channel id is a number (Tauri internal)', () => {
    const ch = agentChannel() as unknown as { id: number };
    expect(typeof ch.id).toBe('number');
  });
});
```

- [ ] **Step 2.2: Run tests — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/ipc.test.ts 2>&1 | tail -15
```

Expected: errors — `api.agent` and `agentChannel` not exported.

- [ ] **Step 2.3: Implement**

In `src/lib/ipc.ts`, add at the top of the imports (alongside the existing
`@tauri-apps/api/core` import):

```typescript
import { Channel } from '@tauri-apps/api/core';
```

Append to the `api` object literal (after `task`):

```typescript
agent: {
  spawn: (workspaceId: string, onEvent: Channel<AgentEvent>): Promise<void> =>
    invoke('spawn_agent', { workspaceId, onEvent }),
  send: (workspaceId: string, text: string): Promise<void> =>
    invoke('send_message', { workspaceId, text }),
  stop: (workspaceId: string): Promise<void> =>
    invoke('stop_agent', { workspaceId }),
},
```

Add the helper at the bottom of the file:

```typescript
export function agentChannel(): Channel<AgentEvent> {
  return new Channel<AgentEvent>();
}
```

Add the import for `AgentEvent`:

```typescript
import type { /* existing */, AgentEvent } from './types';
```

- [ ] **Step 2.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/ipc.test.ts 2>&1 | tail -10
```

Expected: 8 new tests pass.

- [ ] **Step 2.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/ipc.ts src/lib/ipc.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): add api.agent IPC wrappers and agentChannel helper

api.agent.{spawn,send,stop} mirror the three backend Tauri commands.
agentChannel() returns a fresh Tauri Channel<AgentEvent> the caller
passes to spawn so streaming events arrive zero-copy.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: `messages.svelte.ts` — `MessagesStore`

**Files:**

- Create: `src/lib/stores/messages.svelte.ts`
- Create: `src/lib/stores/messages.svelte.test.ts`

- [ ] **Step 3.1: Write failing tests**

```typescript
// New file src/lib/stores/messages.svelte.test.ts:

import { describe, it, expect, beforeEach } from 'vitest';
import { messages } from './messages.svelte';
import type { Message } from '../types';

const make = (id: string, ws = 'ws_a', text = 'body'): Message => ({
  id,
  workspace_id: ws,
  role: 'assistant',
  text,
  is_partial: false,
  tool_use: null,
  tool_result: null,
  created_at: 0,
});

describe('MessagesStore', () => {
  beforeEach(() => {
    messages.reset();
  });

  it('starts empty', () => {
    expect(messages.listForWorkspace('ws_a')).toEqual([]);
  });

  it('upsert adds a new message', () => {
    messages.upsert(make('msg_a'));
    expect(messages.listForWorkspace('ws_a')).toHaveLength(1);
  });

  it('upsert updates an existing message in place', () => {
    messages.upsert(make('msg_a', 'ws_a', 'first'));
    messages.upsert(make('msg_a', 'ws_a', 'second'));
    const list = messages.listForWorkspace('ws_a');
    expect(list).toHaveLength(1);
    expect(list[0].text).toBe('second');
  });

  it('listForWorkspace sorts by created_at ascending', () => {
    messages.upsert({ ...make('msg_2'), created_at: 200 });
    messages.upsert({ ...make('msg_1'), created_at: 100 });
    messages.upsert({ ...make('msg_3'), created_at: 300 });
    const list = messages.listForWorkspace('ws_a');
    expect(list.map((m) => m.id)).toEqual(['msg_1', 'msg_2', 'msg_3']);
  });

  it('separates messages by workspace', () => {
    messages.upsert(make('msg_a', 'ws_x'));
    messages.upsert(make('msg_b', 'ws_y'));
    expect(messages.listForWorkspace('ws_x')).toHaveLength(1);
    expect(messages.listForWorkspace('ws_y')).toHaveLength(1);
  });

  it('apply Init event is a no-op for messages', () => {
    messages.apply(
      { type: 'init', session_id: 'ses_a', model: 'claude' },
      'ws_a'
    );
    expect(messages.listForWorkspace('ws_a')).toEqual([]);
  });

  it('apply Message event upserts assistant message', () => {
    messages.apply(
      {
        type: 'message',
        id: 'msg_a',
        role: 'assistant',
        text: 'hello',
        is_partial: false,
      },
      'ws_a'
    );
    const list = messages.listForWorkspace('ws_a');
    expect(list).toHaveLength(1);
    expect(list[0].text).toBe('hello');
    expect(list[0].is_partial).toBe(false);
  });

  it('apply Status event sets status without touching messages', () => {
    messages.apply({ type: 'status', status: 'running' }, 'ws_a');
    expect(messages.statusFor('ws_a')).toBe('running');
    expect(messages.listForWorkspace('ws_a')).toEqual([]);
  });

  it('apply Error event captures latest error string', () => {
    messages.apply({ type: 'error', message: 'spawn failed' }, 'ws_a');
    expect(messages.errorFor('ws_a')).toBe('spawn failed');
  });

  it('apply ToolUse attaches tool_use to existing message', () => {
    messages.apply(
      {
        type: 'message',
        id: 'msg_a',
        role: 'assistant',
        text: 'using tool',
        is_partial: false,
      },
      'ws_a'
    );
    messages.apply(
      {
        type: 'tool_use',
        message_id: 'msg_a',
        tool_use: {
          id: 'toolu_a',
          name: 'Read',
          input: { path: '/etc/hosts' },
        },
      },
      'ws_a'
    );
    const list = messages.listForWorkspace('ws_a');
    expect(list[0].tool_use).toEqual({
      id: 'toolu_a',
      name: 'Read',
      input: { path: '/etc/hosts' },
    });
  });

  it('apply ToolResult creates a tool message', () => {
    messages.apply(
      {
        type: 'tool_result',
        message_id: 'msg_r',
        tool_result: { tool_use_id: 'toolu_a', content: 'ok', is_error: false },
      },
      'ws_a'
    );
    const list = messages.listForWorkspace('ws_a');
    expect(list).toHaveLength(1);
    expect(list[0].role).toBe('tool');
    expect(list[0].tool_result?.content).toBe('ok');
  });

  it('reset clears all workspaces', () => {
    messages.upsert(make('msg_a', 'ws_x'));
    messages.upsert(make('msg_b', 'ws_y'));
    messages.reset();
    expect(messages.listForWorkspace('ws_x')).toEqual([]);
    expect(messages.listForWorkspace('ws_y')).toEqual([]);
  });
});
```

- [ ] **Step 3.2: Run tests — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/stores/messages.svelte.test.ts 2>&1 | tail -15
```

Expected: errors — `messages` not exported.

- [ ] **Step 3.3: Implement**

Create `src/lib/stores/messages.svelte.ts`:

```typescript
import { SvelteMap } from 'svelte/reactivity';
import type { AgentEvent, AgentStatus, Message, MessageRole } from '../types';

class MessagesStore {
  // Public reactive state — runes-based public field, like TasksStore.
  byWorkspace = $state(new SvelteMap<string, SvelteMap<string, Message>>());
  status = $state(new SvelteMap<string, AgentStatus>());
  error = $state(new SvelteMap<string, string>());

  private getOrCreate(wsId: string): SvelteMap<string, Message> {
    let map = this.byWorkspace.get(wsId);
    if (!map) {
      map = new SvelteMap<string, Message>();
      this.byWorkspace.set(wsId, map);
    }
    return map;
  }

  upsert(msg: Message): void {
    this.getOrCreate(msg.workspace_id).set(msg.id, msg);
  }

  listForWorkspace(wsId: string): Message[] {
    const map = this.byWorkspace.get(wsId);
    if (!map) return [];
    return [...map.values()].sort((a, b) => a.created_at - b.created_at);
  }

  statusFor(wsId: string): AgentStatus | undefined {
    return this.status.get(wsId);
  }

  errorFor(wsId: string): string | undefined {
    return this.error.get(wsId);
  }

  apply(ev: AgentEvent, wsId: string): void {
    switch (ev.type) {
      case 'init':
        // session_id is held by the store; for MVP we only emit on the channel
        // and let the workspace component track it if needed.
        return;
      case 'message': {
        const existing = this.byWorkspace.get(wsId)?.get(ev.id);
        const created_at = existing?.created_at ?? Date.now();
        this.upsert({
          id: ev.id,
          workspace_id: wsId,
          role: ev.role,
          text: ev.text,
          is_partial: ev.is_partial,
          tool_use: existing?.tool_use ?? null,
          tool_result: existing?.tool_result ?? null,
          created_at,
        });
        return;
      }
      case 'tool_use': {
        const existing = this.byWorkspace.get(wsId)?.get(ev.message_id);
        if (existing) {
          this.upsert({ ...existing, tool_use: ev.tool_use });
        } else {
          this.upsert({
            id: ev.message_id,
            workspace_id: wsId,
            role: 'assistant',
            text: '',
            is_partial: false,
            tool_use: ev.tool_use,
            tool_result: null,
            created_at: Date.now(),
          });
        }
        return;
      }
      case 'tool_result': {
        const role: MessageRole = 'tool';
        this.upsert({
          id: ev.message_id,
          workspace_id: wsId,
          role,
          text: '',
          is_partial: false,
          tool_use: null,
          tool_result: ev.tool_result,
          created_at: Date.now(),
        });
        return;
      }
      case 'status':
        this.status.set(wsId, ev.status);
        return;
      case 'error':
        this.error.set(wsId, ev.message);
        return;
    }
  }

  reset(): void {
    this.byWorkspace.clear();
    this.status.clear();
    this.error.clear();
  }
}

export const messages = new MessagesStore();
```

- [ ] **Step 3.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/stores/messages.svelte.test.ts 2>&1 | tail -10
```

Expected: 12 tests pass.

- [ ] **Step 3.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/stores/messages.svelte.ts src/lib/stores/messages.svelte.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): add MessagesStore with apply(AgentEvent) handler

Nested SvelteMap<wsId, SvelteMap<msgId, Message>> mirrors TasksStore.
Public $state fields so reactivity works across module boundaries.
apply() centralises the AgentEvent → store mutation, called by the
workspace view's channel listener.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: `MessageBubble.svelte` — single bubble component

**Files:**

- Create: `src/lib/components/chat/MessageBubble.svelte`
- Create: `src/lib/components/chat/MessageBubble.test.ts`

- [ ] **Step 4.1: Write failing tests**

```typescript
// New file src/lib/components/chat/MessageBubble.test.ts:

import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/svelte';
import MessageBubble from './MessageBubble.svelte';
import type { Message } from '../../types';

const make = (overrides: Partial<Message> = {}): Message => ({
  id: 'msg_a',
  workspace_id: 'ws_a',
  role: 'assistant',
  text: 'Hello',
  is_partial: false,
  tool_use: null,
  tool_result: null,
  created_at: 0,
  ...overrides,
});

describe('MessageBubble', () => {
  it('renders message text', () => {
    const { getByText } = render(MessageBubble, { props: { message: make() } });
    expect(getByText('Hello')).toBeTruthy();
  });

  it('applies user role class on user messages', () => {
    const { container } = render(MessageBubble, {
      props: { message: make({ role: 'user', text: 'hi' }) },
    });
    const bubble = container.querySelector('[data-role="user"]');
    expect(bubble).toBeTruthy();
  });

  it('applies assistant role class on assistant messages', () => {
    const { container } = render(MessageBubble, {
      props: { message: make({ role: 'assistant' }) },
    });
    expect(container.querySelector('[data-role="assistant"]')).toBeTruthy();
  });

  it('shows partial indicator when is_partial', () => {
    const { getByLabelText } = render(MessageBubble, {
      props: { message: make({ is_partial: true }) },
    });
    expect(getByLabelText(/streaming/i)).toBeTruthy();
  });

  it('renders tool_use block when present', () => {
    const { getByText } = render(MessageBubble, {
      props: {
        message: make({
          text: '',
          tool_use: { id: 'toolu_a', name: 'Read', input: { path: '/x' } },
        }),
      },
    });
    expect(getByText('Read')).toBeTruthy();
  });

  it('renders tool_result block on tool role', () => {
    const { getByText } = render(MessageBubble, {
      props: {
        message: make({
          role: 'tool',
          text: '',
          tool_result: {
            tool_use_id: 'toolu_a',
            content: 'ok output',
            is_error: false,
          },
        }),
      },
    });
    expect(getByText(/ok output/)).toBeTruthy();
  });
});
```

- [ ] **Step 4.2: Run tests — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/chat/MessageBubble.test.ts 2>&1 | tail -15
```

Expected: error — `MessageBubble.svelte` not found.

- [ ] **Step 4.3: Implement**

Create `src/lib/components/chat/MessageBubble.svelte`:

```svelte
<script lang="ts">
  import type { Message } from '$lib/types';

  interface Props {
    message: Message;
  }

  const { message }: Props = $props();
</script>

<article
  class="flex flex-col gap-1 px-3 py-2 rounded text-sm break-words"
  class:bg-[var(--bg-card)]={message.role === 'user'}
  class:bg-[var(--bg-base)]={message.role !== 'user'}
  class:border={message.role !== 'user'}
  class:border-[var(--border-light)]={message.role !== 'user'}
  data-role={message.role}
  data-message-id={message.id}
>
  {#if message.tool_use}
    <div
      class="flex items-center gap-2 text-xs font-mono text-[var(--text-secondary)]"
      data-tool-use
    >
      <span class="text-[var(--accent)]">⚙</span>
      <span class="font-semibold text-[var(--text-primary)]"
        >{message.tool_use.name}</span
      >
    </div>
  {/if}

  {#if message.tool_result}
    <pre
      class="text-xs font-mono text-[var(--text-secondary)] whitespace-pre-wrap overflow-x-auto"
      data-tool-result
      class:text-[var(--error)]={message.tool_result.is_error}>{message
        .tool_result.content}</pre>
  {/if}

  {#if message.text}
    <p class="whitespace-pre-wrap text-[var(--text-primary)]">{message.text}</p>
  {/if}

  {#if message.is_partial}
    <span class="text-xs text-[var(--text-muted)]" aria-label="streaming"
      >▍</span
    >
  {/if}
</article>
```

- [ ] **Step 4.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/chat/MessageBubble.test.ts 2>&1 | tail -10
```

Expected: 6 tests pass.

- [ ] **Step 4.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/components/chat/MessageBubble.svelte src/lib/components/chat/MessageBubble.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): add MessageBubble component with tool_use and tool_result

Renders a single Message in chat. Shows tool_use as an icon + name pill,
tool_result as a preformatted block (red text on errors), and a streaming
caret when is_partial. Role drives bg color via data-role attribute.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: `MessageInput.svelte` — input + send

**Files:**

- Create: `src/lib/components/chat/MessageInput.svelte`
- Create: `src/lib/components/chat/MessageInput.test.ts`

- [ ] **Step 5.1: Write failing tests**

```typescript
// New file src/lib/components/chat/MessageInput.test.ts:

import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import MessageInput from './MessageInput.svelte';

describe('MessageInput', () => {
  it('renders a textarea and a send button', () => {
    const onSend = vi.fn();
    const { getByLabelText, getByRole } = render(MessageInput, {
      props: { onSend },
    });
    expect(getByLabelText(/message/i)).toBeTruthy();
    expect(getByRole('button', { name: /send/i })).toBeTruthy();
  });

  it('calls onSend with input text on click', async () => {
    const onSend = vi.fn();
    const { getByLabelText, getByRole } = render(MessageInput, {
      props: { onSend },
    });
    const ta = getByLabelText(/message/i) as HTMLTextAreaElement;
    await fireEvent.input(ta, { target: { value: 'Hello!' } });
    await fireEvent.click(getByRole('button', { name: /send/i }));
    expect(onSend).toHaveBeenCalledWith('Hello!');
  });

  it('clears input after send', async () => {
    const onSend = vi.fn();
    const { getByLabelText, getByRole } = render(MessageInput, {
      props: { onSend },
    });
    const ta = getByLabelText(/message/i) as HTMLTextAreaElement;
    await fireEvent.input(ta, { target: { value: 'msg' } });
    await fireEvent.click(getByRole('button', { name: /send/i }));
    expect(ta.value).toBe('');
  });

  it('does not call onSend on empty input', async () => {
    const onSend = vi.fn();
    const { getByRole } = render(MessageInput, { props: { onSend } });
    await fireEvent.click(getByRole('button', { name: /send/i }));
    expect(onSend).not.toHaveBeenCalled();
  });

  it('cmd+enter / ctrl+enter submits', async () => {
    const onSend = vi.fn();
    const { getByLabelText } = render(MessageInput, { props: { onSend } });
    const ta = getByLabelText(/message/i) as HTMLTextAreaElement;
    await fireEvent.input(ta, { target: { value: 'shortcut' } });
    await fireEvent.keyDown(ta, { key: 'Enter', ctrlKey: true });
    expect(onSend).toHaveBeenCalledWith('shortcut');
  });

  it('disables send button when disabled prop is true', () => {
    const onSend = vi.fn();
    const { getByRole } = render(MessageInput, {
      props: { onSend, disabled: true },
    });
    const btn = getByRole('button', { name: /send/i }) as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });
});
```

- [ ] **Step 5.2: Run tests — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/chat/MessageInput.test.ts 2>&1 | tail -15
```

Expected: error — component not found.

- [ ] **Step 5.3: Implement**

Create `src/lib/components/chat/MessageInput.svelte`:

```svelte
<script lang="ts">
  interface Props {
    onSend: (text: string) => void;
    disabled?: boolean;
  }

  const { onSend, disabled = false }: Props = $props();

  let value = $state('');

  function handleSend() {
    const trimmed = value.trim();
    if (!trimmed) return;
    onSend(trimmed);
    value = '';
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      handleSend();
    }
  }
</script>

<form
  class="flex flex-col gap-2 p-3 border-t border-[var(--border)] bg-[var(--bg-base)]"
  onsubmit={(e) => {
    e.preventDefault();
    handleSend();
  }}
>
  <label class="sr-only" for="message-input">Message</label>
  <textarea
    id="message-input"
    class="w-full px-3 py-2 text-sm rounded bg-[var(--bg-card)] border border-[var(--border-light)] text-[var(--text-primary)] placeholder-[var(--text-muted)] focus:outline-none focus:border-[var(--accent)] resize-none min-h-[60px]"
    placeholder="Ask Claude…"
    bind:value
    onkeydown={handleKeydown}
    {disabled}
  ></textarea>
  <div class="flex justify-end">
    <button
      type="submit"
      class="px-3 py-1.5 text-xs font-semibold rounded bg-[var(--accent)] text-[var(--bg-base)] hover:opacity-90 transition-opacity disabled:opacity-50 cursor-pointer"
      disabled={disabled || !value.trim()}
      aria-label="Send message"
    >
      Send
    </button>
  </div>
</form>
```

- [ ] **Step 5.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/chat/MessageInput.test.ts 2>&1 | tail -10
```

Expected: 6 tests pass.

- [ ] **Step 5.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/components/chat/MessageInput.svelte src/lib/components/chat/MessageInput.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): add MessageInput with cmd/ctrl+enter to send

Trims, ignores empty submits, clears on send. Disabled prop gates the
button + textarea while a workspace is between status transitions.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: `ChatPanel.svelte` — scrollback + bubble list

**Files:**

- Create: `src/lib/components/chat/ChatPanel.svelte`
- Create: `src/lib/components/chat/ChatPanel.test.ts`

- [ ] **Step 6.1: Write failing tests**

```typescript
// New file src/lib/components/chat/ChatPanel.test.ts:

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render } from '@testing-library/svelte';
import ChatPanel from './ChatPanel.svelte';
import { messages } from '$lib/stores/messages.svelte';
import type { Message } from '$lib/types';

const make = (id: string, ws = 'ws_a', text = 'body'): Message => ({
  id,
  workspace_id: ws,
  role: 'assistant',
  text,
  is_partial: false,
  tool_use: null,
  tool_result: null,
  created_at: Number(id.replace(/[^0-9]/g, '')) || 0,
});

describe('ChatPanel', () => {
  beforeEach(() => {
    messages.reset();
  });

  it('renders empty state when no messages', () => {
    const { getByText } = render(ChatPanel, {
      props: { workspaceId: 'ws_a', onSend: vi.fn() },
    });
    expect(getByText(/start the conversation/i)).toBeTruthy();
  });

  it('renders one bubble per message in workspace', () => {
    messages.upsert(make('msg_1', 'ws_a', 'first'));
    messages.upsert(make('msg_2', 'ws_a', 'second'));
    const { container } = render(ChatPanel, {
      props: { workspaceId: 'ws_a', onSend: vi.fn() },
    });
    expect(container.querySelectorAll('[data-message-id]').length).toBe(2);
  });

  it('does not render messages from other workspaces', () => {
    messages.upsert(make('msg_x', 'ws_other'));
    const { container } = render(ChatPanel, {
      props: { workspaceId: 'ws_a', onSend: vi.fn() },
    });
    expect(container.querySelectorAll('[data-message-id]').length).toBe(0);
  });

  it('renders MessageInput', () => {
    const { getByLabelText } = render(ChatPanel, {
      props: { workspaceId: 'ws_a', onSend: vi.fn() },
    });
    expect(getByLabelText(/message/i)).toBeTruthy();
  });

  it('forwards send to onSend prop', async () => {
    const onSend = vi.fn();
    const { getByLabelText, getByRole } = render(ChatPanel, {
      props: { workspaceId: 'ws_a', onSend },
    });
    const ta = getByLabelText(/message/i) as HTMLTextAreaElement;
    const { fireEvent } = await import('@testing-library/svelte');
    await fireEvent.input(ta, { target: { value: 'hi' } });
    await fireEvent.click(getByRole('button', { name: /send/i }));
    expect(onSend).toHaveBeenCalledWith('hi');
  });

  it('disables input when status is not running and not waiting', () => {
    messages.apply({ type: 'status', status: 'error' }, 'ws_a');
    const { getByRole } = render(ChatPanel, {
      props: { workspaceId: 'ws_a', onSend: vi.fn() },
    });
    const btn = getByRole('button', { name: /send/i }) as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });
});
```

- [ ] **Step 6.2: Run tests — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/chat/ChatPanel.test.ts 2>&1 | tail -15
```

Expected: error — `ChatPanel.svelte` not found.

- [ ] **Step 6.3: Implement**

Create `src/lib/components/chat/ChatPanel.svelte`:

```svelte
<script lang="ts">
  import { messages } from '$lib/stores/messages.svelte';
  import MessageBubble from './MessageBubble.svelte';
  import MessageInput from './MessageInput.svelte';

  interface Props {
    workspaceId: string;
    onSend: (text: string) => void;
  }

  const { workspaceId, onSend }: Props = $props();

  const list = $derived(messages.listForWorkspace(workspaceId));
  const status = $derived(messages.statusFor(workspaceId));
  const inputDisabled = $derived(status === 'error' || status === 'stopped');
</script>

<section class="flex flex-col h-full bg-[var(--bg-base)]">
  <div class="flex-1 overflow-y-auto px-3 py-3 flex flex-col gap-2">
    {#if list.length === 0}
      <div
        class="flex-1 flex items-center justify-center text-sm text-[var(--text-muted)]"
      >
        Start the conversation — type a message below.
      </div>
    {:else}
      {#each list as msg (msg.id)}
        <MessageBubble message={msg} />
      {/each}
    {/if}
  </div>

  <MessageInput {onSend} disabled={inputDisabled} />
</section>
```

- [ ] **Step 6.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/chat/ChatPanel.test.ts 2>&1 | tail -10
```

Expected: 6 tests pass.

- [ ] **Step 6.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/components/chat/ChatPanel.svelte src/lib/components/chat/ChatPanel.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): add ChatPanel scrollback + bubble list + input

Reads MessagesStore via listForWorkspace, renders MessageBubble per
message, hosts MessageInput. Empty state prompts the user. Input is
disabled when agent status is error or stopped.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: `WorkspaceView.svelte` — mount: spawn, receive events

**Files:**

- Create: `src/lib/components/workspace/WorkspaceView.svelte`
- Create: `src/lib/components/workspace/WorkspaceView.test.ts`

- [ ] **Step 7.1: Write failing tests**

```typescript
// New file src/lib/components/workspace/WorkspaceView.test.ts:

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';
import WorkspaceView from './WorkspaceView.svelte';
import { messages } from '$lib/stores/messages.svelte';
import type { WorkspaceInfo } from '$lib/types';

const ws = (overrides: Partial<WorkspaceInfo> = {}): WorkspaceInfo => ({
  id: 'ws_a',
  repo_id: 'repo_a',
  title: 'Fix login bug',
  description: 'desc',
  branch: 'feat/x',
  base_branch: 'main',
  custom_branch: false,
  status: 'not_started',
  column: 'in_progress',
  created_at: 0,
  updated_at: 0,
  worktree_dir: '/tmp/ws_a',
  ...overrides,
});

const invokeMock = vi.fn();
const channels: Array<{ onmessage?: (ev: unknown) => void }> = [];

beforeEach(() => {
  messages.reset();
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(undefined);
  channels.length = 0;
  (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'] = {
    invoke: invokeMock,
    transformCallback: () => 0,
  };
});

afterEach(() => {
  delete (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'];
});

describe('WorkspaceView', () => {
  it('renders workspace title and branch in header', () => {
    const { getByText } = render(WorkspaceView, { props: { workspace: ws() } });
    expect(getByText('Fix login bug')).toBeTruthy();
    expect(getByText('feat/x')).toBeTruthy();
  });

  it('calls spawn_agent on mount when status is not_started', async () => {
    render(WorkspaceView, {
      props: { workspace: ws({ status: 'not_started' }) },
    });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'spawn_agent',
        expect.objectContaining({ workspaceId: 'ws_a' })
      );
    });
  });

  it('calls spawn_agent on mount when status is waiting', async () => {
    render(WorkspaceView, { props: { workspace: ws({ status: 'waiting' }) } });
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        'spawn_agent',
        expect.any(Object)
      );
    });
  });

  it('does not spawn_agent when status is running', async () => {
    render(WorkspaceView, { props: { workspace: ws({ status: 'running' }) } });
    // Wait a tick to ensure mount-time effects ran.
    await new Promise((r) => setTimeout(r, 10));
    expect(invokeMock).not.toHaveBeenCalledWith(
      'spawn_agent',
      expect.any(Object)
    );
  });

  it('renders ChatPanel', () => {
    const { getByLabelText } = render(WorkspaceView, {
      props: { workspace: ws() },
    });
    expect(getByLabelText(/message/i)).toBeTruthy();
  });

  it('forwards send to send_message backend', async () => {
    const { getByLabelText, getByRole } = render(WorkspaceView, {
      props: { workspace: ws() },
    });
    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    const ta = getByLabelText(/message/i) as HTMLTextAreaElement;
    const { fireEvent } = await import('@testing-library/svelte');
    await fireEvent.input(ta, { target: { value: 'Hello' } });
    await fireEvent.click(getByRole('button', { name: /send/i }));
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('send_message', {
        workspaceId: 'ws_a',
        text: 'Hello',
      });
    });
  });

  it('shows status pill reflecting the agent status', async () => {
    const { getByText } = render(WorkspaceView, { props: { workspace: ws() } });
    messages.apply({ type: 'status', status: 'running' }, 'ws_a');
    await waitFor(() => expect(getByText(/running/i)).toBeTruthy());
  });
});
```

- [ ] **Step 7.2: Run tests — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/workspace/WorkspaceView.test.ts 2>&1 | tail -15
```

Expected: error — `WorkspaceView.svelte` not found.

- [ ] **Step 7.3: Implement**

Create `src/lib/components/workspace/WorkspaceView.svelte`:

```svelte
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api, agentChannel } from '$lib/ipc';
  import { messages } from '$lib/stores/messages.svelte';
  import ChatPanel from '$lib/components/chat/ChatPanel.svelte';
  import type { AgentEvent, WorkspaceInfo } from '$lib/types';

  interface Props {
    workspace: WorkspaceInfo;
  }

  const { workspace }: Props = $props();

  const status = $derived(messages.statusFor(workspace.id) ?? workspace.status);

  let channel: ReturnType<typeof agentChannel> | undefined;

  onMount(async () => {
    if (workspace.status === 'not_started' || workspace.status === 'waiting') {
      channel = agentChannel();
      channel.onmessage = (ev: AgentEvent) => {
        messages.apply(ev, workspace.id);
      };
      try {
        await api.agent.spawn(workspace.id, channel);
      } catch (err) {
        messages.apply({ type: 'error', message: String(err) }, workspace.id);
      }
    }
  });

  onDestroy(() => {
    // Leave the agent running on workspace switch — Phase 1d may revisit.
    // The Channel's onmessage handler is GC'd when the component unmounts.
  });

  async function handleSend(text: string) {
    try {
      await api.agent.send(workspace.id, text);
    } catch (err) {
      messages.apply({ type: 'error', message: String(err) }, workspace.id);
    }
  }

  function statusLabel(s: string): string {
    if (s === 'running') return 'Running';
    if (s === 'waiting') return 'Waiting';
    if (s === 'error') return 'Error';
    if (s === 'stopped') return 'Stopped';
    return 'Idle';
  }
</script>

<section class="flex flex-col h-full">
  <header
    class="flex items-center justify-between px-4 py-2 border-b border-[var(--border)] bg-[var(--bg-sidebar)]"
  >
    <div class="flex flex-col">
      <h2 class="text-sm font-semibold text-[var(--text-primary)]">
        {workspace.title}
      </h2>
      <code class="text-xs text-[var(--text-muted)]">{workspace.branch}</code>
    </div>
    <span
      class="text-xs px-2 py-0.5 rounded bg-[var(--bg-card)] text-[var(--text-secondary)]"
      data-status={status}
      aria-label="Agent status"
    >
      {statusLabel(status)}
    </span>
  </header>

  <div class="flex-1 overflow-hidden">
    <ChatPanel workspaceId={workspace.id} onSend={handleSend} />
  </div>
</section>
```

- [ ] **Step 7.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/lib/components/workspace/WorkspaceView.test.ts 2>&1 | tail -10
```

Expected: 7 tests pass.

- [ ] **Step 7.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/lib/components/workspace/WorkspaceView.svelte src/lib/components/workspace/WorkspaceView.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): add WorkspaceView orchestrator (spawn + chat + status pill)

On mount, spawns agent if status is not_started/waiting, wires the
agent Channel to MessagesStore.apply. Header shows title + branch +
status pill. Hosts the ChatPanel below.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: Wire `WorkspaceView` into `App.svelte` Work mode

**Files:**

- Modify: `src/App.svelte`
- Modify: `src/App.test.ts`

- [ ] **Step 8.1: Write failing tests**

```typescript
// Append to src/App.test.ts:

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';
import App from './App.svelte';
import { workspaces } from '$lib/stores/workspaces.svelte';
import { repos } from '$lib/stores/repos.svelte';
import { modeStore } from '$lib/stores/mode.svelte';

const invokeMock = vi.fn();

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(undefined);
  (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'] = {
    invoke: invokeMock,
    transformCallback: () => 0,
  };
});

afterEach(() => {
  delete (window as unknown as Record<string, unknown>)['__TAURI_INTERNALS__'];
});

describe('App work mode', () => {
  it('renders WorkspaceView when work mode + selected workspace', async () => {
    repos.set([{ id: 'repo_a', name: 'Demo', path: '/x', updated_at: 0 }]);
    repos.select('repo_a');
    workspaces.set([
      {
        id: 'ws_a',
        repo_id: 'repo_a',
        title: 'Fix login',
        description: '',
        branch: 'feat/x',
        base_branch: 'main',
        custom_branch: false,
        status: 'running',
        column: 'in_progress',
        created_at: 0,
        updated_at: 0,
        worktree_dir: '/tmp/ws_a',
      },
    ]);
    workspaces.select('ws_a');
    modeStore.set('work');
    const { getByText } = render(App);
    await waitFor(() => expect(getByText('Fix login')).toBeTruthy());
  });

  it('falls back to "Select or create" when work mode but no workspace', async () => {
    workspaces.set([]);
    modeStore.set('work');
    const { getByText } = render(App);
    expect(getByText(/select or create/i)).toBeTruthy();
  });

  it('keeps Plan mode rendering KanbanBoard', async () => {
    modeStore.set('plan');
    repos.set([{ id: 'repo_a', name: 'Demo', path: '/x', updated_at: 0 }]);
    repos.select('repo_a');
    const { getByText } = render(App);
    await waitFor(() => expect(getByText(/Todo/)).toBeTruthy());
  });
});
```

> **Note:** the exact `repos.set(...)` / `workspaces.set(...)` / `select(...)`
> APIs depend on what those stores exposed in Phase 1a/1b. If `set` doesn't
> exist, replace with `workspaces.upsert(ws)` or whatever the public API is.

- [ ] **Step 8.2: Run tests — verify fail**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/App.test.ts 2>&1 | tail -15
```

Expected: tests fail because the work-mode placeholder still renders.

- [ ] **Step 8.3: Implement**

In `src/App.svelte`, replace the Work-mode placeholder block:

```svelte
{:else if selectedWorkspace}
  <section
    class="h-full flex flex-col items-center justify-center gap-2 text-[var(--text-secondary)]"
  >
    <p class="text-base font-semibold text-[var(--text-primary)]">
      Workspace: {selectedWorkspace.title}
    </p>
    <p class="text-xs text-[var(--text-muted)]">
      Branch: {selectedWorkspace.branch}
    </p>
    <p class="text-xs text-[var(--text-muted)]">Chat coming in Phase 1c.</p>
  </section>
```

with:

```svelte
{:else if selectedWorkspace}
  <WorkspaceView workspace={selectedWorkspace} />
```

And add the import at the top of `<script>`:

```svelte
import WorkspaceView from '$lib/components/workspace/WorkspaceView.svelte';
```

- [ ] **Step 8.4: Run tests — verify PASS**

```bash
cd /home/handokobeni/Work/ai-editor
bun run test src/App.test.ts 2>&1 | tail -10
bun run check 2>&1 | tail -5
```

Expected: tests pass, `bun run check` clean.

- [ ] **Step 8.5: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add src/App.svelte src/App.test.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
feat(phase-1c): wire WorkspaceView into App.svelte work mode

Replaces the placeholder block with the real WorkspaceView component.
Plan mode still renders KanbanBoard; the empty work-mode fallback
('Select or create a workspace') stays as before when no workspace is
selected.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Extend `tauri-shim` with agent mocks

**Files:**

- Modify: `tests/e2e/helpers/tauri-shim.ts`

- [ ] **Step 9.1: Implement (no Vitest tests; covered by E2E)**

Append to the `switch (cmd)` in `tauri-shim.ts`:

```typescript
case 'spawn_agent': {
  const wsId = args.workspaceId as string;
  const onEvent = args.onEvent as { onmessage?: (ev: unknown) => void };
  // Synchronously emit init + status running, then a fake assistant reply.
  setTimeout(() => onEvent.onmessage?.({
    type: 'init',
    session_id: 'ses_mock',
    model: 'claude-sonnet-4-6-mock',
  }), 0);
  setTimeout(() => onEvent.onmessage?.({
    type: 'status',
    status: 'running',
  }), 0);
  // Stash the channel so future send_message calls can echo back.
  state.agentChannels = state.agentChannels ?? {};
  state.agentChannels[wsId] = onEvent;
  // Mark workspace running.
  const ws = state.workspaces.find((w) => w.id === wsId);
  if (ws) ws.status = 'running';
  return undefined;
}

case 'send_message': {
  const wsId = args.workspaceId as string;
  const text = args.text as string;
  const onEvent = (state.agentChannels ?? {})[wsId];
  if (!onEvent) return undefined;
  // Fake echo reply after a tick.
  setTimeout(() => onEvent.onmessage?.({
    type: 'message',
    id: `msg_reply_${Date.now()}`,
    role: 'assistant',
    text: `Mock reply to: ${text}`,
    is_partial: false,
  }), 30);
  return undefined;
}

case 'stop_agent': {
  const wsId = args.workspaceId as string;
  const onEvent = (state.agentChannels ?? {})[wsId];
  if (onEvent) {
    setTimeout(() => onEvent.onmessage?.({
      type: 'status',
      status: 'stopped',
    }), 0);
    delete state.agentChannels[wsId];
  }
  const ws = state.workspaces.find((w) => w.id === wsId);
  if (ws) ws.status = 'waiting';
  return undefined;
}
```

Add
`agentChannels: undefined as Record<string, { onmessage?: (ev: unknown) => void }> | undefined,`
to the shim state shape (next to `workspaces`, `tasks`, etc.).

- [ ] **Step 9.2: Verify shim compiles**

```bash
cd /home/handokobeni/Work/ai-editor
bun run check 2>&1 | tail -5
```

Expected: 0 errors.

- [ ] **Step 9.3: Commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add tests/e2e/helpers/tauri-shim.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
test(phase-1c): extend tauri-shim with agent mocks (spawn/send/stop)

Mock spawn emits init + status:running on the provided onEvent channel.
Mock send replies with a 'Mock reply to: <text>' assistant message after
30ms. Mock stop emits status:stopped. Fully cookied so the chat E2E
can run without a real claude binary.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: E2E `tests/e2e/phase-1c/chat-flow.spec.ts`

**Files:**

- Create: `tests/e2e/phase-1c/chat-flow.spec.ts`

- [ ] **Step 10.1: Write the spec**

Create `tests/e2e/phase-1c/chat-flow.spec.ts`:

```typescript
import { test, expect } from '../helpers/fixtures';
import { installTauriShim } from '../helpers/tauri-shim';
import { execFileSync } from 'node:child_process';
import * as path from 'node:path';
import * as fs from 'node:fs';
import * as os from 'node:os';

let FIXTURE_REPO_PATH: string;

test.beforeAll(() => {
  const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'ansambel-e2e-chat-'));
  FIXTURE_REPO_PATH = tmpDir;
  execFileSync('git', ['init', '--initial-branch=main'], { cwd: tmpDir });
  execFileSync('git', ['config', 'user.email', 't@e.com'], { cwd: tmpDir });
  execFileSync('git', ['config', 'user.name', 'T'], { cwd: tmpDir });
  execFileSync('git', ['commit', '--allow-empty', '-m', 'init'], {
    cwd: tmpDir,
  });
});

test.afterAll(() => {
  if (FIXTURE_REPO_PATH && fs.existsSync(FIXTURE_REPO_PATH)) {
    fs.rmSync(FIXTURE_REPO_PATH, { recursive: true, force: true });
  }
});

test('Chat: drag task → switch to work mode → send message → see reply', async ({
  page,
  harness,
}) => {
  void harness;
  await installTauriShim(page, { dialogOpenPath: FIXTURE_REPO_PATH });
  await page.goto('/');

  // Add the repo
  await page.waitForSelector('header', { timeout: 10000 });
  await page.getByRole('button', { name: /add repo/i }).click();
  const repoName = path.basename(FIXTURE_REPO_PATH);
  await expect(page.getByText(repoName)).toBeVisible({ timeout: 8000 });

  // Add task in Plan mode
  await page.getByRole('button', { name: /add task/i }).click();
  await page.getByLabel(/title/i).fill('E2E chat task');
  await page.getByLabel(/description/i).fill('Test the chat flow');
  await page.getByRole('button', { name: /^add task$/i }).click();

  // Synthetic finalize → drag Todo → In Progress (auto-create workspace)
  await page.evaluate(async () => {
    type Internals = {
      invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
    };
    const internals = (window as unknown as Record<string, unknown>)[
      '__TAURI_INTERNALS__'
    ] as Internals | undefined;
    if (!internals) return;
    const tasks = (await internals.invoke('list_tasks', {})) as Array<{
      id: string;
      title: string;
    }>;
    const task = tasks.find((t) => t.title === 'E2E chat task');
    if (!task) return;
    const zone = document.querySelector(
      '[data-column="in_progress"]'
    ) as HTMLElement | null;
    zone?.dispatchEvent(
      new CustomEvent('finalize', {
        detail: {
          items: [{ ...task, column: 'in_progress', order: 0 }],
          info: { id: task.id },
        },
        bubbles: true,
      })
    );
  });
  await page.waitForTimeout(500);

  // Click the auto-created workspace in the sidebar
  const sidebar = page.locator('aside').first();
  await sidebar.getByText('E2E chat task').click();

  // Switch to Work mode
  await page.getByRole('button', { name: /work/i }).click();

  // WorkspaceView should be visible (header has title + branch)
  await expect(
    page.getByRole('heading', { name: 'E2E chat task' })
  ).toBeVisible({
    timeout: 5000,
  });

  // Status pill should say "Running" after spawn (mock emits status:running)
  await expect(page.getByText(/running/i)).toBeVisible({ timeout: 5000 });

  // Send a message
  const textarea = page.getByLabel(/message/i);
  await textarea.fill('hello claude');
  await page.getByRole('button', { name: /send/i }).click();

  // Mock replies with "Mock reply to: hello claude" after 30ms
  await expect(page.getByText('Mock reply to: hello claude')).toBeVisible({
    timeout: 5000,
  });
});
```

- [ ] **Step 10.2: Run E2E spec**

```bash
cd /home/handokobeni/Work/ai-editor
bun run e2e tests/e2e/phase-1c/chat-flow.spec.ts 2>&1 | tail -20
```

Expected: spec passes (1 test).

- [ ] **Step 10.3: Tag release + commit**

```bash
cd /home/handokobeni/Work/ai-editor
git add tests/e2e/phase-1c/chat-flow.spec.ts
git -c user.name="handokobeni" -c user.email="benihandoko@student.upi.edu" commit -m "$(cat <<'EOF'
test(phase-1c): add E2E chat-flow spec covering full work mode loop

Drives the golden path: add repo → add task → drag to In Progress →
click workspace in sidebar → switch to Work mode → send message →
assert mock reply visible. Uses the agent shim mocks to avoid a real
claude binary.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"

git tag v0.4.0-phase1c -m "Phase 1c complete — chat + agent + worktree"
git push --tags
```

---

## Final verification

```bash
cd /home/handokobeni/Work/ai-editor
bun run lint 2>&1 | tail -5
bun run check 2>&1 | tail -5
bun run test 2>&1 | tail -5
bun run e2e 2>&1 | tail -10
cd src-tauri && cargo fmt --all -- --check && cargo clippy --lib --all-targets -- -D warnings 2>&1 | tail -5
cd .. && cd src-tauri && cargo test --lib 2>&1 | tail -5
```

All six gates green.

Coverage targets: ≥95% line + branch on changed files (MessagesStore, ChatPanel,
MessageBubble, MessageInput, WorkspaceView, ipc.ts agent additions). The store +
components are pure / easy; the WorkspaceView's onMount async path is the
trickiest — its tests cover the happy path; `onDestroy` is intentionally empty.

---

## Out of scope (Phase 2+)

- xterm.js terminal panel
- Diff viewer (Git diff against base branch)
- File browser tree
- Search modal (`⌘K`) for files + content grep
- @-file mention autocomplete in MessageInput
- Tool-use expandable detail UI
- Streaming partial chunks (relies on backend emitting `is_partial: true` during
  a turn — see Phase 1c-backend out-of-scope)

---

## Done criteria

- All 10 tasks complete; all tests green.
- The user can: add a repo → add a task → drag to In Progress → click workspace
  in sidebar → switch to Work mode → see the chat panel → send a message →
  receive a reply.
- E2E `chat-flow.spec.ts` passes on Linux + Windows.
- `v0.4.0-phase1c` tag pushed.
