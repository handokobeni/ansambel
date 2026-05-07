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

  it('reveals a deeply-nested selectedPath by expanding every ancestor', async () => {
    // Tree:
    //   app/
    //     Http/
    //       Controllers/
    //         web.php  ← target
    //   README.md
    responses.push([dir('app'), file('README.md')]);
    responses.push([dir('Http', 'app/Http')]);
    responses.push([dir('Controllers', 'app/Http/Controllers')]);
    responses.push([file('web.php', 'app/Http/Controllers/web.php')]);
    const { findAllByTestId, container } = render(FileBrowser, {
      props: {
        workspaceId: 'ws_reveal',
        selectedPath: 'app/Http/Controllers/web.php',
      },
    });
    // The deeply-nested file row appears only after every ancestor is
    // expanded and lazy-loaded — proves the reveal walked the tree.
    await waitFor(async () => {
      const targetRow = container.querySelector(
        '[data-testid="file-row"][data-path="app/Http/Controllers/web.php"]'
      );
      expect(targetRow).not.toBeNull();
    });
    // Sanity: the full chain is present (5 rows = 2 root + 3 nested).
    const rows = await findAllByTestId('file-row');
    expect(rows.length).toBeGreaterThanOrEqual(5);
    // After reveal the expanded set contains every ancestor dir path.
    expect(workspaceTabs.expanded('ws_reveal').has('app')).toBe(true);
    expect(workspaceTabs.expanded('ws_reveal').has('app/Http')).toBe(true);
    expect(workspaceTabs.expanded('ws_reveal').has('app/Http/Controllers')).toBe(true);
    // The leaf file isn't a directory — it must NOT be in the expanded set.
    expect(workspaceTabs.expanded('ws_reveal').has('app/Http/Controllers/web.php')).toBe(false);
  });

  it('skips ancestor reloads when children are already cached', async () => {
    // Pre-populate the `app` directory by clicking it open first.
    responses.push([dir('app')]);
    responses.push([dir('Http', 'app/Http')]);
    const { findAllByTestId, rerender } = render(FileBrowser, {
      props: { workspaceId: 'ws_no_reload', selectedPath: null },
    });
    const rows = await findAllByTestId('file-row');
    await fireEvent.click(rows[0].querySelector('button')!);
    // Wait for the first-level expansion to actually surface — proves
    // the cached state is in place before we trigger the reveal flow.
    await waitFor(async () => {
      const after = await findAllByTestId('file-row');
      expect(after.length).toBeGreaterThan(1);
    });

    // Now stage only the LEAF response. If revealPath naively re-fetched
    // every ancestor, the cached `app` and `app/Http` calls would drain
    // the queue and cause a 404 here (or undefined response).
    responses.push([file('routes.php', 'app/Http/routes.php')]);
    await rerender({ workspaceId: 'ws_no_reload', selectedPath: 'app/Http/routes.php' });
    await waitFor(() => {
      expect(workspaceTabs.expanded('ws_no_reload').has('app')).toBe(true);
      expect(workspaceTabs.expanded('ws_no_reload').has('app/Http')).toBe(true);
    });
    // `responses` should be drained to 0 — only the leaf was fetched.
    expect(responses.length).toBe(0);
  });

  it('does not run reveal when selectedPath is null', async () => {
    responses.push([dir('app')]);
    render(FileBrowser, { props: { workspaceId: 'ws_no_target', selectedPath: null } });
    // Wait for the root load to settle, then assert no ancestors expanded.
    await waitFor(() => expect(responses.length).toBe(0));
    expect(workspaceTabs.expanded('ws_no_target').size).toBe(0);
  });
});
