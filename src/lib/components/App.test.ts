// src/lib/components/App.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';

// Mock stores so we can observe their side effects
vi.mock('$lib/stores/mode.svelte', () => ({
  modeStore: { mode: 'plan', set: vi.fn() },
}));
vi.mock('$lib/stores/tasks.svelte', () => ({
  tasks: {
    selectedTaskId: null,
    selectTask: vi.fn(),
    loadForRepo: vi.fn().mockResolvedValue(undefined),
    listForRepo: vi.fn(() => []),
    add: vi.fn(),
    move: vi.fn(),
    remove: vi.fn(),
  },
}));
vi.mock('$lib/stores/repos.svelte', () => ({
  repos: {
    selectedRepoId: null,
    load: vi.fn().mockResolvedValue(undefined),
    getSelected: vi.fn(() => null),
    add: vi.fn(),
    select: vi.fn(),
  },
}));
vi.mock('$lib/stores/workspaces.svelte', () => ({
  workspaces: {
    loadForRepo: vi.fn().mockResolvedValue(undefined),
  },
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

// Append to App.test.ts — integration rendering tests

describe('App mode rendering', () => {
  it('renders KanbanBoard when mode is plan', async () => {
    // Override modeStore.mode to 'plan' for this test
    vi.mocked(modeStore).mode = 'plan';
    // Provide a selected repo so App renders KanbanBoard (not empty state)
    const { repos } = await import('$lib/stores/repos.svelte');
    vi.mocked(repos.getSelected).mockReturnValue({
      id: 'repo_abc123',
      name: 'my-project',
      path: '/home/user/my-project',
      gh_profile: null,
      default_branch: 'main',
      created_at: 1776000000,
      updated_at: 1776000000,
    });
    const { container } = render(App);
    // KanbanBoard renders a div.kanban-board
    expect(container.querySelector('.kanban-board')).toBeTruthy();
  });

  it('renders Work mode placeholder when mode is work', async () => {
    vi.mocked(modeStore).mode = 'work';
    const { container } = render(App);
    expect(container.querySelector('.work-placeholder')).toBeTruthy();
    expect(container.querySelector('.kanban-board')).toBeNull();
  });

  it('TitleBar is always rendered regardless of mode', async () => {
    const { container } = render(App);
    expect(container.querySelector('.titlebar')).toBeTruthy();
  });
});
