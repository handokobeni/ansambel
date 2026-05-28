import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('$lib/ipc', () => ({
  api: {
    repo: {
      add: vi.fn(),
      list: vi.fn(),
      remove: vi.fn(),
      updateGhProfile: vi.fn(),
    },
    settings: {
      getSelectedRepo: vi.fn(),
      setSelectedRepo: vi.fn().mockResolvedValue(undefined),
    },
  },
}));

import { api } from '$lib/ipc';
import { ReposStore } from './repos.svelte';
import type { Repo } from '$lib/types';

const makeRepo = (overrides: Partial<Repo> = {}): Repo => ({
  id: 'repo_abc123',
  name: 'my-project',
  path: '/home/user/my-project',
  gh_profile: null,
  default_branch: 'main',
  created_at: 1776000000,
  updated_at: 1776000000,
  ...overrides,
});

beforeEach(() => {
  vi.clearAllMocks();
  // Re-establish the default success resolution for setSelectedRepo since
  // clearAllMocks wipes implementations along with call history.
  vi.mocked(api.settings.setSelectedRepo).mockResolvedValue(undefined);
});

describe('ReposStore', () => {
  it('load: populates the map from api.repo.list', async () => {
    const repo = makeRepo();
    vi.mocked(api.repo.list).mockResolvedValue([repo]);
    const store = new ReposStore();
    await store.load();
    expect(store.repos.get('repo_abc123')).toEqual(repo);
  });

  it('load: map is empty when backend returns []', async () => {
    vi.mocked(api.repo.list).mockResolvedValue([]);
    const store = new ReposStore();
    await store.load();
    expect(store.repos.size).toBe(0);
  });

  it('add: calls api.repo.add and inserts returned Repo into map', async () => {
    const repo = makeRepo();
    vi.mocked(api.repo.add).mockResolvedValue(repo);
    const store = new ReposStore();
    const result = await store.add('/home/user/my-project');
    expect(api.repo.add).toHaveBeenCalledWith('/home/user/my-project');
    expect(result).toEqual(repo);
    expect(store.repos.get('repo_abc123')).toEqual(repo);
  });

  it('remove: calls api.repo.remove and deletes from map', async () => {
    const repo = makeRepo();
    vi.mocked(api.repo.list).mockResolvedValue([repo]);
    vi.mocked(api.repo.remove).mockResolvedValue(undefined);
    const store = new ReposStore();
    await store.load();
    await store.remove('repo_abc123');
    expect(api.repo.remove).toHaveBeenCalledWith('repo_abc123');
    expect(store.repos.has('repo_abc123')).toBe(false);
  });

  it('updateGhProfile: calls api and updates the in-map entry', async () => {
    const repo = makeRepo();
    vi.mocked(api.repo.list).mockResolvedValue([repo]);
    vi.mocked(api.repo.updateGhProfile).mockResolvedValue(undefined);
    const store = new ReposStore();
    await store.load();
    await store.updateGhProfile('repo_abc123', 'handokoben');
    expect(api.repo.updateGhProfile).toHaveBeenCalledWith('repo_abc123', 'handokoben');
    expect(store.repos.get('repo_abc123')?.gh_profile).toBe('handokoben');
  });

  it('select: sets selectedRepoId', () => {
    const store = new ReposStore();
    store.select('repo_abc123');
    expect(store.selectedRepoId).toBe('repo_abc123');
  });

  it('select: accepts null to deselect', () => {
    const store = new ReposStore();
    store.select('repo_abc123');
    store.select(null);
    expect(store.selectedRepoId).toBeNull();
  });

  it('getSelected: returns null when selectedRepoId is set but repo not in map', () => {
    const store = new ReposStore();
    store.select('repo_nonexistent');
    expect(store.getSelected()).toBeNull();
  });

  it('remove: clears selectedRepoId when the selected repo is removed', async () => {
    const repo = makeRepo();
    vi.mocked(api.repo.list).mockResolvedValue([repo]);
    vi.mocked(api.repo.remove).mockResolvedValue(undefined);
    const store = new ReposStore();
    await store.load();
    store.select('repo_abc123');
    await store.remove('repo_abc123');
    expect(store.selectedRepoId).toBeNull();
  });

  it('getSelected: returns null when nothing selected', () => {
    const store = new ReposStore();
    expect(store.getSelected()).toBeNull();
  });

  it('getSelected: returns the Repo matching selectedRepoId', async () => {
    const repo = makeRepo();
    vi.mocked(api.repo.list).mockResolvedValue([repo]);
    const store = new ReposStore();
    await store.load();
    store.select('repo_abc123');
    expect(store.getSelected()).toEqual(repo);
  });

  it('select(id) persists the selection via api.settings.setSelectedRepo', () => {
    const store = new ReposStore();
    store.select('repo_kelola');
    expect(api.settings.setSelectedRepo).toHaveBeenCalledWith('repo_kelola');
  });

  it('select(null) persists null', () => {
    const store = new ReposStore();
    store.select(null);
    expect(api.settings.setSelectedRepo).toHaveBeenCalledWith(null);
  });

  it('select swallows persistence errors (never throws to caller)', async () => {
    vi.mocked(api.settings.setSelectedRepo).mockRejectedValueOnce('disk full');
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const store = new ReposStore();
    // Sync call must not throw even if the underlying promise rejects.
    expect(() => store.select('repo_x')).not.toThrow();
    // Flush microtasks so the .catch runs.
    await new Promise((r) => setTimeout(r, 0));
    expect(errSpy).toHaveBeenCalledWith('settings.setSelectedRepo failed', 'disk full');
    errSpy.mockRestore();
  });

  it('remove(activeId) clears persistence via select(null)', async () => {
    const repo = makeRepo();
    vi.mocked(api.repo.list).mockResolvedValue([repo]);
    vi.mocked(api.repo.remove).mockResolvedValue(undefined);
    const store = new ReposStore();
    await store.load();
    store.select('repo_abc123');
    vi.mocked(api.settings.setSelectedRepo).mockClear();
    await store.remove('repo_abc123');
    expect(api.settings.setSelectedRepo).toHaveBeenLastCalledWith(null);
    expect(store.selectedRepoId).toBeNull();
  });
});
