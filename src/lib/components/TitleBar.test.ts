// src/lib/components/TitleBar.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import TitleBar from './TitleBar.svelte';

// Mock @tauri-apps/plugin-dialog
vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn(),
}));

// TitleBar mounts a `lark-view-missing` event listener; stub Tauri's event
// module so `listen()` doesn't reach for an IPC bridge that JSDOM lacks.
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

// SettingsDialog → LarkSettings calls invoke('get_lark_status') on mount;
// stub it so opening the dialog in tests doesn't hit the network.
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() =>
    Promise.resolve({
      configured: false,
      app_id: null,
      app_token: null,
      table_id: null,
      base_url: 'https://open.larksuite.com',
      has_secret: false,
    })
  ),
  Channel: class {},
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
import { getToasts, removeToast } from '$lib/stores/toasts.svelte';
import { viewMissing } from '$lib/stores/view-missing.svelte';

/**
 * Helper for the view-missing banner tests: stub `repos.getSelected()` to
 * return a minimal RepoInfo for `repoId`, then render the TitleBar. Keeps the
 * fixture in one place so we don't repeat the RepoInfo literal in every test.
 */
function renderTitleBarForRepo(repoId: string) {
  vi.mocked(repos.getSelected).mockReturnValue({
    id: repoId,
    name: repoId,
    path: `/home/user/${repoId}`,
    gh_profile: null,
    default_branch: 'main',
    created_at: 1776000000,
    updated_at: 1776000000,
  });
  return render(TitleBar);
}

function clearAllToasts(): void {
  for (const id of Array.from(getToasts().keys())) removeToast(id);
}

function findToastByText(needle: string | RegExp): string | null {
  for (const t of getToasts().values()) {
    const match = typeof needle === 'string' ? t.message.includes(needle) : needle.test(t.message);
    if (match) return t.message;
  }
  return null;
}

