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

  it('forwards send to onSend prop with empty attachments by default', async () => {
    const onSend = vi.fn();
    const { container, getByRole } = render(ChatPanel, {
      props: { workspaceId: 'ws_a', onSend },
    });
    const ta = container.querySelector('textarea') as HTMLTextAreaElement;
    const { fireEvent } = await import('@testing-library/svelte');
    await fireEvent.input(ta, { target: { value: 'hi' } });
    await fireEvent.click(getByRole('button', { name: /send/i }));
    // MessageInput now passes the (possibly empty) attachments list as a
    // second argument so multimodal turns flow through unchanged.
    expect(onSend).toHaveBeenCalledWith('hi', []);
  });

  it('disables input when status is not running and not waiting', () => {
    messages.apply({ type: 'status', status: 'error' }, 'ws_a');
    const { getByRole } = render(ChatPanel, {
      props: { workspaceId: 'ws_a', onSend: vi.fn() },
    });
    const btn = getByRole('button', { name: /send/i }) as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });

  it('keeps input enabled when status is stopped (so the user can re-prompt)', async () => {
    messages.apply({ type: 'status', status: 'stopped' }, 'ws_a');
    const { container, getByRole } = render(ChatPanel, {
      props: { workspaceId: 'ws_a', onSend: vi.fn() },
    });
    const ta = container.querySelector('textarea') as HTMLTextAreaElement;
    expect(ta.disabled).toBe(false);
    const { fireEvent } = await import('@testing-library/svelte');
    await fireEvent.input(ta, { target: { value: 'continue' } });
    const btn = getByRole('button', { name: /send/i }) as HTMLButtonElement;
    expect(btn.disabled).toBe(false);
  });

  describe('error banner', () => {
    it('renders an error banner when messages.error is set for the workspace', () => {
      messages.apply({ type: 'error', message: 'CLI: invalid auth token' }, 'ws_a');
      const { getByRole, getByText } = render(ChatPanel, {
        props: { workspaceId: 'ws_a', onSend: vi.fn() },
      });
      const banner = getByRole('alert');
      expect(banner).toBeTruthy();
      expect(getByText(/cli: invalid auth token/i)).toBeTruthy();
    });

    it('does not render the banner when no error is set', () => {
      const { queryByRole } = render(ChatPanel, {
        props: { workspaceId: 'ws_a', onSend: vi.fn() },
      });
      expect(queryByRole('alert')).toBeNull();
    });

    it('does not surface errors from a different workspace', () => {
      messages.apply({ type: 'error', message: 'wrong workspace' }, 'ws_other');
      const { queryByRole } = render(ChatPanel, {
        props: { workspaceId: 'ws_a', onSend: vi.fn() },
      });
      expect(queryByRole('alert')).toBeNull();
    });

    it('hides the banner after the dismiss button is clicked', async () => {
      messages.apply({ type: 'error', message: 'oops' }, 'ws_a');
      const { getByLabelText, queryByRole } = render(ChatPanel, {
        props: { workspaceId: 'ws_a', onSend: vi.fn() },
      });
      const { fireEvent } = await import('@testing-library/svelte');
      await fireEvent.click(getByLabelText(/dismiss error/i));
      expect(queryByRole('alert')).toBeNull();
    });

    it('resurfaces a fresh error after a previous one was dismissed', async () => {
      messages.apply({ type: 'error', message: 'first' }, 'ws_a');
      const { getByLabelText, queryByRole, findByText } = render(ChatPanel, {
        props: { workspaceId: 'ws_a', onSend: vi.fn() },
      });
      const { fireEvent } = await import('@testing-library/svelte');
      await fireEvent.click(getByLabelText(/dismiss error/i));
      expect(queryByRole('alert')).toBeNull();
      messages.apply({ type: 'error', message: 'second' }, 'ws_a');
      // findByText awaits the next render tick — Svelte's $derived
      // re-evaluates synchronously in a microtask, so the new error
      // string should appear by the next macrotask.
      expect(await findByText(/second/i)).toBeTruthy();
    });
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

  describe('DOM virtualization (bounded render window)', () => {
    it('caps DOM at the render window when the list is large', () => {
      const wsId = 'ws_v';
      for (let i = 0; i < 1000; i++) {
        messages.upsert({ ...make(`msg_${i}`, wsId), created_at: i });
      }
      const { container } = render(ChatPanel, {
        props: { workspaceId: wsId, onSend: vi.fn(), initialRenderCount: 100 },
      });
      const rendered = container.querySelectorAll('[data-message-id]').length;
      expect(rendered).toBe(100);
      // The visible window is the *most recent* 100 messages.
      expect(container.querySelector('[data-message-id="msg_999"]')).toBeTruthy();
      expect(container.querySelector('[data-message-id="msg_900"]')).toBeTruthy();
      // Anything older is NOT in the DOM.
      expect(container.querySelector('[data-message-id="msg_0"]')).toBeNull();
      expect(container.querySelector('[data-message-id="msg_500"]')).toBeNull();
    });

    it('renders all messages when the list fits inside the window', () => {
      const wsId = 'ws_small';
      for (let i = 0; i < 5; i++) {
        messages.upsert({ ...make(`msg_${i}`, wsId), created_at: i });
      }
      const { container } = render(ChatPanel, {
        props: { workspaceId: wsId, onSend: vi.fn(), initialRenderCount: 100 },
      });
      expect(container.querySelectorAll('[data-message-id]').length).toBe(5);
    });

    it('streams partial updates to the active bubble in place (same DOM node)', async () => {
      const wsId = 'ws_s';
      messages.upsert({ ...make('msg_a', wsId), created_at: 1 });
      const { container } = render(ChatPanel, {
        props: { workspaceId: wsId, onSend: vi.fn() },
      });
      const initial = container.querySelector('[data-message-id="msg_a"]');
      expect(initial).toBeTruthy();
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
      // Keyed-each preserves the DOM node — it's the same reference,
      // not a remount. Sibling bubbles don't repaint.
      expect(container.querySelector('[data-message-id="msg_a"]')).toBe(initial);
    });

    it('auto-scrolls to bottom on a new message when pinned', async () => {
      const wsId = 'ws_p';
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
        value: 500,
        configurable: true,
        writable: true,
      });
      // Pinned by default; just verify the new-message effect bumps
      // scrollTop to the (new) scrollHeight.
      Object.defineProperty(scroll, 'scrollHeight', {
        value: 1200,
        configurable: true,
      });
      messages.upsert({ ...make('msg_new', wsId), created_at: 100 });
      await vi.waitFor(() => {
        expect((scroll as HTMLElement).scrollTop).toBe(1200);
      });
    });

    it('does NOT auto-scroll when the user has scrolled up', async () => {
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
      // Scrolling up far from the bottom flips the pinned flag false.
      await fireEvent.scroll(scroll);
      messages.upsert({ ...make('msg_late', wsId), created_at: 200 });
      // Wait long enough for the queueMicrotask handler to have fired.
      await new Promise((r) => setTimeout(r, 30));
      expect((scroll as HTMLElement).scrollTop).toBe(100);
    });

    it('expands the render window when the user scrolls to the top', async () => {
      const wsId = 'ws_w';
      for (let i = 0; i < 200; i++) {
        messages.upsert({ ...make(`msg_${i}`, wsId), created_at: i });
      }
      const { container, getByTestId } = render(ChatPanel, {
        props: { workspaceId: wsId, onSend: vi.fn(), initialRenderCount: 100 },
      });
      expect(container.querySelectorAll('[data-message-id]').length).toBe(100);
      const scroll = getByTestId('chat-scroll');
      Object.defineProperty(scroll, 'scrollTop', {
        value: 0,
        configurable: true,
        writable: true,
      });
      Object.defineProperty(scroll, 'scrollHeight', {
        value: 1000,
        configurable: true,
      });
      Object.defineProperty(scroll, 'clientHeight', {
        value: 500,
        configurable: true,
      });
      const { fireEvent } = await import('@testing-library/svelte');
      await fireEvent.scroll(scroll);
      // First scroll-to-top expands by 50 → 150 messages in DOM.
      await vi.waitFor(() => {
        expect(container.querySelectorAll('[data-message-id]').length).toBe(150);
      });
    });
  });
});
