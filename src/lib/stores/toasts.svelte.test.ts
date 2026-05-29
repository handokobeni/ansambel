import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';

async function fresh() {
  vi.resetModules();
  return await import('./toasts.svelte');
}

describe('toasts store', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('addToast returns a unique id and appears in getToasts()', async () => {
    const { addToast, getToasts } = await fresh();
    const id1 = addToast('first', 'info');
    const id2 = addToast('second', 'info');
    expect(id1).not.toBe(id2);
    const map = getToasts();
    expect(map.size).toBe(2);
    expect(map.get(id1)?.message).toBe('first');
    expect(map.get(id2)?.message).toBe('second');
  });

  it('toast records carry message, type, and createdAt', async () => {
    const { addToast, getToasts } = await fresh();
    const before = Date.now();
    const id = addToast('boom', 'error');
    const after = Date.now();
    const t = getToasts().get(id);
    expect(t).toBeDefined();
    expect(t?.message).toBe('boom');
    expect(t?.type).toBe('error');
    expect(t?.createdAt).toBeGreaterThanOrEqual(before);
    expect(t?.createdAt).toBeLessThanOrEqual(after);
  });

  it('default type is error', async () => {
    const { addToast, getToasts } = await fresh();
    const id = addToast('hi');
    expect(getToasts().get(id)?.type).toBe('error');
  });

  it('removeToast deletes from the map', async () => {
    const { addToast, removeToast, getToasts } = await fresh();
    const id = addToast('x');
    removeToast(id);
    expect(getToasts().has(id)).toBe(false);
  });

  it('removeToast on unknown id is a safe no-op', async () => {
    const { removeToast, getToasts } = await fresh();
    removeToast('does-not-exist');
    expect(getToasts().size).toBe(0);
  });

  it('auto-dismisses error toast after 6s', async () => {
    const { addToast, getToasts } = await fresh();
    const id = addToast('err', 'error');
    expect(getToasts().has(id)).toBe(true);
    vi.advanceTimersByTime(5999);
    expect(getToasts().has(id)).toBe(true);
    vi.advanceTimersByTime(1);
    expect(getToasts().has(id)).toBe(false);
  });

  it('auto-dismisses info toast after 4s', async () => {
    const { addToast, getToasts } = await fresh();
    const id = addToast('info', 'info');
    vi.advanceTimersByTime(4000);
    expect(getToasts().has(id)).toBe(false);
  });

  it('auto-dismisses success toast after 3s', async () => {
    const { addToast, getToasts } = await fresh();
    const id = addToast('ok', 'success');
    vi.advanceTimersByTime(3000);
    expect(getToasts().has(id)).toBe(false);
  });

  it('duration override controls dismissal', async () => {
    const { addToast, getToasts } = await fresh();
    const id = addToast('quick', 'error', 500);
    vi.advanceTimersByTime(499);
    expect(getToasts().has(id)).toBe(true);
    vi.advanceTimersByTime(1);
    expect(getToasts().has(id)).toBe(false);
  });

  it('duration of 0 keeps toast indefinitely', async () => {
    const { addToast, getToasts } = await fresh();
    const id = addToast('sticky', 'info', 0);
    vi.advanceTimersByTime(60_000);
    expect(getToasts().has(id)).toBe(true);
  });

  it('manual removeToast clears the pending dismiss timer', async () => {
    const { addToast, removeToast, getToasts } = await fresh();
    const id = addToast('boom', 'error');
    removeToast(id);
    expect(getToasts().has(id)).toBe(false);
    // No throw or stale re-add when the original timer would have fired.
    vi.advanceTimersByTime(10_000);
    expect(getToasts().has(id)).toBe(false);
  });

  it('addToast accepts an opts object with actions + timeoutMs', async () => {
    const { addToast, getToasts } = await fresh();
    const onClick = vi.fn();
    const id = addToast('hello', 'info', {
      timeoutMs: 10_000,
      actions: [{ label: 'Undo', onClick }],
    });
    const t = getToasts().get(id);
    expect(t?.actions).toBeDefined();
    expect(t?.actions?.length).toBe(1);
    expect(t?.actions?.[0].label).toBe('Undo');
    expect(t?.timeoutMs).toBe(10_000);
    // Default 4s would have dismissed; 10s opts keeps it alive at 4s.
    vi.advanceTimersByTime(4000);
    expect(getToasts().has(id)).toBe(true);
    vi.advanceTimersByTime(6000);
    expect(getToasts().has(id)).toBe(false);
  });

  it('addToast preserves numeric duration backward-compat', async () => {
    const { addToast, getToasts } = await fresh();
    const id = addToast('quick', 'info', 250);
    vi.advanceTimersByTime(249);
    expect(getToasts().has(id)).toBe(true);
    vi.advanceTimersByTime(1);
    expect(getToasts().has(id)).toBe(false);
  });
});
