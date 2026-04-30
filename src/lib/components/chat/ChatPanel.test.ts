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

  describe('lazy load', () => {
    it('does not render load-earlier UI when onLoadEarlier is not provided', () => {
      messages.upsert(make('msg_1', 'ws_a'));
      const { queryByTestId } = render(ChatPanel, {
        props: { workspaceId: 'ws_a', onSend: vi.fn() },
      });
      expect(queryByTestId('load-earlier-button')).toBeNull();
    });

    it('does not render load-earlier UI when there are no messages', () => {
      const { queryByTestId } = render(ChatPanel, {
        props: { workspaceId: 'ws_a', onSend: vi.fn(), onLoadEarlier: vi.fn() },
      });
      expect(queryByTestId('load-earlier-button')).toBeNull();
    });

    it('renders the Load earlier button when messages and loader present', () => {
      messages.upsert(make('msg_1', 'ws_a'));
      const { getByTestId } = render(ChatPanel, {
        props: {
          workspaceId: 'ws_a',
          onSend: vi.fn(),
          onLoadEarlier: vi.fn().mockResolvedValue([]),
        },
      });
      expect(getByTestId('load-earlier-button')).toBeTruthy();
    });

    it('clicking Load earlier calls onLoadEarlier with the oldest message id', async () => {
      messages.upsert({ ...make('msg_old', 'ws_a'), created_at: 1 });
      messages.upsert({ ...make('msg_new', 'ws_a'), created_at: 2 });
      const onLoadEarlier = vi.fn().mockResolvedValue([]);
      const { getByTestId } = render(ChatPanel, {
        props: { workspaceId: 'ws_a', onSend: vi.fn(), onLoadEarlier },
      });
      const { fireEvent } = await import('@testing-library/svelte');
      await fireEvent.click(getByTestId('load-earlier-button'));
      expect(onLoadEarlier).toHaveBeenCalledWith('msg_old');
    });

    it('hydrates the returned batch into the messages store', async () => {
      messages.upsert({ ...make('msg_b', 'ws_a'), created_at: 100 });
      const earlier: Message = { ...make('msg_a', 'ws_a'), created_at: 50 };
      const onLoadEarlier = vi.fn().mockResolvedValue([earlier]);
      const { getByTestId } = render(ChatPanel, {
        props: { workspaceId: 'ws_a', onSend: vi.fn(), onLoadEarlier },
      });
      const { fireEvent } = await import('@testing-library/svelte');
      await fireEvent.click(getByTestId('load-earlier-button'));
      await vi.waitFor(() => {
        expect(messages.listForWorkspace('ws_a')).toHaveLength(2);
      });
      expect(messages.listForWorkspace('ws_a')[0].id).toBe('msg_a');
    });

    it('marks history exhausted when loader returns empty array', async () => {
      messages.upsert(make('msg_x', 'ws_a'));
      const onLoadEarlier = vi.fn().mockResolvedValue([]);
      const { getByTestId, findByTestId, queryByTestId } = render(ChatPanel, {
        props: { workspaceId: 'ws_a', onSend: vi.fn(), onLoadEarlier },
      });
      const { fireEvent } = await import('@testing-library/svelte');
      await fireEvent.click(getByTestId('load-earlier-button'));
      expect(await findByTestId('history-exhausted')).toBeTruthy();
      expect(queryByTestId('load-earlier-button')).toBeNull();
    });

    it('does not call loader twice while a load is in flight', async () => {
      messages.upsert(make('msg_z', 'ws_a'));
      let resolveBatch!: (m: Message[]) => void;
      const onLoadEarlier = vi.fn(
        () =>
          new Promise<Message[]>((resolve) => {
            resolveBatch = resolve;
          })
      );
      const { getByTestId } = render(ChatPanel, {
        props: { workspaceId: 'ws_a', onSend: vi.fn(), onLoadEarlier },
      });
      const { fireEvent } = await import('@testing-library/svelte');
      const btn = getByTestId('load-earlier-button');
      await fireEvent.click(btn);
      // While loading, the button is replaced by the loading indicator,
      // so a second invocation must come through the scroll handler.
      const scroll = getByTestId('chat-scroll');
      await fireEvent.scroll(scroll);
      await fireEvent.scroll(scroll);
      expect(onLoadEarlier).toHaveBeenCalledTimes(1);
      resolveBatch([]);
    });

    it('fires loadEarlier when the scroll container reaches the top', async () => {
      messages.upsert(make('msg_q', 'ws_a'));
      const onLoadEarlier = vi.fn().mockResolvedValue([]);
      const { getByTestId } = render(ChatPanel, {
        props: { workspaceId: 'ws_a', onSend: vi.fn(), onLoadEarlier },
      });
      const scroll = getByTestId('chat-scroll');
      Object.defineProperty(scroll, 'scrollTop', { value: 0, configurable: true });
      const { fireEvent } = await import('@testing-library/svelte');
      await fireEvent.scroll(scroll);
      expect(onLoadEarlier).toHaveBeenCalled();
    });

    it('does not fire loadEarlier when scrolled below the threshold', async () => {
      messages.upsert(make('msg_q', 'ws_a'));
      const onLoadEarlier = vi.fn().mockResolvedValue([]);
      const { getByTestId } = render(ChatPanel, {
        props: { workspaceId: 'ws_a', onSend: vi.fn(), onLoadEarlier },
      });
      const scroll = getByTestId('chat-scroll');
      Object.defineProperty(scroll, 'scrollTop', { value: 500, configurable: true });
      const { fireEvent } = await import('@testing-library/svelte');
      await fireEvent.scroll(scroll);
      expect(onLoadEarlier).not.toHaveBeenCalled();
    });

    it('resets loading flag when loader rejects so retries are possible', async () => {
      messages.upsert(make('msg_a', 'ws_a'));
      const onLoadEarlier = vi.fn().mockRejectedValueOnce('boom').mockResolvedValueOnce([]);
      const { getByTestId } = render(ChatPanel, {
        props: { workspaceId: 'ws_a', onSend: vi.fn(), onLoadEarlier },
      });
      const { fireEvent } = await import('@testing-library/svelte');
      try {
        await fireEvent.click(getByTestId('load-earlier-button'));
      } catch {
        // swallow expected rejection from first call
      }
      // Wait for re-render after error
      await vi.waitFor(() => {
        expect(getByTestId('load-earlier-button')).toBeTruthy();
      });
      await fireEvent.click(getByTestId('load-earlier-button'));
      expect(onLoadEarlier).toHaveBeenCalledTimes(2);
    });
  });
});
