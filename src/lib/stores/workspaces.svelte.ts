// src/lib/stores/workspaces.svelte.ts
import { SvelteMap } from 'svelte/reactivity';
import { api } from '$lib/ipc';
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
