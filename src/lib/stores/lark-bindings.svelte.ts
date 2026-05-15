import { SvelteMap } from 'svelte/reactivity';
import { api } from '$lib/ipc';
import { addToast } from '$lib/stores/toasts.svelte';
import type { BitableBinding } from '$lib/types';

export class LarkBindingsStore {
  readonly bindings = new SvelteMap<string, BitableBinding>();

  async load(): Promise<void> {
    const list = await api.lark.listRepoBindings();
    this.bindings.clear();
    for (const [repoId, b] of Object.entries(list)) {
      this.bindings.set(repoId, b);
    }
  }

  has(repoId: string): boolean {
    return this.bindings.has(repoId);
  }

  get(repoId: string): BitableBinding | undefined {
    return this.bindings.get(repoId);
  }

  /** Optimistic upsert: writes locally first, awaits backend, reverts on error. */
  async setBinding(repoId: string, binding: BitableBinding): Promise<void> {
    const prev = this.bindings.get(repoId);
    this.bindings.set(repoId, binding);
    try {
      await api.lark.setRepoBinding(repoId, binding);
    } catch (err) {
      if (prev) this.bindings.set(repoId, prev);
      else this.bindings.delete(repoId);
      addToast(`Save binding failed: ${err instanceof Error ? err.message : String(err)}`, 'error');
      throw err;
    }
  }

  async deleteBinding(repoId: string): Promise<void> {
    const prev = this.bindings.get(repoId);
    this.bindings.delete(repoId);
    try {
      await api.lark.deleteRepoBinding(repoId);
    } catch (err) {
      if (prev) this.bindings.set(repoId, prev);
      addToast(
        `Delete binding failed: ${err instanceof Error ? err.message : String(err)}`,
        'error'
      );
      throw err;
    }
  }
}

export const larkBindings = new LarkBindingsStore();
