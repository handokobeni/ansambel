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
  };
});

import { invoke } from '@tauri-apps/api/core';
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

beforeEach(() => {
  messages.reset();
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockResolvedValue(undefined);
});

afterEach(() => {
  vi.clearAllMocks();
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
      expect(messages.listForWorkspace('ws_a').find((m) => m.id === 'msg_live')?.text).toBe(
        'live'
      );
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
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith('send_message', {
        workspaceId: 'ws_a',
        text: 'Hello',
      });
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
});
