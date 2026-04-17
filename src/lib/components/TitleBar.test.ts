// src/lib/components/TitleBar.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
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
    getSelected: vi.fn(() => null),
  },
}));

import { open } from '@tauri-apps/plugin-dialog';
import { repos } from '$lib/stores/repos.svelte';

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(repos.getSelected).mockReturnValue(null);
  (repos as { selectedRepoId: string | null }).selectedRepoId = null;
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

  it('clicking "Add Repo" opens folder dialog and calls repos.add with returned path', async () => {
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
    const addBtn = screen.getByRole('button', { name: /add repo/i });
    await fireEvent.click(addBtn);
    expect(open).toHaveBeenCalledWith({ directory: true, multiple: false });
    expect(repos.add).toHaveBeenCalledWith('/home/user/new-project');
  });

  it('does nothing when dialog is cancelled (open returns null)', async () => {
    vi.mocked(open).mockResolvedValue(null);
    render(TitleBar);
    await fireEvent.click(screen.getByRole('button', { name: /add repo/i }));
    expect(repos.add).not.toHaveBeenCalled();
  });

  it('does nothing when dialog returns an empty string', async () => {
    vi.mocked(open).mockResolvedValue('');
    render(TitleBar);
    await fireEvent.click(screen.getByRole('button', { name: /add repo/i }));
    expect(repos.add).not.toHaveBeenCalled();
  });
});
