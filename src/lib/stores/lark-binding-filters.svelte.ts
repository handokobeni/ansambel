import { api } from '$lib/ipc';
import { larkBindings } from '$lib/stores/lark-bindings.svelte';
import { addToast } from '$lib/stores/toasts.svelte';
import type { FilterSpec } from '$lib/types';

const DEBOUNCE_MS = 300;

class FilterStore {
  private timeouts = new Map<string, ReturnType<typeof setTimeout>>();

  async update(repoId: string, spec: FilterSpec): Promise<void> {
    const current = larkBindings.get(repoId);
    if (!current) return;
    const previous = { ...current };

    // Optimistic local update.
    larkBindings.bindings.set(repoId, { ...current, filters: spec });

    const existing = this.timeouts.get(repoId);
    if (existing) clearTimeout(existing);

    this.timeouts.set(
      repoId,
      setTimeout(async () => {
        try {
          await api.lark.setRepoBinding(repoId, { ...current, filters: spec });
          await api.task.refresh(repoId);
        } catch (err) {
          // Revert optimistic update.
          larkBindings.bindings.set(repoId, previous);
          addToast(
            `Filter save failed: ${err instanceof Error ? err.message : String(err)}`,
            'error'
          );
        }
      }, DEBOUNCE_MS)
    );
  }
}

export const filterStore = new FilterStore();
