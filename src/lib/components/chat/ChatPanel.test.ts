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
    const { container } = render(ChatPanel, {
      props: { workspaceId: 'ws_a', onSend: vi.fn() },
    });
    expect(container.querySelector('textarea')).toBeTruthy();
  });

  it('forwards send to onSend prop', async () => {
    const onSend = vi.fn();
    const { container, getByRole } = render(ChatPanel, {
      props: { workspaceId: 'ws_a', onSend },
    });
    const ta = container.querySelector('textarea') as HTMLTextAreaElement;
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
