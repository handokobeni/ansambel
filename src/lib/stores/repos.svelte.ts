// src/lib/stores/repos.svelte.ts
import { SvelteMap } from 'svelte/reactivity';
import { api } from '$lib/ipc';
import type { Repo } from '$lib/types';

export class ReposStore {
  readonly repos = new SvelteMap<string, Repo>();
  #selectedRepoId = $state<string | null>(null);

  get selectedRepoId(): string | null {
    return this.#selectedRepoId;
  }

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
    if (this.#selectedRepoId === id) {
      this.#selectedRepoId = null;
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
    this.#selectedRepoId = id;
  }

  getSelected(): Repo | null {
    if (this.#selectedRepoId === null) return null;
    return this.repos.get(this.#selectedRepoId) ?? null;
  }
}

export const repos = new ReposStore();
