import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, waitFor } from '@testing-library/svelte';

// Mock @tauri-apps/api/core before importing anything that depends on it
vi.mock('@tauri-apps/api/core', () => {
  class MockChannel {
    id = Math.random();
    onmessage?: (ev: unknown) => void;
  }

  return {
    invoke: vi.fn(),
    Channel: MockChannel,
    // MessageInput's attachment chip uses convertFileSrc — stub it so the
    // <img src=…> preview doesn't crash in jsdom.
    convertFileSrc: (path: string) => `mock-asset://${path}`,
  };
});

// MessageInput pulls in @tauri-apps/plugin-dialog for the file picker. The
// real plugin requires the Tauri runtime, so stub it.
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));

// xterm.js / FitAddon are heavy ESM modules with canvas/measurement
// expectations jsdom can't satisfy. Mock the parts the Terminal component
// actually calls.
vi.mock('@xterm/xterm', () => {
  class MockTerminal {
    cols = 80;
    rows = 24;
    open = vi.fn();
    write = vi.fn();
    writeln = vi.fn();
    loadAddon = vi.fn();
    onData = vi.fn();
    dispose = vi.fn();
  }
  return { Terminal: MockTerminal };
});

vi.mock('@xterm/addon-fit', () => {
  class MockFitAddon {
    fit = vi.fn();
    activate = vi.fn();
    dispose = vi.fn();
  }
  return { FitAddon: MockFitAddon };
});

// Fire once with a synthetic non-zero contentRect on observe() so
// Terminal's production waitForLayout helper unblocks immediately —
// without it onMount blocks for 500 ms in every test.
type RoCallback = (entries: ResizeObserverEntry[], observer: ResizeObserver) => void;
class FiringResizeObserver {
  cb: RoCallback;
  constructor(cb: RoCallback) {
    this.cb = cb;
  }
  observe(target: Element): void {
    queueMicrotask(() => {
      const entry = {
        target,
        contentRect: { width: 800, height: 600 } as DOMRectReadOnly,
      } as ResizeObserverEntry;
      this.cb([entry], this as unknown as ResizeObserver);
    });
  }
  unobserve(): void {}
  disconnect(): void {}
}
vi.stubGlobal('ResizeObserver', FiringResizeObserver);

import { invoke } from '@tauri-apps/api/core';
import WorkspaceView from './WorkspaceView.svelte';
import { messages } from '$lib/stores/messages.svelte';
import { editorTabs } from '$lib/stores/editor-tabs.svelte';
import { workspaceTabs } from '$lib/stores/workspace-tabs.svelte';
import { workspaces } from '$lib/stores/workspaces.svelte';
import { getToasts, removeToast } from '$lib/stores/toasts.svelte';
import type { WorkspaceInfo } from '$lib/types';

function clearToasts(): void {
  for (const id of Array.from(getToasts().keys())) removeToast(id);
}

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
  task_id: null,
  ...overrides,
});

beforeEach(() => {
  messages.reset();
  editorTabs.reset();
  workspaceTabs.reset();
  // No public reset on the workspaces store; clear it in place so the
  // optimistic-toggle test starts from a clean slate.
  for (const repoMap of workspaces.byRepo.values()) repoMap.clear();
  workspaces.byRepo.clear();
  workspaces.selectedWorkspaceId = null;
  clearToasts();
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockResolvedValue(undefined);
});

