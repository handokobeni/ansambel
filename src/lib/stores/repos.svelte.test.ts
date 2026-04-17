import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('$lib/ipc', () => ({
  api: {
    repo: {
      add: vi.fn(),
      list: vi.fn(),
      remove: vi.fn(),
      updateGhProfile: vi.fn(),
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
});
