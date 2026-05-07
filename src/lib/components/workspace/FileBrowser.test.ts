import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import type { FileEntry } from '$lib/types';

/** Per-call response queue so tests can stage what `workspace_files`
 *  returns for each invocation, in order. */
const responses: Array<FileEntry[] | Error> = [];

vi.mock('@tauri-apps/api/core', () => {
  return {
    invoke: vi.fn(async (cmd: string) => {
      if (cmd === 'workspace_files') {
        const next = responses.shift();
        if (next instanceof Error) throw next;
        return next ?? [];
      }
      return undefined;
    }),
    Channel: class {},
  };
});

import FileBrowser from './FileBrowser.svelte';
import { workspaceTabs } from '$lib/stores/workspace-tabs.svelte';

beforeEach(() => {
  responses.length = 0;
  workspaceTabs.reset();
});

afterEach(() => {
  vi.clearAllMocks();
});

const dir = (name: string, path = name): FileEntry => ({ name, path, kind: 'dir' });
const file = (name: string, path = name): FileEntry => ({ name, path, kind: 'file' });

describe('FileBrowser', () => {
  it('renders the loading state until the root listing arrives', async () => {
    // Make the response arrive after a microtask delay.
    let resolveRoot: (entries: FileEntry[]) => void = () => {};
    const pending = new Promise<FileEntry[]>((r) => (resolveRoot = r));
    responses.push([]); // unused — we stub the mock per-call below
    const { invoke } = await import('@tauri-apps/api/core');
    vi.mocked(invoke).mockImplementationOnce(() => pending);
    const { findByTestId, queryByTestId } = render(FileBrowser, {
      props: { workspaceId: 'ws_load' },
    });
    expect(await findByTestId('file-browser-loading')).toBeTruthy();
    resolveRoot([file('a.txt')]);
    await waitFor(() => expect(queryByTestId('file-browser-loading')).toBeNull());
  });

  it('renders root entries in returned order', async () => {
    responses.push([dir('src'), file('a.txt'), file('b.md')]);
    const { findAllByTestId } = render(FileBrowser, { props: { workspaceId: 'ws_root' } });
    const rows = await findAllByTestId('file-row');
    expect(rows.map((r) => r.getAttribute('data-path'))).toEqual(['src', 'a.txt', 'b.md']);
  });

  it('renders the empty state when the worktree has no entries', async () => {
    responses.push([]);
    const { findByTestId } = render(FileBrowser, { props: { workspaceId: 'ws_empty' } });
    expect(await findByTestId('file-browser-empty')).toBeTruthy();
  });

  it('expands a directory on click and lazy-loads its children', async () => {
    responses.push([dir('src')]);
    responses.push([file('app.ts', 'src/app.ts')]); // for src expansion
    const { findAllByTestId } = render(FileBrowser, { props: { workspaceId: 'ws_lazy' } });
    let rows = await findAllByTestId('file-row');
    expect(rows.map((r) => r.getAttribute('data-path'))).toEqual(['src']);
    await fireEvent.click(rows[0].querySelector('button')!);
    await waitFor(async () => {
      rows = await findAllByTestId('file-row');
      expect(rows.map((r) => r.getAttribute('data-path'))).toContain('src/app.ts');
    });
  });

  it('collapses a directory on second click without re-fetching', async () => {
    responses.push([dir('src')]);
    responses.push([file('app.ts', 'src/app.ts')]);
    const { findAllByTestId } = render(FileBrowser, { props: { workspaceId: 'ws_collapse' } });
    const rowsAfter = async () => await findAllByTestId('file-row');
    const rows = await rowsAfter();
    await fireEvent.click(rows[0].querySelector('button')!);
    await waitFor(async () => expect((await rowsAfter()).length).toBe(2));
    // Second click collapses — only the parent row remains.
    await fireEvent.click(rows[0].querySelector('button')!);
    await waitFor(async () => expect((await rowsAfter()).length).toBe(1));
    // Reopening shouldn't trigger another fetch — the response queue is
    // empty and the `responses` array would throw if drained beyond setup.
    await fireEvent.click(rows[0].querySelector('button')!);
    await waitFor(async () => expect((await rowsAfter()).length).toBe(2));
  });

  it('persists expansion state via workspace-tabs store', async () => {
    responses.push([dir('src')]);
    responses.push([file('app.ts', 'src/app.ts')]);
    const { findAllByTestId } = render(FileBrowser, { props: { workspaceId: 'ws_persist' } });
    const rows = await findAllByTestId('file-row');
    await fireEvent.click(rows[0].querySelector('button')!);
    await waitFor(() => {
      expect(workspaceTabs.expanded('ws_persist').has('src')).toBe(true);
    });
  });

  it('clicking a file invokes the onOpen callback', async () => {
    responses.push([file('readme.md')]);
    const onOpen = vi.fn();
    const { findAllByTestId } = render(FileBrowser, {
      props: { workspaceId: 'ws_open', onOpen },
    });
    const rows = await findAllByTestId('file-row');
    await fireEvent.click(rows[0].querySelector('button')!);
    expect(onOpen).toHaveBeenCalledWith('readme.md');
  });

  it('surfaces a per-directory error when expansion fails', async () => {
    responses.push([dir('locked')]);
    responses.push(new Error('permission denied'));
    const { findAllByTestId, findByTestId } = render(FileBrowser, {
      props: { workspaceId: 'ws_err' },
    });
    const rows = await findAllByTestId('file-row');
    await fireEvent.click(rows[0].querySelector('button')!);
    const banner = await findByTestId('file-row-error');
    expect(banner.textContent).toMatch(/permission denied/);
  });

  it('surfaces a root-level error when the initial load fails', async () => {
    responses.push(new Error('fs unavailable'));
    const { findByTestId } = render(FileBrowser, { props: { workspaceId: 'ws_root_err' } });
    const banner = await findByTestId('file-browser-root-error');
    expect(banner.textContent).toMatch(/fs unavailable/);
  });

  it('highlights the row whose path matches selectedPath', async () => {
    responses.push([file('readme.md')]);
    const { findAllByTestId } = render(FileBrowser, {
      props: { workspaceId: 'ws_sel', selectedPath: 'readme.md' },
    });
    const rows = await findAllByTestId('file-row');
    expect(rows[0].getAttribute('aria-selected')).toBe('true');
  });
});
