import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('$lib/ipc', () => ({
  api: {
    workspace: {
      create: vi.fn(),
      list: vi.fn(),
      remove: vi.fn(),
    },
  },
}));

import { api } from '$lib/ipc';
import { WorkspacesStore } from './workspaces.svelte';
import type { WorkspaceInfo } from '$lib/types';

const makeWorkspace = (overrides: Partial<WorkspaceInfo> = {}): WorkspaceInfo => ({
  id: 'ws_abc123',
  repo_id: 'repo_abc123',
  branch: 'feat/task-1',
  base_branch: 'main',
  custom_branch: false,
  title: 'Fix login',
  description: 'Fixing the login bug',
  status: 'not_started',
  column: 'todo',
  created_at: 1776000000,
  updated_at: 1776000000,
  worktree_dir: '/tmp/ws_abc123',
  ...overrides,
});

beforeEach(() => {
  vi.clearAllMocks();
});

describe('WorkspacesStore', () => {
  it('loadForRepo: populates nested map for a repoId', async () => {
    const ws = makeWorkspace();
    vi.mocked(api.workspace.list).mockResolvedValue([ws]);
    const store = new WorkspacesStore();
    await store.loadForRepo('repo_abc123');
    expect(api.workspace.list).toHaveBeenCalledWith('repo_abc123');
    expect(store.byRepo.get('repo_abc123')?.get('ws_abc123')).toEqual(ws);
  });

  it('loadForRepo: empty inner map when no workspaces returned', async () => {
    vi.mocked(api.workspace.list).mockResolvedValue([]);
    const store = new WorkspacesStore();
    await store.loadForRepo('repo_abc123');
    expect(store.byRepo.get('repo_abc123')?.size).toBe(0);
  });

  it('create: calls api and inserts into nested map', async () => {
    const ws = makeWorkspace();
    vi.mocked(api.workspace.create).mockResolvedValue(ws);
    const store = new WorkspacesStore();
    const result = await store.create({
      repoId: 'repo_abc123',
      title: 'Fix login',
      description: 'Fixing the login bug',
    });
    expect(result).toEqual(ws);
    expect(store.byRepo.get('repo_abc123')?.get('ws_abc123')).toEqual(ws);
  });

  it('remove: calls api and deletes from nested map', async () => {
    const ws = makeWorkspace();
    vi.mocked(api.workspace.list).mockResolvedValue([ws]);
    vi.mocked(api.workspace.remove).mockResolvedValue(undefined);
    const store = new WorkspacesStore();
    await store.loadForRepo('repo_abc123');
    await store.remove('ws_abc123', 'repo_abc123');
    expect(api.workspace.remove).toHaveBeenCalledWith('ws_abc123');
    expect(store.byRepo.get('repo_abc123')?.has('ws_abc123')).toBe(false);
  });

  it('listForRepo: returns workspaces array for a repoId', async () => {
    const ws1 = makeWorkspace({ id: 'ws_111111' });
    const ws2 = makeWorkspace({ id: 'ws_222222' });
    vi.mocked(api.workspace.list).mockResolvedValue([ws1, ws2]);
    const store = new WorkspacesStore();
    await store.loadForRepo('repo_abc123');
    const list = store.listForRepo('repo_abc123');
    expect(list).toHaveLength(2);
    expect(list.map((w) => w.id)).toContain('ws_111111');
    expect(list.map((w) => w.id)).toContain('ws_222222');
  });

  it('listForRepo: returns [] for unknown repoId', () => {
    const store = new WorkspacesStore();
    expect(store.listForRepo('repo_unknown')).toEqual([]);
  });

  it('select: sets selectedWorkspaceId', () => {
    const store = new WorkspacesStore();
    store.select('ws_abc123');
    expect(store.selectedWorkspaceId).toBe('ws_abc123');
  });

  it('getSelected: returns null when nothing selected', () => {
    const store = new WorkspacesStore();
    expect(store.getSelected()).toBeNull();
  });

  it('getSelected: returns the Workspace matching selectedWorkspaceId', async () => {
    const ws = makeWorkspace();
    vi.mocked(api.workspace.list).mockResolvedValue([ws]);
    const store = new WorkspacesStore();
    await store.loadForRepo('repo_abc123');
    store.select('ws_abc123');
    expect(store.getSelected()).toEqual(ws);
  });

  it('getSelected: returns null when selectedWorkspaceId is set but no matching workspace exists', async () => {
    vi.mocked(api.workspace.list).mockResolvedValue([makeWorkspace()]);
    const store = new WorkspacesStore();
    await store.loadForRepo('repo_abc123');
    store.select('ws_nonexistent');
    expect(store.getSelected()).toBeNull();
  });

  it('remove: clears selectedWorkspaceId when the selected workspace is removed', async () => {
    const ws = makeWorkspace();
    vi.mocked(api.workspace.list).mockResolvedValue([ws]);
    vi.mocked(api.workspace.remove).mockResolvedValue(undefined);
    const store = new WorkspacesStore();
    await store.loadForRepo('repo_abc123');
    store.select('ws_abc123');
    await store.remove('ws_abc123', 'repo_abc123');
    expect(store.selectedWorkspaceId).toBeNull();
  });

  it('create: reuses existing inner map when repo already has workspaces', async () => {
    const ws1 = makeWorkspace({ id: 'ws_first' });
    const ws2 = makeWorkspace({ id: 'ws_second', title: 'Second task' });
    vi.mocked(api.workspace.list).mockResolvedValue([ws1]);
    vi.mocked(api.workspace.create).mockResolvedValue(ws2);
    const store = new WorkspacesStore();
    await store.loadForRepo('repo_abc123');
    await store.create({ repoId: 'repo_abc123', title: 'Second task', description: '' });
    // Both workspaces should be in the same inner map
    expect(store.byRepo.get('repo_abc123')?.size).toBe(2);
  });
});
