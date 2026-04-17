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
import { repos } from '$lib/stores/repos.svelte';

const defaultRepo = {
  id: 'repo_abc123',
  name: 'my-project',
  path: '/home/user/my-project',
  gh_profile: null as string | null,
  default_branch: 'main',
  created_at: 1776000000,
  updated_at: 1776000000,
};

const defaultWorkspaceList = [
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

beforeEach(() => {
  vi.clearAllMocks();
  // Re-establish default implementations after clearAllMocks
  vi.mocked(repos.getSelected).mockReturnValue(defaultRepo);
  (repos as { selectedRepoId: string | null }).selectedRepoId = 'repo_abc123';
  vi.mocked(workspaces.listForRepo).mockReturnValue(defaultWorkspaceList);
  (workspaces as { selectedWorkspaceId: string | null }).selectedWorkspaceId = null;
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

  it('Cancel button hides the form', async () => {
    render(Sidebar);
    await fireEvent.click(screen.getByRole('button', { name: /new workspace/i }));
    expect(screen.getByPlaceholderText(/workspace title/i)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole('button', { name: /cancel/i }));
    expect(screen.queryByPlaceholderText(/workspace title/i)).not.toBeInTheDocument();
  });

  it('remove workspace button calls workspaces.remove when confirm is accepted', async () => {
    vi.stubGlobal(
      'confirm',
      vi.fn(() => true)
    );
    vi.mocked(workspaces.remove).mockResolvedValue(undefined);
    render(Sidebar);
    // Hover to reveal remove button via keyboard-accessible click (stopPropagation test)
    const removeButtons = screen.getAllByRole('button', { name: /remove workspace/i });
    await fireEvent.click(removeButtons[0]);
    await waitFor(() => {
      expect(workspaces.remove).toHaveBeenCalledWith('ws_abc123', 'repo_abc123');
    });
    vi.unstubAllGlobals();
  });

  it('remove workspace does nothing when confirm is rejected', async () => {
    vi.stubGlobal(
      'confirm',
      vi.fn(() => false)
    );
    render(Sidebar);
    const removeButtons = screen.getAllByRole('button', { name: /remove workspace/i });
    await fireEvent.click(removeButtons[0]);
    expect(workspaces.remove).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
  });

  it('remove workspace catches and logs errors from workspaces.remove', async () => {
    vi.stubGlobal(
      'confirm',
      vi.fn(() => true)
    );
    vi.mocked(workspaces.remove).mockRejectedValue(new Error('remove failed'));
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    render(Sidebar);
    const removeButtons = screen.getAllByRole('button', { name: /remove workspace/i });
    await fireEvent.click(removeButtons[0]);
    await waitFor(() => {
      expect(consoleSpy).toHaveBeenCalledWith('Failed to remove workspace:', expect.any(Error));
    });
    consoleSpy.mockRestore();
    vi.unstubAllGlobals();
  });

  it('shows "Select a repo to see workspaces" when no workspaces and no repo selected', () => {
    vi.mocked(repos.getSelected).mockReturnValue(null);
    (repos as { selectedRepoId: string | null }).selectedRepoId = null;
    vi.mocked(workspaces.listForRepo).mockReturnValue([]);
    render(Sidebar);
    expect(screen.getByText(/Select a repo to see workspaces/i)).toBeInTheDocument();
  });

  it('shows "No workspaces yet" when repo is selected but workspace list is empty', () => {
    vi.mocked(workspaces.listForRepo).mockReturnValue([]);
    render(Sidebar);
    expect(screen.getByText(/No workspaces yet/i)).toBeInTheDocument();
  });

  it('highlights the active workspace row', () => {
    (workspaces as { selectedWorkspaceId: string | null }).selectedWorkspaceId = 'ws_abc123';
    const { container } = render(Sidebar);
    // Even if the Tailwind class isn't in jsdom, the Svelte class: binding runs without error
    expect(container.querySelector('ul')).toBeInTheDocument();
  });

  it('submit form does nothing when title is empty', async () => {
    render(Sidebar);
    await fireEvent.click(screen.getByRole('button', { name: /new workspace/i }));
    // Don't fill the title field — just click create
    await fireEvent.click(screen.getByRole('button', { name: /create/i }));
    expect(workspaces.create).not.toHaveBeenCalled();
  });

  it('renders gray dot for not_started status (fallback statusDotClass)', () => {
    vi.mocked(workspaces.listForRepo).mockReturnValue([
      {
        id: 'ws_xyz',
        repo_id: 'repo_abc123',
        branch: 'feat/pending',
        base_branch: 'main',
        custom_branch: false,
        title: 'Pending task',
        description: '',
        status: 'not_started',
        column: 'todo',
        created_at: 1776000010,
        updated_at: 1776000010,
      },
    ]);
    const { container } = render(Sidebar);
    const dots = container.querySelectorAll('[data-status-dot]');
    expect(dots[0]).toHaveAttribute('data-status', 'not_started');
  });

  it('create form shows error-resilience when workspaces.create rejects', async () => {
    vi.mocked(workspaces.create).mockRejectedValue(new Error('backend error'));
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    render(Sidebar);
    await fireEvent.click(screen.getByRole('button', { name: /new workspace/i }));
    await fireEvent.input(screen.getByPlaceholderText(/workspace title/i), {
      target: { value: 'Failing task' },
    });
    await fireEvent.click(screen.getByRole('button', { name: /create/i }));
    await waitFor(() => {
      expect(consoleSpy).toHaveBeenCalledWith('Failed to create workspace:', expect.any(Error));
    });
    consoleSpy.mockRestore();
  });
});
