import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import type { DiffChunk } from '$lib/types';

// Capture the Channel instance handed to api.workspace.diff so tests can
// drive it. Each `invoke('workspace_diff', { channel })` call replaces
// `lastDiffChannel`, so tests dealing with refresh use it sequentially.
let lastDiffChannel: { onmessage?: (chunk: DiffChunk) => void } | null = null;

vi.mock('@tauri-apps/api/core', () => {
  class MockChannel {
    onmessage?: (chunk: DiffChunk) => void;
  }
  return {
    Channel: MockChannel,
    invoke: vi.fn((cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'workspace_diff' && args && 'channel' in args) {
        lastDiffChannel = args.channel as { onmessage?: (chunk: DiffChunk) => void };
      }
      return Promise.resolve(undefined);
    }),
  };
});

import { invoke } from '@tauri-apps/api/core';
import DiffView from './DiffView.svelte';

beforeEach(() => {
  lastDiffChannel = null;
  vi.mocked(invoke).mockClear();
});

afterEach(() => {
  vi.clearAllMocks();
});

function send(chunk: DiffChunk) {
  if (!lastDiffChannel) throw new Error('no channel captured yet');
  lastDiffChannel.onmessage?.(chunk);
}

describe('DiffView', () => {
  it('renders the loading state on initial mount', () => {
    const { getByTestId } = render(DiffView, { props: { workspaceId: 'ws_a' } });
    expect(getByTestId('diff-loading')).toBeTruthy();
  });

  it('invokes workspace_diff with the workspace id', async () => {
    render(DiffView, { props: { workspaceId: 'ws_b' } });
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith(
        'workspace_diff',
        expect.objectContaining({ workspaceId: 'ws_b' })
      );
    });
  });

  it('renders the empty state when the worktree is clean', async () => {
    const { findByTestId } = render(DiffView, { props: { workspaceId: 'ws_clean' } });
    await waitFor(() => expect(lastDiffChannel).not.toBeNull());
    send({ kind: 'eof' });
    expect(await findByTestId('diff-empty')).toBeTruthy();
  });

  it('renders one block per file with green/red row tagging', async () => {
    const { findAllByTestId, container } = render(DiffView, {
      props: { workspaceId: 'ws_mod' },
    });
    await waitFor(() => expect(lastDiffChannel).not.toBeNull());
    send({
      kind: 'text',
      text: [
        'diff --git a/foo.txt b/foo.txt',
        '--- a/foo.txt',
        '+++ b/foo.txt',
        '@@ -1 +1 @@',
        '-old',
        '+new',
        '',
      ].join('\n'),
    });
    send({ kind: 'eof' });
    const files = await findAllByTestId('diff-file');
    expect(files).toHaveLength(1);
    expect(files[0].getAttribute('data-path')).toBe('foo.txt');
    const lines = container.querySelectorAll('[data-testid="diff-line"]');
    const kinds = Array.from(lines).map((el) => el.getAttribute('data-line-kind'));
    expect(kinds).toEqual(['del', 'add']);
  });

  it('handles chunked arrival even when a hunk header splits across chunks', async () => {
    const full = [
      'diff --git a/a.ts b/a.ts',
      '--- a/a.ts',
      '+++ b/a.ts',
      '@@ -1,1 +1,1 @@',
      '-old',
      '+new',
      '',
    ].join('\n');
    // Split mid-way through a hunk header — `parseUnifiedDiff` should still
    // see a coherent buffer because chunks are concatenated before parsing.
    const splitAt = full.indexOf('@@ -1') + 4;
    const head = full.slice(0, splitAt);
    const tail = full.slice(splitAt);

    const { findAllByTestId } = render(DiffView, { props: { workspaceId: 'ws_chunked' } });
    await waitFor(() => expect(lastDiffChannel).not.toBeNull());
    send({ kind: 'text', text: head });
    send({ kind: 'text', text: tail });
    send({ kind: 'eof' });

    const files = await findAllByTestId('diff-file');
    expect(files).toHaveLength(1);
    expect(files[0].getAttribute('data-path')).toBe('a.ts');
  });

  it('renders an error banner when the backend emits an error chunk', async () => {
    const { findByTestId } = render(DiffView, { props: { workspaceId: 'ws_err' } });
    await waitFor(() => expect(lastDiffChannel).not.toBeNull());
    send({ kind: 'error', message: 'git: not a repository' });
    const banner = await findByTestId('diff-error');
    expect(banner.textContent).toMatch(/not a repository/);
  });

  it('re-invokes workspace_diff when refresh is clicked', async () => {
    const { getByTestId } = render(DiffView, { props: { workspaceId: 'ws_refresh' } });
    await waitFor(() => expect(lastDiffChannel).not.toBeNull());
    send({ kind: 'eof' });
    const initialCalls = vi
      .mocked(invoke)
      .mock.calls.filter((c) => c[0] === 'workspace_diff').length;
    await fireEvent.click(getByTestId('diff-refresh'));
    await waitFor(() => {
      const after = vi.mocked(invoke).mock.calls.filter((c) => c[0] === 'workspace_diff').length;
      expect(after).toBe(initialCalls + 1);
    });
  });

  it('renders context, add, del, and meta lines with their respective row kinds', async () => {
    // Drives the lineBg/lineSign branches for every DiffLineKind so the
    // styling logic for `ctx`, `del`, `add`, and `meta` is all exercised
    // in one render.
    const { findAllByTestId } = render(DiffView, { props: { workspaceId: 'ws_kinds' } });
    await waitFor(() => expect(lastDiffChannel).not.toBeNull());
    send({
      kind: 'text',
      text: [
        'diff --git a/foo b/foo',
        '--- a/foo',
        '+++ b/foo',
        '@@ -1,3 +1,3 @@',
        ' ctx-line',
        '-removed',
        '+added',
        '\\ No newline at end of file',
        '',
      ].join('\n'),
    });
    send({ kind: 'eof' });
    const lines = await findAllByTestId('diff-line');
    const kinds = lines.map((el) => el.getAttribute('data-line-kind'));
    expect(kinds).toEqual(['ctx', 'del', 'add', 'meta']);
  });

  it('renders the binary-file marker for files the parser flags as binary', async () => {
    // Drives the `{#if file.isBinary}` branch in the template so the
    // "Binary file — diff not shown." message gets rendered.
    const { findByText } = render(DiffView, { props: { workspaceId: 'ws_bin' } });
    await waitFor(() => expect(lastDiffChannel).not.toBeNull());
    send({
      kind: 'text',
      text: ['diff --git a/img.png b/img.png', 'Binary file img.png not shown', ''].join('\n'),
    });
    send({ kind: 'eof' });
    expect(await findByText(/Binary file/)).toBeTruthy();
  });

  it('renders the error banner when the invoke promise rejects', async () => {
    vi.mocked(invoke).mockImplementationOnce(((cmd: string, args?: unknown) => {
      const a = args as Record<string, unknown> | undefined;
      if (cmd === 'workspace_diff' && a && 'channel' in a) {
        lastDiffChannel = a.channel as { onmessage?: (chunk: DiffChunk) => void };
      }
      return Promise.reject('git binary not found');
    }) as never);
    const { findByTestId } = render(DiffView, { props: { workspaceId: 'ws_reject' } });
    const banner = await findByTestId('diff-error');
    expect(banner.textContent).toMatch(/git binary not found/);
  });

  it('drops chunks from a stale generation after refresh', async () => {
    const { getByTestId, queryByTestId, findByTestId } = render(DiffView, {
      props: { workspaceId: 'ws_stale' },
    });
    await waitFor(() => expect(lastDiffChannel).not.toBeNull());
    const stale = lastDiffChannel!;
    // Begin a refresh — this sets up a new channel.
    await fireEvent.click(getByTestId('diff-refresh'));
    await waitFor(() => expect(lastDiffChannel).not.toBe(stale));
    // Have the stale channel emit an error AFTER refresh — should be ignored.
    stale.onmessage?.({ kind: 'error', message: 'late error from old gen' });
    expect(queryByTestId('diff-error')).toBeNull();
    // The new channel still drives the visible state.
    send({ kind: 'eof' });
    expect(await findByTestId('diff-empty')).toBeTruthy();
  });
});
