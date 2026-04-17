// src/lib/components/Sidebar.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import Sidebar from './Sidebar.svelte';

vi.mock('$lib/stores/repos.svelte', () => ({
  repos: {
    selectedRepoId: 'repo_abc123' as string | null,
    getSelected: vi.fn(() => ({
      id: 'repo_abc123',
      name: 'my-project',
      path: '/home/user/my-project',
      gh_profile: null,
      default_branch: 'main',
      created_at: 1776000000,
      updated_at: 1776000000,
    })),
  },
}));

vi.mock('$lib/stores/workspaces.svelte', () => {
  const workspaceList = [
    {
      id: 'ws_abc123',
      repo_id: 'repo_abc123',
      branch: 'feat/task-1',
      base_branch: 'main',
      custom_branch: false,
      title: 'Fix login',
      description: 'Fixing the login bug',
      status: 'running' as const,
      column: 'in_progress' as const,
      created_at: 1776000001,
      updated_at: 1776000001,
    },
    {
      id: 'ws_def456',
      repo_id: 'repo_abc123',
      branch: 'feat/task-2',
      base_branch: 'main',
      custom_branch: false,
      title: 'Add dark mode',
      description: '',
      status: 'waiting' as const,
      column: 'todo' as const,
      created_at: 1776000002,
      updated_at: 1776000002,
    },
  ];
  return {
    workspaces: {
      selectedWorkspaceId: null as string | null,
      listForRepo: vi.fn(() => workspaceList),
      create: vi.fn(),
      remove: vi.fn(),
      select: vi.fn(),
    },
  };
});

import { workspaces } from '$lib/stores/workspaces.svelte';

beforeEach(() => {
  vi.clearAllMocks();
});

describe('Sidebar', () => {
  it('renders workspace titles from the store for the selected repo', () => {
    render(Sidebar);
    expect(screen.getByText('Fix login')).toBeInTheDocument();
    expect(screen.getByText('Add dark mode')).toBeInTheDocument();
  });

  it('shows amber status dot for running workspace and olive for waiting', () => {
    const { container } = render(Sidebar);
    const dots = container.querySelectorAll('[data-status-dot]');
    expect(dots[0]).toHaveAttribute('data-status', 'running');
    expect(dots[1]).toHaveAttribute('data-status', 'waiting');
  });

  it('clicking a workspace row calls workspaces.select with its id', async () => {
    render(Sidebar);
    await fireEvent.click(screen.getByText('Fix login'));
    expect(workspaces.select).toHaveBeenCalledWith('ws_abc123');
  });

  it('"New Workspace" button reveals the inline form', async () => {
    render(Sidebar);
    expect(screen.queryByPlaceholderText(/workspace title/i)).not.toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: /new workspace/i }));
    expect(screen.getByPlaceholderText(/workspace title/i)).toBeInTheDocument();
  });

  it('submitting the form calls workspaces.create with repoId and form values', async () => {
    vi.mocked(workspaces.create).mockResolvedValue({
      id: 'ws_new111',
      repo_id: 'repo_abc123',
      branch: 'feat/new-ws',
      base_branch: 'main',
      custom_branch: false,
      title: 'My new task',
      description: 'A description',
      status: 'not_started',
      column: 'todo',
      created_at: 1776000003,
      updated_at: 1776000003,
    });
    render(Sidebar);
    await fireEvent.click(screen.getByRole('button', { name: /new workspace/i }));
    await fireEvent.input(screen.getByPlaceholderText(/workspace title/i), {
      target: { value: 'My new task' },
    });
    await fireEvent.input(screen.getByPlaceholderText(/description/i), {
      target: { value: 'A description' },
    });
    await fireEvent.click(screen.getByRole('button', { name: /create/i }));
    await waitFor(() => {
      expect(workspaces.create).toHaveBeenCalledWith({
        repoId: 'repo_abc123',
        title: 'My new task',
        description: 'A description',
        branchName: undefined,
      });
    });
  });
});
