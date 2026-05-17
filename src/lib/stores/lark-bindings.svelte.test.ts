import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('$lib/ipc', () => ({
  api: {
    lark: {
      listRepoBindings: vi.fn(),
      setRepoBinding: vi.fn(),
      deleteRepoBinding: vi.fn(),
    },
  },
}));

vi.mock('$lib/stores/toasts.svelte', () => ({
  addToast: vi.fn(),
}));

import { api } from '$lib/ipc';
import { addToast } from '$lib/stores/toasts.svelte';
import { LarkBindingsStore } from './lark-bindings.svelte';
import type { BitableBinding } from '$lib/types';

const makeBinding = (overrides: Partial<BitableBinding> = {}): BitableBinding => ({
  app_token: 'bascn',
  table_id: 'tbl',
  view_id: null,
  field_mapping: {
    title: { field_id: 'fld_t', field_name: 'Task name' },
    description: null,
    status: null,
    order: null,
  },
  status_value_mapping: { entries: {}, default_column: 'todo' },
  created_at: 0,
  updated_at: 0,
  ...overrides,
});

beforeEach(() => {
  vi.clearAllMocks();
});

describe('LarkBindingsStore', () => {
  it('load populates map from list response', async () => {
    vi.mocked(api.lark.listRepoBindings).mockResolvedValue({
      repo_x: makeBinding({ app_token: 'one' }),
      repo_y: makeBinding({ app_token: 'two' }),
    });
    const s = new LarkBindingsStore();
    await s.load();
    expect(s.has('repo_x')).toBe(true);
    expect(s.get('repo_y')?.app_token).toBe('two');
  });

  it('setBinding optimistically writes then reconciles', async () => {
    vi.mocked(api.lark.setRepoBinding).mockResolvedValue(undefined);
    const s = new LarkBindingsStore();
    const b = makeBinding();
    const p = s.setBinding('repo_x', b);
    expect(s.has('repo_x')).toBe(true);
    await p;
    expect(s.get('repo_x')).toEqual(b);
  });

  it('setBinding reverts and toasts on error', async () => {
    vi.mocked(api.lark.setRepoBinding).mockRejectedValueOnce(new Error('IPC fail'));
    const s = new LarkBindingsStore();
    await s.setBinding('repo_x', makeBinding()).catch(() => {});
    expect(s.has('repo_x')).toBe(false);
    expect(addToast).toHaveBeenCalledWith(expect.stringContaining('IPC fail'), 'error');
  });

  it('deleteBinding optimistically removes then reconciles', async () => {
    vi.mocked(api.lark.deleteRepoBinding).mockResolvedValue(undefined);
    const s = new LarkBindingsStore();
    s.bindings.set('repo_x', makeBinding());
    await s.deleteBinding('repo_x');
    expect(s.has('repo_x')).toBe(false);
  });

  it('deleteBinding reverts and toasts on error', async () => {
    vi.mocked(api.lark.deleteRepoBinding).mockRejectedValueOnce(new Error('IPC fail'));
    const s = new LarkBindingsStore();
    s.bindings.set('repo_x', makeBinding());
    await s.deleteBinding('repo_x').catch(() => {});
    expect(s.has('repo_x')).toBe(true);
    expect(addToast).toHaveBeenCalled();
  });

  it('setBinding reverts to previous binding (not just deletes) when set fails with existing binding', async () => {
    vi.mocked(api.lark.setRepoBinding).mockRejectedValueOnce(new Error('IPC fail'));
    const s = new LarkBindingsStore();
    const original = makeBinding({ app_token: 'original_token' });
    const updated = makeBinding({ app_token: 'new_token' });
    s.bindings.set('repo_x', original);
    await s.setBinding('repo_x', updated).catch(() => {});
    // Should have reverted to original, not deleted
    expect(s.has('repo_x')).toBe(true);
    expect(s.get('repo_x')?.app_token).toBe('original_token');
  });

  it('deleteBinding toast uses String(err) when error is not an Error instance', async () => {
    vi.mocked(api.lark.deleteRepoBinding).mockRejectedValueOnce('string error value');
    const s = new LarkBindingsStore();
    s.bindings.set('repo_x', makeBinding());
    await s.deleteBinding('repo_x').catch(() => {});
    expect(addToast).toHaveBeenCalledWith(expect.stringContaining('string error value'), 'error');
  });

  it('setBinding toast uses String(err) when error is not an Error instance', async () => {
    vi.mocked(api.lark.setRepoBinding).mockRejectedValueOnce('plain string fail');
    const s = new LarkBindingsStore();
    await s.setBinding('repo_x', makeBinding()).catch(() => {});
    expect(addToast).toHaveBeenCalledWith(expect.stringContaining('plain string fail'), 'error');
  });
});
