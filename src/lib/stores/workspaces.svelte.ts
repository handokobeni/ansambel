// src/lib/stores/workspaces.svelte.ts
import { SvelteMap } from 'svelte/reactivity';
import { api } from '$lib/ipc';
import { addToast } from '$lib/stores/toasts.svelte';
import type { WorkspaceInfo, CreateWorkspaceArgs } from '$lib/types';

export class WorkspacesStore {
  readonly byRepo = new SvelteMap<string, SvelteMap<string, WorkspaceInfo>>();
  selectedWorkspaceId = $state<string | null>(null);

  private getOrCreateInner(repoId: string): SvelteMap<string, WorkspaceInfo> {
    let inner = this.byRepo.get(repoId);
    if (!inner) {
      inner = new SvelteMap<string, WorkspaceInfo>();
      this.byRepo.set(repoId, inner);
    }
    return inner;
  }

  async loadForRepo(repoId: string): Promise<void> {
    const list = await api.workspace.list(repoId);
    const inner = this.getOrCreateInner(repoId);
    inner.clear();
    for (const ws of list) {
      inner.set(ws.id, ws);
    }
  }

  async create(args: CreateWorkspaceArgs): Promise<WorkspaceInfo> {
    const ws = await api.workspace.create(args);
    const inner = this.getOrCreateInner(ws.repo_id);
    inner.set(ws.id, ws);
    return ws;
  }

  async remove(id: string, repoId: string): Promise<void> {
    await api.workspace.remove(id);
    this.byRepo.get(repoId)?.delete(id);
    if (this.selectedWorkspaceId === id) {
      this.selectedWorkspaceId = null;
    }
  }

  /** Phase 3a-3 Task 18: optimistically flip the workspace's
   *  `team_activity_private` flag, then call the backend. On failure
   *  revert the flag in place and surface a toast.
   *
   *  Returns true on success, false on revert — the UI can use this to
   *  drive button-disabled state without re-reading the store.
   *
   *  When the workspace isn't yet in the store (e.g. component rendered
   *  before the sidebar loaded), the IPC is still issued so the toggle
   *  works from a deep link; the optimistic update is skipped because
   *  there's nothing local to flip yet. */
  async setTeamActivityPrivate(
    workspaceId: string,
    repoId: string,
    isPrivate: boolean
  ): Promise<boolean> {
    const inner = this.byRepo.get(repoId);
    const ws = inner?.get(workspaceId);
    const previous = ws?.team_activity_private ?? false;
    if (inner && ws) {
      // Replace the SvelteMap entry to trigger reactivity (mutating
      // the in-place object would not flip $derived consumers).
      inner.set(workspaceId, { ...ws, team_activity_private: isPrivate });
    }
    try {
      await api.workspace.setTeamActivityPrivate(workspaceId, isPrivate);
      return true;
    } catch (err) {
      // Revert the optimistic update if we made one.
      if (inner) {
        const current = inner.get(workspaceId);
        if (current) {
          inner.set(workspaceId, { ...current, team_activity_private: previous });
        }
      }
      addToast(`Privacy toggle failed: ${String(err)}`, 'error');
      return false;
    }
  }

  listForRepo(repoId: string): WorkspaceInfo[] {
    const inner = this.byRepo.get(repoId);
    if (!inner) return [];
    return [...inner.values()];
  }

  select(id: string | null): void {
    this.selectedWorkspaceId = id;
  }

  getSelected(): WorkspaceInfo | null {
    if (this.selectedWorkspaceId === null) return null;
    for (const inner of this.byRepo.values()) {
      const ws = inner.get(this.selectedWorkspaceId);
      if (ws) return ws;
    }
    return null;
  }
}

export const workspaces = new WorkspacesStore();
