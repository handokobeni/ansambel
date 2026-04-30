// src/lib/components/TitleBar.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import TitleBar from './TitleBar.svelte';

// Mock @tauri-apps/plugin-dialog
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));

// Mock the repos store
vi.mock('$lib/stores/repos.svelte', () => ({
  repos: {
    selectedRepoId: null as string | null,
    repos: new Map(),
    add: vi.fn(),
    select: vi.fn(),
    getSelected: vi.fn(() => null),
  },
}));

// Mock the workspaces store (TitleBar calls workspaces.loadForRepo after add)
vi.mock('$lib/stores/workspaces.svelte', () => ({
  workspaces: {
    loadForRepo: vi.fn(),
  },
}));

// Mock the tasks store — TitleBar should refresh kanban tasks after Add Repo
// so that re-adding an existing repo (or first-add) populates the board
// without waiting for an app restart.
vi.mock('$lib/stores/tasks.svelte', () => ({
  tasks: {
    loadForRepo: vi.fn(),
  },
}));

import { open } from '@tauri-apps/plugin-dialog';
import { repos } from '$lib/stores/repos.svelte';
import { workspaces } from '$lib/stores/workspaces.svelte';
import { tasks } from '$lib/stores/tasks.svelte';

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(repos.getSelected).mockReturnValue(null);
  (repos as { selectedRepoId: string | null }).selectedRepoId = null;
  // Default loadForRepo to resolve immediately so the promise chain completes.
  vi.mocked(workspaces.loadForRepo).mockResolvedValue(undefined);
  vi.mocked(tasks.loadForRepo).mockResolvedValue(undefined);
  // Silence alert() during error-path tests.
  vi.stubGlobal('alert', vi.fn());
});

describe('TitleBar', () => {
  it('renders "No repo selected" when no repo is selected', () => {
    render(TitleBar);
    expect(screen.getByText('No repo selected')).toBeInTheDocument();
  });

  it('shows selected repo name when a repo is selected', () => {
    vi.mocked(repos.getSelected).mockReturnValue({
      id: 'repo_abc123',
      name: 'my-project',
      path: '/home/user/my-project',
      gh_profile: null,
      default_branch: 'main',
      created_at: 1776000000,
      updated_at: 1776000000,
    });
    render(TitleBar);
    expect(screen.getByText('my-project')).toBeInTheDocument();
  });

  it('clicking "Add Repo" opens folder dialog, calls repos.add, selects, and loads workspaces + tasks', async () => {
    vi.mocked(open).mockResolvedValue('/home/user/new-project');
    vi.mocked(repos.add).mockResolvedValue({
      id: 'repo_new111',
      name: 'new-project',
      path: '/home/user/new-project',
      gh_profile: null,
      default_branch: 'main',
      created_at: 1776000001,
      updated_at: 1776000001,
    });
    render(TitleBar);
    await fireEvent.click(screen.getByRole('button', { name: /add repo/i }));
    expect(open).toHaveBeenCalledWith({ directory: true, multiple: false });
    expect(repos.add).toHaveBeenCalledWith('/home/user/new-project');
    await waitFor(() => {
      expect(repos.select).toHaveBeenCalledWith('repo_new111');
      expect(workspaces.loadForRepo).toHaveBeenCalledWith('repo_new111');
      // Re-Add of an existing repo (idempotent on the backend) must also
      // hydrate the kanban — otherwise the board stays empty until the next
      // app restart even though tasks.json already contains them.
      expect(tasks.loadForRepo).toHaveBeenCalledWith('repo_new111');
    });
  });

  it('does nothing when dialog is cancelled (open returns null)', async () => {
    vi.mocked(open).mockResolvedValue(null);
    render(TitleBar);
    await fireEvent.click(screen.getByRole('button', { name: /add repo/i }));
    expect(repos.add).not.toHaveBeenCalled();
    expect(repos.select).not.toHaveBeenCalled();
    expect(workspaces.loadForRepo).not.toHaveBeenCalled();
  });

  it('does nothing when dialog returns an empty string', async () => {
    vi.mocked(open).mockResolvedValue('');
    render(TitleBar);
    await fireEvent.click(screen.getByRole('button', { name: /add repo/i }));
    expect(repos.add).not.toHaveBeenCalled();
  });

  it('surfaces an alert when repos.add throws', async () => {
    vi.mocked(open).mockResolvedValue('/home/user/bad-project');
    vi.mocked(repos.add).mockRejectedValue(new Error('not a git repository'));
    render(TitleBar);
    await fireEvent.click(screen.getByRole('button', { name: /add repo/i }));
    await waitFor(() => {
      expect(globalThis.alert).toHaveBeenCalledWith(
        expect.stringContaining('not a git repository')
      );
    });
    expect(repos.select).not.toHaveBeenCalled();
  });

  it('coerces non-Error rejections to a string for the alert', async () => {
    // Covers the err-instanceof-Error fallback branch. Tauri commands
    // commonly reject with a plain string rather than an Error object.
    vi.mocked(open).mockResolvedValue('/home/user/raw-string-error');
    vi.mocked(repos.add).mockRejectedValue('plain string failure');
    render(TitleBar);
    await fireEvent.click(screen.getByRole('button', { name: /add repo/i }));
    await waitFor(() => {
      expect(globalThis.alert).toHaveBeenCalledWith(
        expect.stringContaining('plain string failure')
      );
    });
  });

  it('ignores a second click while the first add is in flight', async () => {
    // Covers the `if (adding) return;` short-circuit so a fast double-tap
    // doesn't open two dialogs / fire two backend calls.
    let resolveOpen!: (v: string) => void;
    vi.mocked(open).mockReturnValue(
      new Promise<string>((r) => {
        resolveOpen = r;
      }) as unknown as Promise<string | string[] | null>
    );
    render(TitleBar);
    const btn = screen.getByRole('button', { name: /add repo/i });
    await fireEvent.click(btn);
    await fireEvent.click(btn);
    // Only one open() call regardless of double-click.
    expect(open).toHaveBeenCalledTimes(1);
    resolveOpen('/cancel-anyway');
  });
});

describe('TitleBar mode toggle', () => {
  it('renders Plan and Work buttons', () => {
    render(TitleBar, {
      props: { mode: 'plan', onModeChange: vi.fn() },
    });
    expect(screen.getByRole('button', { name: /^plan$/i })).toBeTruthy();
    expect(screen.getByRole('button', { name: /^work$/i })).toBeTruthy();
  });

  it('Plan button has active class when mode is plan', () => {
    render(TitleBar, {
      props: { mode: 'plan', onModeChange: vi.fn() },
    });
    const planBtn = screen.getByRole('button', { name: /^plan$/i });
    expect(planBtn.classList.contains('active')).toBe(true);
  });

  it('clicking Work button calls onModeChange with work', async () => {
    const onModeChange = vi.fn();
    render(TitleBar, {
      props: { mode: 'plan', onModeChange },
    });
    await fireEvent.click(screen.getByRole('button', { name: /^work$/i }));
    expect(onModeChange).toHaveBeenCalledWith('work');
  });
});
