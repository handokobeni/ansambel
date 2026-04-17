// src/App.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import App from './App.svelte';

vi.mock('$lib/stores/repos.svelte', () => ({
  repos: {
    selectedRepoId: null as string | null,
    load: vi.fn().mockResolvedValue(undefined),
    getSelected: vi.fn(() => null),
    repos: new Map(),
  },
}));

vi.mock('$lib/stores/workspaces.svelte', () => ({
  workspaces: {
    selectedWorkspaceId: null as string | null,
    loadForRepo: vi.fn().mockResolvedValue(undefined),
    listForRepo: vi.fn(() => []),
    select: vi.fn(),
    create: vi.fn(),
    remove: vi.fn(),
    getSelected: vi.fn(() => null),
  },
}));

vi.mock('$lib/stores/tasks.svelte', () => ({
  tasks: {
    selectedTaskId: null as string | null,
    loadForRepo: vi.fn().mockResolvedValue(undefined),
    listForRepo: vi.fn(() => []),
    listForColumn: vi.fn(() => []),
    add: vi.fn(),
    update: vi.fn(),
    move: vi.fn().mockResolvedValue(undefined),
    remove: vi.fn(),
  },
}));

vi.mock('$lib/stores/mode.svelte', () => {
  const state = { mode: 'plan' as 'plan' | 'work' };
  return {
    modeStore: {
      get mode() {
        return state.mode;
      },
      set: vi.fn((next: 'plan' | 'work') => {
        state.mode = next;
      }),
    },
  };
});

vi.mock('$lib/keyboard', () => ({
  ShortcutRegistry: vi.fn().mockImplementation(() => ({
    register: vi.fn(),
    destroy: vi.fn(),
  })),
}));

import { repos } from '$lib/stores/repos.svelte';
import { workspaces } from '$lib/stores/workspaces.svelte';
import { tasks } from '$lib/stores/tasks.svelte';
import { modeStore } from '$lib/stores/mode.svelte';

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(repos.getSelected).mockReturnValue(null);
  vi.mocked(workspaces.getSelected).mockReturnValue(null);
  (repos as { selectedRepoId: string | null }).selectedRepoId = null;
  (workspaces as { selectedWorkspaceId: string | null }).selectedWorkspaceId = null;
  modeStore.set('plan');
});

describe('App', () => {
  it('renders TitleBar and Sidebar shells', () => {
    render(App);
    expect(screen.getByRole('button', { name: /add repo/i })).toBeInTheDocument();
    // Sidebar header label "WORKSPACES" — match by exact uppercase to disambiguate
    expect(screen.getByText('Workspaces')).toBeInTheDocument();
  });

  it('plan mode shows "Add a repo to start" when no repo selected', async () => {
    render(App);
    await waitFor(() => {
      expect(screen.getByText(/add a repo to start managing tasks/i)).toBeInTheDocument();
    });
  });

  it('plan mode renders KanbanBoard columns when a repo is selected', async () => {
    vi.mocked(repos.getSelected).mockReturnValue({
      id: 'repo_abc123',
      name: 'my-project',
      path: '/home/user/my-project',
      gh_profile: null,
      default_branch: 'main',
      created_at: 1776000000,
      updated_at: 1776000000,
    });
    render(App);
    await waitFor(() => {
      expect(screen.getByText(/in progress/i)).toBeInTheDocument();
      expect(screen.getByText(/review/i)).toBeInTheDocument();
    });
  });

  it('work mode shows "Select or create a workspace" when none selected', async () => {
    modeStore.set('work');
    render(App);
    await waitFor(() => {
      expect(screen.getByText(/select or create a workspace/i)).toBeInTheDocument();
    });
  });

  it('work mode shows workspace name when one is selected', async () => {
    modeStore.set('work');
    vi.mocked(workspaces.getSelected).mockReturnValue({
      id: 'ws_abc',
      repo_id: 'repo_abc',
      branch: 'feat/test',
      base_branch: 'main',
      custom_branch: false,
      title: 'Test workspace',
      description: '',
      status: 'waiting',
      column: 'in_progress',
      created_at: 1776000001,
      updated_at: 1776000001,
    });
    render(App);
    await waitFor(() => {
      expect(screen.getByText(/workspace: test workspace/i)).toBeInTheDocument();
    });
  });

  it('hydrates tasks for the selected repo on mount', async () => {
    vi.mocked(repos.getSelected).mockReturnValue({
      id: 'repo_xyz',
      name: 'xyz',
      path: '/x/y/z',
      gh_profile: null,
      default_branch: 'main',
      created_at: 1,
      updated_at: 1,
    });
    (repos as { selectedRepoId: string | null }).selectedRepoId = 'repo_xyz';
    render(App);
    await waitFor(() => {
      expect(tasks.loadForRepo).toHaveBeenCalledWith('repo_xyz');
      expect(workspaces.loadForRepo).toHaveBeenCalledWith('repo_xyz');
    });
  });

  it('clicking Plan/Work toggle switches mode store', async () => {
    render(App);
    const workBtn = await screen.findByRole('button', { name: /^work$/i });
    await fireEvent.click(workBtn);
    expect(modeStore.set).toHaveBeenCalledWith('work');
  });
});
