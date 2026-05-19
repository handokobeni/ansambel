import { api } from '$lib/ipc';
import { larkBindings } from '$lib/stores/lark-bindings.svelte';
import { tasks } from '$lib/stores/tasks.svelte';
import { addToast } from '$lib/stores/toasts.svelte';
import type { BitableBinding, FilterSpec } from '$lib/types';

const DEBOUNCE_MS = 300;

class FilterStore {
  private timeouts = new Map<string, ReturnType<typeof setTimeout>>();
  // True baseline captured at the FIRST update() in a debounce sequence,
  // so failed persists revert to the pre-edit state rather than an
  // intermediate optimistic state from a cancelled earlier call.
  private originals = new Map<string, BitableBinding>();

  async update(repoId: string, spec: FilterSpec): Promise<void> {
    const current = larkBindings.get(repoId);
    if (!current) return;

    // Capture the true baseline only once per debounce window.
    if (!this.originals.has(repoId)) {
      this.originals.set(repoId, { ...current });
    }

    // Optimistic local update.
    larkBindings.bindings.set(repoId, { ...current, filters: spec });

    const existing = this.timeouts.get(repoId);
    if (existing) clearTimeout(existing);

    this.timeouts.set(
      repoId,
      setTimeout(async () => {
        try {
          await api.lark.setRepoBinding(repoId, { ...current, filters: spec });
          await tasks.refresh(repoId);
          this.originals.delete(repoId);
        } catch (err) {
          const baseline = this.originals.get(repoId);
          if (baseline) {
            larkBindings.bindings.set(repoId, baseline);
          }
          this.originals.delete(repoId);
          addToast(`Filter save failed: ${err instanceof Error ? err.message : err}`, 'error');
        }
      }, DEBOUNCE_MS)
    );
  }
}

export const filterStore = new FilterStore();
