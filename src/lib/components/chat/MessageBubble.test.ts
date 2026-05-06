import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/svelte';

// MessageBubble uses convertFileSrc to serve local image files. The real
// implementation requires the Tauri runtime, so stub it for tests.
vi.mock('@tauri-apps/api/core', () => ({
  convertFileSrc: (path: string) => `mock-asset://${path}`,
}));

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

  describe('attachments', () => {
    it('renders an image thumbnail when message has an image attachment', () => {
      const { container } = render(MessageBubble, {
        props: {
          message: make({
            role: 'user',
            text: 'look at this',
            attachments: [
              {
                kind: 'image',
                media_type: 'image/png',
                path: '/data/attachments/ws_a/msg_a/design.png',
                filename: 'design.png',
              },
            ],
          }),
        },
      });
      const img = container.querySelector(
        '[data-testid="attachment-image"]'
      ) as HTMLImageElement | null;
      expect(img).toBeTruthy();
      // convertFileSrc is stubbed to prefix paths with mock-asset://.
      expect(img!.getAttribute('src')).toContain('mock-asset://');
      expect(img!.getAttribute('src')).toContain('design.png');
    });

    it('uses the filename as alt text when present', () => {
      const { container } = render(MessageBubble, {
        props: {
          message: make({
            role: 'user',
            text: '',
            attachments: [
              {
                kind: 'image',
                media_type: 'image/jpeg',
                path: '/abs/screenshot.jpg',
                filename: 'screenshot.jpg',
              },
            ],
          }),
        },
      });
      const img = container.querySelector(
        '[data-testid="attachment-image"]'
      ) as HTMLImageElement | null;
      expect(img).toBeTruthy();
      expect(img!.getAttribute('alt')).toBe('screenshot.jpg');
    });

    it('renders multiple thumbnails when multiple attachments are present', () => {
      const { container } = render(MessageBubble, {
        props: {
          message: make({
            role: 'user',
            text: 'two pics',
            attachments: [
              {
                kind: 'image',
                media_type: 'image/png',
                path: '/a/1.png',
                filename: '1.png',
              },
              {
                kind: 'image',
                media_type: 'image/png',
                path: '/a/2.png',
                filename: '2.png',
              },
            ],
          }),
        },
      });
      const imgs = container.querySelectorAll('[data-testid="attachment-image"]');
      expect(imgs.length).toBe(2);
    });

    it('does not render an attachments grid when message has no attachments', () => {
      const { container } = render(MessageBubble, {
        props: { message: make({ text: 'plain' }) },
      });
      expect(container.querySelector('[data-testid="attachment-grid"]')).toBeNull();
    });

    it('does not treat an attachment-only user message as empty', () => {
      // Echoed multimodal turns may have `text: ""` — they should still render
      // as a bubble because the attachment is the content.
      const { container } = render(MessageBubble, {
        props: {
          message: make({
            role: 'user',
            text: '',
            attachments: [
              {
                kind: 'image',
                media_type: 'image/png',
                path: '/a/x.png',
                filename: 'x.png',
              },
            ],
          }),
        },
      });
      expect(container.querySelector('article')).toBeTruthy();
      expect(container.querySelector('[data-testid="attachment-image"]')).toBeTruthy();
    });
  });
});
