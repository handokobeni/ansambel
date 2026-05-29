import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('$lib/ipc', () => ({
  api: {
    workspace: {
      create: vi.fn(),
      list: vi.fn(),
      remove: vi.fn(),
      setTeamActivityPrivate: vi.fn(),
    },
  },
}));

import { api } from '$lib/ipc';
import { WorkspacesStore } from './workspaces.svelte';
import { getToasts, removeToast } from '$lib/stores/toasts.svelte';
import type { WorkspaceInfo } from '$lib/types';

function clearToasts(): void {
  for (const id of Array.from(getToasts().keys())) removeToast(id);
}

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
  task_ids: [],
  ...overrides,
});

beforeEach(() => {
  vi.clearAllMocks();
  clearToasts();
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

  // ── Task 18: setTeamActivityPrivate ─────────────────────────────────
  describe('setTeamActivityPrivate', () => {
    it('optimistically toggles the workspace flag before the IPC resolves', async () => {
      const ws = makeWorkspace({ team_activity_private: false });
      vi.mocked(api.workspace.list).mockResolvedValue([ws]);
      // Hold the IPC promise open so we can observe the optimistic update.
      let resolveIpc!: () => void;
      vi.mocked(api.workspace.setTeamActivityPrivate).mockImplementation(
        () =>
          new Promise<void>((r) => {
            resolveIpc = r;
          })
      );
      const store = new WorkspacesStore();
      await store.loadForRepo('repo_abc123');
      const p = store.setTeamActivityPrivate('ws_abc123', 'repo_abc123', true);
      // Optimistic flip is visible immediately.
      expect(store.byRepo.get('repo_abc123')?.get('ws_abc123')?.team_activity_private).toBe(true);
      resolveIpc();
      const ok = await p;
      expect(ok).toBe(true);
      expect(api.workspace.setTeamActivityPrivate).toHaveBeenCalledWith('ws_abc123', true);
      // Stays toggled after the IPC resolves.
      expect(store.byRepo.get('repo_abc123')?.get('ws_abc123')?.team_activity_private).toBe(true);
    });

    it('reverts the flag and surfaces a toast when the IPC rejects', async () => {
      const ws = makeWorkspace({ team_activity_private: false });
      vi.mocked(api.workspace.list).mockResolvedValue([ws]);
      vi.mocked(api.workspace.setTeamActivityPrivate).mockRejectedValue('write fail');
      const store = new WorkspacesStore();
      await store.loadForRepo('repo_abc123');
      const ok = await store.setTeamActivityPrivate('ws_abc123', 'repo_abc123', true);
      expect(ok).toBe(false);
      // Reverted to original value.
      expect(store.byRepo.get('repo_abc123')?.get('ws_abc123')?.team_activity_private).toBe(false);
      const toasts = Array.from(getToasts().values());
      expect(toasts.some((t) => t.message.includes('write fail'))).toBe(true);
    });

    it('still calls IPC when the workspace is not yet in the local store', async () => {
      // Deep-link / pre-hydration path: the store doesn't know about the
      // workspace yet, but the IPC should still fire — the publisher will
      // catch up when the sidebar finishes loading.
      vi.mocked(api.workspace.setTeamActivityPrivate).mockResolvedValue(undefined);
      const store = new WorkspacesStore();
      const ok = await store.setTeamActivityPrivate('ws_unseen', 'repo_abc123', true);
      expect(ok).toBe(true);
      expect(api.workspace.setTeamActivityPrivate).toHaveBeenCalledWith('ws_unseen', true);
    });
  });
});

describe('WorkspacesStore.byId', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('returns the workspace when found across all repos', async () => {
    const ws = makeWorkspace();
    vi.mocked(api.workspace.list).mockResolvedValue([ws]);
    const store = new WorkspacesStore();
    await store.loadForRepo('repo_abc123');
    expect(store.byId('ws_abc123')).toEqual(ws);
  });

  it('returns undefined when the id does not exist in any repo', async () => {
    const ws = makeWorkspace();
    vi.mocked(api.workspace.list).mockResolvedValue([ws]);
    const store = new WorkspacesStore();
    await store.loadForRepo('repo_abc123');
    expect(store.byId('ws_nonexistent')).toBeUndefined();
  });
});
