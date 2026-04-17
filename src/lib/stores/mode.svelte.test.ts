// src/lib/stores/mode.svelte.test.ts
import { describe, it, expect } from 'vitest';
import { ModeStore } from './mode.svelte';

describe('ModeStore', () => {
  it('starts in plan mode', () => {
    const store = new ModeStore();
    expect(store.mode).toBe('plan');
  });

  it('set: switches to work mode', () => {
    const store = new ModeStore();
    store.set('work');
    expect(store.mode).toBe('work');
  });

  it('set: can switch back to plan mode', () => {
    const store = new ModeStore();
    store.set('work');
    store.set('plan');
    expect(store.mode).toBe('plan');
  });
});
