// src/lib/stores/repos.svelte.ts
import { SvelteMap } from 'svelte/reactivity';
import { api } from '$lib/ipc';
import type { Repo } from '$lib/types';

export class ReposStore {
  readonly repos = new SvelteMap<string, Repo>();
  selectedRepoId = $state<string | null>(null);

  async load(): Promise<void> {
    const list = await api.repo.list();
    this.repos.clear();
    for (const repo of list) {
      this.repos.set(repo.id, repo);
    }
  }

  async add(path: string): Promise<Repo> {
    const repo = await api.repo.add(path);
    this.repos.set(repo.id, repo);
    return repo;
  }

  async remove(id: string): Promise<void> {
    await api.repo.remove(id);
    this.repos.delete(id);
    if (this.selectedRepoId === id) {
      // Route through the single persistence choke point so the persisted
      // selection is cleared too — otherwise the last-opened-repo restore
      // would resurrect a deleted repo on next start.
      this.select(null);
    }
  }

  async updateGhProfile(id: string, profile: string | null): Promise<void> {
    await api.repo.updateGhProfile(id, profile);
    const existing = this.repos.get(id);
    if (existing) {
      this.repos.set(id, { ...existing, gh_profile: profile });
    }
  }

  select(id: string | null): void {
    this.selectedRepoId = id;
    // Fire-and-forget: selection persistence must NEVER throw to a caller of
    // select(), which runs from synchronous click paths. A disk failure here
    // only costs us last-selection restore on next launch.
    api.settings.setSelectedRepo(id).catch((err) => {
      console.error('settings.setSelectedRepo failed', err);
    });
  }

  getSelected(): Repo | null {
    if (this.selectedRepoId === null) return null;
    return this.repos.get(this.selectedRepoId) ?? null;
  }
}

export const repos = new ReposStore();
