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
          tool_use: { id: 'toolu_a', name: 'Read', input: { file_path: '/x' } },
        }),
      },
    });
    expect(getByText('Read')).toBeTruthy();
  });

  it('renders per-tool detail (Read shows basename + range)', () => {
    const { getByText } = render(MessageBubble, {
      props: {
        message: make({
          text: '',
          tool_use: {
            id: 'toolu_b',
            name: 'Read',
            input: { file_path: '/repo/src/foo.ts', offset: 1, limit: 50 },
          },
        }),
      },
    });
    expect(getByText('foo.ts:1-50')).toBeTruthy();
  });

  it('renders Bash detail with $ prefix and the command', () => {
    const { getByText } = render(MessageBubble, {
      props: {
        message: make({
          text: '',
          tool_use: { id: 'toolu_c', name: 'Bash', input: { command: 'ls -la' } },
        }),
      },
    });
    expect(getByText('$ ls -la')).toBeTruthy();
  });

  it('renders system role messages as centered marker (for compact boundary)', () => {
    const { container, getByText } = render(MessageBubble, {
      props: {
        message: make({
          role: 'system',
          text: 'Compacted earlier conversation',
        }),
      },
    });
    // Not wrapped in an <article> — markers use a div sentinel.
    expect(container.querySelector('article')).toBeNull();
    expect(container.querySelector('[data-role="system"]')).toBeTruthy();
    expect(getByText(/compacted earlier/i)).toBeTruthy();
  });

  it('renders nothing when the message has no text, tool, or partial state', () => {
    // Legacy parser used to emit Message{text:""} for thinking-only turns —
    // those should render to nothing rather than an empty rounded bubble.
    const { container } = render(MessageBubble, {
      props: { message: make({ text: '', tool_use: null, tool_result: null, is_partial: false }) },
    });
    expect(container.querySelector('article')).toBeNull();
    expect(container.querySelector('[data-role]')).toBeNull();
  });

  it('truncates very long tool_result content with a tail hint', () => {
    const long = 'x'.repeat(2000);
    const { container } = render(MessageBubble, {
      props: {
        message: make({
          role: 'tool',
          text: '',
          tool_result: { tool_use_id: 'toolu_a', content: long, is_error: false },
        }),
      },
    });
    const pre = container.querySelector('[data-tool-result]') as HTMLElement;
    expect(pre.textContent!.length).toBeLessThan(long.length);
    expect(pre.textContent).toMatch(/more chars/);
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
