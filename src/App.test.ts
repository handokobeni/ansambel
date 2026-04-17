// src/App.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '@testing-library/svelte';
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

import { repos } from '$lib/stores/repos.svelte';
import { workspaces } from '$lib/stores/workspaces.svelte';

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(repos.getSelected).mockReturnValue(null);
  vi.mocked(workspaces.getSelected).mockReturnValue(null);
  (repos as { selectedRepoId: string | null }).selectedRepoId = null;
  (workspaces as { selectedWorkspaceId: string | null }).selectedWorkspaceId = null;
});

describe('App', () => {
  it('renders TitleBar and Sidebar shell without errors', () => {
    render(App);
    // TitleBar renders
    expect(screen.getByText('No repo selected')).toBeInTheDocument();
    // Sidebar renders empty state
    expect(screen.getByText(/Select a repo/i)).toBeInTheDocument();
  });

  it('shows "Select or create a workspace" placeholder in main area when none selected', () => {
    render(App);
    expect(screen.getByText(/select or create a workspace/i)).toBeInTheDocument();
  });

  it('shows selected workspace title in main area when a workspace is selected', () => {
    vi.mocked(workspaces.getSelected).mockReturnValue({
      id: 'ws_abc123',
      repo_id: 'repo_abc123',
      branch: 'feat/task-1',
      base_branch: 'main',
      custom_branch: false,
      title: 'Fix login',
      description: '',
      status: 'not_started',
      column: 'todo',
      created_at: 1776000000,
      updated_at: 1776000000,
    });
    (workspaces as { selectedWorkspaceId: string | null }).selectedWorkspaceId = 'ws_abc123';
    render(App);
    expect(screen.getByText('Workspace: Fix login')).toBeInTheDocument();
  });
});