beforeEach(() => {
  vi.clearAllMocks();
  clearAllToasts();
  vi.mocked(repos.getSelected).mockReturnValue(null);
  (repos as { selectedRepoId: string | null }).selectedRepoId = null;
  // Default loadForRepo to resolve immediately so the promise chain completes.
  vi.mocked(workspaces.loadForRepo).mockResolvedValue(undefined);
  vi.mocked(tasks.loadForRepo).mockResolvedValue(undefined);
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

  it('shows an error toast when repos.add throws', async () => {
    vi.mocked(open).mockResolvedValue('/home/user/bad-project');
    vi.mocked(repos.add).mockRejectedValue(new Error('not a git repository'));
    render(TitleBar);
    await fireEvent.click(screen.getByRole('button', { name: /add repo/i }));
    await waitFor(() => {
      expect(findToastByText('not a git repository')).not.toBeNull();
    });
    expect(repos.select).not.toHaveBeenCalled();
  });

  it('coerces non-Error rejections to a string for the toast', async () => {
    // Covers the err-instanceof-Error fallback branch. Tauri commands
    // commonly reject with a plain string rather than an Error object.
    vi.mocked(open).mockResolvedValue('/home/user/raw-string-error');
    vi.mocked(repos.add).mockRejectedValue('plain string failure');
    render(TitleBar);
    await fireEvent.click(screen.getByRole('button', { name: /add repo/i }));
    await waitFor(() => {
      expect(findToastByText('plain string failure')).not.toBeNull();
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

describe('TitleBar theme toggle', () => {
  it('renders a theme toggle button reflecting the current mode', () => {
    const { container } = render(TitleBar);
    const btn = container.querySelector<HTMLButtonElement>('[data-theme-toggle]');
    expect(btn).not.toBeNull();
    // Default install lands on dark, so the button shows the sun (click to flip to light).
    expect(btn?.dataset.mode).toBe('dark');
    expect(btn?.getAttribute('aria-label')).toMatch(/light/i);
  });

  it('clicking the theme toggle flips dark↔light', async () => {
    const themeMod = await import('$lib/stores/theme.svelte');
    // Reset to a known starting point so the test is independent of order.
    themeMod.theme.setColorMode('dark');
    const { container } = render(TitleBar);
    const btn = container.querySelector<HTMLButtonElement>('[data-theme-toggle]')!;
    expect(btn.dataset.mode).toBe('dark');
    await fireEvent.click(btn);
    expect(themeMod.theme.colorMode).toBe('light');
    await fireEvent.click(btn);
    expect(themeMod.theme.colorMode).toBe('dark');
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

describe('TitleBar repo context menu', () => {
  beforeEach(() => {
    vi.mocked(repos.getSelected).mockReturnValue({
      id: 'repo_abc123',
      name: 'my-project',
      path: '/home/user/my-project',
      gh_profile: null,
      default_branch: 'main',
      created_at: 1776000000,
      updated_at: 1776000000,
    });
  });

  it('right-click on the repo name opens RepoSettingsDialog', async () => {
    render(TitleBar);
    const repoRow = screen.getByTestId('repo-row-repo_abc123');
    await fireEvent.contextMenu(repoRow);
    expect(screen.getByTestId('binding-empty')).toBeInTheDocument();
  });

  it('closing RepoSettingsDialog via close button hides it', async () => {
    render(TitleBar);
    const repoRow = screen.getByTestId('repo-row-repo_abc123');
    await fireEvent.contextMenu(repoRow);
    expect(screen.getByTestId('binding-empty')).toBeInTheDocument();
    await fireEvent.click(screen.getByTestId('repo-settings-close'));
    expect(screen.queryByTestId('binding-empty')).toBeNull();
  });

  it('does not render the repo-row testid when no repo is selected', () => {
    vi.mocked(repos.getSelected).mockReturnValue(null);
    render(TitleBar);
    expect(screen.queryByTestId(/^repo-row-/)).toBeNull();
  });

  it('clicking the gear icon opens RepoSettingsDialog', async () => {
    render(TitleBar);
    await fireEvent.click(screen.getByTestId('open-repo-settings'));
    expect(screen.getByTestId('binding-empty')).toBeInTheDocument();
  });

  it('does not render the gear icon when no repo is selected', () => {
    vi.mocked(repos.getSelected).mockReturnValue(null);
    render(TitleBar);
    expect(screen.queryByTestId('open-repo-settings')).toBeNull();
  });
});

describe('TitleBar view-missing banner', () => {
  beforeEach(() => {
    viewMissing.clear();
  });

  it('shows banner when viewMissing store has entry for selected repo', async () => {
    viewMissing.clear();
    viewMissing.report('repo_x', 'vw_gone');
    renderTitleBarForRepo('repo_x');
    expect(screen.getByTestId('view-missing-banner')).toHaveTextContent('vw_gone');
  });

  it('dismiss removes the banner', async () => {
    viewMissing.report('repo_x', 'vw_gone');
    renderTitleBarForRepo('repo_x');
    await fireEvent.click(screen.getByTestId('view-missing-dismiss'));
    expect(screen.queryByTestId('view-missing-banner')).not.toBeInTheDocument();
  });

  it('hides banner when selected repo has no missing view', async () => {
    viewMissing.clear();
    viewMissing.report('repo_other', 'vw_gone');
    renderTitleBarForRepo('repo_x');
    expect(screen.queryByTestId('view-missing-banner')).not.toBeInTheDocument();
  });
});

describe('TitleBar settings button', () => {
  it('renders a settings button', () => {
    render(TitleBar);
    expect(screen.getByTestId('open-settings')).toBeTruthy();
  });

  it('clicking settings button opens the settings dialog', async () => {
    render(TitleBar);
    // Dialog is hidden initially.
    expect(screen.queryByRole('dialog')).toBeNull();
    await fireEvent.click(screen.getByTestId('open-settings'));
    expect(screen.getByRole('dialog')).toBeTruthy();
    expect(screen.getByText('Settings')).toBeTruthy();
  });

  it('closing the dialog returns focus to title bar (dialog unmounts)', async () => {
    render(TitleBar);
    await fireEvent.click(screen.getByTestId('open-settings'));
    expect(screen.getByRole('dialog')).toBeTruthy();
    await fireEvent.click(screen.getByTestId('settings-close'));
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});
