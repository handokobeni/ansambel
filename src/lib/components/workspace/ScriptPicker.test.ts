import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import type { RepoScript } from '$lib/types';

// State that the @tauri-apps/api/core mock reads. Reset per-test in
// beforeEach so cases are isolated.
let scripts: RepoScript[] = [];
let listError: unknown = null;
let runError: unknown = null;
const runCalls: { workspaceId: string; scriptId: string }[] = [];

vi.mock('@tauri-apps/api/core', () => {
  return {
    Channel: class {},
    invoke: vi.fn(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === 'script_list') {
        if (listError !== null) throw listError;
        return scripts;
      }
      if (cmd === 'script_run' && args) {
        if (runError !== null) throw runError;
        runCalls.push({
          workspaceId: args.workspaceId as string,
          scriptId: args.scriptId as string,
        });
        return undefined;
      }
      return undefined;
    }),
  };
});

import ScriptPicker from './ScriptPicker.svelte';
import { getToasts, removeToast } from '$lib/stores/toasts.svelte';

function clearToasts(): void {
  for (const id of Array.from(getToasts().keys())) removeToast(id);
}

beforeEach(() => {
  scripts = [];
  listError = null;
  runError = null;
  runCalls.length = 0;
  clearToasts();
});

afterEach(() => {
  vi.clearAllMocks();
  clearToasts();
});

describe('ScriptPicker', () => {
  it('renders the empty placeholder when the repo has no scripts', async () => {
    scripts = [];
    const { findByTestId } = render(ScriptPicker, {
      props: { repoId: 'repo_a', workspaceId: 'ws_a' },
    });
    const empty = await findByTestId('script-picker-empty');
    expect(empty.textContent).toMatch(/no scripts/i);
  });

  it('renders one button per script with its name', async () => {
    scripts = [
      { id: 'sc_1', name: 'dev', command: 'bun run dev' },
      { id: 'sc_2', name: 'test', command: 'bun test' },
    ];
    const { findAllByTestId } = render(ScriptPicker, {
      props: { repoId: 'repo_a', workspaceId: 'ws_a' },
    });
    const buttons = await findAllByTestId('script-picker-item');
    expect(buttons).toHaveLength(2);
    expect(buttons[0].textContent).toMatch(/dev/);
    expect(buttons[1].textContent).toMatch(/test/);
  });

  it('clicking a script invokes script_run with the right ids', async () => {
    scripts = [{ id: 'sc_1', name: 'dev', command: 'bun run dev' }];
    const { findAllByTestId } = render(ScriptPicker, {
      props: { repoId: 'repo_a', workspaceId: 'ws_a' },
    });
    const buttons = await findAllByTestId('script-picker-item');
    await fireEvent.click(buttons[0]);
    await waitFor(() => expect(runCalls.length).toBe(1));
    expect(runCalls[0]).toEqual({ workspaceId: 'ws_a', scriptId: 'sc_1' });
  });

  it('script_run failure surfaces a toast without crashing', async () => {
    scripts = [{ id: 'sc_1', name: 'dev', command: 'bun run dev' }];
    runError = 'pty failed to spawn';
    const { findAllByTestId } = render(ScriptPicker, {
      props: { repoId: 'repo_a', workspaceId: 'ws_a' },
    });
    const buttons = await findAllByTestId('script-picker-item');
    await fireEvent.click(buttons[0]);
    await waitFor(() => {
      const toasts = Array.from(getToasts().values());
      expect(toasts.some((t) => t.message.includes('pty failed to spawn'))).toBe(true);
    });
  });

  it('list failure surfaces an inline error message', async () => {
    listError = 'unknown repo';
    const { findByTestId } = render(ScriptPicker, {
      props: { repoId: 'repo_missing', workspaceId: 'ws_a' },
    });
    const err = await findByTestId('script-picker-error');
    expect(err.textContent).toMatch(/unknown repo/);
  });

  it('refetches scripts when repoId changes', async () => {
    scripts = [{ id: 'sc_1', name: 'dev', command: 'bun run dev' }];
    const { rerender, findAllByTestId } = render(ScriptPicker, {
      props: { repoId: 'repo_a', workspaceId: 'ws_a' },
    });
    await waitFor(async () => {
      const items = await findAllByTestId('script-picker-item');
      expect(items.map((b) => b.textContent?.trim())).toEqual(['dev']);
    });

    scripts = [
      { id: 'sc_2', name: 'lint', command: 'bun run lint' },
      { id: 'sc_3', name: 'check', command: 'bun run check' },
    ];
    await rerender({ repoId: 'repo_b', workspaceId: 'ws_a' });
    await waitFor(async () => {
      const items = await findAllByTestId('script-picker-item');
      expect(items).toHaveLength(2);
      expect(items[0].textContent).toMatch(/lint/);
      expect(items[1].textContent).toMatch(/check/);
    });
  });
});
