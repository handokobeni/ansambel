import { describe, expect, it, vi, beforeEach } from 'vitest';

const setRepoBinding = vi.fn();
const refresh = vi.fn();
const addToast = vi.fn();

vi.mock('$lib/ipc', () => ({
  api: {
    lark: { setRepoBinding: (...a: unknown[]) => setRepoBinding(...a) },
    task: { refresh: (...a: unknown[]) => refresh(...a) },
  },
}));
vi.mock('$lib/stores/toasts.svelte', () => ({ addToast: (...a: unknown[]) => addToast(...a) }));

// Mock larkBindings store with a tiny SvelteMap-like impl.
const bindings = new Map<string, any>();
const baseBinding = {
  app_token: 'appA',
  table_id: 'tblA',
  filters: { conjunction: 'and' as const, conditions: [] },
  field_mapping: { title: { field_id: 'f', field_name: 'F' } },
  status_value_mapping: { entries: {}, default_column: 'todo' as const },
  created_at: 0,
  updated_at: 0,
};
bindings.set('repo-1', { ...baseBinding });

vi.mock('$lib/stores/lark-bindings.svelte', () => ({
  larkBindings: {
    get: (repoId: string) => bindings.get(repoId),
    bindings: { set: (k: string, v: unknown) => bindings.set(k, v) },
  },
}));

beforeEach(() => {
  setRepoBinding.mockReset();
  refresh.mockReset();
  addToast.mockReset();
  bindings.set('repo-1', { ...baseBinding });
  vi.useFakeTimers();
});

describe('filterStore.update', () => {
  it('lands optimistic update immediately then persists after 300 ms debounce', async () => {
    const { filterStore } = await import('./lark-binding-filters.svelte');
    setRepoBinding.mockResolvedValue(undefined);
    refresh.mockResolvedValue(undefined);

    const next = {
      conjunction: 'and' as const,
      conditions: [{ field_id: 'f1', field_name: 'F1', operator: 'is' as const, value: ['x'] }],
    };
    await filterStore.update('repo-1', next);

    expect(bindings.get('repo-1').filters).toEqual(next);
    expect(setRepoBinding).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(300);
    expect(setRepoBinding).toHaveBeenCalledOnce();
    expect(refresh).toHaveBeenCalledOnce();
  });

  it('reverts optimistic update and toasts when persist fails', async () => {
    const { filterStore } = await import('./lark-binding-filters.svelte');
    setRepoBinding.mockRejectedValue(new Error('disk full'));

    const next = { conjunction: 'or' as const, conditions: [] };
    await filterStore.update('repo-1', next);
    await vi.advanceTimersByTimeAsync(300);
    await vi.runAllTimersAsync();

    expect(bindings.get('repo-1').filters).toEqual(baseBinding.filters);
    expect(addToast).toHaveBeenCalledOnce();
  });
});
