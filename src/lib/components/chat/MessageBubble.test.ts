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