afterEach(() => {
  vi.clearAllMocks();
  clearToasts();
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
      expect(invoke).toHaveBeenCalledWith(
        'spawn_agent',
        expect.objectContaining({ workspaceId: 'ws_a' })
      );
    });
  });

  it('calls spawn_agent on mount when status is waiting', async () => {
    render(WorkspaceView, { props: { workspace: ws({ status: 'waiting' }) } });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('spawn_agent', expect.any(Object));
    });
  });

  it('does not spawn_agent when status is running', async () => {
    render(WorkspaceView, { props: { workspace: ws({ status: 'running' }) } });
    await new Promise((r) => setTimeout(r, 10));
    expect(invoke).not.toHaveBeenCalledWith('spawn_agent', expect.any(Object));
  });

  it('calls reattach_agent on mount when status is running', async () => {
    render(WorkspaceView, { props: { workspace: ws({ status: 'running' }) } });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'reattach_agent',
        expect.objectContaining({ workspaceId: 'ws_a' })
      );
    });
  });

  it('reattaches (does not respawn) on remount when messages store shows running', async () => {
    // Repro: first mount with not_started prop spawns; status flips to
    // running via events; component unmounts on tab switch; remounts with
    // the same stale prop. The agent is still alive on the backend, so
    // we must reattach rather than spawn (which would error with
    // "agent already running").
    messages.apply({ type: 'status', status: 'running' }, 'ws_a');
    render(WorkspaceView, { props: { workspace: ws({ status: 'not_started' }) } });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'reattach_agent',
        expect.objectContaining({ workspaceId: 'ws_a' })
      );
    });
    expect(invoke).not.toHaveBeenCalledWith('spawn_agent', expect.any(Object));
  });

  it('does not stamp status synchronously before spawn (no premature turn indicator)', async () => {
    // Regression: an earlier fix stamped 'running' optimistically before
    // awaiting spawn to guard against rapid remount. That had a UX bug —
    // the messages store auto-creates a TurnState on status flip to
    // 'running', so the chat showed a "Crunching… (elapsed)" indicator
    // before the user had sent any prompt. The fix moved idempotency to
    // the backend (spawn_agent downgrades to reattach when the agent is
    // already alive), so the stamp is no longer needed. This test pins
    // the no-stamp behavior so it isn't reintroduced.
    let resolveSpawn: () => void = () => {};
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === 'spawn_agent') {
        return new Promise<void>((res) => {
          resolveSpawn = res;
        });
      }
      return undefined;
    });
    render(WorkspaceView, { props: { workspace: ws({ status: 'waiting' }) } });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('spawn_agent', expect.any(Object));
    });
    // Status should NOT be stamped before spawn resolves and the backend
    // broadcasts Status::Running through the channel.
    expect(messages.statusFor('ws_a')).toBeUndefined();
    resolveSpawn();
  });

  it('routes reattach channel events through messages.apply', async () => {
    let captured: { onmessage?: (ev: unknown) => void } | undefined;
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (cmd === 'reattach_agent') {
        captured = (args as { onEvent: { onmessage?: (ev: unknown) => void } }).onEvent;
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
      expect(messages.listForWorkspace('ws_a').find((m) => m.id === 'msg_live')?.text).toBe('live');
    });
  });

  it('captures reattach rejection as error in messages store', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === 'reattach_agent') throw 'no agent for workspace ws_a';
      return undefined;
    });
    render(WorkspaceView, { props: { workspace: ws({ status: 'running' }) } });
    await waitFor(() => {
      expect(messages.errorFor('ws_a')).toBe('no agent for workspace ws_a');
    });
  });

  // ── Phase 3a-3 Task 18: team-activity privacy toggle ─────────────
  describe('team-activity privacy toggle', () => {
    it('renders the toggle in the header with the public label by default', async () => {
      const { findByTestId } = render(WorkspaceView, { props: { workspace: ws() } });
      const btn = await findByTestId('team-activity-privacy-toggle');
      expect(btn).toBeTruthy();
      expect(btn.getAttribute('data-private')).toBe('false');
      expect(btn.textContent).toMatch(/Public/);
    });

    it('reflects the workspace prop when no store entry exists', async () => {
      const { findByTestId } = render(WorkspaceView, {
        props: { workspace: ws({ team_activity_private: true }) },
      });
      const btn = await findByTestId('team-activity-privacy-toggle');
      expect(btn.getAttribute('data-private')).toBe('true');
      expect(btn.textContent).toMatch(/Private/);
    });

    it('clicking the toggle invokes set_workspace_team_activity_private with isPrivate=true', async () => {
      // The store's optimistic-toggle path only fires the IPC when the
      // workspace is registered in the SvelteMap — mirror the real
      // workflow by seeding it before render.
      (workspaces.byRepo as unknown as Map<string, Map<string, WorkspaceInfo>>).set(
        'repo_a',
        new Map([['ws_a', ws({ team_activity_private: false })]])
      );
      const { findByTestId } = render(WorkspaceView, { props: { workspace: ws() } });
      const btn = await findByTestId('team-activity-privacy-toggle');
      const { fireEvent } = await import('@testing-library/svelte');
      await fireEvent.click(btn);
      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith(
          'set_workspace_team_activity_private',
          expect.objectContaining({ workspaceId: 'ws_a', isPrivate: true })
        );
      });
    });

    it('optimistically flips the button label before the IPC settles', async () => {
      // Seed the store with the workspace so the optimistic update has
      // somewhere to land.
      const repoMap = new (workspaces.byRepo.constructor as new () => Map<string, WorkspaceInfo>)();
      const seeded = ws({ team_activity_private: false });
      // svelte/reactivity SvelteMap accepts .set the same way.
      (workspaces.byRepo as unknown as Map<string, Map<string, WorkspaceInfo>>).set(
        'repo_a',
        repoMap as unknown as Map<string, WorkspaceInfo>
      );
      (repoMap as unknown as Map<string, WorkspaceInfo>).set('ws_a', seeded);

      let resolve!: () => void;
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === 'set_workspace_team_activity_private') {
          return new Promise<void>((r) => {
            resolve = r;
          });
        }
        if (cmd === 'list_messages') return [];
        return undefined;
      });

      const { findByTestId } = render(WorkspaceView, { props: { workspace: seeded } });
      const btn = await findByTestId('team-activity-privacy-toggle');
      const { fireEvent } = await import('@testing-library/svelte');
      await fireEvent.click(btn);
      // While the IPC is in flight, the label should already read "Private".
      await waitFor(() => {
        expect(btn.getAttribute('data-private')).toBe('true');
      });
      resolve();
    });

    it('reverts label and shows a toast when the IPC rejects', async () => {
      // Seed the store first.
      (workspaces.byRepo as unknown as Map<string, Map<string, WorkspaceInfo>>).set(
        'repo_a',
        new Map([['ws_a', ws({ team_activity_private: false })]])
      );

      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === 'set_workspace_team_activity_private') throw 'write fail';
        if (cmd === 'list_messages') return [];
        return undefined;
      });

      const { findByTestId } = render(WorkspaceView, {
        props: { workspace: ws({ team_activity_private: false }) },
      });
      const btn = await findByTestId('team-activity-privacy-toggle');
      const { fireEvent } = await import('@testing-library/svelte');
      await fireEvent.click(btn);
      await waitFor(() => {
        const toasts = Array.from(getToasts().values());
        expect(toasts.some((t) => t.message.includes('write fail'))).toBe(true);
      });
      expect(btn.getAttribute('data-private')).toBe('false');
    });
  });

  describe('Stop button', () => {
    it('renders Stop button when status is running', () => {
      messages.apply({ type: 'status', status: 'running' }, 'ws_a');
      const { getByRole } = render(WorkspaceView, { props: { workspace: ws() } });
      expect(getByRole('button', { name: /stop/i })).toBeTruthy();
    });

    it('does not render Stop button when status is waiting', async () => {
      messages.apply({ type: 'status', status: 'waiting' }, 'ws_a');
      const { queryByRole } = render(WorkspaceView, {
        props: { workspace: ws({ status: 'waiting' }) },
      });
      await new Promise((r) => setTimeout(r, 5));
      expect(queryByRole('button', { name: /stop/i })).toBeNull();
    });

    it('clicking Stop calls stop_agent', async () => {
      messages.apply({ type: 'status', status: 'running' }, 'ws_a');
      const { getByRole } = render(WorkspaceView, { props: { workspace: ws() } });
      const { fireEvent } = await import('@testing-library/svelte');
      await fireEvent.click(getByRole('button', { name: /stop/i }));
      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith('stop_agent', { workspaceId: 'ws_a' });
      });
    });

    it('disables Stop button while stop_agent is in flight', async () => {
      messages.apply({ type: 'status', status: 'running' }, 'ws_a');
      let resolveStop!: () => void;
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === 'stop_agent') {
          return new Promise<void>((r) => {
            resolveStop = r as () => void;
          });
        }
        return undefined;
      });
      const { getByRole } = render(WorkspaceView, { props: { workspace: ws() } });
      const { fireEvent } = await import('@testing-library/svelte');
      const btn = getByRole('button', { name: /stop/i }) as HTMLButtonElement;
      await fireEvent.click(btn);
      await waitFor(() => expect(btn.disabled).toBe(true));
      resolveStop();
    });

    it('captures stop_agent rejection as error in messages store', async () => {
      messages.apply({ type: 'status', status: 'running' }, 'ws_a');
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === 'stop_agent') throw 'stop failed';
        return undefined;
      });
      const { getByRole } = render(WorkspaceView, { props: { workspace: ws() } });
      const { fireEvent } = await import('@testing-library/svelte');
      await fireEvent.click(getByRole('button', { name: /stop/i }));
      await waitFor(() => {
        expect(messages.errorFor('ws_a')).toBe('stop failed');
      });
    });
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
    await waitFor(() => expect(invoke).toHaveBeenCalled());
    const ta = getByLabelText(/message/i) as HTMLTextAreaElement;
    const { fireEvent } = await import('@testing-library/svelte');
    await fireEvent.input(ta, { target: { value: 'Hello' } });
    await fireEvent.click(getByRole('button', { name: /send/i }));
    // Plain text turns get attachments=null on the wire (the backend treats
    // null and an empty list the same way).
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('send_message', {
        workspaceId: 'ws_a',
        text: 'Hello',
        attachments: null,
      });
    });
  });

  it('echoes user message with preview attachments when drafts are provided', async () => {
    // Drive ChatPanel by calling api.agent.send through the WorkspaceView's
    // handleSend pathway. The MessageInput is exercised in its own suite —
    // here we hand a fake draft straight to the wired-up handler by
    // instrumenting invoke. We intercept the send_message call to confirm
    // attachments flow through, and verify the echoed user Message in the
    // store carries the preview attachment rendered before the backend reply.
    const sentArgs: Record<string, unknown> = {};
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (cmd === 'send_message') {
        Object.assign(sentArgs, args);
      }
      if (cmd === 'list_messages') return [];
      return undefined;
    });
    // Force a draft into MessageInput by stubbing its file picker.
    const { open } = await import('@tauri-apps/plugin-dialog');
    vi.mocked(open).mockResolvedValue('/home/u/pic.png');

    const { getByLabelText, getByRole, getByTestId, findByTestId } = render(WorkspaceView, {
      props: { workspace: ws() },
    });
    await waitFor(() => expect(invoke).toHaveBeenCalled());
    const { fireEvent } = await import('@testing-library/svelte');
    await fireEvent.click(getByTestId('attach-button'));
    // chip appears once the picker resolves.
    await findByTestId('attachment-chip');
    const ta = getByLabelText(/message/i) as HTMLTextAreaElement;
    await fireEvent.input(ta, { target: { value: 'see this' } });
    await fireEvent.click(getByRole('button', { name: /send/i }));

    await waitFor(() => {
      expect(sentArgs.text).toBe('see this');
      expect(Array.isArray(sentArgs.attachments)).toBe(true);
      expect((sentArgs.attachments as Array<unknown>).length).toBe(1);
      expect(sentArgs.attachments).toEqual([
        { sourcePath: '/home/u/pic.png', mediaType: 'image/png', filename: 'pic.png' },
      ]);
    });

    const userEcho = messages.listForWorkspace('ws_a').find((m) => m.role === 'user');
    expect(userEcho?.text).toBe('see this');
    expect(userEcho?.attachments?.[0]).toEqual({
      kind: 'image',
      media_type: 'image/png',
      path: '/home/u/pic.png',
      filename: 'pic.png',
    });
  });

  it('echoes user message to messages store immediately on send', async () => {
    const { getByLabelText, getByRole } = render(WorkspaceView, {
      props: { workspace: ws() },
    });
    await waitFor(() => expect(invoke).toHaveBeenCalled());
    const ta = getByLabelText(/message/i) as HTMLTextAreaElement;
    const { fireEvent } = await import('@testing-library/svelte');
    await fireEvent.input(ta, { target: { value: 'Hello user' } });
    await fireEvent.click(getByRole('button', { name: /send/i }));
    await waitFor(() => {
      const list = messages.listForWorkspace('ws_a');
      const userMsg = list.find((m) => m.role === 'user');
      expect(userMsg?.text).toBe('Hello user');
    });
  });

  it('shows status pill reflecting the agent status', async () => {
    const { getByText } = render(WorkspaceView, { props: { workspace: ws() } });
    messages.apply({ type: 'status', status: 'running' }, 'ws_a');
    await waitFor(() => expect(getByText(/running/i)).toBeTruthy());
  });

  it('captures spawn rejection as error in messages store', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === 'spawn_agent') throw 'spawn failed';
      return undefined;
    });
    render(WorkspaceView, {
      props: { workspace: ws({ status: 'not_started' }) },
    });
    await waitFor(() => {
      expect(messages.errorFor('ws_a')).toBe('spawn failed');
    });
  });

  it('renders Settings CTA when spawn fails with claude-binary-not-found', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === 'spawn_agent') throw 'spawn_agent: claude binary not found';
      return undefined;
    });
    const { findByTestId } = render(WorkspaceView, {
      props: { workspace: ws({ status: 'not_started' }) },
    });
    const cta = await findByTestId('settings-cta');
    expect(cta).toBeTruthy();
    expect((cta as HTMLAnchorElement).getAttribute('href')).toContain('settings');
  });

  it('does NOT render Settings CTA for unrelated spawn errors', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === 'spawn_agent') throw 'spawn_agent: random unrelated thing';
      return undefined;
    });
    const { queryByTestId } = render(WorkspaceView, {
      props: { workspace: ws({ status: 'not_started' }) },
    });
    // Wait for the rejection to flow into messages store and re-render.
    await waitFor(() => {
      expect(messages.errorFor('ws_a')).toContain('random unrelated thing');
    });
    expect(queryByTestId('settings-cta')).toBeNull();
  });

  it('re-spawns the agent before sending when status is stopped', async () => {
    const calls: string[] = [];
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      calls.push(cmd);
      if (cmd === 'list_messages') return [];
      return undefined;
    });
    const { getByLabelText, getByRole } = render(WorkspaceView, {
      props: { workspace: ws({ status: 'running' }) },
    });
    await waitFor(() => expect(calls).toContain('reattach_agent'));
    // Simulate Stop arriving via the channel.
    messages.apply({ type: 'status', status: 'stopped' }, 'ws_a');
    // User types the next prompt. Send should re-spawn first, then send.
    const ta = getByLabelText(/message/i) as HTMLTextAreaElement;
    const { fireEvent } = await import('@testing-library/svelte');
    await fireEvent.input(ta, { target: { value: 'next turn' } });
    await fireEvent.click(getByRole('button', { name: /send/i }));
    await waitFor(() => {
      expect(calls).toContain('spawn_agent');
      expect(calls).toContain('send_message');
    });
    const spawnIdx = calls.lastIndexOf('spawn_agent');
    const sendIdx = calls.lastIndexOf('send_message');
    expect(spawnIdx).toBeLessThan(sendIdx);
  });

  it('does not re-spawn when status is already running', async () => {
    let spawnCalls = 0;
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === 'list_messages') return [];
      if (cmd === 'spawn_agent') spawnCalls += 1;
      return undefined;
    });
    const { getByLabelText, getByRole } = render(WorkspaceView, {
      props: { workspace: ws({ status: 'running' }) },
    });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('reattach_agent', expect.any(Object)));
    const ta = getByLabelText(/message/i) as HTMLTextAreaElement;
    const { fireEvent } = await import('@testing-library/svelte');
    await fireEvent.input(ta, { target: { value: 'live turn' } });
    await fireEvent.click(getByRole('button', { name: /send/i }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        'send_message',
        expect.objectContaining({ text: 'live turn' })
      )
    );
    expect(spawnCalls).toBe(0);
  });

  it('captures send_message rejection as error in messages store', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === 'send_message') throw 'send failed';
      return undefined;
    });
    const { getByLabelText, getByRole } = render(WorkspaceView, {
      props: { workspace: ws() },
    });
    await waitFor(() => expect(invoke).toHaveBeenCalled());
    const ta = getByLabelText(/message/i) as HTMLTextAreaElement;
    const { fireEvent } = await import('@testing-library/svelte');
    await fireEvent.input(ta, { target: { value: 'Hello' } });
    await fireEvent.click(getByRole('button', { name: /send/i }));
    await waitFor(() => {
      expect(messages.errorFor('ws_a')).toBe('send failed');
    });
  });

  it('routes channel onmessage events through messages.apply', async () => {
    let capturedChannel: { onmessage?: (ev: unknown) => void } | undefined;
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (cmd === 'spawn_agent') {
        capturedChannel = (args as unknown as { onEvent: { onmessage?: (ev: unknown) => void } })
          .onEvent;
      }
      return undefined;
    });
    render(WorkspaceView, {
      props: { workspace: ws({ status: 'not_started' }) },
    });
    await waitFor(() => expect(capturedChannel).toBeDefined());
    // Fire a message event through the channel
    capturedChannel?.onmessage?.({
      type: 'message',
      id: 'msg_x',
      role: 'assistant',
      text: 'streamed reply',
      is_partial: false,
    });
    await waitFor(() => {
      const list = messages.listForWorkspace('ws_a');
      expect(list.find((m) => m.id === 'msg_x')?.text).toBe('streamed reply');
    });
  });

  it('hydrates persisted message history on mount', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === 'list_messages') {
        return [
          {
            id: 'msg_h1',
            workspace_id: 'ws_a',
            role: 'user',
            text: 'old user',
            is_partial: false,
            tool_use: null,
            tool_result: null,
            created_at: 100,
          },
          {
            id: 'msg_h2',
            workspace_id: 'ws_a',
            role: 'assistant',
            text: 'old reply',
            is_partial: false,
            tool_use: null,
            tool_result: null,
            created_at: 200,
          },
        ];
      }
      return undefined;
    });
    render(WorkspaceView, { props: { workspace: ws() } });
    await waitFor(() => {
      const list = messages.listForWorkspace('ws_a');
      expect(list).toHaveLength(2);
      expect(list[0].text).toBe('old user');
      expect(list[1].text).toBe('old reply');
    });
  });

  it('calls list_messages on mount before spawn_agent', async () => {
    const callOrder: string[] = [];
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      callOrder.push(cmd);
      if (cmd === 'list_messages') return [];
      return undefined;
    });
    render(WorkspaceView, {
      props: { workspace: ws({ status: 'not_started' }) },
    });
    await waitFor(() => {
      expect(callOrder).toContain('spawn_agent');
    });
    const listIdx = callOrder.indexOf('list_messages');
    const spawnIdx = callOrder.indexOf('spawn_agent');
    expect(listIdx).toBeGreaterThanOrEqual(0);
    expect(listIdx).toBeLessThan(spawnIdx);
  });

  it('captures list_messages rejection as error in messages store', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === 'list_messages') throw 'history load failed';
      return undefined;
    });
    render(WorkspaceView, { props: { workspace: ws() } });
    await waitFor(() => {
      expect(messages.errorFor('ws_a')).toBe('history load failed');
    });
  });

  it('still spawns the agent when history load fails', async () => {
    vi.mocked(invoke).mockImplementation(async (cmd) => {
      if (cmd === 'list_messages') throw 'load fail';
      return undefined;
    });
    render(WorkspaceView, {
      props: { workspace: ws({ status: 'not_started' }) },
    });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'spawn_agent',
        expect.objectContaining({ workspaceId: 'ws_a' })
      );
    });
  });

  it('passes onLoadEarlier callback that invokes list_messages with beforeId', async () => {
    let listCallCount = 0;
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (cmd === 'list_messages') {
        listCallCount += 1;
        if (listCallCount === 1) {
          // initial hydration — populate one message so Load earlier appears
          return [
            {
              id: 'msg_recent',
              workspace_id: 'ws_a',
              role: 'user',
              text: 'recent',
              is_partial: false,
              tool_use: null,
              tool_result: null,
              created_at: 200,
            },
          ];
        }
        // subsequent paginated call
        expect((args as { beforeId?: string }).beforeId).toBe('msg_recent');
        return [];
      }
      return undefined;
    });
    const { findByTestId } = render(WorkspaceView, { props: { workspace: ws() } });
    const btn = await findByTestId('load-earlier-button');
    const { fireEvent } = await import('@testing-library/svelte');
    await fireEvent.click(btn);
    await waitFor(() => {
      expect(listCallCount).toBeGreaterThanOrEqual(2);
    });
  });

  // ── Phase 2a: tab integration ─────────────────────────────────────

  describe('tab strip integration', () => {
    it('renders the tab strip with all five tabs', async () => {
      const { findByTestId } = render(WorkspaceView, { props: { workspace: ws() } });
      expect(await findByTestId('tab-strip')).toBeTruthy();
      expect(await findByTestId('tab-chat')).toBeTruthy();
      expect(await findByTestId('tab-diff')).toBeTruthy();
      expect(await findByTestId('tab-files')).toBeTruthy();
      expect(await findByTestId('tab-editor')).toBeTruthy();
      expect(await findByTestId('tab-terminal')).toBeTruthy();
    });

    it('defaults to the chat tab on first open', async () => {
      const { findByTestId } = render(WorkspaceView, { props: { workspace: ws() } });
      const chat = await findByTestId('tab-chat');
      expect(chat.getAttribute('aria-selected')).toBe('true');
    });

    it('clicking the diff tab makes the diff panel visible', async () => {
      const { findByTestId, container } = render(WorkspaceView, { props: { workspace: ws() } });
      const diffTab = await findByTestId('tab-diff');
      const { fireEvent } = await import('@testing-library/svelte');
      await fireEvent.click(diffTab);
      await waitFor(() => expect(diffTab.getAttribute('aria-selected')).toBe('true'));
      const diffPanel = container.querySelector('#tabpanel-diff');
      expect(diffPanel?.classList.contains('hidden')).toBe(false);
      const chatPanel = container.querySelector('#tabpanel-chat');
      // ChatPanel stays in DOM but is hidden — proves the hidden-mount
      // pattern preserves chat scroll position when the user pops away.
      expect(chatPanel).not.toBeNull();
      expect(chatPanel?.classList.contains('hidden')).toBe(true);
    });

    it('clicking the files tab makes the files panel visible', async () => {
      const { findByTestId, container } = render(WorkspaceView, { props: { workspace: ws() } });
      const filesTab = await findByTestId('tab-files');
      const { fireEvent } = await import('@testing-library/svelte');
      await fireEvent.click(filesTab);
      await waitFor(() => expect(filesTab.getAttribute('aria-selected')).toBe('true'));
      const filesPanel = container.querySelector('#tabpanel-files');
      expect(filesPanel?.classList.contains('hidden')).toBe(false);
    });

    it('the editor panel is mounted but hidden when other tabs are active', async () => {
      const { container } = render(WorkspaceView, { props: { workspace: ws() } });
      const editorPanel = container.querySelector('#tabpanel-editor');
      // Editor panel must always be in the DOM — its CodeMirror instance
      // and undo history are too expensive to throw away on tab switch.
      expect(editorPanel).not.toBeNull();
      expect(editorPanel?.classList.contains('hidden')).toBe(true);
    });

    it('the terminal panel is mounted but hidden when other tabs are active', async () => {
      const { container } = render(WorkspaceView, { props: { workspace: ws() } });
      const terminalPanel = container.querySelector('#tabpanel-terminal');
      // Same hidden-mount rule — xterm.js scrollback survives tab switches.
      expect(terminalPanel).not.toBeNull();
      expect(terminalPanel?.classList.contains('hidden')).toBe(true);
    });
  });

  // ── Phase 2b: file-open wiring ────────────────────────────────────

  describe('FileBrowser onOpen wiring', () => {
    it('reads file content via api.file.read when the user clicks a file', async () => {
      vi.mocked(invoke).mockImplementation(async (cmd, args) => {
        if (cmd === 'list_messages') return [];
        if (cmd === 'workspace_files') return [{ name: 'a.ts', path: 'a.ts', kind: 'file' }];
        if (cmd === 'file_read' && args) {
          return {
            content: 'console.log(1);',
            is_binary: false,
            size: 16,
            sha1: 'sha-orig',
          };
        }
        return undefined;
      });
      const { findByTestId, container } = render(WorkspaceView, { props: { workspace: ws() } });
      // Switch to files tab so the FileBrowser renders rows.
      const filesTab = await findByTestId('tab-files');
      const { fireEvent } = await import('@testing-library/svelte');
      await fireEvent.click(filesTab);
      // Wait for the FileBrowser's first row.
      const row = await waitFor(() => {
        const r = container.querySelector('[data-testid="file-row"][data-path="a.ts"] button');
        if (!r) throw new Error('row not found');
        return r;
      });
      await fireEvent.click(row as Element);
      await waitFor(() => {
        expect(invoke).toHaveBeenCalledWith('file_read', { workspaceId: 'ws_a', path: 'a.ts' });
      });
    });

    it('after a successful read, the file is open in the editor-tabs store', async () => {
      vi.mocked(invoke).mockImplementation(async (cmd, args) => {
        if (cmd === 'list_messages') return [];
        if (cmd === 'workspace_files') return [{ name: 'a.ts', path: 'a.ts', kind: 'file' }];
        if (cmd === 'file_read' && args) {
          return { content: 'hi', is_binary: false, size: 2, sha1: 'sha-a' };
        }
        return undefined;
      });
      const { findByTestId, container } = render(WorkspaceView, { props: { workspace: ws() } });
      const filesTab = await findByTestId('tab-files');
      const { fireEvent } = await import('@testing-library/svelte');
      await fireEvent.click(filesTab);
      const row = await waitFor(() => {
        const r = container.querySelector('[data-testid="file-row"][data-path="a.ts"] button');
        if (!r) throw new Error('row not found');
        return r;
      });
      await fireEvent.click(row as Element);
      await waitFor(() => {
        const open = editorTabs.open('ws_a');
        expect(open.find((f) => f.path === 'a.ts')?.content).toBe('hi');
      });
    });

    it('after a successful read, the active tab switches to editor', async () => {
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === 'list_messages') return [];
        if (cmd === 'workspace_files') return [{ name: 'a.ts', path: 'a.ts', kind: 'file' }];
        if (cmd === 'file_read') {
          return { content: 'hi', is_binary: false, size: 2, sha1: 'sha-a' };
        }
        return undefined;
      });
      const { findByTestId, container } = render(WorkspaceView, { props: { workspace: ws() } });
      const filesTab = await findByTestId('tab-files');
      const { fireEvent } = await import('@testing-library/svelte');
      await fireEvent.click(filesTab);
      const row = await waitFor(() => {
        const r = container.querySelector('[data-testid="file-row"][data-path="a.ts"] button');
        if (!r) throw new Error('row not found');
        return r;
      });
      await fireEvent.click(row as Element);
      await waitFor(() => expect(workspaceTabs.active('ws_a')).toBe('editor'));
    });

    it('file_read failure surfaces a toast and leaves tabs untouched', async () => {
      vi.mocked(invoke).mockImplementation(async (cmd) => {
        if (cmd === 'list_messages') return [];
        if (cmd === 'workspace_files') return [{ name: 'bad', path: 'bad', kind: 'file' }];
        if (cmd === 'file_read') throw 'boom';
        return undefined;
      });
      const { findByTestId, container } = render(WorkspaceView, { props: { workspace: ws() } });
      const filesTab = await findByTestId('tab-files');
      const { fireEvent } = await import('@testing-library/svelte');
      await fireEvent.click(filesTab);
      const row = await waitFor(() => {
        const r = container.querySelector('[data-testid="file-row"][data-path="bad"] button');
        if (!r) throw new Error('row not found');
        return r;
      });
      await fireEvent.click(row as Element);
      await waitFor(() => {
        const toasts = Array.from(getToasts().values());
        expect(toasts.some((t) => t.message.includes('boom'))).toBe(true);
      });
      // Stays on files tab — no half-baked editor switch.
      expect(workspaceTabs.active('ws_a')).toBe('files');
    });
  });
});
