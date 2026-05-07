import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import type { SearchHit } from '$lib/types';

let lastSearchChannel: { onmessage?: (hit: SearchHit) => void } | null = null;
const lastSearchArgs: { mode?: string; query?: string } = {};

vi.mock('@tauri-apps/api/core', () => {
  class MockChannel {
    onmessage?: (hit: SearchHit) => void;
  }
  return {
    Channel: MockChannel,
    invoke: vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'workspace_search' && args) {
        lastSearchChannel = args.channel as { onmessage?: (hit: SearchHit) => void };
        lastSearchArgs.mode = args.mode as string;
        lastSearchArgs.query = args.query as string;
      }
    }),
  };
});

import { invoke } from '@tauri-apps/api/core';
import SearchModal from './SearchModal.svelte';

beforeEach(() => {
  lastSearchChannel = null;
  lastSearchArgs.mode = undefined;
  lastSearchArgs.query = undefined;
  vi.mocked(invoke).mockClear();
});

afterEach(() => {
  vi.clearAllMocks();
});

function send(hit: SearchHit) {
  if (!lastSearchChannel) throw new Error('no search channel captured yet');
  lastSearchChannel.onmessage?.(hit);
}

describe('SearchModal', () => {
  it('renders nothing when open is false', () => {
    const { queryByTestId } = render(SearchModal, {
      props: {
        open: false,
        workspaceId: 'ws_a',
        onClose: () => {},
        onJump: () => {},
      },
    });
    expect(queryByTestId('search-modal')).toBeNull();
  });

  it('renders when open is true and focuses the input', async () => {
    const { findByTestId } = render(SearchModal, {
      props: {
        open: true,
        workspaceId: 'ws_a',
        onClose: () => {},
        onJump: () => {},
      },
    });
    const input = await findByTestId('search-input');
    await waitFor(() => {
      expect(document.activeElement).toBe(input);
    });
  });

  it('Escape closes the modal', async () => {
    const onClose = vi.fn();
    const { findByTestId } = render(SearchModal, {
      props: {
        open: true,
        workspaceId: 'ws_a',
        onClose,
        onJump: () => {},
      },
    });
    const modal = await findByTestId('search-modal');
    await fireEvent.keyDown(modal, { key: 'Escape' });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('clicking the close button fires onClose', async () => {
    const onClose = vi.fn();
    const { findByTestId } = render(SearchModal, {
      props: {
        open: true,
        workspaceId: 'ws_a',
        onClose,
        onJump: () => {},
      },
    });
    const close = await findByTestId('search-close');
    await fireEvent.click(close);
    expect(onClose).toHaveBeenCalled();
  });

  it('Enter submits a search in the default filename mode', async () => {
    const { findByTestId } = render(SearchModal, {
      props: {
        open: true,
        workspaceId: 'ws_b',
        onClose: () => {},
        onJump: () => {},
      },
    });
    const input = (await findByTestId('search-input')) as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'foo' } });
    await fireEvent.submit(input.closest('form')!);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'workspace_search',
        expect.objectContaining({ workspaceId: 'ws_b', query: 'foo', mode: 'filename' })
      );
    });
  });

  it('switching to content tab + Enter sends mode=content', async () => {
    const { findByTestId } = render(SearchModal, {
      props: {
        open: true,
        workspaceId: 'ws_c',
        onClose: () => {},
        onJump: () => {},
      },
    });
    const contentTab = await findByTestId('search-mode-content');
    await fireEvent.click(contentTab);
    const input = (await findByTestId('search-input')) as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'foo' } });
    await fireEvent.submit(input.closest('form')!);
    await waitFor(() => {
      expect(lastSearchArgs.mode).toBe('content');
    });
  });

  it('renders streamed filename hits', async () => {
    const { findByTestId, findAllByTestId } = render(SearchModal, {
      props: {
        open: true,
        workspaceId: 'ws_d',
        onClose: () => {},
        onJump: () => {},
      },
    });
    const input = (await findByTestId('search-input')) as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'foo' } });
    await fireEvent.submit(input.closest('form')!);
    await waitFor(() => expect(lastSearchChannel).not.toBeNull());
    send({ kind: 'filename', path: 'src/foo.ts' });
    send({ kind: 'filename', path: 'tests/foo.test.ts' });
    send({ kind: 'eof' });
    const hits = await findAllByTestId('search-hit');
    expect(hits).toHaveLength(2);
    expect(hits[0].textContent).toMatch(/src\/foo\.ts/);
  });

  it('clicking a hit fires onJump and closes the modal', async () => {
    const onJump = vi.fn();
    const onClose = vi.fn();
    const { findByTestId, findAllByTestId } = render(SearchModal, {
      props: {
        open: true,
        workspaceId: 'ws_e',
        onClose,
        onJump,
      },
    });
    const input = (await findByTestId('search-input')) as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'foo' } });
    await fireEvent.submit(input.closest('form')!);
    await waitFor(() => expect(lastSearchChannel).not.toBeNull());
    send({ kind: 'content', path: 'src/foo.ts', line_number: 42, line_text: 'const foo = 1;' });
    send({ kind: 'eof' });
    const hits = await findAllByTestId('search-hit');
    await fireEvent.click(hits[0].querySelector('button')!);
    expect(onJump).toHaveBeenCalledWith('src/foo.ts', 42);
    expect(onClose).toHaveBeenCalled();
  });

  it('shows the install-rg banner when content mode reports unavailable', async () => {
    const { findByTestId } = render(SearchModal, {
      props: {
        open: true,
        workspaceId: 'ws_f',
        initialMode: 'content',
        onClose: () => {},
        onJump: () => {},
      },
    });
    const input = (await findByTestId('search-input')) as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'foo' } });
    await fireEvent.submit(input.closest('form')!);
    await waitFor(() => expect(lastSearchChannel).not.toBeNull());
    send({ kind: 'ripgrep_unavailable', reason: 'rg not in PATH' });
    send({ kind: 'eof' });
    const banner = await findByTestId('search-unavailable');
    expect(banner.textContent).toMatch(/rg not in PATH/);
  });

  it('shows the empty state after a query yields no results', async () => {
    const { findByTestId } = render(SearchModal, {
      props: {
        open: true,
        workspaceId: 'ws_g',
        onClose: () => {},
        onJump: () => {},
      },
    });
    const input = (await findByTestId('search-input')) as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'nothingmatches' } });
    await fireEvent.submit(input.closest('form')!);
    await waitFor(() => expect(lastSearchChannel).not.toBeNull());
    send({ kind: 'eof' });
    const empty = await findByTestId('search-empty');
    expect(empty.textContent).toMatch(/No results/);
  });

  it('does not invoke search when query is empty whitespace', async () => {
    const { findByTestId } = render(SearchModal, {
      props: {
        open: true,
        workspaceId: 'ws_h',
        onClose: () => {},
        onJump: () => {},
      },
    });
    const input = (await findByTestId('search-input')) as HTMLInputElement;
    await fireEvent.input(input, { target: { value: '   ' } });
    await fireEvent.submit(input.closest('form')!);
    await new Promise((r) => setTimeout(r, 10));
    expect(invoke).not.toHaveBeenCalled();
  });

  it('drops chunks from a stale search after a re-submit', async () => {
    const { findByTestId, queryByTestId } = render(SearchModal, {
      props: {
        open: true,
        workspaceId: 'ws_stale',
        onClose: () => {},
        onJump: () => {},
      },
    });
    const input = (await findByTestId('search-input')) as HTMLInputElement;
    await fireEvent.input(input, { target: { value: 'foo' } });
    await fireEvent.submit(input.closest('form')!);
    await waitFor(() => expect(lastSearchChannel).not.toBeNull());
    const stale = lastSearchChannel!;
    // Submit again — this swaps the channel.
    await fireEvent.submit(input.closest('form')!);
    await waitFor(() => expect(lastSearchChannel).not.toBe(stale));
    // Stale channel emits a hit — should be ignored.
    stale.onmessage?.({ kind: 'filename', path: 'should-not-render.ts' });
    expect(queryByTestId('search-hit')).toBeNull();
  });
});
