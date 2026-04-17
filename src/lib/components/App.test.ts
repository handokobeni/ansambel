// src/lib/components/App.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock stores so we can observe their side effects
vi.mock('$lib/stores/mode.svelte', () => ({
  modeStore: { mode: 'plan', set: vi.fn() },
}));
vi.mock('$lib/stores/tasks.svelte', () => ({
  tasks: { selectedTaskId: null, selectTask: vi.fn() },
}));
vi.mock('$lib/ipc', () => ({
  api: {
    repo: { list: vi.fn().mockResolvedValue([]) },
    workspace: { list: vi.fn().mockResolvedValue([]) },
    task: { list: vi.fn().mockResolvedValue([]) },
    system: { getAppVersion: vi.fn().mockResolvedValue('0.3.0') },
  },
}));

import { render } from '@testing-library/svelte';
import { modeStore } from '$lib/stores/mode.svelte';
import App from './App.svelte';

function fire(key: string, opts: { ctrlKey?: boolean; metaKey?: boolean } = {}) {
  window.dispatchEvent(
    new KeyboardEvent('keydown', {
      key,
      ctrlKey: opts.ctrlKey ?? false,
      metaKey: opts.metaKey ?? false,
      bubbles: true,
    })
  );
}

describe('App shortcuts', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('Ctrl+1 switches to plan mode', async () => {
    render(App);
    fire('1', { ctrlKey: true });
    expect(modeStore.set).toHaveBeenCalledWith('plan');
  });

  it('Ctrl+2 switches to work mode', async () => {
    render(App);
    fire('2', { ctrlKey: true });
    expect(modeStore.set).toHaveBeenCalledWith('work');
  });

  it('Meta+1 (macOS) switches to plan mode', async () => {
    render(App);
    fire('1', { metaKey: true });
    expect(modeStore.set).toHaveBeenCalledWith('plan');
  });
});
